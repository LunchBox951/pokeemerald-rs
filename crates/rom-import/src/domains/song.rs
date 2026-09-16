//! Songs: `audio/song/*`, decoded from the m4a engine's track byte-code.
//!
//! A song is a `SongHeader` (`pokeemerald/include/gba/m4a_internal.h`):
//! `trackCount`, `blockCount`, `priority`, `reverb`, the voicegroup
//! pointer, then one pointer per track. Each track is a byte stream the
//! engine's `MPlayMain` walks (`pokeemerald/src/m4a_1.s`), and this module
//! is that walk turned into [`assets::SongEvent`]s, reimplemented from the
//! engine's dispatch rather than from `tools/mid2agb`'s printer
//! `(no-verbatim)`:
//!
//! - A byte under `0x80` is an operand, or, at command position, the
//!   track's *running status*: the last command at or above `0xBD` repeats
//!   with this byte as its first operand. A command at or above `0xBD`
//!   becomes the running status as it executes.
//! - `0x80..=0xB0` is a rest, `gClockTable[cmd - 0x80]` ticks.
//! - `0xB1..=0xCE` are the named commands, dispatched by `cmd - 0xB1`
//!   through `gMPlayJumpTable`; the table's unused entries are errors here.
//! - `0xCF` and up is a note: `gClockTable[cmd - 0xCF]` ticks of gate
//!   (`0` for `TIE`), then an optional key, an optional velocity, and an
//!   optional gate extension, each present only if the byte before it was
//!   and each under `0x80`. An elided key or velocity repeats the track's
//!   last one, so the decoder carries both. `EOT` elides its key the same
//!   way, and the event always names the key the engine would end.
//!
//! `PATT` calls a block ending in `PEND` and is expanded inline; the pack
//! has no pattern primitive (`assets::audio`'s "Deferred commands").
//! `GOTO` and branching `MEMACC` targets become event indices specialized
//! for the incoming running status, note operands, and pattern return stack.
//! Both conditional paths remain available to the runtime. `TEMPO` stores
//! BPM halved; `PAN`, `BEND`, and `TUNE` store `64 + value`. Decoding ends
//! at `FINE` or reconnects to an already decoded execution context.
//!
//! Rests come out of the ROM in `tools/mid2agb`'s chunking and go into the
//! pack in [`assets::Song::new`]'s canonical shape; nothing here has to
//! know the difference.

use assets::audio::{MemAccCondition, MemAccOp};
use assets::{Song, SongEvent, VoiceGroupId};
use pack_format::{raw_entry, PackEntry, PackWriter};

use super::check_pointer;
use crate::error::{ImportError, SongFault};
use crate::reader::RomReader;
use crate::rom::Rom;
use crate::roots::{AudioRoots, Roots, SongRoot};

mod decode;
use decode::{decode_track, Context};

/// `gClockTable` (`pokeemerald/src/m4a_tables.c`): the tick count each
/// rest and note opcode stands for.
const CLOCK_TABLE: [u8; 49] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 28,
    30, 32, 36, 40, 42, 44, 48, 52, 54, 56, 60, 64, 66, 68, 72, 76, 78, 80, 84, 88, 90, 92, 96,
];
/// The first rest opcode.
const CMD_WAIT: u8 = 0x80;
/// The first named command.
const CMD_FINE: u8 = 0xB1;
const CMD_GOTO: u8 = 0xB2;
const CMD_PATT: u8 = 0xB3;
const CMD_PEND: u8 = 0xB4;
const CMD_REPT: u8 = 0xB5;
const CMD_MEMACC: u8 = 0xB9;
const CMD_PRIO: u8 = 0xBA;
const CMD_TEMPO: u8 = 0xBB;
const CMD_KEYSH: u8 = 0xBC;
/// The first command that sets the running status.
const CMD_VOICE: u8 = 0xBD;
const CMD_VOL: u8 = 0xBE;
const CMD_PAN: u8 = 0xBF;
const CMD_BEND: u8 = 0xC0;
const CMD_BENDR: u8 = 0xC1;
const CMD_LFOS: u8 = 0xC2;
const CMD_LFODL: u8 = 0xC3;
const CMD_MOD: u8 = 0xC4;
const CMD_MODT: u8 = 0xC5;
const CMD_TUNE: u8 = 0xC8;
const CMD_XCMD: u8 = 0xCD;
const CMD_EOT: u8 = 0xCE;
/// `TIE`: a note with no gate. Every higher opcode is a note too.
const CMD_TIE: u8 = 0xCF;
/// `XCMD` sub-commands.
const XCMD_IECV: u8 = 0x08;
const XCMD_IECL: u8 = 0x09;
/// `C_V`, the centre `PAN`/`BEND`/`TUNE` store.
const CENTER: u8 = 0x40;
/// The engine's `patternStack` depth.
const MAX_PATTERN_DEPTH: usize = 3;
/// `SongHeader` field offsets.
const HEADER_TRACK_COUNT: usize = 0;
const HEADER_PRIORITY: usize = 2;
const HEADER_REVERB: usize = 3;
const HEADER_TONE: usize = 4;
const HEADER_PARTS: usize = 8;
/// `SongHeader.reverb`'s "set" bit.
const REVERB_SET: u8 = 0x80;
/// One `gSongTable` entry: header pointer, `ms`, `me`.
const SONG_TABLE_STRIDE: usize = 8;

/// Write every song.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if `gSongTable` or the `SongHeader`
/// disagrees with the profile; [`ImportError::UnresolvedPointer`] if the
/// header's voicegroup is not one the profile records;
/// [`ImportError::Song`] for a track the decoder cannot follow;
/// [`ImportError::Audio`] if the schema rejects the result;
/// [`ImportError::Truncated`] if any read runs past the end of the ROM.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let audio = &roots.audio;
    for root in audio.songs {
        writer.push(song(&reader, audio, root)?);
    }
    Ok(())
}

/// Read one song: corroborate its table entry and header, decode its
/// tracks, and encode the result.
pub(crate) fn song(
    reader: &RomReader<'_>,
    audio: &AudioRoots,
    root: &SongRoot,
) -> Result<PackEntry, ImportError> {
    let id = root.id;
    // `gSongTable[index].header` names the header the profile recorded.
    check_pointer(
        reader,
        audio.song_table.offset(),
        usize::from(root.index) * SONG_TABLE_STRIDE,
        root.header,
        id,
        "Song.header",
    )?;

    let base = root.header.offset();
    let track_count = reader.u8(base + HEADER_TRACK_COUNT)?;
    if track_count != root.track_count {
        return Err(ImportError::StructMismatch {
            root: id,
            field: "SongHeader.trackCount",
        });
    }
    let priority = reader.u8(base + HEADER_PRIORITY)?;
    let reverb_byte = reader.u8(base + HEADER_REVERB)?;
    let reverb = (reverb_byte & REVERB_SET != 0).then_some(reverb_byte & !REVERB_SET);
    check_pointer(
        reader,
        base,
        HEADER_TONE,
        root.voicegroup,
        id,
        "SongHeader.tone",
    )?;
    let voicegroup = audio
        .voicegroups
        .iter()
        .find(|group| group.addr == root.voicegroup)
        .map(|group| VoiceGroupId(group.id.to_owned()))
        .ok_or(ImportError::UnresolvedPointer {
            root: id,
            slot: 0,
            what: "a voicegroup",
            ptr: root.voicegroup,
        })?;

    let mut tracks = Vec::with_capacity(usize::from(track_count));
    for track in 0..usize::from(track_count) {
        let start = reader.ptr(base + HEADER_PARTS + track * 4)?;
        tracks.push(decode_track(reader, id, track, start)?);
    }

    let song = Song::new(voicegroup, priority, reverb, tracks)
        .map_err(|source| ImportError::Audio { id, source })?;
    Ok(raw_entry(id.to_owned(), song.encode()))
}

/// The decoder's per-track state: what `MusicPlayerTrack` carries between
/// commands, plus the bookkeeping that turns pointers into indices.
struct Track<'r, 'a> {
    reader: &'r RomReader<'a>,
    id: &'static str,
    index: usize,
    events: Vec<SongEvent>,
    state: Context,
}

impl Track<'_, '_> {
    fn fail(&self, at: usize, fault: SongFault) -> ImportError {
        ImportError::Song {
            id: self.id,
            track: self.index,
            at,
            fault,
        }
    }

    /// One operand byte.
    fn operand(&self, at: usize) -> Result<u8, ImportError> {
        self.reader.u8(at)
    }

    /// An operand that is only present if it is under `0x80`.
    fn optional(&self, at: usize) -> Result<Option<u8>, ImportError> {
        let byte = self.reader.u8(at)?;
        Ok((byte < CMD_WAIT).then_some(byte))
    }

    fn destination(&self, at: usize) -> Result<usize, ImportError> {
        let target = self.reader.ptr(at)?.offset();
        if target >= self.reader.len() {
            return Err(self.fail(at, SongFault::JumpOutsideRom));
        }
        Ok(target)
    }

    fn push(&mut self, event: SongEvent) {
        self.events.push(event);
    }
}

/// Where the decoder goes after one command.
enum Step {
    Next(usize),
    Branch { next: usize, target: usize },
    Fine,
}

/// Execute one command whose operands start at `at`.
fn step(track: &mut Track<'_, '_>, cmd: u8, at: usize) -> Result<Step, ImportError> {
    let next = match cmd {
        CMD_TIE..=u8::MAX => note(track, cmd, at)?,
        CMD_WAIT..CMD_FINE => {
            track.push(SongEvent::Wait(CLOCK_TABLE[usize::from(cmd - CMD_WAIT)]));
            at
        }
        CMD_FINE => {
            track.push(SongEvent::Fine);
            return Ok(Step::Fine);
        }
        CMD_GOTO => track.destination(at)?,
        CMD_PATT => {
            if track.state.patterns.len() >= MAX_PATTERN_DEPTH {
                return Err(track.fail(at, SongFault::PatternTooDeep));
            }
            track.state.patterns.push(at + 4);
            track.destination(at)?
        }
        CMD_PEND => track.state.patterns.pop().unwrap_or(at),
        CMD_REPT => return Err(track.fail(at, SongFault::Repeat)),
        CMD_MEMACC => return memacc(track, at),
        CMD_PRIO => unary(track, at, SongEvent::Priority)?,
        CMD_TEMPO => unary(track, at, |half| SongEvent::Tempo(u16::from(half) * 2))?,
        CMD_KEYSH => unary(track, at, |b| SongEvent::KeyShift(i8::from_le_bytes([b])))?,
        CMD_VOICE => unary(track, at, SongEvent::Voice)?,
        CMD_VOL => unary(track, at, SongEvent::Volume)?,
        CMD_PAN => unary(track, at, |b| SongEvent::Pan(centred(b)))?,
        CMD_BEND => unary(track, at, |b| SongEvent::Bend(centred(b)))?,
        CMD_BENDR => unary(track, at, SongEvent::BendRange)?,
        CMD_LFOS => unary(track, at, SongEvent::LfoSpeed)?,
        CMD_LFODL => unary(track, at, SongEvent::LfoDelay)?,
        CMD_MOD => unary(track, at, SongEvent::Modulation)?,
        CMD_MODT => unary(track, at, SongEvent::ModType)?,
        CMD_TUNE => unary(track, at, |b| SongEvent::Tune(centred(b)))?,
        CMD_XCMD => {
            let sub = track.operand(at)?;
            let arg = track.operand(at + 1)?;
            track.push(match sub {
                XCMD_IECV => SongEvent::PseudoEchoVolume(arg),
                XCMD_IECL => SongEvent::PseudoEchoLength(arg),
                other => return Err(track.fail(at, SongFault::UnknownExtendedCommand(other))),
            });
            at + 2
        }
        CMD_EOT => {
            // `ply_endtie` ends the track's last key when the operand is
            // elided, so the event names that key outright: the checkout
            // compiler never elides, and the two have to agree.
            let key = track.optional(at)?;
            if let Some(key) = key {
                track.state.key = key;
            }
            track.push(SongEvent::EndOfTie {
                key: Some(track.state.key),
            });
            at + usize::from(key.is_some())
        }
        other => return Err(track.fail(at, SongFault::UnknownCommand(other))),
    };
    Ok(Step::Next(next))
}

/// A one-operand command.
fn unary(
    track: &mut Track<'_, '_>,
    at: usize,
    event: impl FnOnce(u8) -> SongEvent,
) -> Result<usize, ImportError> {
    let operand = track.operand(at)?;
    track.push(event(operand));
    Ok(at + 1)
}

/// A `C_V`-centred operand.
fn centred(byte: u8) -> i8 {
    i8::from_le_bytes([byte.wrapping_sub(CENTER)])
}

/// A note: gate from the opcode, then the optional key, velocity, and gate
/// extension, each only if the one before it was present.
fn note(track: &mut Track<'_, '_>, cmd: u8, at: usize) -> Result<usize, ImportError> {
    let mut gate = CLOCK_TABLE[usize::from(cmd - CMD_TIE)];
    let mut at = at;
    if let Some(key) = track.optional(at)? {
        track.state.key = key;
        at += 1;
        if let Some(velocity) = track.optional(at)? {
            track.state.velocity = velocity;
            at += 1;
            if let Some(extension) = track.optional(at)? {
                gate = gate.wrapping_add(extension);
                at += 1;
            }
        }
    }
    track.push(SongEvent::Note {
        key: track.state.key,
        velocity: track.state.velocity,
        gate,
    });
    Ok(at)
}

/// `MEMACC op addr data`, plus a pointer for the branching ops.
fn memacc(track: &mut Track<'_, '_>, at: usize) -> Result<Step, ImportError> {
    let op = track.operand(at)?;
    let address = track.operand(at + 1)?;
    let data = track.operand(at + 2)?;
    let after = at + 3;
    let operation = match op {
        0 => MemAccOp::Set,
        1 => MemAccOp::Add,
        2 => MemAccOp::Sub,
        3 => MemAccOp::MemSet,
        4 => MemAccOp::MemAdd,
        5 => MemAccOp::MemSub,
        6..=17 => {
            let condition = match op {
                6 => MemAccCondition::Eq,
                7 => MemAccCondition::Ne,
                8 => MemAccCondition::Hi,
                9 => MemAccCondition::Hs,
                10 => MemAccCondition::Ls,
                11 => MemAccCondition::Lo,
                12 => MemAccCondition::MemEq,
                13 => MemAccCondition::MemNe,
                14 => MemAccCondition::MemHi,
                15 => MemAccCondition::MemHs,
                16 => MemAccCondition::MemLs,
                _ => MemAccCondition::MemLo,
            };
            track.push(SongEvent::MemAccBranch {
                condition,
                address,
                data,
                target: 0,
            });
            return Ok(Step::Branch {
                next: after + 4,
                target: track.destination(after)?,
            });
        }
        other => return Err(track.fail(at, SongFault::UnknownMemAccOp(other))),
    };
    track.push(SongEvent::MemAcc {
        op: operation,
        address,
        data,
    });
    Ok(Step::Next(after))
}

#[cfg(test)]
mod tests;
