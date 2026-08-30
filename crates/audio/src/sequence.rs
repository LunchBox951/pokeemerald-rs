//! Decodes MP2K track bytecode into typed sequencer events.

use std::error::Error;
use std::fmt;
use std::mem::size_of;

/// A decoded track command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Plays a note for `gate` ticks. A zero gate ties it until an end-of-tie
    /// or explicit stop.
    Note { key: u8, velocity: u8, gate: u8 },
    /// Ends a tie, using the track's current key when `key` is `None`.
    EndOfTie { key: Option<u8> },
    /// Delays this track for the given number of sequencer ticks.
    Wait(u8),
    /// Selects an instrument from the song's voicegroup.
    Voice(u8),
    /// Sets track volume in `0..=127`.
    Volume(u8),
    /// Sets centre-relative track pan in `-64..=63`.
    Pan(i8),
    /// Sets tempo in BPM, clamped by the sequencer to [`MAX_TEMPO_BPM`].
    Tempo(u16),
    /// Transposes the track by whole semitones.
    KeyShift(i8),
    /// Sets centre-relative pitch bend.
    Bend(i8),
    /// Sets pitch-bend range in semitones.
    BendRange(u8),
    /// Sets centre-relative fine tuning.
    Tune(i8),
    /// Jumps to another decoded event.
    Goto(usize),
    /// Ends the track.
    Fine,
    /// Sets track priority.
    Priority(u8),
    /// Sets LFO speed.
    LfoSpeed(u8),
    /// Sets LFO delay.
    LfoDelay(u8),
    /// Sets modulation depth.
    Modulation(u8),
    /// Sets modulation type.
    ModType(u8),
    /// Calls a pattern at another decoded event.
    Pattern(usize),
    /// Returns from a pattern.
    PatternEnd,
    /// Repeats from `target` for `count` iterations.
    Repeat { count: u8, target: usize },
    /// Mutates or compares sequence memory. Conditional operations carry a
    /// decoded jump target; mutation operations do not.
    MemAcc {
        op: u8,
        addr: u8,
        value: u8,
        target: Option<usize>,
    },
    /// Carries an extended command and its little-endian payload.
    Xcmd { kind: u8, value: u32 },
    /// Writes `value` to a CGB sound control selected by `control`. Decoded
    /// for stream fidelity; the sequencer does not perform the write.
    Port { control: u8, value: u8 },
}

/// Largest BPM representable by a doubled one-byte tempo operand.
pub const MAX_TEMPO_BPM: u16 = u8::MAX as u16 * 2;

/// Clamps BPM to the sequence tempo command's domain.
#[must_use]
pub fn clamp_tempo(bpm: u16) -> u16 {
    bpm.min(MAX_TEMPO_BPM)
}

/// An invalid track byte program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The stream ended in the middle of a command.
    UnexpectedEnd,
    /// An argument byte appeared with no preceding running-status command.
    RunningStatusWithoutCommand { offset: usize },
    /// A status byte has no defined command.
    UnknownCommand { offset: usize, byte: u8 },
    /// A `GOTO`/`PATT`/`REPT` target byte offset did not land on a decoded
    /// event boundary.
    UnresolvedJump { offset: usize, target: u32 },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "track ended mid-command"),
            Self::RunningStatusWithoutCommand { offset } => {
                write!(f, "argument byte at {offset} with no running status")
            }
            Self::UnknownCommand { offset, byte } => {
                write!(f, "unknown command {byte:#04x} at {offset}")
            }
            Self::UnresolvedJump { offset, target } => {
                write!(f, "jump at {offset} to unaligned target {target:#x}")
            }
        }
    }
}

impl Error for DecodeError {}

const STATUS_BYTE_MIN: u8 = 0x80;
const WAIT_LO: u8 = 0x80;
const WAIT_HI: u8 = 0xB0;
const FINE: u8 = 0xB1;
const GOTO: u8 = 0xB2;
const PATT: u8 = 0xB3;
const PEND: u8 = 0xB4;
const REPT: u8 = 0xB5;
const MEMACC: u8 = 0xB9;
const PRIO: u8 = 0xBA;
const TEMPO: u8 = 0xBB;
const KEYSH: u8 = 0xBC;
const VOICE: u8 = 0xBD;
const VOL: u8 = 0xBE;
const PAN: u8 = 0xBF;
const BEND: u8 = 0xC0;
const BENDR: u8 = 0xC1;
const LFOS: u8 = 0xC2;
const LFODL: u8 = 0xC3;
const MOD: u8 = 0xC4;
const MODT: u8 = 0xC5;
const TUNE: u8 = 0xC8;
const PORT: u8 = 0xCC;
const XCMD: u8 = 0xCD;
const EOT: u8 = 0xCE;
const TIE: u8 = 0xCF;
const CENTER: u8 = 0x40;
/// Running status is remembered only for commands at or above this byte
/// (`cmp r1, 0xBD` in `MPlayMain`).
const RUNNING_STATUS_MIN: u8 = 0xBD;
const FIRST_MEMACC_CONDITION: u8 = 6;
const LAST_MEMACC_CONDITION: u8 = 17;
const MEMACC_CONDITIONS: std::ops::RangeInclusive<u8> =
    FIRST_MEMACC_CONDITION..=LAST_MEMACC_CONDITION;
const XCMD_NO_OP: u8 = 0x00;
const XCMD_WAVE: u8 = 0x01;
const XCMD_RESERVED: u8 = 0x03;
const XCMD_WAIT: u8 = 0x0C;
const XCMD_UNKNOWN_0D: u8 = 0x0D;

/// MP2K's `gClockTable` note lengths (`m4a_tables.c:177`).
#[rustfmt::skip]
const CLOCK_TABLE: [u8; 49] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    21, 22, 23, 24, 28, 30, 32, 36, 40, 42, 44, 48, 52, 54, 56, 60, 64, 66, 68,
    72, 76, 78, 80, 84, 88, 90, 92, 96,
];

const DEFAULT_KEY: u8 = 60;
const DEFAULT_VELOCITY: u8 = u8::MAX / 2;

struct JumpFixup {
    event_index: usize,
    target_byte_offset: u32,
    command_byte_offset: usize,
}

/// Decode one track's byte program into typed [`Event`]s.
///
/// Jump operands are little-endian byte offsets within `bytes`; the decoder
/// resolves them to event indices. It decodes the entire slice because pattern
/// and repeat bodies may follow the main track's first end event.
///
/// # Errors
///
/// Returns [`DecodeError`] on a truncated command, an orphan argument byte, an
/// unknown status byte, or a jump target that misses every event boundary.
pub fn decode_track(bytes: &[u8]) -> Result<Vec<Event>, DecodeError> {
    let mut decoder = Decoder {
        bytes,
        cursor: 0,
        running_status: None,
        last_key: DEFAULT_KEY,
        last_velocity: DEFAULT_VELOCITY,
        events: Vec::new(),
        event_byte_offsets: Vec::new(),
        jump_fixups: Vec::new(),
    };
    decoder.run()?;
    decoder.resolve_fixups()?;
    Ok(decoder.events)
}

struct Decoder<'a> {
    bytes: &'a [u8],
    cursor: usize,
    running_status: Option<u8>,
    last_key: u8,
    last_velocity: u8,
    events: Vec<Event>,
    event_byte_offsets: Vec<usize>,
    jump_fixups: Vec<JumpFixup>,
}

impl Decoder<'_> {
    fn run(&mut self) -> Result<(), DecodeError> {
        while self.cursor < self.bytes.len() {
            let command_byte_offset = self.cursor;
            let command = self.next_command(command_byte_offset)?;
            self.dispatch(command, command_byte_offset)?;
        }
        Ok(())
    }

    fn next_command(&mut self, offset: usize) -> Result<u8, DecodeError> {
        let byte = self.bytes[self.cursor];
        if byte < STATUS_BYTE_MIN {
            return self
                .running_status
                .ok_or(DecodeError::RunningStatusWithoutCommand { offset });
        }

        self.cursor += 1;
        if byte >= RUNNING_STATUS_MIN {
            self.running_status = Some(byte);
        }
        Ok(byte)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "MP2K command decoding remains clearest as one byte-dispatch match"
    )]
    fn dispatch(&mut self, command: u8, command_byte_offset: usize) -> Result<(), DecodeError> {
        match command {
            WAIT_LO..=WAIT_HI => {
                let ticks = CLOCK_TABLE[(command - WAIT_LO) as usize];
                self.push(command_byte_offset, Event::Wait(ticks));
            }
            FINE => {
                self.push(command_byte_offset, Event::Fine);
            }
            GOTO => self.jump(command_byte_offset, Event::Goto)?,
            PATT => self.jump(command_byte_offset, Event::Pattern)?,
            PEND => self.push(command_byte_offset, Event::PatternEnd),
            REPT => {
                let count = self.next_byte()?;
                let target = self.u32_le()?;
                let event_index = self.events.len();
                self.push(command_byte_offset, Event::Repeat { count, target: 0 });
                self.jump_fixups.push(JumpFixup {
                    event_index,
                    target_byte_offset: target,
                    command_byte_offset,
                });
            }
            MEMACC => self.memacc(command_byte_offset)?,
            PRIO => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::Priority(value));
            }
            TEMPO => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::Tempo(u16::from(value) * 2));
            }
            KEYSH => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::KeyShift(as_signed(value)));
            }
            VOICE => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::Voice(value));
            }
            VOL => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::Volume(value));
            }
            PAN => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::Pan(centered(value)));
            }
            BEND => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::Bend(centered(value)));
            }
            BENDR => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::BendRange(value));
            }
            LFOS => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::LfoSpeed(value));
            }
            LFODL => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::LfoDelay(value));
            }
            MOD => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::Modulation(value));
            }
            MODT => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::ModType(value));
            }
            TUNE => {
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::Tune(centered(value)));
            }
            XCMD => {
                let kind = self.next_byte()?;
                let width = xcmd_payload_width(kind);
                let mut value = 0u32;
                for i in 0..width {
                    value |= u32::from(self.next_byte()?) << (8 * i);
                }
                self.push(command_byte_offset, Event::Xcmd { kind, value });
            }
            PORT => {
                let control = self.next_byte()?;
                let value = self.next_byte()?;
                self.push(command_byte_offset, Event::Port { control, value });
            }
            EOT => {
                let key = if self.peek_argument().is_some() {
                    let k = self.next_byte()?;
                    self.last_key = k;
                    Some(k)
                } else {
                    None
                };
                self.push(command_byte_offset, Event::EndOfTie { key });
            }
            TIE..=u8::MAX => self.note(command, command_byte_offset)?,
            _ => {
                return Err(DecodeError::UnknownCommand {
                    offset: command_byte_offset,
                    byte: command,
                });
            }
        }
        Ok(())
    }

    fn memacc(&mut self, command_byte_offset: usize) -> Result<(), DecodeError> {
        let op = self.next_byte()?;
        let addr = self.next_byte()?;
        let value = self.next_byte()?;
        let target_byte_offset = if MEMACC_CONDITIONS.contains(&op) {
            Some(self.u32_le()?)
        } else {
            None
        };
        let event_index = self.events.len();
        let unresolved_target = target_byte_offset.map(|_| 0);
        self.push(
            command_byte_offset,
            Event::MemAcc {
                op,
                addr,
                value,
                target: unresolved_target,
            },
        );
        if let Some(target_byte_offset) = target_byte_offset {
            self.jump_fixups.push(JumpFixup {
                event_index,
                target_byte_offset,
                command_byte_offset,
            });
        }
        Ok(())
    }

    fn note(&mut self, command: u8, command_byte_offset: usize) -> Result<(), DecodeError> {
        let mut gate = CLOCK_TABLE[(command - TIE) as usize];

        let key = if self.peek_argument().is_some() {
            let k = self.next_byte()?;
            self.last_key = k;
            k
        } else {
            self.last_key
        };

        let velocity = if self.peek_argument().is_some() {
            let v = self.next_byte()?;
            self.last_velocity = v;
            v
        } else {
            self.last_velocity
        };

        if let Some(ext) = self.peek_argument() {
            self.cursor += 1;
            gate = gate.saturating_add(ext);
        }

        self.push(
            command_byte_offset,
            Event::Note {
                key,
                velocity,
                gate,
            },
        );
        Ok(())
    }

    fn jump(
        &mut self,
        command_byte_offset: usize,
        make_event: fn(usize) -> Event,
    ) -> Result<(), DecodeError> {
        let target_byte_offset = self.u32_le()?;
        let event_index = self.events.len();
        self.push(command_byte_offset, make_event(0));
        self.jump_fixups.push(JumpFixup {
            event_index,
            target_byte_offset,
            command_byte_offset,
        });
        Ok(())
    }

    fn push(&mut self, offset: usize, event: Event) {
        self.event_byte_offsets.push(offset);
        self.events.push(event);
    }

    fn peek_argument(&self) -> Option<u8> {
        self.bytes
            .get(self.cursor)
            .copied()
            .filter(|&byte| byte < STATUS_BYTE_MIN)
    }

    fn next_byte(&mut self) -> Result<u8, DecodeError> {
        let byte = *self
            .bytes
            .get(self.cursor)
            .ok_or(DecodeError::UnexpectedEnd)?;
        self.cursor += 1;
        Ok(byte)
    }

    fn u32_le(&mut self) -> Result<u32, DecodeError> {
        let mut value = 0u32;
        for i in 0..4 {
            value |= u32::from(self.next_byte()?) << (8 * i);
        }
        Ok(value)
    }

    fn resolve_fixups(&mut self) -> Result<(), DecodeError> {
        for fixup in &self.jump_fixups {
            let index = self
                .event_byte_offsets
                .iter()
                .position(|&offset| offset == fixup.target_byte_offset as usize)
                .ok_or(DecodeError::UnresolvedJump {
                    offset: fixup.command_byte_offset,
                    target: fixup.target_byte_offset,
                })?;
            match &mut self.events[fixup.event_index] {
                Event::Goto(t)
                | Event::Pattern(t)
                | Event::Repeat { target: t, .. }
                | Event::MemAcc {
                    target: Some(t), ..
                } => {
                    *t = index;
                }
                _ => unreachable!("jump fixups only reference jump events"),
            }
        }
        Ok(())
    }
}

/// Returns the payload width read by each `gXcmdTable` handler (`m4a.c:1523`,
/// `:1531`..`:1652`). Unknown kinds retain one argument byte.
fn xcmd_payload_width(kind: u8) -> usize {
    match kind {
        XCMD_NO_OP | XCMD_RESERVED => 0,
        XCMD_WAVE | XCMD_UNKNOWN_0D => size_of::<u32>(),
        XCMD_WAIT => size_of::<u16>(),
        _ => size_of::<u8>(),
    }
}

fn centered(value: u8) -> i8 {
    as_signed(value.wrapping_sub(CENTER))
}

fn as_signed(value: u8) -> i8 {
    i8::from_ne_bytes([value])
}

#[cfg(test)]
mod tests {
    use super::*;

    // Canonical MP2K wire bytes, spelled as literals rather than derived from
    // the decoder's own TIE/WAIT_LO/XCMD constants: a drift in a production
    // boundary must fail these tests, not move the fixtures with it.
    const NOTE_1: u8 = 0xD0;
    const NOTE_4: u8 = 0xD3;
    const NOTE_24: u8 = 0xE7;
    const NOTE_96: u8 = 0xFF;
    const WAIT_0: u8 = 0x80;
    const WAIT_24: u8 = 0x98;
    const MEMACC_ADD: u8 = 1;
    const MEMACC_EQUAL_IMMEDIATE: u8 = 6;
    const XCMD_WAVE_KIND: u8 = 0x01;
    const XCMD_WAIT_KIND: u8 = 0x0C;
    const XCMD_RELEASE_KIND: u8 = 0x07;

    #[test]
    fn decodes_voice_note_wait_fine() {
        let bytes = [VOICE, 0, NOTE_24, 60, 127, WAIT_24, FINE];
        let events = decode_track(&bytes).unwrap();
        assert_eq!(
            events,
            vec![
                Event::Voice(0),
                Event::Note {
                    key: 60,
                    velocity: 127,
                    gate: 24
                },
                Event::Wait(24),
                Event::Fine,
            ]
        );
    }

    #[test]
    fn note_length_comes_from_clock_table() {
        let short = decode_track(&[NOTE_1, 60, 100]).unwrap();
        assert_eq!(
            short[0],
            Event::Note {
                key: 60,
                velocity: 100,
                gate: 1
            }
        );
        let long = decode_track(&[NOTE_96, 60, 100]).unwrap();
        assert_eq!(
            long[0],
            Event::Note {
                key: 60,
                velocity: 100,
                gate: 96
            }
        );
    }

    #[test]
    fn tie_decodes_to_gate_zero() {
        let events = decode_track(&[TIE, 55, 90]).unwrap();
        assert_eq!(
            events[0],
            Event::Note {
                key: 55,
                velocity: 90,
                gate: 0
            }
        );
    }

    #[test]
    fn running_status_repeats_note_duration_and_reuses_velocity() {
        let bytes = [NOTE_24, 60, 127, WAIT_0, 62, WAIT_0, 64];
        let events = decode_track(&bytes).unwrap();
        let notes: Vec<&Event> = events
            .iter()
            .filter(|e| matches!(e, Event::Note { .. }))
            .collect();
        assert_eq!(
            notes,
            vec![
                &Event::Note {
                    key: 60,
                    velocity: 127,
                    gate: 24
                },
                &Event::Note {
                    key: 62,
                    velocity: 127,
                    gate: 24
                },
                &Event::Note {
                    key: 64,
                    velocity: 127,
                    gate: 24
                },
            ]
        );
    }

    #[test]
    fn gate_extension_operand_adds_to_note_length() {
        let events = decode_track(&[NOTE_24, 60, 100, 5]).unwrap();
        assert_eq!(
            events[0],
            Event::Note {
                key: 60,
                velocity: 100,
                gate: 29
            }
        );
    }

    #[test]
    fn centered_commands_are_signed() {
        let events = decode_track(&[PAN, 0, TUNE, 127, BEND, CENTER]).unwrap();
        assert_eq!(events[0], Event::Pan(-64));
        assert_eq!(events[1], Event::Tune(63));
        assert_eq!(events[2], Event::Bend(0));
    }

    #[test]
    fn tempo_doubles_the_stored_value() {
        let events = decode_track(&[TEMPO, 75]).unwrap();
        assert_eq!(events[0], Event::Tempo(150));
    }

    #[test]
    fn goto_resolves_to_an_event_index() {
        let note_byte_offset = 2_u32;
        let mut bytes = vec![VOICE, 0, NOTE_24, 60, 127, GOTO];
        bytes.extend_from_slice(&note_byte_offset.to_le_bytes());
        let events = decode_track(&bytes).unwrap();
        assert_eq!(events[2], Event::Goto(1));
    }

    #[test]
    fn patt_body_after_main_fine_decodes_and_resolves() {
        let pattern_body_byte_offset = 8_u32;
        let mut bytes = vec![VOICE, 0, PATT];
        bytes.extend_from_slice(&pattern_body_byte_offset.to_le_bytes());
        bytes.extend_from_slice(&[FINE, NOTE_4, 60, 127, PEND]);
        let events = decode_track(&bytes).unwrap();
        assert_eq!(events[1], Event::Pattern(3));
        assert_eq!(events[2], Event::Fine);
        assert_eq!(
            events[3],
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 4
            }
        );
        assert_eq!(events[4], Event::PatternEnd);
    }

    #[test]
    fn dangling_jump_past_end_still_errors() {
        let target_past_end = 99_u32;
        let mut bytes = vec![VOICE, 0, PATT];
        bytes.extend_from_slice(&target_past_end.to_le_bytes());
        bytes.push(FINE);
        assert!(matches!(
            decode_track(&bytes),
            Err(DecodeError::UnresolvedJump { .. })
        ));
    }

    #[test]
    fn unaligned_goto_target_errors() {
        let middle_of_voice_command = 1_u32;
        let mut bytes = vec![VOICE, 0, GOTO];
        bytes.extend_from_slice(&middle_of_voice_command.to_le_bytes());
        assert!(matches!(
            decode_track(&bytes),
            Err(DecodeError::UnresolvedJump { .. })
        ));
    }

    #[test]
    fn orphan_argument_byte_errors() {
        assert!(matches!(
            decode_track(&[60]),
            Err(DecodeError::RunningStatusWithoutCommand { offset: 0 })
        ));
    }

    #[test]
    fn truncated_command_errors() {
        assert!(matches!(
            decode_track(&[VOICE]),
            Err(DecodeError::UnexpectedEnd)
        ));
    }

    #[test]
    fn end_of_track_without_fine_stops_cleanly() {
        let events = decode_track(&[VOICE, 1]).unwrap();
        assert_eq!(events, vec![Event::Voice(1)]);
    }

    #[test]
    fn port_command_is_decoded_with_two_operands() {
        let bytes = [PORT, 2, 127, FINE];
        let events = decode_track(&bytes).unwrap();
        assert_eq!(
            events,
            vec![
                Event::Port {
                    control: 2,
                    value: 127,
                },
                Event::Fine,
            ]
        );
    }

    #[test]
    fn memacc_conditional_op_consumes_and_resolves_its_jump_target() {
        let voice_event_byte_offset = 0_u32;
        let mut bytes = vec![VOICE, 0, MEMACC, MEMACC_EQUAL_IMMEDIATE, 1, 5];
        bytes.extend_from_slice(&voice_event_byte_offset.to_le_bytes());
        bytes.push(FINE);
        let events = decode_track(&bytes).unwrap();
        assert_eq!(
            events,
            vec![
                Event::Voice(0),
                Event::MemAcc {
                    op: 6,
                    addr: 1,
                    value: 5,
                    target: Some(0),
                },
                Event::Fine,
            ]
        );
    }

    #[test]
    fn unconditional_memacc_has_no_jump_target() {
        let events = decode_track(&[MEMACC, MEMACC_ADD, 2, 3, FINE]).unwrap();
        assert_eq!(
            events,
            vec![
                Event::MemAcc {
                    op: 1,
                    addr: 2,
                    value: 3,
                    target: None,
                },
                Event::Fine,
            ]
        );
    }

    #[test]
    fn xcmd_payload_widths_track_the_sub_command() {
        let wave_pointer = 0x0800_0C0D_u32;
        let wait_length = 60_u16;
        let release = 5_u8;
        let mut bytes = vec![XCMD, XCMD_WAVE_KIND];
        bytes.extend_from_slice(&wave_pointer.to_le_bytes());
        bytes.extend_from_slice(&[XCMD, XCMD_WAIT_KIND]);
        bytes.extend_from_slice(&wait_length.to_le_bytes());
        bytes.extend_from_slice(&[XCMD, XCMD_RELEASE_KIND, release, FINE]);
        let events = decode_track(&bytes).unwrap();
        assert_eq!(
            events,
            vec![
                Event::Xcmd {
                    kind: XCMD_WAVE_KIND,
                    value: wave_pointer,
                },
                Event::Xcmd {
                    kind: XCMD_WAIT_KIND,
                    value: u32::from(wait_length),
                },
                Event::Xcmd {
                    kind: XCMD_RELEASE_KIND,
                    value: u32::from(release),
                },
                Event::Fine,
            ]
        );
    }
}
