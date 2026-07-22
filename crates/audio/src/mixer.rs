//! The software mixer: owns the active [`Voice`]s, sums them into interleaved
//! stereo `f32`, and clips.
//!
//! Behavioural model of `SoundMainRAM`'s outer channel loop (`m4a_1.s:153`):
//! every active voice steps its envelope once per frame, then adds its
//! contribution to the shared buffer. The hardware sums into 8-bit lanes that
//! wrap; here contributions accumulate in `i32` and are **clipped** (not
//! wrapped) to the `s8` range on the way out, then normalised to `[-1.0, 1.0)`
//! for the `f32` producer `(no-verbatim)`. Clipping rather than wrapping is a
//! deliberate, benign fidelity choice for this slice — the wrap-and-carry
//! behaviour of the packed DMA lanes is a deferred quirk.

use crate::cgb_voice::CgbVoice;
use crate::pitch::SAMPLES_PER_FRAME;
use crate::voice::{StereoAcc, Voice};

/// Default global mix level (`0..=15`). Emerald's `m4aSoundInit` reconfigures
/// the driver the instant it starts, calling `m4aSoundMode` with
/// `12 << SOUND_MODE_MASVOL_SHIFT` (`m4a.c:78`..`:81`); the generic
/// `SoundInit` placeholder of `15` (`m4a.c:383`) never reaches gameplay.
pub const DEFAULT_MASTER_VOLUME: u8 = 12;

/// Default DirectSound voice cap. The hardware allows up to
/// `MAX_DIRECTSOUND_CHANNELS` (12); Emerald's `m4aSoundInit` configures `5`
/// via `5 << SOUND_MODE_MAXCHN_SHIFT` (`m4a.c:78`..`:81`), overriding the
/// generic `SoundInit` placeholder of `8` (`m4a.c:384`). New notes past the cap
/// are dropped for this slice (priority-based voice stealing is out of scope).
pub const DEFAULT_MAX_VOICES: usize = 5;

/// Owns the playing voices and renders them to interleaved stereo `f32`.
#[derive(Debug)]
pub struct Mixer {
    voices: Vec<Voice>,
    /// The four fixed CGB PSG hardware channels (square 1, square 2, wave,
    /// noise — indexed by [`crate::cgb_voice::CgbChannelNumber::slot`]).
    /// Unlike the pooled DirectSound voices, each of these exists at most
    /// once: starting a new note on the same channel number replaces
    /// whatever was already sounding there, mirroring `CgbSound`'s loop over
    /// fixed channel slots (`m4a.c:946`).
    cgb_voices: [Option<CgbVoice>; 4],
    master_volume: u8,
    max_voices: usize,
    /// Reusable per-frame accumulator, sized to [`SAMPLES_PER_FRAME`], so
    /// steady-state rendering does not allocate.
    scratch: Vec<StereoAcc>,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new(DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES)
    }
}

impl Mixer {
    /// A mixer with an explicit master volume and voice cap.
    #[must_use]
    pub fn new(master_volume: u8, max_voices: usize) -> Self {
        Self {
            voices: Vec::new(),
            cgb_voices: [None, None, None, None],
            master_volume,
            max_voices,
            scratch: vec![(0, 0); SAMPLES_PER_FRAME],
        }
    }

    /// Number of voices currently sounding, DirectSound and CGB combined.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.voices.len() + self.cgb_voices.iter().filter(|v| v.is_some()).count()
    }

    /// Whether any voice (DirectSound or CGB) is active.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.voices.is_empty() && self.cgb_voices.iter().all(Option::is_none)
    }

    /// Read-only view of the live voices, for the sequencer's own tests to
    /// inspect mid-note volume/pitch updates.
    #[cfg(test)]
    pub(crate) fn voices(&self) -> &[Voice] {
        &self.voices
    }

    /// The global mix level.
    #[must_use]
    pub fn master_volume(&self) -> u8 {
        self.master_volume
    }

    /// The DirectSound voice cap.
    #[must_use]
    pub fn max_voices(&self) -> usize {
        self.max_voices
    }

    /// Add a voice, honouring the voice cap. Returns `false` (dropping the
    /// voice) when already at capacity.
    pub fn add_voice(&mut self, voice: Voice) -> bool {
        if self.voices.len() >= self.max_voices {
            return false;
        }
        self.voices.push(voice);
        true
    }

    /// Start a CGB voice, replacing whatever already occupied that hardware
    /// channel slot (there is no pool to cap — see [`Self::cgb_voices`]'s
    /// doc comment).
    pub fn add_cgb_voice(&mut self, voice: CgbVoice) {
        let slot = voice.channel().slot();
        self.cgb_voices[slot] = Some(voice);
    }

    /// Tick every voice's note-off gate down by one sequencer tick.
    pub fn tick_gates(&mut self) {
        for voice in &mut self.voices {
            voice.tick_gate();
        }
        for voice in self.cgb_voices.iter_mut().flatten() {
            voice.tick_gate();
        }
    }

    /// Release the *newest* still-sounding voice on `track` whose MIDI key is
    /// `key` (end-of-tie), then stop.
    ///
    /// This mirrors `ply_endtie` (`m4a_1.s:1819`): it walks the track's channel
    /// chain from the head and stops the first channel matching `track->key`.
    /// Note-on prepends each new channel at the head of that chain (`ply_note`,
    /// `m4a_1.s:1724`), so "first in the chain" is the most recently started
    /// voice. We store voices oldest-first, so the scan runs in reverse. A track
    /// can hold several voices with the same key (overlapping ties); an `EOT`
    /// retires exactly one — the newest.
    pub fn note_off_track(&mut self, track: usize, key: u8) {
        for voice in self.voices.iter_mut().rev() {
            if voice.track() == track && !voice.is_stopping() && voice.midi_key() == key {
                voice.note_off();
                return;
            }
        }
        // CGB channels are not pooled (at most one per hardware channel
        // number), so at most one can ever match here.
        for voice in self.cgb_voices.iter_mut().flatten() {
            if voice.track() == track && !voice.is_stopping() && voice.midi_key() == key {
                voice.note_off();
                return;
            }
        }
    }

    /// Release every still-sounding voice on `track` (track-end / `FINE`).
    ///
    /// Mirrors `ply_fine` (`m4a_1.s:750`): it walks the track's channel chain
    /// and sets `SOUND_CHANNEL_SF_STOP` on every channel still flagged
    /// `SOUND_CHANNEL_SF_ON`, releasing it, before the track is cleared. Without
    /// this a track owning a tied voice (`gate_time == 0`, never auto-releasing)
    /// or a looping wave would leave that voice sounding forever.
    pub fn release_track(&mut self, track: usize) {
        for voice in &mut self.voices {
            if voice.track() == track && !voice.is_stopping() {
                voice.note_off();
            }
        }
        for voice in self.cgb_voices.iter_mut().flatten() {
            if voice.track() == track && !voice.is_stopping() {
                voice.note_off();
            }
        }
    }

    /// Rewrite the live base volumes of `track`'s channels from updated track
    /// `volMR`/`volML` (`MPT_FLG_VOLCHG`).
    ///
    /// Mirrors `MPlayMain`'s per-tick re-run of `ChnVolSetAsm` for each ON
    /// channel of a flagged track (`m4a_1.s:1394`): each voice keeps its own
    /// velocity, so a mid-note `VOL`/`PAN` change is audible on the held note.
    pub fn set_track_volume(&mut self, track: usize, vol_mr: u8, vol_ml: u8) {
        for voice in &mut self.voices {
            if voice.track() == track {
                voice.set_track_volume(vol_mr, vol_ml);
            }
        }
        for voice in self.cgb_voices.iter_mut().flatten() {
            if voice.track() == track {
                voice.set_track_volume(vol_mr, vol_ml);
            }
        }
    }

    /// Rewrite the live playback frequency of `track`'s channels from updated
    /// track `keyM`/`pitM` (`MPT_FLG_PITCHG`).
    ///
    /// Mirrors `MPlayMain`'s per-tick pitch re-run for each ON channel of a
    /// flagged track (`m4a_1.s:1403`..`:1451`): it recomputes the played
    /// frequency from the channel's stored key plus the track's updated key/fine
    /// offset via `MidiKeyToFreq`, so a mid-note `BEND`/`BENDR`/`KEYSH`/`TUNE`
    /// bends the held note.
    pub fn set_track_pitch(&mut self, track: usize, key_m: i32, pit_m: u8) {
        for voice in &mut self.voices {
            if voice.track() == track {
                voice.set_track_pitch(key_m, pit_m);
            }
        }
        for voice in self.cgb_voices.iter_mut().flatten() {
            if voice.track() == track {
                voice.set_track_pitch(key_m, pit_m);
            }
        }
    }

    /// Render exactly one frame ([`SAMPLES_PER_FRAME`] stereo samples) into
    /// `out`, which must hold `SAMPLES_PER_FRAME * 2` interleaved `f32`s
    /// (`[l, r, l, r, …]`). Retires voices that fell silent.
    ///
    /// # Panics
    ///
    /// Panics if `out.len() != SAMPLES_PER_FRAME * 2`.
    pub fn mix_frame(&mut self, out: &mut [f32]) {
        assert_eq!(
            out.len(),
            SAMPLES_PER_FRAME * 2,
            "mix_frame expects one frame of interleaved stereo",
        );

        for acc in &mut self.scratch {
            *acc = (0, 0);
        }
        for voice in &mut self.voices {
            voice.begin_frame(self.master_volume);
            voice.render(&mut self.scratch);
        }
        self.voices.retain(Voice::is_active);

        for slot in &mut self.cgb_voices {
            if let Some(voice) = slot {
                voice.begin_frame(self.master_volume);
                voice.render(&mut self.scratch);
                if !voice.is_active() {
                    *slot = None;
                }
            }
        }

        for (frame, acc) in self.scratch.iter().enumerate() {
            out[frame * 2] = clip(acc.0);
            out[frame * 2 + 1] = clip(acc.1);
        }
    }
}

/// Clip a summed accumulator to the `s8` range and normalise to `[-1.0, 1.0)`.
fn clip(sample: i32) -> f32 {
    // Clamped to `[-128, 127]`, every value is exactly representable in `f32`.
    #[allow(clippy::cast_precision_loss)]
    let clamped = sample.clamp(-128, 127) as f32;
    clamped / 128.0
}

#[cfg(test)]
// Expected values are computed from small integer terms and compared with an
// epsilon; the casts are exact for these magnitudes. Silence checks compare
// exactly-representable `0.0`/`-1.0` values on purpose.
#[allow(clippy::cast_precision_loss, clippy::float_cmp)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::envelope::Adsr;
    use crate::pitch::{DIV_FREQ, FRAC_BITS};
    use crate::sample::WaveData;

    fn unity_freq() -> u32 {
        (1 << FRAC_BITS) / DIV_FREQ
    }

    fn constant_voice(level: i8, track: usize) -> Voice {
        keyed_voice(level, track, 60, 0xFF, 0xFF)
    }

    fn keyed_voice(level: i8, track: usize, key: u8, right: u8, left: u8) -> Voice {
        // A long constant wave so a whole frame renders without ending; a `0`
        // gate makes it tied (it only stops on an explicit note-off).
        let data = vec![level; SAMPLES_PER_FRAME + 4];
        let wave = Arc::new(WaveData::one_shot(0, data));
        Voice::new(
            wave,
            Adsr::flat(),
            unity_freq(),
            right,
            left,
            127,
            0,
            key,
            track,
            0,
            0,
        )
    }

    #[test]
    fn empty_mixer_renders_silence() {
        let mut mixer = Mixer::default();
        let mut out = vec![9.0; SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert!(mixer.is_idle());
    }

    #[test]
    fn single_voice_is_scaled_and_normalised() {
        // Pin master volume to 15 so the documented `254` env gain holds; this
        // test exercises the mixing math, not the Emerald default (12).
        let mut mixer = Mixer::new(15, DEFAULT_MAX_VOICES);
        mixer.add_voice(constant_voice(50, 0));
        let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        // env gain 254, contribution (254*50)>>8 = 49, /128.
        let expected = (((254 * 50) >> 8) as f32) / 128.0;
        assert!((out[0] - expected).abs() < 1e-6);
    }

    #[test]
    fn two_voices_sum() {
        let mut mixer = Mixer::new(15, DEFAULT_MAX_VOICES);
        mixer.add_voice(constant_voice(40, 0));
        mixer.add_voice(constant_voice(30, 1));
        assert_eq!(mixer.voice_count(), 2);
        let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        let a = (254 * 40) >> 8;
        let b = (254 * 30) >> 8;
        let expected = ((a + b) as f32) / 128.0;
        assert!((out[0] - expected).abs() < 1e-6);
    }

    #[test]
    fn loud_sum_clips_to_full_scale() {
        let mut mixer = Mixer::new(15, DEFAULT_MAX_VOICES);
        // Four hard-driven voices sum past the s8 range and must clip.
        for track in 0..4 {
            mixer.add_voice(constant_voice(127, track));
        }
        let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        // Each contributes (254*127)>>8 = 125; 4*125 = 500 clips to 127.
        assert!((out[0] - (127.0 / 128.0)).abs() < 1e-6);
    }

    #[test]
    fn negative_sum_clips_to_minus_one() {
        let mut mixer = Mixer::new(15, DEFAULT_MAX_VOICES);
        for track in 0..4 {
            mixer.add_voice(constant_voice(-128, track));
        }
        let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        assert!((out[0] - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn note_off_track_stops_only_the_first_matching_voice() {
        // Two overlapping voices share key 60 on track 0; a third holds key 64.
        // `EOT` retires exactly one key-60 voice, leaving the other still
        // sounding (mirrors `ply_endtie`'s break-on-first-match).
        let mut mixer = Mixer::default();
        mixer.add_voice(keyed_voice(50, 0, 60, 0xFF, 0xFF));
        mixer.add_voice(keyed_voice(50, 0, 60, 0xFF, 0xFF));
        mixer.add_voice(keyed_voice(50, 0, 64, 0xFF, 0xFF));
        mixer.note_off_track(0, 60);
        let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        // Only the first key-60 voice was released and retired.
        assert_eq!(mixer.voice_count(), 2);
    }

    #[test]
    fn note_off_track_matches_the_requested_key() {
        // Key 60 is panned hard-left, key 64 hard-right, both on track 0. An
        // `EOT` on key 64 stops only that voice, silencing the right channel
        // while the left keeps sounding.
        let mut mixer = Mixer::default();
        mixer.add_voice(keyed_voice(60, 0, 60, 0x00, 0xFF));
        mixer.add_voice(keyed_voice(60, 0, 64, 0xFF, 0x00));
        mixer.note_off_track(0, 64);
        let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        let left: f32 = out.iter().step_by(2).map(|s| s.abs()).sum();
        let right: f32 = out.iter().skip(1).step_by(2).map(|s| s.abs()).sum();
        assert_eq!(mixer.voice_count(), 1);
        assert!(left > 0.0, "surviving key-60 voice should keep sounding");
        assert_eq!(right, 0.0, "released key-64 voice should be silent");
    }

    #[test]
    fn note_off_track_releases_the_newest_matching_voice() {
        // Upstream `ply_note` prepends each new channel at the head of the
        // track's chain and `ply_endtie` stops the first match — the newest
        // voice. Two key-60 voices overlap on track 0: the older is panned
        // hard-left, the newer hard-right. An `EOT` must retire the newer
        // (right) voice, leaving the left one sounding. Before the fix the scan
        // ran oldest-first and silenced the left channel instead.
        let mut mixer = Mixer::default();
        mixer.add_voice(keyed_voice(60, 0, 60, 0x00, 0xFF)); // older: left only
        mixer.add_voice(keyed_voice(60, 0, 60, 0xFF, 0x00)); // newer: right only
        mixer.note_off_track(0, 60);
        let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        let left: f32 = out.iter().step_by(2).map(|s| s.abs()).sum();
        let right: f32 = out.iter().skip(1).step_by(2).map(|s| s.abs()).sum();
        assert_eq!(mixer.voice_count(), 1);
        assert!(left > 0.0, "older left voice should keep sounding");
        assert_eq!(right, 0.0, "newer right voice should be released");
    }

    #[test]
    fn voice_cap_drops_extra_notes() {
        let mut mixer = Mixer::new(DEFAULT_MASTER_VOLUME, 2);
        assert!(mixer.add_voice(constant_voice(10, 0)));
        assert!(mixer.add_voice(constant_voice(10, 1)));
        assert!(!mixer.add_voice(constant_voice(10, 2)));
        assert_eq!(mixer.voice_count(), 2);
    }
}
