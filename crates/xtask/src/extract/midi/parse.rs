//! Standard MIDI track event parsing.
//!
//! Each track is scanned once. Channel events retain their source channel, while
//! meta events remain channel-agnostic for [`super::compile`] to filter.
//!
//! System-common and real-time status bytes fail closed. Upstream's
//! `DetermineEventCategory` (`tools/mid2agb/midi.cpp:182-225`) treats every status
//! from `0xF0` onward as length-prefixed system-exclusive data, which misframes
//! wire-only messages such as song position and MIDI clock. Standard MIDI files
//! do not contain those messages, so rejecting them only narrows malformed input.
//! The unified scan likewise validates time signatures in every track, unlike
//! upstream's separate sequence and channel passes (`midi.cpp:316-340, 524-535`).

use super::error::MidiError;
use super::reader::MidiReader;

const DATA_BYTE_BOUNDARY: u8 = 0x80;
const CHANNEL_MASK: u8 = 0x0F;
const MESSAGE_KIND_MASK: u8 = 0xF0;

const NOTE_OFF: u8 = 0x80;
const NOTE_ON: u8 = 0x90;
const POLYPHONIC_KEY_PRESSURE: u8 = 0xA0;
const CONTROL_CHANGE: u8 = 0xB0;
const PROGRAM_CHANGE: u8 = 0xC0;
const CHANNEL_PRESSURE: u8 = 0xD0;
const PITCH_BEND: u8 = 0xE0;
const LAST_CHANNEL_STATUS: u8 = 0xEF;

const SYSTEM_EXCLUSIVE: u8 = 0xF0;
const SYSTEM_EXCLUSIVE_ESCAPE: u8 = 0xF7;
const META_EVENT: u8 = 0xFF;

const FIRST_TEXT_META_EVENT: u8 = 0x01;
const LAST_TEXT_META_EVENT: u8 = 0x07;
const END_OF_TRACK_META_EVENT: u8 = 0x2F;
const TEMPO_META_EVENT: u8 = 0x51;
const TIME_SIGNATURE_META_EVENT: u8 = 0x58;
const TEMPO_PAYLOAD_LEN: u32 = 3;
const TIME_SIGNATURE_PAYLOAD_LEN: u32 = 4;
const MAX_DENOMINATOR_EXPONENT: u8 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RawEvent {
    NoteOn {
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        key: u8,
    },
    Controller {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
    /// Coarse pitch bend; upstream emits only the parsed MSB (`agb.cpp:512-514`).
    PitchBend {
        channel: u8,
        msb: u8,
    },
    /// Microseconds per quarter note.
    Tempo(u32),
    LoopBegin,
    LoopEndBegin,
    LoopEnd,
    Label,
    /// The grid period is derived later because it also needs clocks per beat.
    TimeSignature {
        numerator: u8,
        denominator_exponent: u8,
    },
}

impl RawEvent {
    pub(super) fn channel(self) -> Option<u8> {
        match self {
            Self::NoteOn { channel, .. }
            | Self::NoteOff { channel, .. }
            | Self::Controller { channel, .. }
            | Self::ProgramChange { channel, .. }
            | Self::PitchBend { channel, .. } => Some(channel),
            Self::Tempo(_)
            | Self::LoopBegin
            | Self::LoopEndBegin
            | Self::LoopEnd
            | Self::Label
            | Self::TimeSignature { .. } => None,
        }
    }
}

#[derive(Debug)]
pub(super) struct ParsedTrack {
    pub(super) events: Vec<(u32, RawEvent)>,
    pub(super) end_of_track: u32,
}

fn read_payload<'a>(r: &mut MidiReader<'a>) -> Result<&'a [u8], MidiError> {
    let len = usize::try_from(r.vlq()?).map_err(|_| MidiError::Truncated)?;
    r.bytes(len)
}

fn marker_event(text: &[u8]) -> Option<RawEvent> {
    match text {
        b"[" => Some(RawEvent::LoopBegin),
        b"][" => Some(RawEvent::LoopEndBegin),
        b"]" => Some(RawEvent::LoopEnd),
        b":" => Some(RawEvent::Label),
        _ => None,
    }
}

fn read_data_byte(r: &mut MidiReader<'_>) -> Result<u8, MidiError> {
    let byte = r.u8()?;
    if byte >= DATA_BYTE_BOUNDARY {
        return Err(MidiError::InvalidDataByte(byte));
    }
    Ok(byte)
}

fn read_channel_voice_event(
    r: &mut MidiReader<'_>,
    events: &mut Vec<(u32, RawEvent)>,
    absolute_time: u32,
    status: u8,
) -> Result<(), MidiError> {
    let channel = status & CHANNEL_MASK;
    match status & MESSAGE_KIND_MASK {
        NOTE_OFF => {
            let key = read_data_byte(r)?;
            let _release_velocity = read_data_byte(r)?;
            events.push((absolute_time, RawEvent::NoteOff { channel, key }));
        }
        NOTE_ON => {
            let key = read_data_byte(r)?;
            let velocity = read_data_byte(r)?;
            let event = if velocity == 0 {
                RawEvent::NoteOff { channel, key }
            } else {
                RawEvent::NoteOn {
                    channel,
                    key,
                    velocity,
                }
            };
            events.push((absolute_time, event));
        }
        POLYPHONIC_KEY_PRESSURE => {
            let _key = read_data_byte(r)?;
            let _pressure = read_data_byte(r)?;
        }
        CONTROL_CHANGE => {
            let controller = read_data_byte(r)?;
            let value = read_data_byte(r)?;
            events.push((
                absolute_time,
                RawEvent::Controller {
                    channel,
                    controller,
                    value,
                },
            ));
        }
        PROGRAM_CHANGE => {
            let program = read_data_byte(r)?;
            events.push((absolute_time, RawEvent::ProgramChange { channel, program }));
        }
        CHANNEL_PRESSURE => {
            let _pressure = read_data_byte(r)?;
        }
        PITCH_BEND => {
            let _lsb = read_data_byte(r)?;
            let msb = read_data_byte(r)?;
            events.push((absolute_time, RawEvent::PitchBend { channel, msb }));
        }
        _ => unreachable!("a channel status always has a recognized message kind"),
    }
    Ok(())
}

enum MetaOutcome {
    Continue,
    EndOfTrack,
}

fn read_meta_event(
    r: &mut MidiReader<'_>,
    events: &mut Vec<(u32, RawEvent)>,
    absolute_time: u32,
) -> Result<MetaOutcome, MidiError> {
    let meta_type = r.u8()?;
    match meta_type {
        FIRST_TEXT_META_EVENT..=LAST_TEXT_META_EVENT => {
            if let Some(event) = marker_event(read_payload(r)?) {
                events.push((absolute_time, event));
            }
        }
        END_OF_TRACK_META_EVENT => {
            let _payload = read_payload(r)?;
            return Ok(MetaOutcome::EndOfTrack);
        }
        TEMPO_META_EVENT => {
            let len = r.vlq()?;
            if len != TEMPO_PAYLOAD_LEN {
                return Err(MidiError::BadTempoLength(len));
            }
            let microseconds_per_quarter_note = r.u24_be()?;
            if microseconds_per_quarter_note == 0 {
                return Err(MidiError::ZeroTempo);
            }
            events.push((
                absolute_time,
                RawEvent::Tempo(microseconds_per_quarter_note),
            ));
        }
        TIME_SIGNATURE_META_EVENT => {
            let len = r.vlq()?;
            if len != TIME_SIGNATURE_PAYLOAD_LEN {
                return Err(MidiError::BadTimeSignatureLength(len));
            }
            let numerator = r.u8()?;
            let denominator_exponent = r.u8()?;
            if denominator_exponent > MAX_DENOMINATOR_EXPONENT {
                return Err(MidiError::BadTimeSignatureDenominator(denominator_exponent));
            }
            let _midi_clocks_per_metronome_click = r.u8()?;
            let _notated_32nd_notes_per_quarter_note = r.u8()?;
            events.push((
                absolute_time,
                RawEvent::TimeSignature {
                    numerator,
                    denominator_exponent,
                },
            ));
        }
        _ => {
            let _payload = read_payload(r)?;
        }
    }
    Ok(MetaOutcome::Continue)
}

/// Parses one `MTrk` body.
///
/// # Errors
///
/// Returns a [`MidiError`] when framing, status, event data, or tick arithmetic
/// is invalid.
pub(super) fn parse_track(data: &[u8]) -> Result<ParsedTrack, MidiError> {
    let mut r = MidiReader::new(data);
    let mut events = Vec::new();
    let mut absolute_time: u32 = 0;
    let mut running_status: Option<u8> = None;

    loop {
        let delta = r.vlq()?;
        absolute_time = absolute_time
            .checked_add(delta)
            .ok_or(MidiError::Truncated)?;

        let next_byte = r.peek_u8()?;
        let status = if next_byte < DATA_BYTE_BOUNDARY {
            running_status.ok_or(MidiError::InvalidStatusByte(next_byte))?
        } else {
            r.u8()?
        };

        match status {
            META_EVENT => {
                running_status = None;
                if matches!(
                    read_meta_event(&mut r, &mut events, absolute_time)?,
                    MetaOutcome::EndOfTrack
                ) {
                    return Ok(ParsedTrack {
                        events,
                        end_of_track: absolute_time,
                    });
                }
            }
            SYSTEM_EXCLUSIVE | SYSTEM_EXCLUSIVE_ESCAPE => {
                running_status = None;
                let _payload = read_payload(&mut r)?;
            }
            NOTE_OFF..=LAST_CHANNEL_STATUS => {
                running_status = Some(status);
                read_channel_voice_event(&mut r, &mut events, absolute_time, status)?;
            }
            other => return Err(MidiError::InvalidStatusByte(other)),
        }
    }
}

#[cfg(test)]
mod tests;
