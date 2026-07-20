//! The typed sequence model and the decoder that turns MP2K's byte-coded
//! track stream into it.
//!
//! Upstream stores each track as a compact byte program interpreted live by
//! `MPlayMain` (`m4a_1.s:1129`): status bytes `>= 0x80`, argument bytes
//! `< 0x80`, and a running-status shorthand. This slice decodes that program
//! **once**, ahead of playback, into a `Vec<Event>` so the runtime never walks
//! raw bytes `(no-verbatim)`.
//!
//! Command byte values are the authoritative `#define`s from
//! `src/m4a_tables.c:230`.. and the note/wait ranges from `MPlayMain`'s
//! dispatch. Running-status note repeats are expanded into explicit
//! [`Event::Note`]s; per-track key/velocity carry across notes exactly as the
//! hardware's `track->key`/`track->velocity` do.

use std::error::Error;
use std::fmt;

/// A decoded track command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Play a note. `gate` is the note-off delay in ticks (`gClockTable`
    /// value); `0` marks a tied note (`TIE`) that sounds until an
    /// [`Event::EndOfTie`] or explicit stop.
    Note { key: u8, velocity: u8, gate: u8 },
    /// End a tie (`EOT`); `key` is `None` when the command omitted its key
    /// operand, in which case the sequencer matches on the track's current key
    /// (`track->key`, the last note's key).
    EndOfTie { key: Option<u8> },
    /// Delay this track for `ticks` sequencer ticks (`W..` commands).
    Wait(u8),
    /// Select instrument `index` from the song's voicegroup (`VOICE`).
    Voice(u8),
    /// Set track volume `0..=127` (`VOL`).
    Volume(u8),
    /// Set track pan, centre-relative `-64..=63` (`PAN`).
    Pan(i8),
    /// Set tempo in BPM (`TEMPO`, stored on disk as BPM/2).
    Tempo(u16),
    /// Transpose the track by whole semitones (`KEYSH`).
    KeyShift(i8),
    /// Pitch bend, centre-relative (`BEND`).
    Bend(i8),
    /// Pitch-bend range in semitones (`BENDR`).
    BendRange(u8),
    /// Fine tune, centre-relative (`TUNE`).
    Tune(i8),
    /// Jump to another decoded event index (`GOTO`) — the loop primitive.
    Goto(usize),
    /// End of track (`FINE`).
    Fine,

    // --- Decoded for stream fidelity, but not acted on by this slice's
    // engine. A later slice implements these; decoding them keeps the byte
    // stream in sync so the in-scope events around them stay correct. ---
    /// Track priority (`PRIO`).
    Priority(u8),
    /// LFO speed (`LFOS`).
    LfoSpeed(u8),
    /// LFO delay (`LFODL`).
    LfoDelay(u8),
    /// Modulation depth (`MOD`).
    Modulation(u8),
    /// Modulation type (`MODT`).
    ModType(u8),
    /// Begin a pattern (`PATT`), resolved to an event index.
    Pattern(usize),
    /// Return from a pattern (`PEND`).
    PatternEnd,
    /// Repeat (`REPT`) `count` times from an event index.
    Repeat { count: u8, target: usize },
    /// Memory-accumulator op (`MEMACC`).
    MemAcc { op: u8, addr: u8, value: u8 },
    /// Extended command (`XCMD`).
    Xcmd { kind: u8, value: u8 },
}

/// Something wrong with a track's byte program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The stream ended mid-command (a status byte with missing operands).
    UnexpectedEnd,
    /// An argument byte appeared with no preceding running-status command.
    RunningStatusWithoutCommand { offset: usize },
    /// A status byte with no defined handler in this slice.
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

// --- Command byte constants (`m4a_tables.c:230`..) ---
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
const XCMD: u8 = 0xCD;
const EOT: u8 = 0xCE;
const TIE: u8 = 0xCF;
/// Centre value for pan/bend/tune (`C_V`, `m4a_internal.h:10`).
const CENTER: u8 = 0x40;
/// Running status is remembered only for commands at or above this byte
/// (`cmp r1, 0xBD` in `MPlayMain`).
const RUNNING_STATUS_MIN: u8 = 0xBD;

/// Note-length lookup, `gClockTable` (`m4a_tables.c:177`), indexed by
/// `note_byte - TIE`.
#[rustfmt::skip]
const CLOCK_TABLE: [u8; 49] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
    21, 22, 23, 24, 28, 30, 32, 36, 40, 42, 44, 48, 52, 54, 56, 60, 64, 66, 68,
    72, 76, 78, 80, 84, 88, 90, 92, 96,
];

/// Default MIDI key when a note omits its key operand before any key has been
/// seen (middle-of-the-road; real data always sets a key on the first note).
const DEFAULT_KEY: u8 = 60;
/// Default velocity, likewise.
const DEFAULT_VELOCITY: u8 = 127;

/// A `GOTO`/`PATT`/`REPT` whose target byte offset must be resolved to an
/// event index after the whole track is decoded.
struct Fixup {
    event: usize,
    target: u32,
    source_offset: usize,
}

/// Decode one track's byte program into typed [`Event`]s.
///
/// The `bytes` are a self-contained track: `GOTO`/`PATT`/`REPT` operands are
/// 4-byte little-endian **byte offsets into `bytes`** (this crate's convention
/// for self-contained sequences), resolved here to event indices. Decoding
/// stops at `FINE` or the end of the slice.
///
/// # Errors
///
/// Returns [`DecodeError`] on a truncated command, an orphan argument byte, an
/// unknown status byte, or a jump target that misses every event boundary.
pub fn decode_track(bytes: &[u8]) -> Result<Vec<Event>, DecodeError> {
    let mut d = Decoder {
        bytes,
        cur: 0,
        running: None,
        last_key: DEFAULT_KEY,
        last_velocity: DEFAULT_VELOCITY,
        events: Vec::new(),
        offsets: Vec::new(),
        fixups: Vec::new(),
    };
    d.run()?;
    d.resolve_fixups()?;
    Ok(d.events)
}

struct Decoder<'a> {
    bytes: &'a [u8],
    cur: usize,
    running: Option<u8>,
    last_key: u8,
    last_velocity: u8,
    events: Vec<Event>,
    /// Source byte offset each event began at (parallel to `events`).
    offsets: Vec<usize>,
    fixups: Vec<Fixup>,
}

impl Decoder<'_> {
    fn run(&mut self) -> Result<(), DecodeError> {
        while self.cur < self.bytes.len() {
            let start = self.cur;
            let byte = self.bytes[self.cur];

            let cmd = if byte < 0x80 {
                self.running
                    .ok_or(DecodeError::RunningStatusWithoutCommand { offset: start })?
            } else {
                self.cur += 1;
                if byte >= RUNNING_STATUS_MIN {
                    self.running = Some(byte);
                }
                byte
            };

            if self.dispatch(cmd, start)? {
                break; // FINE
            }
        }
        Ok(())
    }

    /// Handle one command, returning `true` if it terminates the track.
    // A flat command-dispatch table: long by nature, but each arm is trivial.
    #[allow(clippy::too_many_lines)]
    fn dispatch(&mut self, cmd: u8, start: usize) -> Result<bool, DecodeError> {
        match cmd {
            WAIT_LO..=WAIT_HI => {
                let ticks = CLOCK_TABLE[(cmd - WAIT_LO) as usize];
                self.push(start, Event::Wait(ticks));
            }
            FINE => {
                self.push(start, Event::Fine);
                return Ok(true);
            }
            GOTO => self.jump(start, Event::Goto)?,
            PATT => self.jump(start, Event::Pattern)?,
            PEND => self.push(start, Event::PatternEnd),
            REPT => {
                let count = self.byte()?;
                let target = self.u32_le()?;
                let event = self.events.len();
                self.push(start, Event::Repeat { count, target: 0 });
                self.fixups.push(Fixup {
                    event,
                    target,
                    source_offset: start,
                });
            }
            MEMACC => {
                let op = self.byte()?;
                let addr = self.byte()?;
                let value = self.byte()?;
                self.push(start, Event::MemAcc { op, addr, value });
            }
            PRIO => {
                let v = self.byte()?;
                self.push(start, Event::Priority(v));
            }
            TEMPO => {
                let v = self.byte()?;
                self.push(start, Event::Tempo(u16::from(v) * 2));
            }
            KEYSH => {
                let v = self.byte()?;
                self.push(start, Event::KeyShift(as_signed(v)));
            }
            VOICE => {
                let v = self.byte()?;
                self.push(start, Event::Voice(v));
            }
            VOL => {
                let v = self.byte()?;
                self.push(start, Event::Volume(v));
            }
            PAN => {
                let v = self.byte()?;
                self.push(start, Event::Pan(centered(v)));
            }
            BEND => {
                let v = self.byte()?;
                self.push(start, Event::Bend(centered(v)));
            }
            BENDR => {
                let v = self.byte()?;
                self.push(start, Event::BendRange(v));
            }
            LFOS => {
                let v = self.byte()?;
                self.push(start, Event::LfoSpeed(v));
            }
            LFODL => {
                let v = self.byte()?;
                self.push(start, Event::LfoDelay(v));
            }
            MOD => {
                let v = self.byte()?;
                self.push(start, Event::Modulation(v));
            }
            MODT => {
                let v = self.byte()?;
                self.push(start, Event::ModType(v));
            }
            TUNE => {
                let v = self.byte()?;
                self.push(start, Event::Tune(centered(v)));
            }
            XCMD => {
                let kind = self.byte()?;
                let value = self.byte()?;
                self.push(start, Event::Xcmd { kind, value });
            }
            EOT => {
                let key = if self.peek_arg().is_some() {
                    let k = self.byte()?;
                    self.last_key = k;
                    Some(k)
                } else {
                    None
                };
                self.push(start, Event::EndOfTie { key });
            }
            TIE..=0xFF => self.note(cmd, start)?,
            _ => {
                return Err(DecodeError::UnknownCommand {
                    offset: start,
                    byte: cmd,
                });
            }
        }
        Ok(false)
    }

    /// Decode a note (or tie) with its optional key/velocity/gate operands,
    /// reusing the track's carried key/velocity when omitted.
    fn note(&mut self, cmd: u8, start: usize) -> Result<(), DecodeError> {
        let mut gate = CLOCK_TABLE[(cmd - TIE) as usize];

        let key = if self.peek_arg().is_some() {
            let k = self.byte()?;
            self.last_key = k;
            k
        } else {
            self.last_key
        };

        let velocity = if self.peek_arg().is_some() {
            let v = self.byte()?;
            self.last_velocity = v;
            v
        } else {
            self.last_velocity
        };

        if let Some(ext) = self.peek_arg() {
            self.cur += 1;
            gate = gate.saturating_add(ext);
        }

        self.push(
            start,
            Event::Note {
                key,
                velocity,
                gate,
            },
        );
        Ok(())
    }

    /// Read a `GOTO`/`PATT` 4-byte target and record a fixup.
    fn jump(&mut self, start: usize, make: fn(usize) -> Event) -> Result<(), DecodeError> {
        let target = self.u32_le()?;
        let event = self.events.len();
        self.push(start, make(0));
        self.fixups.push(Fixup {
            event,
            target,
            source_offset: start,
        });
        Ok(())
    }

    fn push(&mut self, offset: usize, event: Event) {
        self.offsets.push(offset);
        self.events.push(event);
    }

    /// The next byte if it is an argument (`< 0x80`), else `None`.
    fn peek_arg(&self) -> Option<u8> {
        self.bytes.get(self.cur).copied().filter(|&b| b < 0x80)
    }

    fn byte(&mut self) -> Result<u8, DecodeError> {
        let b = *self.bytes.get(self.cur).ok_or(DecodeError::UnexpectedEnd)?;
        self.cur += 1;
        Ok(b)
    }

    fn u32_le(&mut self) -> Result<u32, DecodeError> {
        let mut v = 0u32;
        for i in 0..4 {
            v |= u32::from(self.byte()?) << (8 * i);
        }
        Ok(v)
    }

    /// Rewrite each `GOTO`/`PATT`/`REPT` fixup's target byte offset into the
    /// event index recorded at that offset.
    fn resolve_fixups(&mut self) -> Result<(), DecodeError> {
        for fixup in &self.fixups {
            let index = self
                .offsets
                .iter()
                .position(|&o| o == fixup.target as usize)
                .ok_or(DecodeError::UnresolvedJump {
                    offset: fixup.source_offset,
                    target: fixup.target,
                })?;
            match &mut self.events[fixup.event] {
                Event::Goto(t) | Event::Pattern(t) | Event::Repeat { target: t, .. } => {
                    *t = index;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Interpret a byte as a centre-relative signed value (`value - C_V`,
/// e.g. `subs r3, C_V` in `ply_pan`).
fn centered(value: u8) -> i8 {
    as_signed(value.wrapping_sub(CENTER))
}

/// Reinterpret a byte's bits as `i8` (the C `(s8)` cast), with no lint noise.
fn as_signed(value: u8) -> i8 {
    i8::from_ne_bytes([value])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_voice_note_wait_fine() {
        // VOICE 0; note N24 (0xE7) key 60 vel 127; W24 (0x98); FINE.
        let bytes = [VOICE, 0x00, 0xE7, 60, 127, 0x98, FINE];
        let events = decode_track(&bytes).unwrap();
        // 0xE7 = TIE + 24 -> gClockTable[24] = 24; 0x98 = W00 + 24 -> 24 ticks.
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
        // 0xD0 = N01 -> gClockTable[1] = 1; 0xFF = N96 -> gClockTable[48] = 96.
        let short = decode_track(&[0xD0, 60, 100]).unwrap();
        assert_eq!(
            short[0],
            Event::Note {
                key: 60,
                velocity: 100,
                gate: 1
            }
        );
        let long = decode_track(&[0xFF, 60, 100]).unwrap();
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
        // N24 key 60 vel 127; W00; bare key 62 (running status, same duration,
        // carried velocity); W00; bare key 64. The `W00`s separate notes so a
        // following key is not swallowed as a gate-extension operand.
        let bytes = [0xE7, 60, 127, 0x80, 62, 0x80, 64];
        let events = decode_track(&bytes).unwrap();
        let gate = 24; // 0xE7 = TIE + 24 -> gClockTable[24]
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
                    gate
                },
                &Event::Note {
                    key: 62,
                    velocity: 127,
                    gate
                },
                &Event::Note {
                    key: 64,
                    velocity: 127,
                    gate
                },
            ]
        );
    }

    #[test]
    fn gate_extension_operand_adds_to_note_length() {
        // N24 key 60 vel 100 gate-ext 5 -> gate 24 + 5. All three operands are
        // consumed greedily, exactly as `ply_note` reads them.
        let events = decode_track(&[0xE7, 60, 100, 5]).unwrap();
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
        let events = decode_track(&[PAN, 0x00, TUNE, 0x7F, BEND, CENTER]).unwrap();
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
        // VOICE 0 (offset 0), note (offset 2), GOTO -> offset 2 (loop the note).
        // Layout: [BD 00][E7 3C 7F][B2 02 00 00 00]
        let bytes = [VOICE, 0x00, 0xE7, 60, 127, GOTO, 0x02, 0x00, 0x00, 0x00];
        let events = decode_track(&bytes).unwrap();
        // events: [Voice, Note, Goto]; the note is index 1.
        assert_eq!(events[2], Event::Goto(1));
    }

    #[test]
    fn unaligned_goto_target_errors() {
        // Target offset 1 is in the middle of the VOICE command.
        let bytes = [VOICE, 0x00, GOTO, 0x01, 0x00, 0x00, 0x00];
        assert!(matches!(
            decode_track(&bytes),
            Err(DecodeError::UnresolvedJump { .. })
        ));
    }

    #[test]
    fn orphan_argument_byte_errors() {
        assert!(matches!(
            decode_track(&[0x3C]),
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
        let events = decode_track(&[VOICE, 0x01]).unwrap();
        assert_eq!(events, vec![Event::Voice(1)]);
    }
}
