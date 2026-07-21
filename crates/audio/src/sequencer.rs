//! The owned sequencer: the tick engine that walks each track's decoded
//! events, turns them into note-ons/offs, and drives the [`Mixer`].
//!
//! Behavioural port of `MPlayMain` (`m4a_1.s:1129`) plus the volume/pitch
//! resolution in `TrkVolPitSet` (`m4a.c:765`) and `ChnVolSetAsm`
//! (`m4a_1.s:1508`). Tempo accumulates at `TEMPO_UNIT` per tick; each tick
//! counts note-off gates down, then processes commands until every track is
//! blocked on a `Wait`. Rendering happens one frame at a time, matching
//! `SoundMain`'s once-per-V-blank cadence.
//!
//! Out of scope for this slice (decoded but not executed): patterns
//! (`PATT`/`PEND`/`REPT`), memory-accumulator (`MEMACC`), extended commands
//! (`XCMD`), LFO/vibrato, priority-based voice stealing.

use crate::pitch::{self, SAMPLES_PER_FRAME};
use crate::sequence::Event;
use crate::song::Song;
use crate::voice::{channel_volume, Voice};
use crate::{Mixer, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES};

/// Tempo units per tick; `MPlayMain` fires a tick each time `tempoC` crosses
/// `150` (`subs r0, 150`).
const TEMPO_UNIT: u16 = 150;

/// Default track volume when a track plays a note before any `VOL` command.
/// (Upstream leaves `track->vol` at `0`; this crate defaults to full so a
/// minimal hand-authored sequence is audible — a deliberate convenience.)
const DEFAULT_TRACK_VOLUME: u8 = 127;

/// Default pitch-bend range (`track->bendRange = 2`, `m4a_1.s:1223`).
const DEFAULT_BEND_RANGE: u8 = 2;

/// The instrument volume scaler `volX` (`0x40`, set at track init).
const VOL_X: u32 = 0x40;

/// Safety bound on commands processed for one track in one tick, so a
/// malformed loop with no `Wait` cannot hang the mixer.
const MAX_COMMANDS_PER_TICK: u32 = 4096;

/// Per-track runtime state (the mutable half of `struct MusicPlayerTrack`).
#[derive(Clone, Debug)]
struct TrackState {
    cursor: usize,
    wait: u16,
    ended: bool,
    voice: usize,
    vol: u8,
    pan: i8,
    bend: i8,
    bend_range: u8,
    tune: i8,
    key_shift: i8,
    /// The track's current MIDI key (`track->key`): the raw key of the last
    /// note, reused when an `EOT` omits its key operand (`m4a_1.s:1830`).
    key: u8,
}

impl TrackState {
    fn new() -> Self {
        Self {
            cursor: 0,
            wait: 0,
            ended: false,
            voice: 0,
            vol: DEFAULT_TRACK_VOLUME,
            pan: 0,
            bend: 0,
            bend_range: DEFAULT_BEND_RANGE,
            tune: 0,
            key_shift: 0,
            // Upstream zeroes `track->key` at init; no `EOT` should fire before
            // a note sets it in real data.
            key: 0,
        }
    }
}

/// An owned M4A sequencer + mixer. Construct from a [`Song`], then pull audio
/// one frame at a time with [`Self::render_frame`] (or many frames with
/// [`Self::mix_into`]).
#[derive(Debug)]
pub struct Sequencer {
    song: Song,
    tracks: Vec<TrackState>,
    mixer: Mixer,
    /// Tempo increment per frame (BPM); accumulates into `tempo_c`.
    tempo_i: u16,
    tempo_c: u16,
}

impl Sequencer {
    /// Interleaved-stereo samples one [`Self::render_frame`] produces
    /// (`SAMPLES_PER_FRAME * 2`).
    pub const FRAME_SAMPLES: usize = SAMPLES_PER_FRAME * 2;

    /// Build a sequencer for `song` with the default mixer configuration.
    #[must_use]
    pub fn new(song: Song) -> Self {
        Self::with_config(song, DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES)
    }

    /// Build a sequencer with an explicit master volume and voice cap.
    #[must_use]
    pub fn with_config(song: Song, master_volume: u8, max_voices: usize) -> Self {
        let tracks = (0..song.track_count()).map(|_| TrackState::new()).collect();
        let tempo_i = song.initial_tempo();
        Self {
            song,
            tracks,
            mixer: Mixer::new(master_volume, max_voices),
            tempo_i,
            tempo_c: 0,
        }
    }

    /// Number of voices currently sounding.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.mixer.voice_count()
    }

    /// Whether every track has ended and all voices have decayed to silence.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.tracks.iter().all(|t| t.ended) && self.mixer.is_idle()
    }

    /// Advance the sequencer by one V-blank frame and render its audio into
    /// `out`, which must hold exactly [`Self::FRAME_SAMPLES`] interleaved
    /// stereo `f32`s.
    ///
    /// # Panics
    ///
    /// Panics if `out.len() != Self::FRAME_SAMPLES`.
    pub fn render_frame(&mut self, out: &mut [f32]) {
        self.advance_frame();
        self.mixer.mix_frame(out);
    }

    /// Render whole frames into `out`, whose length must be a multiple of
    /// [`Self::FRAME_SAMPLES`]. Deterministic and device-free — the offline
    /// rendering path unit tests assert against.
    ///
    /// # Panics
    ///
    /// Panics if `out.len()` is not a positive multiple of
    /// [`Self::FRAME_SAMPLES`].
    pub fn mix_into(&mut self, out: &mut [f32]) {
        assert!(
            !out.is_empty() && out.len().is_multiple_of(Self::FRAME_SAMPLES),
            "mix_into length must be a multiple of FRAME_SAMPLES",
        );
        for frame in out.chunks_mut(Self::FRAME_SAMPLES) {
            self.render_frame(frame);
        }
    }

    /// Run the tempo accumulator for one frame, firing ticks as it crosses
    /// [`TEMPO_UNIT`] (`m4a_1.s:1169`..`:1359`).
    fn advance_frame(&mut self) {
        self.tempo_c += self.tempo_i;
        while self.tempo_c >= TEMPO_UNIT {
            self.tempo_c -= TEMPO_UNIT;
            self.do_tick();
        }
    }

    /// One sequencer tick: gate countdown, then command processing per track.
    fn do_tick(&mut self) {
        self.mixer.tick_gates();
        // Disjoint field borrows so each track can touch the shared mixer.
        let Self {
            song,
            tracks,
            mixer,
            tempo_i,
            ..
        } = self;
        for (track_id, track) in tracks.iter_mut().enumerate() {
            Self::process_track(song, track, mixer, track_id, tempo_i);
        }
    }

    /// Process one track for one tick: run commands until it blocks on a
    /// `Wait`, then consume one tick of that wait.
    fn process_track(
        song: &Song,
        track: &mut TrackState,
        mixer: &mut Mixer,
        track_id: usize,
        tempo_i: &mut u16,
    ) {
        if track.ended {
            return;
        }
        let events = &song.tracks()[track_id];

        if track.wait == 0 {
            let mut guard = 0;
            loop {
                guard += 1;
                if guard > MAX_COMMANDS_PER_TICK {
                    track.ended = true;
                    break;
                }
                if track.cursor >= events.len() {
                    track.ended = true;
                    break;
                }
                let event = events[track.cursor].clone();
                track.cursor += 1;
                Self::handle_event(song, track, mixer, track_id, tempo_i, &event);
                if track.wait > 0 || track.ended {
                    break;
                }
            }
        }

        if track.wait > 0 {
            track.wait -= 1;
        }
    }

    /// Apply one decoded event to a track (and the mixer/tempo).
    fn handle_event(
        song: &Song,
        track: &mut TrackState,
        mixer: &mut Mixer,
        track_id: usize,
        tempo_i: &mut u16,
        event: &Event,
    ) {
        match *event {
            Event::Wait(ticks) => track.wait = u16::from(ticks),
            Event::Fine => {
                // `ply_fine` releases every still-ON channel this track owns
                // before clearing it, so tied voices and looping waves stop.
                mixer.release_track(track_id);
                track.ended = true;
            }
            Event::Goto(index) => track.cursor = index,
            Event::Voice(v) => track.voice = usize::from(v),
            // Volume/pan set `MPT_FLG_VOLCHG`; pitch commands set
            // `MPT_FLG_PITCHG`. Upstream re-runs `TrkVolPitSet` + per-channel
            // `ChnVolSetAsm`/`MidiKeyToFreq` for the flagged track each tick;
            // here every event executes before the frame renders, so applying
            // the change to the track's live voices immediately is equivalent.
            Event::Volume(v) => {
                track.vol = v;
                Self::apply_track_volume(track, mixer, track_id);
            }
            Event::Pan(p) => {
                track.pan = p;
                Self::apply_track_volume(track, mixer, track_id);
            }
            Event::Tempo(bpm) => *tempo_i = bpm,
            Event::KeyShift(k) => {
                track.key_shift = k;
                Self::apply_track_pitch(track, mixer, track_id);
            }
            Event::Bend(b) => {
                track.bend = b;
                Self::apply_track_pitch(track, mixer, track_id);
            }
            Event::BendRange(r) => {
                track.bend_range = r;
                Self::apply_track_pitch(track, mixer, track_id);
            }
            Event::Tune(t) => {
                track.tune = t;
                Self::apply_track_pitch(track, mixer, track_id);
            }
            Event::Note {
                key,
                velocity,
                gate,
            } => {
                // `ply_note` records the raw command key as `track->key`.
                track.key = key;
                Self::note_on(song, track, mixer, track_id, key, velocity, gate);
            }
            Event::EndOfTie { key } => {
                // With an operand, `ply_endtie` stores it as the new `track->key`
                // and matches on it; without one, it matches the current key.
                let match_key = match key {
                    Some(k) => {
                        track.key = k;
                        k
                    }
                    None => track.key,
                };
                mixer.note_off_track(track_id, match_key);
            }
            // Decoded but not executed by this slice (see the module docs).
            _ => {}
        }
    }

    /// Re-run `TrkVolPitSet`'s volume half for the track and push the new
    /// `volMR`/`volML` onto its live voices (`MPT_FLG_VOLCHG`,
    /// `m4a_1.s:1391`..`:1394`).
    fn apply_track_volume(track: &TrackState, mixer: &mut Mixer, track_id: usize) {
        let (vol_mr, vol_ml) = track_volume(track);
        mixer.set_track_volume(track_id, vol_mr, vol_ml);
    }

    /// Re-run `TrkVolPitSet`'s pitch half for the track and push the new
    /// `keyM`/`pitM` onto its live voices (`MPT_FLG_PITCHG`,
    /// `m4a_1.s:1403`..`:1451`).
    fn apply_track_pitch(track: &TrackState, mixer: &mut Mixer, track_id: usize) {
        let (key_m, pit_m) = track_pitch(track);
        mixer.set_track_pitch(track_id, key_m, pit_m);
    }

    /// Allocate a voice for a note, resolving its stereo volume and pitch from
    /// the track's current state (`TrkVolPitSet` + `ChnVolSetAsm`).
    fn note_on(
        song: &Song,
        track: &TrackState,
        mixer: &mut Mixer,
        track_id: usize,
        key: u8,
        velocity: u8,
        gate: u8,
    ) {
        let Some(tone) = song.voice(track.voice) else {
            return;
        };

        let (vol_mr, vol_ml) = track_volume(track);
        let (key_m, pit_m) = track_pitch(track);
        let right = channel_volume(vol_mr, 0x80, velocity);
        let left = channel_volume(vol_ml, 0x7F, velocity);

        // key + keyM, floored at 0 (`bpl _081DDCA0; movs r3, 0`), then the sum
        // is passed to `MidiKeyToFreq`'s `u8 key` param, truncating the register
        // to its low byte — so a sum above 255 wraps modulo 256, it does not
        // saturate (`ldrb r1, [key]; adds r3, r1, r0; ... bl MidiKeyToFreq`).
        let note_key = u8::try_from((i32::from(key) + key_m).max(0) & 0xFF).unwrap_or(0);
        let freq = pitch::midi_key_to_freq(tone.wave.freq(), note_key, pit_m);

        let voice = Voice::new(
            tone.wave.clone(),
            tone.adsr,
            freq,
            right,
            left,
            velocity,
            u16::from(gate),
            key,
            track_id,
            0,
            0,
        );
        mixer.add_voice(voice);
    }
}

/// `TrkVolPitSet`'s volume half: track vol/pan → right/left channel base
/// volumes (`volMR`/`volML`), with `volX = 0x40`, `panX = 0` (`m4a.c:772`).
fn track_volume(track: &TrackState) -> (u8, u8) {
    let x = (u32::from(track.vol) * VOL_X) >> 5;
    let y = (2 * i32::from(track.pan)).clamp(-128, 127);
    // `(y + 128)` and `(127 - y)` are both in `0..=255`.
    let vol_mr = (u32::try_from(y + 128).unwrap_or(0) * x) >> 8;
    let vol_ml = (u32::try_from(127 - y).unwrap_or(0) * x) >> 8;
    (
        u8::try_from(vol_mr.min(255)).unwrap_or(255),
        u8::try_from(vol_ml.min(255)).unwrap_or(255),
    )
}

/// `TrkVolPitSet`'s pitch half: track key-shift/bend/tune → integer key offset
/// (`keyM`) and 8-bit fine adjust (`pitM`), with `keyShiftX`/`pitX`/`modM = 0`
/// (`m4a.c:791`).
fn track_pitch(track: &TrackState) -> (i32, u8) {
    let bend = i32::from(track.bend) * i32::from(track.bend_range);
    let x = (i32::from(track.tune) + bend) * 4 + (i32::from(track.key_shift) << 8);
    // Hardware stores `keyM = x >> 8` into a `u8` field
    // (`m4a_internal.h:282`) and reads it back with a signed byte load
    // (`ldrsb r0, [keyM]`, `m4a_1.s:1762`): the effective offset is
    // `(s8)((x >> 8) & 0xFF)`, wrapping modulo 256 rather than staying full
    // width. Truncate to a byte, then reinterpret those bits as signed.
    let key_m_byte = u8::try_from((x >> 8) & 0xFF).unwrap_or(0);
    let key_m = i32::from(i8::from_le_bytes([key_m_byte]));
    let pit_m = u8::try_from(x & 0xFF).unwrap_or(0);
    (key_m, pit_m)
}

#[cfg(test)]
// The reciprocal wave-frequency the test song derives narrows to `u32` (well
// within range for these inputs); silence/pan checks compare exact `0.0`.
#[allow(clippy::cast_possible_truncation, clippy::float_cmp)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::envelope::Adsr;
    use crate::sample::WaveData;
    use crate::sequence::decode_track;
    use crate::song::ToneData;

    fn unity_freq() -> u32 {
        (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ
    }

    /// A voicegroup with one loud, long, flat-envelope instrument at unity
    /// pitch for the reference key 60.
    fn test_song(tracks: Vec<Vec<Event>>, tempo: u16) -> Song {
        // Choose wave freq so key 60 renders near unity: pick freq that makes
        // midi_key_to_freq(freq, 60, 0) close to the unity frequency.
        let target = unity_freq();
        // midi_key_to_freq(freq, 60, 0) ~= freq * ratio60 >> 32; invert roughly.
        let ratio60 = pitch::midi_key_to_freq(1 << 20, 60, 0);
        let freq = ((u64::from(target) << 20) / u64::from(ratio60)) as u32;
        let wave = Arc::new(WaveData::one_shot(freq, vec![100; SAMPLES_PER_FRAME * 4]));
        let voices = vec![ToneData::new(wave, Adsr::flat())];
        Song::new(voices, tracks, tempo)
    }

    #[test]
    fn new_uses_emerald_init_defaults() {
        // `m4aSoundInit` reconfigures the driver to master volume 12 and 5
        // DirectSound channels (`m4a.c:78`..`:81`), not the generic `SoundInit`
        // placeholders (15/8).
        assert_eq!(DEFAULT_MASTER_VOLUME, 12);
        assert_eq!(DEFAULT_MAX_VOICES, 5);
        let seq = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
        assert_eq!(seq.mixer.master_volume(), 12);
        assert_eq!(seq.mixer.max_voices(), 5);
    }

    #[test]
    fn silent_song_renders_zero() {
        let song = test_song(vec![vec![Event::Fine]], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![7.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert!(seq.is_finished());
    }

    #[test]
    fn a_note_produces_sound_then_the_track_ends() {
        // VOICE 0; note key 60 vel 127 gate ~ (N04 -> 4 ticks); W48; FINE.
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 4,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);

        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        // The note is on this frame: audible output.
        assert!(out.iter().any(|&s| s.abs() > 0.0));
        assert_eq!(seq.voice_count(), 1);
    }

    #[test]
    fn gate_time_releases_the_note() {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 2,
            },
            Event::Wait(64),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        // One tick per frame at tempo 150; gate 2 releases after ~2 ticks and
        // the flat envelope (release 0) then retires the voice quickly.
        for _ in 0..8 {
            seq.render_frame(&mut out);
        }
        assert_eq!(seq.voice_count(), 0);
    }

    #[test]
    fn goto_loops_the_track_forever() {
        // A one-note loop: VOICE, [loop:] Note, Wait, Goto(loop).
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 1,
            },
            Event::Wait(24),
            Event::Goto(1),
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        for _ in 0..200 {
            seq.render_frame(&mut out);
        }
        // Never finishes: the Goto keeps re-triggering the note.
        assert!(!seq.is_finished());
    }

    #[test]
    fn faster_tempo_reaches_the_end_in_fewer_frames() {
        // The same track at double tempo crosses TEMPO_UNIT twice as often, so
        // it processes its waits — and reaches FINE — in fewer frames.
        let track = || {
            vec![
                Event::Voice(0),
                Event::Note {
                    key: 60,
                    velocity: 127,
                    gate: 8,
                },
                Event::Wait(12),
                Event::Fine,
            ]
        };
        let frames_to_finish = |tempo: u16| {
            let mut seq = Sequencer::new(test_song(vec![track()], tempo));
            let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
            let mut frames = 0;
            while !seq.is_finished() && frames < 1000 {
                seq.render_frame(&mut out);
                frames += 1;
            }
            frames
        };
        assert!(frames_to_finish(300) < frames_to_finish(150));
    }

    #[test]
    fn panned_note_is_louder_on_one_side() {
        // Hard-left pan: left channel should carry more energy than right.
        let track = vec![
            Event::Voice(0),
            Event::Pan(-64),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 8,
            },
            Event::Wait(48),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        let left: f32 = out.iter().step_by(2).map(|s| s.abs()).sum();
        let right: f32 = out.iter().skip(1).step_by(2).map(|s| s.abs()).sum();
        assert!(left > right, "left {left} should exceed right {right}");
        assert_eq!(right, 0.0);
    }

    #[test]
    fn decoded_bytes_drive_the_engine_end_to_end() {
        // Decode a real byte program and play it: VOICE 0; N04 key60 vel127;
        // W48; FINE.
        let bytes = [0xBD, 0x00, 0xD3, 60, 127, 0xB0, 0xB1];
        let events = decode_track(&bytes).unwrap();
        let song = test_song(vec![events], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out);
        assert!(out.iter().any(|&s| s.abs() > 0.0));
    }

    #[test]
    fn mix_into_renders_multiple_frames() {
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 90,
            },
            Event::Wait(96),
            Event::Fine,
        ];
        let song = test_song(vec![track], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES * 3];
        seq.mix_into(&mut out);
        assert!(out.iter().any(|&s| s.abs() > 0.0));
    }

    /// A song whose instrument reads a wave at frequency `0`, so a tied voice
    /// never advances through the sample and only stops on an explicit
    /// note-off — isolating end-of-tie behaviour from wave exhaustion.
    fn held_note_song(track: Vec<Event>) -> Song {
        let wave = Arc::new(WaveData::one_shot(0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![ToneData::new(wave, Adsr::flat())];
        Song::new(voices, vec![track], 150)
    }

    #[test]
    fn end_of_tie_without_operand_stops_only_the_last_keyed_note() {
        // Two overlapping tied notes (keys 60 then 64). An `EOT` with no
        // operand resolves to the track's current key (64, the last note),
        // retiring only that voice; the key-60 note keeps sounding. A later
        // `EOT` naming key 60 then retires the survivor — proving the omitted
        // operand resolved to 64, not 60 and not "every voice".
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Note {
                key: 64,
                velocity: 127,
                gate: 0,
            },
            Event::EndOfTie { key: None },
            Event::Wait(2),
            Event::EndOfTie { key: Some(60) },
            Event::Wait(2),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(held_note_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];

        seq.render_frame(&mut out);
        // Exactly one voice retired: the last-keyed note (64). Before the fix
        // this stopped both voices, leaving zero.
        assert_eq!(seq.voice_count(), 1);

        // Advance until the `EOT{Some(60)}` fires; the survivor was key 60, so
        // it is now retired too.
        for _ in 0..4 {
            seq.render_frame(&mut out);
        }
        assert_eq!(seq.voice_count(), 0);
    }

    #[test]
    fn fine_releases_a_tied_voice_and_the_song_finishes() {
        // A tied note (gate 0) never auto-releases; the freq-0 wave never
        // exhausts. Before the fix, FINE only marked the track ended, leaving
        // the voice sounding forever so `is_finished()` never returned true.
        let track = vec![
            Event::Voice(0),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(2),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(held_note_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        let mut frames = 0;
        while !seq.is_finished() && frames < 500 {
            seq.render_frame(&mut out);
            frames += 1;
        }
        assert!(seq.is_finished(), "FINE must release the tied voice");
    }

    #[test]
    fn volume_change_updates_a_held_notes_gains() {
        // Start a tied note at full volume, then drop VOL mid-note: the live
        // voice's base gains must follow (before the fix, note-on baked the
        // gains once and later VOL commands never touched the voice).
        let track = vec![
            Event::Voice(0),
            Event::Volume(127),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(4),
            Event::Volume(20),
            Event::Wait(4),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(held_note_song(track));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out); // tick 1: note on at full volume
        let (loud_r, loud_l) = seq.mixer.voices()[0].base_volume();
        // Advance past the mid-note `VOL 20` (fires once the first wait drains).
        for _ in 0..5 {
            seq.render_frame(&mut out);
        }
        let (soft_r, soft_l) = seq.mixer.voices()[0].base_volume();
        assert!(
            soft_r < loud_r && soft_l < loud_l,
            "VOL must lower the held note ({soft_r},{soft_l} vs {loud_r},{loud_l})",
        );
        assert!(soft_r > 0 && soft_l > 0);
    }

    #[test]
    fn bend_changes_a_held_notes_frequency() {
        // Start a tied note, then BEND up mid-note: the live voice's frequency
        // must rise (before the fix, BEND only mutated track state).
        let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; SAMPLES_PER_FRAME]));
        let voices = vec![ToneData::new(wave, Adsr::flat())];
        let track = vec![
            Event::Voice(0),
            Event::BendRange(2),
            Event::Note {
                key: 60,
                velocity: 127,
                gate: 0,
            },
            Event::Wait(4),
            Event::Bend(63),
            Event::Wait(4),
            Event::Fine,
        ];
        let mut seq = Sequencer::new(Song::new(voices, vec![track], 150));
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES];
        seq.render_frame(&mut out); // tick 1: note on, unbent
        let base_freq = seq.mixer.voices()[0].frequency();
        for _ in 0..5 {
            seq.render_frame(&mut out);
        }
        let bent_freq = seq.mixer.voices()[0].frequency();
        assert!(
            bent_freq > base_freq,
            "BEND up must raise the held note's frequency ({bent_freq} vs {base_freq})",
        );
    }

    #[test]
    fn track_pitch_key_m_stays_full_width_within_a_signed_byte() {
        // In-range key offsets pass through untouched: `keyM = x >> 8` fits a
        // signed byte, so truncation is a no-op.
        let mut track = TrackState::new();
        track.key_shift = 127; // x >> 8 == 127
        assert_eq!(track_pitch(&track).0, 127);
        track.key_shift = -128; // x >> 8 == -128
        assert_eq!(track_pitch(&track).0, -128);
    }

    #[test]
    fn track_pitch_key_m_wraps_through_a_signed_byte() {
        // KEYSH 127 with a positive bend pushes `x >> 8` to 128, one past the
        // signed-byte range. Hardware stores it in a `u8` and reads it back
        // signed, so the effective offset wraps to `(s8)128 == -128`, not +128.
        let mut track = TrackState::new();
        track.key_shift = 127;
        track.bend = 1;
        track.bend_range = 64; // bend == 64; (64*4 + 127*256) >> 8 == 128
        assert_eq!(track_pitch(&track).0, -128);
    }

    #[test]
    #[should_panic(expected = "multiple of FRAME_SAMPLES")]
    fn mix_into_rejects_partial_frames() {
        let song = test_song(vec![vec![Event::Fine]], 150);
        let mut seq = Sequencer::new(song);
        let mut out = vec![0.0; Sequencer::FRAME_SAMPLES + 1];
        seq.mix_into(&mut out);
    }
}
