//! Normalized song metadata and per-track musical event streams.
//!
//! [`SongEvent`] is independent of both MIDI encoding and compiled MP2K bytecode. Same-track jumps
//! use event indices. [`Song::new`] removes zero-length waits, merges adjacent waits without
//! crossing a jump target, and splits delays into `u8` chunks. [`Song::decode`] preserves the
//! encoded wait sequence while validating its structure.
//!
//! [`Song`] retains the priority, reverb, voicegroup, and track order represented by upstream's
//! `struct SongHeader` (`include/gba/m4a_internal.h`). Pattern-block bookkeeping is absent because
//! the normalized streams contain the expanded events. Tempo remains a [`SongEvent::Tempo`] in its
//! track because `SongHeader` has no tempo field.

use super::cursor::{check_id_len, Reader, Writer};
use super::error::AudioError;
use super::voicegroup::VoiceGroupId;

/// One normalized command in a song track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongEvent {
    /// Delays this track by `ticks` sequencer ticks.
    Wait(u8),
    /// Plays `key` at `velocity`; `gate` is the note-off delay in ticks, or zero for a tie.
    Note { key: u8, velocity: u8, gate: u8 },
    /// Ends a tie. `None` uses the track's currently sounding key.
    EndOfTie { key: Option<u8> },
    /// Selects an instrument byte from the song's [`super::VoiceGroup`]. Normalized sources use
    /// `0..=127`; the schema preserves the full `u8` range.
    Voice(u8),
    /// Sets track volume. Normalized sources use `0..=127`; the schema preserves the full `u8`
    /// range.
    Volume(u8),
    /// Sets centre-relative track pan. Normalized sources use `-64..=63`; the schema preserves the
    /// full `i8` range.
    Pan(i8),
    /// Sets centre-relative pitch bend across the full `i8` range.
    Bend(i8),
    /// Sets the pitch-bend range in semitones across the full `u8` range.
    BendRange(u8),
    /// Sets centre-relative fine tuning across the full `i8` range.
    Tune(i8),
    /// Transposes the track by an `i8` number of semitones.
    KeyShift(i8),
    /// Sets tempo in BPM across the full `u16` range.
    Tempo(u16),
    /// Sets track priority across the full `u8` range.
    Priority(u8),
    /// Sets LFO speed across the full `u8` range.
    LfoSpeed(u8),
    /// Sets LFO delay across the full `u8` range.
    LfoDelay(u8),
    /// Sets modulation depth across the full `u8` range.
    Modulation(u8),
    /// Selects modulation type: `0` for vibrato, `1` for tremolo, and `2` for automatic pan. The
    /// schema preserves other byte values.
    ModType(u8),
    /// Sets the note-release echo volume across the full `u8` range. Upstream copies this track
    /// value onto both `DirectSound` and CGB channels (`src/m4a.c:1591-1601`,
    /// `src/m4a_1.s:1757-1758`).
    PseudoEchoVolume(u8),
    /// Sets the [`PseudoEchoVolume`](Self::PseudoEchoVolume) tail length in ticks across the full
    /// `u8` range.
    PseudoEchoLength(u8),
    /// Jumps to an event index in this track.
    Goto(u32),
    /// Applies `op` to memory cell `address`. `data` is a literal for direct operations and a cell
    /// address for [`MemAccOp::MemSet`], [`MemAccOp::MemAdd`], and [`MemAccOp::MemSub`].
    MemAcc { op: MemAccOp, address: u8, data: u8 },
    /// Jumps to event index `target` when memory cell `address` satisfies `condition` against
    /// `data`. Direct conditions treat `data` as a literal; `Mem*` conditions treat it as a cell
    /// address.
    MemAccBranch {
        condition: MemAccCondition,
        address: u8,
        data: u8,
        target: u32,
    },
    /// Ends the track.
    Fine,
}

/// A non-branching [`SongEvent::MemAcc`] operation. Each discriminant is its encoded `mem_*` value
/// from `sound/MPlayDef.s:410-415`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MemAccOp {
    /// `mem_set`: `cell = data`.
    Set = 0,
    /// `mem_add`: `cell += data`.
    Add = 1,
    /// `mem_sub`: `cell -= data`.
    Sub = 2,
    /// `mem_mem_set`: `cell = cells[data]`.
    MemSet = 3,
    /// `mem_mem_add`: `cell += cells[data]`.
    MemAdd = 4,
    /// `mem_mem_sub`: `cell -= cells[data]`.
    MemSub = 5,
}

impl MemAccOp {
    fn from_byte(byte: u8) -> Result<Self, AudioError> {
        Ok(match byte {
            0 => Self::Set,
            1 => Self::Add,
            2 => Self::Sub,
            3 => Self::MemSet,
            4 => Self::MemAdd,
            5 => Self::MemSub,
            other => return Err(AudioError::UnknownMemAccOp(other)),
        })
    }
}

/// A [`SongEvent::MemAccBranch`] condition. Each discriminant is its encoded `mem_b*` value from
/// `sound/MPlayDef.s:416-427`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MemAccCondition {
    /// `mem_beq`: jump if `cell == data`.
    Eq = 6,
    /// `mem_bne`: jump if `cell != data`.
    Ne = 7,
    /// `mem_bhi`: jump if `cell > data`.
    Hi = 8,
    /// `mem_bhs`: jump if `cell >= data`.
    Hs = 9,
    /// `mem_bls`: jump if `cell <= data`.
    Ls = 10,
    /// `mem_blo`: jump if `cell < data`.
    Lo = 11,
    /// `mem_mem_beq`: jump if `cell == cells[data]`.
    MemEq = 12,
    /// `mem_mem_bne`: jump if `cell != cells[data]`.
    MemNe = 13,
    /// `mem_mem_bhi`: jump if `cell > cells[data]`.
    MemHi = 14,
    /// `mem_mem_bhs`: jump if `cell >= cells[data]`.
    MemHs = 15,
    /// `mem_mem_bls`: jump if `cell <= cells[data]`.
    MemLs = 16,
    /// `mem_mem_blo`: jump if `cell < cells[data]`.
    MemLo = 17,
}

impl MemAccCondition {
    fn from_byte(byte: u8) -> Result<Self, AudioError> {
        Ok(match byte {
            6 => Self::Eq,
            7 => Self::Ne,
            8 => Self::Hi,
            9 => Self::Hs,
            10 => Self::Ls,
            11 => Self::Lo,
            12 => Self::MemEq,
            13 => Self::MemNe,
            14 => Self::MemHi,
            15 => Self::MemHs,
            16 => Self::MemLs,
            17 => Self::MemLo,
            other => return Err(AudioError::UnknownMemAccOp(other)),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum EventTag {
    Wait = 0,
    Note = 1,
    EndOfTie = 2,
    Voice = 3,
    Volume = 4,
    Pan = 5,
    Bend = 6,
    BendRange = 7,
    Tune = 8,
    KeyShift = 9,
    Tempo = 10,
    Priority = 11,
    LfoSpeed = 12,
    LfoDelay = 13,
    Modulation = 14,
    ModType = 15,
    Goto = 16,
    Fine = 17,
    PseudoEchoVolume = 18,
    PseudoEchoLength = 19,
    MemAcc = 20,
    MemAccBranch = 21,
}

impl EventTag {
    const fn byte(self) -> u8 {
        self as u8
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, AudioError> {
        Ok(match r.u8()? {
            0 => Self::Wait,
            1 => Self::Note,
            2 => Self::EndOfTie,
            3 => Self::Voice,
            4 => Self::Volume,
            5 => Self::Pan,
            6 => Self::Bend,
            7 => Self::BendRange,
            8 => Self::Tune,
            9 => Self::KeyShift,
            10 => Self::Tempo,
            11 => Self::Priority,
            12 => Self::LfoSpeed,
            13 => Self::LfoDelay,
            14 => Self::Modulation,
            15 => Self::ModType,
            16 => Self::Goto,
            17 => Self::Fine,
            18 => Self::PseudoEchoVolume,
            19 => Self::PseudoEchoLength,
            20 => Self::MemAcc,
            21 => Self::MemAccBranch,
            other => return Err(AudioError::UnknownSongEvent(other)),
        })
    }
}

fn tag_u8(w: &mut Writer, tag: EventTag, value: u8) {
    w.u8(tag.byte());
    w.u8(value);
}

fn tag_i8(w: &mut Writer, tag: EventTag, value: i8) {
    w.u8(tag.byte());
    w.i8(value);
}

impl SongEvent {
    fn write(&self, w: &mut Writer) {
        match *self {
            Self::Wait(ticks) => tag_u8(w, EventTag::Wait, ticks),
            Self::Note {
                key,
                velocity,
                gate,
            } => {
                w.u8(EventTag::Note.byte());
                w.u8(key);
                w.u8(velocity);
                w.u8(gate);
            }
            Self::EndOfTie { key } => {
                w.u8(EventTag::EndOfTie.byte());
                w.bool(key.is_some());
                w.u8(key.unwrap_or(0));
            }
            Self::Voice(index) => tag_u8(w, EventTag::Voice, index),
            Self::Volume(v) => tag_u8(w, EventTag::Volume, v),
            Self::Pan(v) => tag_i8(w, EventTag::Pan, v),
            Self::Bend(v) => tag_i8(w, EventTag::Bend, v),
            Self::BendRange(v) => tag_u8(w, EventTag::BendRange, v),
            Self::Tune(v) => tag_i8(w, EventTag::Tune, v),
            Self::KeyShift(v) => tag_i8(w, EventTag::KeyShift, v),
            Self::Tempo(v) => {
                w.u8(EventTag::Tempo.byte());
                w.u16(v);
            }
            Self::Priority(v) => tag_u8(w, EventTag::Priority, v),
            Self::LfoSpeed(v) => tag_u8(w, EventTag::LfoSpeed, v),
            Self::LfoDelay(v) => tag_u8(w, EventTag::LfoDelay, v),
            Self::Modulation(v) => tag_u8(w, EventTag::Modulation, v),
            Self::ModType(v) => tag_u8(w, EventTag::ModType, v),
            Self::PseudoEchoVolume(v) => tag_u8(w, EventTag::PseudoEchoVolume, v),
            Self::PseudoEchoLength(v) => tag_u8(w, EventTag::PseudoEchoLength, v),
            Self::Goto(target) => {
                w.u8(EventTag::Goto.byte());
                w.u32(target);
            }
            Self::MemAcc { op, address, data } => {
                w.u8(EventTag::MemAcc.byte());
                w.u8(op as u8);
                w.u8(address);
                w.u8(data);
            }
            Self::MemAccBranch {
                condition,
                address,
                data,
                target,
            } => {
                w.u8(EventTag::MemAccBranch.byte());
                w.u8(condition as u8);
                w.u8(address);
                w.u8(data);
                w.u32(target);
            }
            Self::Fine => w.u8(EventTag::Fine.byte()),
        }
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, AudioError> {
        match EventTag::read(r)? {
            EventTag::Wait => Ok(Self::Wait(r.u8()?)),
            EventTag::Note => {
                let key = r.u8()?;
                let velocity = r.u8()?;
                let gate = r.u8()?;
                Ok(Self::Note {
                    key,
                    velocity,
                    gate,
                })
            }
            EventTag::EndOfTie => {
                let has_key = r.bool()?;
                let raw = r.u8()?;
                Ok(Self::EndOfTie {
                    key: has_key.then_some(raw),
                })
            }
            EventTag::Voice => Ok(Self::Voice(r.u8()?)),
            EventTag::Volume => Ok(Self::Volume(r.u8()?)),
            EventTag::Pan => Ok(Self::Pan(r.i8()?)),
            EventTag::Bend => Ok(Self::Bend(r.i8()?)),
            EventTag::BendRange => Ok(Self::BendRange(r.u8()?)),
            EventTag::Tune => Ok(Self::Tune(r.i8()?)),
            EventTag::KeyShift => Ok(Self::KeyShift(r.i8()?)),
            EventTag::Tempo => Ok(Self::Tempo(r.u16()?)),
            EventTag::Priority => Ok(Self::Priority(r.u8()?)),
            EventTag::LfoSpeed => Ok(Self::LfoSpeed(r.u8()?)),
            EventTag::LfoDelay => Ok(Self::LfoDelay(r.u8()?)),
            EventTag::Modulation => Ok(Self::Modulation(r.u8()?)),
            EventTag::ModType => Ok(Self::ModType(r.u8()?)),
            EventTag::PseudoEchoVolume => Ok(Self::PseudoEchoVolume(r.u8()?)),
            EventTag::PseudoEchoLength => Ok(Self::PseudoEchoLength(r.u8()?)),
            EventTag::Goto => Ok(Self::Goto(r.u32()?)),
            EventTag::MemAcc => {
                let op = MemAccOp::from_byte(r.u8()?)?;
                let address = r.u8()?;
                let data = r.u8()?;
                Ok(Self::MemAcc { op, address, data })
            }
            EventTag::MemAccBranch => {
                let condition = MemAccCondition::from_byte(r.u8()?)?;
                let address = r.u8()?;
                let data = r.u8()?;
                let target = r.u32()?;
                Ok(Self::MemAccBranch {
                    condition,
                    address,
                    data,
                    target,
                })
            }
            EventTag::Fine => Ok(Self::Fine),
        }
    }
}

/// The maximum track count encodable by the schema's `u8` field.
pub const MAX_TRACKS: usize = u8::MAX as usize;

// Bound speculative allocation before the decoder proves that all declared events exist.
const MAX_PREALLOC_EVENTS: usize = 1 << 16;

fn check_jump_targets(
    track_index: usize,
    track: &[SongEvent],
    event_count: u32,
) -> Result<(), AudioError> {
    for (event_index, event) in track.iter().enumerate() {
        let (SongEvent::Goto(target) | SongEvent::MemAccBranch { target, .. }) = *event else {
            continue;
        };
        if target >= event_count {
            return Err(AudioError::JumpTargetOutOfRange {
                track_index,
                event_index,
                target,
                event_count,
            });
        }
    }
    Ok(())
}

/// A song's voicegroup, priority, optional reverb override, and normalized tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    voicegroup: VoiceGroupId,
    priority: u8,
    reverb: Option<u8>,
    tracks: Vec<Vec<SongEvent>>,
}

impl Song {
    /// Builds a song and canonicalizes each track's waits.
    ///
    /// `voicegroup` identifies the [`super::VoiceGroup`] selected by `Voice` events. `reverb` is
    /// `None` when the song does not override the player setting. Track order is playback order.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::IdTooLong`] when the voicegroup id exceeds its `u16` byte-length
    /// field, [`AudioError::TooManyTracks`] above [`MAX_TRACKS`],
    /// [`AudioError::TooManyEvents`] when a track length exceeds `u32::MAX`, or
    /// [`AudioError::JumpTargetOutOfRange`] when a jump does not address its own track.
    pub fn new(
        voicegroup: VoiceGroupId,
        priority: u8,
        reverb: Option<u8>,
        tracks: Vec<Vec<SongEvent>>,
    ) -> Result<Self, AudioError> {
        check_id_len(&voicegroup.0)?;
        if tracks.len() > MAX_TRACKS {
            return Err(AudioError::TooManyTracks(tracks.len()));
        }
        let tracks: Vec<Vec<SongEvent>> = tracks
            .into_iter()
            .map(|track| canonical::canonicalize_waits(&track))
            .collect();
        for (track_index, track) in tracks.iter().enumerate() {
            let event_count =
                u32::try_from(track.len()).map_err(|_| AudioError::TooManyEvents(track.len()))?;
            check_jump_targets(track_index, track, event_count)?;
        }
        Ok(Self {
            voicegroup,
            priority,
            reverb,
            tracks,
        })
    }

    /// Returns the [`super::VoiceGroup`] pack id this song plays through.
    #[must_use]
    pub fn voicegroup(&self) -> &VoiceGroupId {
        &self.voicegroup
    }

    /// Returns the song's playback priority.
    #[must_use]
    pub fn priority(&self) -> u8 {
        self.priority
    }

    /// Returns the song's reverb override, or `None` when it inherits the player setting.
    #[must_use]
    pub fn reverb(&self) -> Option<u8> {
        self.reverb
    }

    /// Returns the event streams in playback order.
    #[must_use]
    pub fn tracks(&self) -> &[Vec<SongEvent>] {
        &self.tracks
    }

    /// Encodes the song for the versioned asset-pack schema.
    ///
    /// # Panics
    ///
    /// Panics if the private track or event counts exceed their encoded widths. [`Self::new`]
    /// prevents those states.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.string(&self.voicegroup.0);
        w.u8(self.priority);
        w.bool(self.reverb.is_some());
        w.u8(self.reverb.unwrap_or(0));
        let track_count =
            u8::try_from(self.tracks.len()).expect("Song::new enforces tracks.len() <= MAX_TRACKS");
        w.u8(track_count);
        for track in &self.tracks {
            let event_count = u32::try_from(track.len())
                .expect("Song::new enforces every track's event count fits a u32");
            w.u32(event_count);
            for event in track {
                event.write(&mut w);
            }
        }
        w.into_bytes()
    }

    /// Decodes one complete song payload. Jump targets are validated within each track; resolving
    /// the cross-entry [`voicegroup`](Self::voicegroup) id remains the caller's responsibility.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::Truncated`] for incomplete or structurally malformed data,
    /// [`AudioError::InvalidString`] for a non-UTF-8 voicegroup id, [`AudioError::UnknownSongEvent`] or
    /// [`AudioError::UnknownMemAccOp`] for undefined tags, [`AudioError::JumpTargetOutOfRange`] for
    /// an invalid same-track target, and [`AudioError::TrailingBytes`] for bytes after the payload.
    pub fn decode(bytes: &[u8]) -> Result<Self, AudioError> {
        let mut r = Reader::new(bytes);
        let voicegroup = VoiceGroupId(r.string()?);
        let priority = r.u8()?;
        let has_reverb = r.bool()?;
        let reverb_value = r.u8()?;
        let reverb = has_reverb.then_some(reverb_value);
        let track_count = usize::from(r.u8()?);
        let mut tracks = Vec::with_capacity(track_count);
        for track_index in 0..track_count {
            let raw_event_count = r.u32()?;
            let event_count =
                usize::try_from(raw_event_count).map_err(|_| AudioError::Truncated)?;
            let mut events = Vec::with_capacity(event_count.min(MAX_PREALLOC_EVENTS));
            for _ in 0..event_count {
                events.push(SongEvent::read(&mut r)?);
            }
            check_jump_targets(track_index, &events, raw_event_count)?;
            tracks.push(events);
        }
        r.expect_eof()?;
        Ok(Self {
            voicegroup,
            priority,
            reverb,
            tracks,
        })
    }
}

mod canonical;

#[cfg(test)]
mod tests;
