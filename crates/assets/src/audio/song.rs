//! [`Song`]: a normalized, MIDI-semantic event stream per track, plus the
//! song-level metadata upstream's `struct SongHeader`
//! (`pokeemerald/include/gba/m4a_internal.h`) carries alongside it.
//!
//! # Normalized, not raw MIDI and not upstream's compiled byte-code
//!
//! [`SongEvent`] is neither a transcription of standard-MIDI file bytes
//! (running status, variable-length quantities, the 16-channel model) nor of
//! upstream's own compiled MP2K track byte-code (status/argument bytes,
//! `WAIT_LO..WAIT_HI` ranges, running-status shorthand — the shape
//! `pokeemerald/tools/mid2agb` emits and a from-ROM backend would have to
//! decompile). It is the shared *musical* meaning both a `.mid`-source
//! backend and a compiled-byte-code backend normalize down to: notes with
//! an explicit gate length, ties, and named controller changes — one
//! variant per distinct command, addressed by name rather than by the
//! magic byte ranges either source format uses. `#115` child 2 (the MIDI
//! compiler) is what actually produces this from a `.mid` file; this module
//! only defines the shape.
//!
//! # Looping
//!
//! [`SongEvent::Goto`] carries a target as an *event index* into the same
//! track's own event list (not a byte offset, and not a label name) — a
//! self-contained addressing scheme requiring no separate symbol table,
//! matching how a compiled form must ultimately resolve loop targets
//! anyway.
//!
//! # Tempo is a track event, not header metadata
//!
//! [`Song`] carries no starting-tempo field, because upstream's
//! `struct SongHeader` has none: its fields are `trackCount`, `blockCount`
//! (dropped here because the engine never reads it —
//! `pokeemerald/src/m4a_tables.c:262` sets it to `0` for the cry song, and
//! `tools/mid2agb` uses its `s_blockCount` purely as an asm-label counter),
//! `priority`, `reverb`, `tone`, and the per-track pointers. Tempo reaches
//! the engine as a `TEMPO` command inside a track's own event stream
//! (`ply_tempo`, `pokeemerald/src/m4a.c`), conventionally as one of track
//! 0's first events, and is modelled here as exactly that:
//! [`SongEvent::Tempo`]. A song with no `Tempo` event simply never sets one,
//! same as upstream — there is no separate default for this schema to
//! record.
//!
//! # Deliberately out of scope
//!
//! See `super`'s module docs, "Deliberately deferred", for the full list
//! with per-command rationale: `PATT`/`PEND` (on-disk loop compression),
//! `REPT` and `PORT` (never emitted by `tools/mid2agb`), `MEMACC` (one
//! song), and the `XCMD` sub-commands other than `xIECV`/`xIECL` (never
//! emitted either) have no [`SongEvent`] variant in this slice.

use super::cursor::{check_id_len, Reader, Writer};
use super::error::AudioError;
use super::voicegroup::VoiceGroupId;

/// One normalized track command. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongEvent {
    /// Delay this track for `ticks` sequencer ticks before its next event.
    Wait(u8),
    /// Play a note. `gate` is the note-off delay in ticks; `0` marks a tied
    /// note that sounds until an [`SongEvent::EndOfTie`] or the next note.
    Note { key: u8, velocity: u8, gate: u8 },
    /// End a tie. `key` is `None` when the source omitted its key operand,
    /// in which case a player matches on the track's currently-sounding key.
    EndOfTie { key: Option<u8> },
    /// Select instrument `index` from the song's [`super::VoiceGroup`]
    /// (`0..=255` on the wire; `0..=127` by convention, matching a MIDI
    /// program number — see `super`'s module docs on slot 127).
    Voice(u8),
    /// Track volume, `0..=127`.
    Volume(u8),
    /// Track pan, centre-relative (`-64..=63`).
    Pan(i8),
    /// Pitch bend, centre-relative.
    Bend(i8),
    /// Pitch-bend range in semitones.
    BendRange(u8),
    /// Fine tune, centre-relative.
    Tune(i8),
    /// Transpose the track by whole semitones.
    KeyShift(i8),
    /// Set tempo in BPM.
    Tempo(u16),
    /// Track priority.
    Priority(u8),
    /// LFO speed.
    LfoSpeed(u8),
    /// LFO delay.
    LfoDelay(u8),
    /// Modulation depth.
    Modulation(u8),
    /// Modulation type.
    ModType(u8),
    /// Pseudo-echo volume, as a percentage of the note's own volume
    /// (upstream `XCMD xIECV` — extended command `8`, `sound/MPlayDef.s`).
    ///
    /// `ply_xiecv` (`pokeemerald/src/m4a.c`) stores it in
    /// `MusicPlayerTrack::pseudoEchoVolume`, which `ply_note` copies onto
    /// each note's channel, where both families sound the decay tail once
    /// the envelope's release completes — `DirectSound` via `SoundMainRAM`'s
    /// `SOUND_CHANNEL_SF_IEC` branch (`pokeemerald/src/m4a_1.s:206-232`) and
    /// CGB via `CgbSound` (`pokeemerald/src/m4a.c:925`). The channel carries
    /// the pair for either family: `pseudoEchoVolume`/`pseudoEchoLength` are
    /// `struct SoundChannel` fields
    /// (`pokeemerald/include/gba/m4a_internal.h:101-102`), not CGB-only
    /// state. That tail is audible musical content, not a compression
    /// artifact, which is why this pair is modelled while the rest of `XCMD`
    /// is not (see `super`'s "Deliberately deferred"). `tools/mid2agb` emits
    /// it — and it is one of only two extended commands it emits at all —
    /// for a large minority of upstream's MIDIs, so a song stream that
    /// dropped it would play a noticeably drier version of those tracks.
    PseudoEchoVolume(u8),
    /// Pseudo-echo length in ticks — how long the
    /// [`PseudoEchoVolume`](Self::PseudoEchoVolume) tail sounds for
    /// (upstream `XCMD xIECL`, extended command `9`; `ply_xiecl` →
    /// `MusicPlayerTrack::pseudoEchoLength`).
    PseudoEchoLength(u8),
    /// Jump to another event index in this same track's event list — the
    /// loop primitive (see the module docs).
    Goto(u32),
    /// End of track.
    Fine,
}

const TAG_WAIT: u8 = 0;
const TAG_NOTE: u8 = 1;
const TAG_END_OF_TIE: u8 = 2;
const TAG_VOICE: u8 = 3;
const TAG_VOLUME: u8 = 4;
const TAG_PAN: u8 = 5;
const TAG_BEND: u8 = 6;
const TAG_BEND_RANGE: u8 = 7;
const TAG_TUNE: u8 = 8;
const TAG_KEY_SHIFT: u8 = 9;
const TAG_TEMPO: u8 = 10;
const TAG_PRIORITY: u8 = 11;
const TAG_LFO_SPEED: u8 = 12;
const TAG_LFO_DELAY: u8 = 13;
const TAG_MODULATION: u8 = 14;
const TAG_MOD_TYPE: u8 = 15;
const TAG_GOTO: u8 = 16;
const TAG_FINE: u8 = 17;
// Appended, not renumbered: tag bytes are the wire discriminator, so new
// variants take the next free values rather than disturbing existing ones.
const TAG_PSEUDO_ECHO_VOLUME: u8 = 18;
const TAG_PSEUDO_ECHO_LENGTH: u8 = 19;

impl SongEvent {
    fn write(&self, w: &mut Writer) {
        match *self {
            Self::Wait(ticks) => {
                w.u8(TAG_WAIT);
                w.u8(ticks);
            }
            Self::Note {
                key,
                velocity,
                gate,
            } => {
                w.u8(TAG_NOTE);
                w.u8(key);
                w.u8(velocity);
                w.u8(gate);
            }
            Self::EndOfTie { key } => {
                w.u8(TAG_END_OF_TIE);
                w.bool(key.is_some());
                w.u8(key.unwrap_or(0));
            }
            Self::Voice(index) => {
                w.u8(TAG_VOICE);
                w.u8(index);
            }
            Self::Volume(v) => {
                w.u8(TAG_VOLUME);
                w.u8(v);
            }
            Self::Pan(v) => {
                w.u8(TAG_PAN);
                w.i8(v);
            }
            Self::Bend(v) => {
                w.u8(TAG_BEND);
                w.i8(v);
            }
            Self::BendRange(v) => {
                w.u8(TAG_BEND_RANGE);
                w.u8(v);
            }
            Self::Tune(v) => {
                w.u8(TAG_TUNE);
                w.i8(v);
            }
            Self::KeyShift(v) => {
                w.u8(TAG_KEY_SHIFT);
                w.i8(v);
            }
            Self::Tempo(v) => {
                w.u8(TAG_TEMPO);
                w.u16(v);
            }
            Self::Priority(v) => {
                w.u8(TAG_PRIORITY);
                w.u8(v);
            }
            Self::LfoSpeed(v) => {
                w.u8(TAG_LFO_SPEED);
                w.u8(v);
            }
            Self::LfoDelay(v) => {
                w.u8(TAG_LFO_DELAY);
                w.u8(v);
            }
            Self::Modulation(v) => {
                w.u8(TAG_MODULATION);
                w.u8(v);
            }
            Self::ModType(v) => {
                w.u8(TAG_MOD_TYPE);
                w.u8(v);
            }
            Self::PseudoEchoVolume(v) => {
                w.u8(TAG_PSEUDO_ECHO_VOLUME);
                w.u8(v);
            }
            Self::PseudoEchoLength(v) => {
                w.u8(TAG_PSEUDO_ECHO_LENGTH);
                w.u8(v);
            }
            Self::Goto(target) => {
                w.u8(TAG_GOTO);
                w.u32(target);
            }
            Self::Fine => w.u8(TAG_FINE),
        }
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, AudioError> {
        match r.u8()? {
            TAG_WAIT => Ok(Self::Wait(r.u8()?)),
            TAG_NOTE => {
                let key = r.u8()?;
                let velocity = r.u8()?;
                let gate = r.u8()?;
                Ok(Self::Note {
                    key,
                    velocity,
                    gate,
                })
            }
            TAG_END_OF_TIE => {
                let has_key = r.bool()?;
                let raw = r.u8()?;
                Ok(Self::EndOfTie {
                    key: has_key.then_some(raw),
                })
            }
            TAG_VOICE => Ok(Self::Voice(r.u8()?)),
            TAG_VOLUME => Ok(Self::Volume(r.u8()?)),
            TAG_PAN => Ok(Self::Pan(r.i8()?)),
            TAG_BEND => Ok(Self::Bend(r.i8()?)),
            TAG_BEND_RANGE => Ok(Self::BendRange(r.u8()?)),
            TAG_TUNE => Ok(Self::Tune(r.i8()?)),
            TAG_KEY_SHIFT => Ok(Self::KeyShift(r.i8()?)),
            TAG_TEMPO => Ok(Self::Tempo(r.u16()?)),
            TAG_PRIORITY => Ok(Self::Priority(r.u8()?)),
            TAG_LFO_SPEED => Ok(Self::LfoSpeed(r.u8()?)),
            TAG_LFO_DELAY => Ok(Self::LfoDelay(r.u8()?)),
            TAG_MODULATION => Ok(Self::Modulation(r.u8()?)),
            TAG_MOD_TYPE => Ok(Self::ModType(r.u8()?)),
            TAG_PSEUDO_ECHO_VOLUME => Ok(Self::PseudoEchoVolume(r.u8()?)),
            TAG_PSEUDO_ECHO_LENGTH => Ok(Self::PseudoEchoLength(r.u8()?)),
            TAG_GOTO => Ok(Self::Goto(r.u32()?)),
            TAG_FINE => Ok(Self::Fine),
            other => Err(AudioError::UnknownSongEvent(other)),
        }
    }
}

/// The most tracks a [`Song`] may hold: the encoding's `u8` track-count
/// field's ceiling. Upstream's own engine caps a playing song at
/// `MAX_MUSICPLAYER_TRACKS` (16, `pokeemerald/include/gba/m4a_internal.h`),
/// so this is a wire-format bound with a wide margin over anything a real
/// song uses, not a limit the schema imposes on musical content.
pub const MAX_TRACKS: usize = u8::MAX as usize;

/// Cap on how many events [`Song::decode`] pre-reserves per track from the
/// untrusted per-track event count, mirroring
/// [`super::sample`]'s `MAX_PREALLOC_SAMPLES` and `crate::pack::format`'s
/// `MAX_PREALLOC_ENTRIES` (same rationale: a corrupt count near `u32::MAX`
/// must not speculatively allocate gigabytes before the first short read
/// fails the decode). The `Vec` still grows to whatever the input holds.
const MAX_PREALLOC_EVENTS: usize = 1 << 16;

/// A song: which [`super::VoiceGroup`] it plays through, its
/// priority/reverb (the upstream `struct SongHeader` fields this schema
/// carries — `trackCount` is implicit in [`tracks`](Self::tracks) and
/// `blockCount` is dropped, see the module docs), and one normalized event
/// stream per track. Build one with [`Song::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    voicegroup: VoiceGroupId,
    priority: u8,
    reverb: Option<u8>,
    tracks: Vec<Vec<SongEvent>>,
}

impl Song {
    /// Build a song from its header metadata and per-track event streams.
    ///
    /// Checked rather than a plain struct literal, for the same reason
    /// [`super::VoiceGroup::new`] is: the bounds below are what make
    /// [`Self::encode`] total, so they are enforced where they can still be
    /// reported as an error instead of at encode time as a panic.
    ///
    /// * `voicegroup` — the [`super::VoiceGroup`] pack entry this song's
    ///   `VOICE` commands select instruments from (upstream
    ///   `SongHeader::tone`).
    /// * `priority` — upstream `SongHeader::priority`.
    /// * `reverb` — upstream `SongHeader::reverb`: `None` when the song does
    ///   not override the master reverb setting, `Some(value)` otherwise
    ///   (mirrors `mid2agb`'s own `g_reverb >= 0` sentinel check,
    ///   `tools/mid2agb/agb.cpp`).
    /// * `tracks` — one normalized event stream per track,
    ///   `MusicPlayerTrack`-order.
    ///
    /// # Errors
    ///
    /// [`AudioError::TooManyTracks`] if `tracks.len() > `[`MAX_TRACKS`];
    /// [`AudioError::TooManyEvents`] if any one track holds more than
    /// `u32::MAX` events; [`AudioError::IdTooLong`] if `voicegroup`'s id
    /// does not fit the `u16` length prefix the encoding writes for it.
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
        for track in &tracks {
            if u32::try_from(track.len()).is_err() {
                return Err(AudioError::TooManyEvents(track.len()));
            }
        }
        Ok(Self {
            voicegroup,
            priority,
            reverb,
            tracks,
        })
    }

    /// The [`super::VoiceGroup`] pack entry this song plays through.
    #[must_use]
    pub fn voicegroup(&self) -> &VoiceGroupId {
        &self.voicegroup
    }

    /// Upstream `SongHeader::priority`.
    #[must_use]
    pub fn priority(&self) -> u8 {
        self.priority
    }

    /// Upstream `SongHeader::reverb`, or `None` for no override.
    #[must_use]
    pub fn reverb(&self) -> Option<u8> {
        self.reverb
    }

    /// Every track's event stream, `MusicPlayerTrack`-order.
    #[must_use]
    pub fn tracks(&self) -> &[Vec<SongEvent>] {
        &self.tracks
    }

    /// Encode to this schema's binary form. Staleness is gated by
    /// [`crate::pack::FORMAT_VERSION`] — see `super`'s module docs,
    /// "Versioning".
    ///
    /// # Panics
    ///
    /// Unreachable: [`Self::new`] is the only way to build a `Song`, and it
    /// rejects a track list longer than [`MAX_TRACKS`] and any track whose
    /// event count overflows a `u32` — returning [`AudioError`] where a
    /// caller can see it. Every field here is private, so neither count can
    /// grow after construction.
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

    /// Decode from [`encode`](Self::encode)'s binary form.
    ///
    /// Structural decode only: a [`SongEvent::Goto`] target is not validated
    /// against the length of the track it appears in, the same way
    /// [`super::Sample::decode`] does not validate a loop point against its
    /// sample data. Cross-field and cross-entry validation belongs to the
    /// later `#115` child that loads a pack's audio entries together.
    ///
    /// # Errors
    ///
    /// [`AudioError::Truncated`] if `bytes` is shorter than the format
    /// requires; [`AudioError::InvalidString`] if the voicegroup id is not
    /// valid UTF-8; [`AudioError::UnknownSongEvent`] for an unrecognized
    /// event tag byte.
    pub fn decode(bytes: &[u8]) -> Result<Self, AudioError> {
        let mut r = Reader::new(bytes);
        let voicegroup = VoiceGroupId(r.string()?);
        let priority = r.u8()?;
        let has_reverb = r.bool()?;
        let reverb_value = r.u8()?;
        let reverb = has_reverb.then_some(reverb_value);
        // The encoded count is a `u8`, so it cannot exceed `MAX_TRACKS` --
        // no decode-side bound check is reachable here, unlike
        // `VoiceGroup::decode`, whose `u8` count can overrun its 128 slots.
        let track_count = usize::from(r.u8()?);
        let mut tracks = Vec::with_capacity(track_count);
        for _ in 0..track_count {
            let event_count = usize::try_from(r.u32()?).map_err(|_| AudioError::Truncated)?;
            let mut events = Vec::with_capacity(event_count.min(MAX_PREALLOC_EVENTS));
            for _ in 0..event_count {
                events.push(SongEvent::read(&mut r)?);
            }
            tracks.push(events);
        }
        Ok(Self {
            voicegroup,
            priority,
            reverb,
            tracks,
        })
    }
}

#[cfg(test)]
mod tests;
