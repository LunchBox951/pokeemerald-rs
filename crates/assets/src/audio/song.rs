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
//! # Deliberately out of scope
//!
//! See `super`'s module docs, "Deliberately deferred": upstream's
//! loop-compression commands (`PATT`/`PEND`/`REPT`), `MEMACC`, `XCMD`, and
//! `PORT` have no [`SongEvent`] variant in this slice.

use super::cursor::{Reader, Writer};
use super::error::AudioError;
use super::voicegroup::VoiceGroupId;
use super::AUDIO_SCHEMA_VERSION;

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
            TAG_GOTO => Ok(Self::Goto(r.u32()?)),
            TAG_FINE => Ok(Self::Fine),
            other => Err(AudioError::UnknownSongEvent(other)),
        }
    }
}

/// A song: which [`super::VoiceGroup`] it plays through, its
/// priority/reverb/initial tempo (upstream `struct SongHeader`'s
/// non-track-pointer fields), and one normalized event stream per track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    /// The [`super::VoiceGroup`] pack entry this song's `VOICE` commands
    /// select instruments from (upstream `SongHeader::tone`).
    pub voicegroup: VoiceGroupId,
    /// Upstream `SongHeader::priority`.
    pub priority: u8,
    /// Upstream `SongHeader::reverb`: `None` when the song does not
    /// override the master reverb setting, `Some(value)` otherwise
    /// (mirrors `mid2agb`'s own `g_reverb >= 0` sentinel check,
    /// `tools/mid2agb/agb.cpp`).
    pub reverb: Option<u8>,
    /// The song's starting tempo in BPM.
    pub initial_tempo: u16,
    /// One normalized event stream per track, `MusicPlayerTrack`-order.
    pub tracks: Vec<Vec<SongEvent>>,
}

impl Song {
    /// Encode to this schema's versioned binary form (see `super`'s module
    /// docs, "Versioning").
    ///
    /// # Panics
    ///
    /// Never in practice: only if `tracks` holds 256 or more tracks
    /// (upstream caps `MAX_MUSICPLAYER_TRACKS` at 16) or a single track
    /// holds `u32::MAX` or more events, neither of which any real song
    /// approaches.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u32(AUDIO_SCHEMA_VERSION);
        w.string(&self.voicegroup.0);
        w.u8(self.priority);
        w.bool(self.reverb.is_some());
        w.u8(self.reverb.unwrap_or(0));
        w.u16(self.initial_tempo);
        let track_count =
            u8::try_from(self.tracks.len()).expect("a song has well under 256 tracks");
        w.u8(track_count);
        for track in &self.tracks {
            let event_count =
                u32::try_from(track.len()).expect("a track has well under 2^32 events");
            w.u32(event_count);
            for event in track {
                event.write(&mut w);
            }
        }
        w.into_bytes()
    }

    /// Decode from [`encode`](Self::encode)'s binary form.
    ///
    /// # Errors
    ///
    /// [`AudioError::Truncated`] if `bytes` is shorter than the format
    /// requires; [`AudioError::UnsupportedVersion`] if the leading version
    /// field doesn't match [`AUDIO_SCHEMA_VERSION`]; [`AudioError::InvalidString`]
    /// if the voicegroup id is not valid UTF-8; [`AudioError::UnknownSongEvent`]
    /// for an unrecognized event tag byte.
    pub fn decode(bytes: &[u8]) -> Result<Self, AudioError> {
        let mut r = Reader::new(bytes);
        let version = r.u32()?;
        if version != AUDIO_SCHEMA_VERSION {
            return Err(AudioError::UnsupportedVersion {
                schema: "song",
                found: version,
            });
        }
        let voicegroup = VoiceGroupId(r.string()?);
        let priority = r.u8()?;
        let has_reverb = r.bool()?;
        let reverb_value = r.u8()?;
        let reverb = has_reverb.then_some(reverb_value);
        let initial_tempo = r.u16()?;
        let track_count = usize::from(r.u8()?);
        let mut tracks = Vec::with_capacity(track_count);
        for _ in 0..track_count {
            let event_count = usize::try_from(r.u32()?).map_err(|_| AudioError::Truncated)?;
            let mut events = Vec::with_capacity(event_count.min(1 << 16));
            for _ in 0..event_count {
                events.push(SongEvent::read(&mut r)?);
            }
            tracks.push(events);
        }
        Ok(Self {
            voicegroup,
            priority,
            reverb,
            initial_tempo,
            tracks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track() -> Vec<SongEvent> {
        vec![
            SongEvent::Voice(0),
            SongEvent::Volume(127),
            SongEvent::Note {
                key: 60,
                velocity: 127,
                gate: 24,
            },
            SongEvent::Wait(24),
            SongEvent::Note {
                key: 62,
                velocity: 100,
                gate: 0,
            },
            SongEvent::EndOfTie { key: None },
            SongEvent::Fine,
        ]
    }

    #[test]
    fn a_typical_song_round_trips() {
        let song = Song {
            voicegroup: VoiceGroupId("audio/voicegroup/title".to_owned()),
            priority: 0,
            reverb: Some(80),
            initial_tempo: 144,
            tracks: vec![sample_track(), sample_track()],
        };
        let bytes = song.encode();
        assert_eq!(Song::decode(&bytes).unwrap(), song);
    }

    #[test]
    fn no_reverb_override_round_trips() {
        let song = Song {
            voicegroup: VoiceGroupId("audio/voicegroup/dummy".to_owned()),
            priority: 5,
            reverb: None,
            initial_tempo: 120,
            tracks: vec![vec![SongEvent::Fine]],
        };
        let bytes = song.encode();
        assert_eq!(Song::decode(&bytes).unwrap(), song);
    }

    #[test]
    fn every_controller_and_note_event_kind_round_trips() {
        let track = vec![
            SongEvent::Wait(96),
            SongEvent::Note {
                key: 0,
                velocity: 0,
                gate: 0,
            },
            SongEvent::EndOfTie { key: Some(72) },
            SongEvent::Voice(127),
            SongEvent::Volume(0),
            SongEvent::Pan(-64),
            SongEvent::Pan(63),
            SongEvent::Bend(0),
            SongEvent::BendRange(2),
            SongEvent::Tune(-64),
            SongEvent::KeyShift(12),
            SongEvent::Tempo(300),
            SongEvent::Priority(255),
            SongEvent::LfoSpeed(16),
            SongEvent::LfoDelay(0),
            SongEvent::Modulation(127),
            SongEvent::ModType(1),
            SongEvent::Goto(0),
            SongEvent::Fine,
        ];
        let song = Song {
            voicegroup: VoiceGroupId("audio/voicegroup/vs_rayquaza".to_owned()),
            priority: 1,
            reverb: Some(0),
            initial_tempo: 1,
            tracks: vec![track],
        };
        let bytes = song.encode();
        assert_eq!(Song::decode(&bytes).unwrap(), song);
    }

    #[test]
    fn voice_command_selects_the_highest_slot_127() {
        // MUS_TITLE's own MIDI source selects instrument 127 on one channel
        // -- pin that a `Voice` event carrying that value round-trips.
        let song = Song {
            voicegroup: VoiceGroupId("audio/voicegroup/title".to_owned()),
            priority: 0,
            reverb: None,
            initial_tempo: 144,
            tracks: vec![vec![SongEvent::Voice(127), SongEvent::Fine]],
        };
        let bytes = song.encode();
        let decoded = Song::decode(&bytes).unwrap();
        assert_eq!(decoded.tracks[0][0], SongEvent::Voice(127));
    }

    #[test]
    fn empty_track_list_round_trips() {
        let song = Song {
            voicegroup: VoiceGroupId("audio/voicegroup/dummy".to_owned()),
            priority: 0,
            reverb: None,
            initial_tempo: 120,
            tracks: vec![],
        };
        let bytes = song.encode();
        assert_eq!(Song::decode(&bytes).unwrap(), song);
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let mut bytes = Song {
            voicegroup: VoiceGroupId("x".to_owned()),
            priority: 0,
            reverb: None,
            initial_tempo: 1,
            tracks: vec![],
        }
        .encode();
        bytes[0] = 0xFF;
        assert_eq!(
            Song::decode(&bytes),
            Err(AudioError::UnsupportedVersion {
                schema: "song",
                found: u32::from_le_bytes([0xFF, 0, 0, 0]),
            })
        );
    }

    #[test]
    fn unknown_event_tag_is_rejected() {
        let mut bytes = Song {
            voicegroup: VoiceGroupId("x".to_owned()),
            priority: 0,
            reverb: None,
            initial_tempo: 1,
            tracks: vec![vec![SongEvent::Fine]],
        }
        .encode();
        let last = bytes.len() - 1;
        bytes[last] = 0xFF; // the lone event's tag byte
        assert_eq!(
            Song::decode(&bytes),
            Err(AudioError::UnknownSongEvent(0xFF))
        );
    }

    #[test]
    fn truncated_input_is_rejected() {
        let bytes = Song {
            voicegroup: VoiceGroupId("audio/voicegroup/title".to_owned()),
            priority: 3,
            reverb: Some(10),
            initial_tempo: 120,
            tracks: vec![sample_track()],
        }
        .encode();
        for cut in 0..bytes.len() {
            assert!(Song::decode(&bytes[..cut]).is_err());
        }
    }
}
