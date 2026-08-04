//! Parses one `MTrk` chunk's bytes into a flat, time-ordered list of
//! [`RawEvent`]s: channel-voice messages (tagged with their channel) plus
//! the handful of meta events this compiler cares about (tempo, the four
//! text-based loop markers). Absolute tick positions, running status, and
//! event recognition mirror `tools/mid2agb/midi.cpp`'s `ReadSeqEvent`
//! (`:253-348`) and `ReadTrackEvent` (`:450-540`) — but as a single unified
//! pass rather than upstream's two separately-scoped scans.
//!
//! # One pass, not sixteen (or two)
//!
//! Upstream re-scans each track's raw bytes once as "sequence events"
//! (`ReadSeqEvent`, meta-only: tempo + loop markers) and up to sixteen more
//! times as "track events" (`ReadTrackEvent`, once per possible MIDI
//! channel, channel-voice-only) — a byte-stream/global-mutable-state
//! artifact of the original C tool's design, not a behavioural requirement.
//! Running status is track-scoped and reset only by meta/sysex bytes
//! (`DetermineEventCategory`, `:182-225`), so a single linear scan that
//! records *every* recognized event (tagging channel-voice messages with
//! their channel) sees the exact same bytes in the exact same order upstream
//! does; [`super::compile`] filters this one parse's output by purpose
//! (meta-only from track 0 for "sequence events", channel-matching from
//! every track for a given channel's "track events") instead of re-scanning
//! per channel (`no-verbatim`: re-expressed idiomatically, not
//! transliterated).
//!
//! # What's recognized, what's dropped
//!
//! Recognized: note-on (velocity `0` folds to note-off, matching
//! `midi.cpp:478`), note-off, control change, program change, pitch bend
//! (only the MSB — see [`RawEvent::PitchBend`]'s docs), the tempo meta event,
//! and the four one/two-character text meta events `ReadSeqEvent` treats as
//! loop markers (`:287-296`). Recognized-but-ignored per upstream: poly/channel
//! aftertouch (skipped, matching `ReadTrackEvent`'s own `default: Skip`
//! branch), every other meta event including time signature (matching
//! `ReadSeqEvent`'s `default: SkipEventData` — time signature only ever fed
//! upstream's own whole-note/pattern-compression bookkeeping, which this
//! compiler doesn't implement; see `super`'s module docs). `SysEx` events are
//! skipped by declared length. Anything not listed here that still consumes
//! bytes (e.g. a text meta event whose payload doesn't match `[`/`]`/`][`/`:`)
//! is parsed far enough to skip correctly but yields no [`RawEvent`].
//!
//! ## System-common/real-time status bytes: rejected, not skipped
//!
//! One deliberate narrowing: [`parse_track`] accepts only `0xF0`/`0xF7` out
//! of the `0xF_` range (plus `0xFF`, the meta-event escape) and rejects
//! `0xF1`..=`0xF6` and `0xF8`..=`0xFE` as
//! [`MidiError::InvalidStatusByte`]. `DetermineEventCategory`
//! (`midi.cpp:199-204`) instead lumps *every* `typeChan >= 0xF0` into
//! `MidiEventCategory::SysEx` and skips it by a VLQ length — right for
//! `0xF0`/`0xF7`, wrong for the rest: song position (`0xF2`) carries two
//! fixed data bytes and the real-time bytes (`0xF8`..=`0xFE`) carry none,
//! so reading a length there consumes the wrong bytes and desynchronizes
//! the scan from that point on. No standard MIDI *file* stores those (they
//! are wire-protocol messages), so upstream's mis-framing is unreachable in
//! practice and this rejection costs no real input; failing closed on a
//! byte whose correct handling upstream gets wrong is the safer of the two.
//! Recorded because it is a genuine difference in what each side accepts,
//! not just in diagnostics.

use super::error::MidiError;
use super::reader::MidiReader;

/// One recognized event out of a single `MTrk` chunk's linear scan, with its
/// absolute tick position folded in by the caller (see [`parse_track`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RawEvent {
    /// A note-on with nonzero velocity (`midi.cpp:473-478`).
    NoteOn { channel: u8, key: u8, velocity: u8 },
    /// A note-off, or a note-on with velocity `0` (which upstream treats
    /// identically — `midi.cpp:401-402`).
    NoteOff { channel: u8, key: u8 },
    /// A control-change message (`midi.cpp:494-497`).
    Controller {
        channel: u8,
        controller: u8,
        value: u8,
    },
    /// A program-change message (`midi.cpp:499-502`).
    ProgramChange { channel: u8, program: u8 },
    /// A pitch-bend message. Only the MSB (`data2`) is kept: upstream reads
    /// both bytes (`midi.cpp:504-508`: `param1` = LSB, `param2` = MSB) but
    /// `agb.cpp:512-514`'s `PrintAgbTrack` only ever reads `param2` when it
    /// emits `BEND` — the LSB (fine bend) is parsed and then never
    /// referenced again. This compiler drops it at the same point upstream
    /// effectively does, rather than threading a dead field through the
    /// rest of the pipeline.
    PitchBend { channel: u8, msb: u8 },
    /// A tempo meta event; the value is microseconds per quarter note
    /// (`midi.cpp:308-315`).
    Tempo(u32),
    /// Text meta event `"["` (`midi.cpp:288`).
    LoopBegin,
    /// Text meta event `"]["` (`midi.cpp:290`).
    LoopEndBegin,
    /// Text meta event `"]"` (`midi.cpp:292`).
    LoopEnd,
    /// Text meta event `":"` (`midi.cpp:294`).
    Label,
}

impl RawEvent {
    /// The channel a channel-voice event carries, or `None` for the
    /// channel-agnostic meta events. Used by [`super::compile`] to filter
    /// one track's parse down to a single channel's events.
    pub(super) fn channel(self) -> Option<u8> {
        match self {
            Self::NoteOn { channel, .. }
            | Self::NoteOff { channel, .. }
            | Self::Controller { channel, .. }
            | Self::ProgramChange { channel, .. }
            | Self::PitchBend { channel, .. } => Some(channel),
            Self::Tempo(_) | Self::LoopBegin | Self::LoopEndBegin | Self::LoopEnd | Self::Label => {
                None
            }
        }
    }
}

/// One `MTrk` chunk's parse: every recognized event with its absolute tick
/// position (ascending — VLQ deltas are non-negative), plus the absolute
/// tick position of the chunk's own `EndOfTrack` meta event (needed to
/// compute the final trailing wait before `FINE`; see
/// [`super::compile`]).
#[derive(Debug)]
pub(super) struct ParsedTrack {
    pub(super) events: Vec<(u32, RawEvent)>,
    pub(super) end_of_track: u32,
}

/// Read a 1-or-2-character text meta event's payload, matching
/// `tools/mid2agb/midi.cpp:234-251`'s `ReadEventText`: payloads of length
/// `<= 2` are read and compared; anything longer is skipped and treated as
/// non-matching (no marker text this compiler recognizes is longer than 2
/// characters, so the two are behaviourally identical — this compiler
/// simplifies to "skip and return `None`" rather than upstream's
/// "read into a fixed 2-byte buffer and return an empty string").
fn read_marker_text(r: &mut MidiReader<'_>) -> Result<Option<([u8; 2], usize)>, MidiError> {
    let len = r.vlq()?;
    if len > 2 {
        r.skip(usize::try_from(len).map_err(|_| MidiError::Truncated)?)?;
        return Ok(None);
    }
    #[allow(clippy::cast_possible_truncation)]
    let len = len as usize;
    let bytes = r.bytes(len)?;
    let mut buf = [0u8; 2];
    buf[..len].copy_from_slice(bytes);
    Ok(Some((buf, len)))
}

fn marker_event(text: [u8; 2], len: usize) -> Option<RawEvent> {
    match (len, text[0], text[1]) {
        (1, b'[', _) => Some(RawEvent::LoopBegin),
        (1, b']', _) => Some(RawEvent::LoopEnd),
        (1, b':', _) => Some(RawEvent::Label),
        (2, b']', b'[') => Some(RawEvent::LoopEndBegin),
        _ => None,
    }
}

/// Read one channel-voice message's operand bytes (the status byte itself
/// is already consumed/resolved by the caller) and push whatever
/// [`RawEvent`] it produces, if any (`0xA0` poly aftertouch and `0xD0`
/// channel aftertouch are read and dropped, matching `ReadTrackEvent`'s own
/// `default: Skip` branch).
fn read_channel_voice_event(
    r: &mut MidiReader<'_>,
    events: &mut Vec<(u32, RawEvent)>,
    abs_time: u32,
    status: u8,
) -> Result<(), MidiError> {
    let channel = status & 0x0F;
    match status & 0xF0 {
        0x80 => {
            let key = r.u8()?;
            let _velocity = r.u8()?;
            events.push((abs_time, RawEvent::NoteOff { channel, key }));
        }
        0x90 => {
            let key = r.u8()?;
            let velocity = r.u8()?;
            let event = if velocity == 0 {
                RawEvent::NoteOff { channel, key }
            } else {
                RawEvent::NoteOn {
                    channel,
                    key,
                    velocity,
                }
            };
            events.push((abs_time, event));
        }
        0xA0 => {
            r.u8()?;
            r.u8()?;
        }
        0xB0 => {
            let controller = r.u8()?;
            let value = r.u8()?;
            events.push((
                abs_time,
                RawEvent::Controller {
                    channel,
                    controller,
                    value,
                },
            ));
        }
        0xC0 => {
            let program = r.u8()?;
            events.push((abs_time, RawEvent::ProgramChange { channel, program }));
        }
        0xD0 => {
            r.u8()?;
        }
        0xE0 => {
            let _lsb = r.u8()?;
            let msb = r.u8()?;
            events.push((abs_time, RawEvent::PitchBend { channel, msb }));
        }
        _ => unreachable!("0x80..0xF0 masked by 0xF0 always lands on a handled nibble"),
    }
    Ok(())
}

/// The outcome of reading one meta event (`0xFF`): either it's the track's
/// `EndOfTrack`, in which case parsing is done, or it isn't and the caller's
/// loop continues.
enum MetaOutcome {
    Continue,
    EndOfTrack,
}

/// Read one meta event's type byte and payload, pushing whatever
/// [`RawEvent`] it produces (tempo, a recognized loop-marker text) and
/// reporting whether it was `EndOfTrack`.
fn read_meta_event(
    r: &mut MidiReader<'_>,
    events: &mut Vec<(u32, RawEvent)>,
    abs_time: u32,
) -> Result<MetaOutcome, MidiError> {
    let meta_type = r.u8()?;
    if (1..=7).contains(&meta_type) {
        if let Some((text, len)) = read_marker_text(r)? {
            if let Some(event) = marker_event(text, len) {
                events.push((abs_time, event));
            }
        }
        return Ok(MetaOutcome::Continue);
    }
    if meta_type == 0x2F {
        let len = r.vlq()?;
        r.skip(usize::try_from(len).map_err(|_| MidiError::Truncated)?)?;
        return Ok(MetaOutcome::EndOfTrack);
    }
    if meta_type == 0x51 {
        let len = r.vlq()?;
        if len != 3 {
            return Err(MidiError::BadTempoLength(len));
        }
        let microseconds = r.u24_be()?;
        events.push((abs_time, RawEvent::Tempo(microseconds)));
        return Ok(MetaOutcome::Continue);
    }
    let len = r.vlq()?;
    r.skip(usize::try_from(len).map_err(|_| MidiError::Truncated)?)?;
    Ok(MetaOutcome::Continue)
}

/// Parse one `MTrk` chunk's body (the bytes right after its 4-byte id and
/// `u32` length — see [`super::reader::split_tracks`]).
///
/// # Errors
///
/// See [`MidiError`]'s variants for the byte-level failure modes; a
/// malformed running-status state (a data byte with no status yet in
/// effect, or an unrecognized channel-voice nibble) is
/// [`MidiError::InvalidStatusByte`].
pub(super) fn parse_track(data: &[u8]) -> Result<ParsedTrack, MidiError> {
    let mut r = MidiReader::new(data);
    let mut events = Vec::new();
    let mut abs_time: u32 = 0;
    let mut running_status: u8 = 0;

    loop {
        let delta = r.vlq()?;
        abs_time = abs_time.checked_add(delta).ok_or(MidiError::Truncated)?;

        let peeked = r.peek_u8()?;
        let status = if peeked < 0x80 {
            if running_status == 0 {
                return Err(MidiError::InvalidStatusByte(peeked));
            }
            running_status
        } else {
            r.u8()?
        };

        match status {
            0xFF => {
                running_status = 0;
                if matches!(
                    read_meta_event(&mut r, &mut events, abs_time)?,
                    MetaOutcome::EndOfTrack
                ) {
                    return Ok(ParsedTrack {
                        events,
                        end_of_track: abs_time,
                    });
                }
            }
            0xF0 | 0xF7 => {
                running_status = 0;
                let len = r.vlq()?;
                r.skip(usize::try_from(len).map_err(|_| MidiError::Truncated)?)?;
            }
            _ if (0x80..0xF0).contains(&status) => {
                running_status = status;
                read_channel_voice_event(&mut r, &mut events, abs_time, status)?;
            }
            other => return Err(MidiError::InvalidStatusByte(other)),
        }
    }
}

#[cfg(test)]
mod tests;
