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

use crate::pitch::SAMPLES_PER_FRAME;
use crate::voice::{StereoAcc, Voice};

/// Default global mix level (`0..=15`); upstream's max master volume.
pub const DEFAULT_MASTER_VOLUME: u8 = 15;

/// Default DirectSound voice cap. The hardware allows up to
/// `MAX_DIRECTSOUND_CHANNELS` (12); games configure fewer. New notes past the
/// cap are dropped for this slice (priority-based voice stealing is out of
/// scope).
pub const DEFAULT_MAX_VOICES: usize = 8;

/// Owns the playing voices and renders them to interleaved stereo `f32`.
#[derive(Debug)]
pub struct Mixer {
    voices: Vec<Voice>,
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
            master_volume,
            max_voices,
            scratch: vec![(0, 0); SAMPLES_PER_FRAME],
        }
    }

    /// Number of voices currently sounding.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// Whether any voice is active.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.voices.is_empty()
    }

    /// The global mix level.
    #[must_use]
    pub fn master_volume(&self) -> u8 {
        self.master_volume
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

    /// Tick every voice's note-off gate down by one sequencer tick.
    pub fn tick_gates(&mut self) {
        for voice in &mut self.voices {
            voice.tick_gate();
        }
    }

    /// Release the *first* still-sounding voice on `track` whose MIDI key is
    /// `key` (end-of-tie), then stop.
    ///
    /// This mirrors `ply_endtie` (`m4a_1.s:1837`): it walks the track's channel
    /// list and stops only the first channel matching `track->key`, breaking out
    /// of the loop on the first hit. A track can hold several voices with the
    /// same key (overlapping ties), and an `EOT` retires just one of them.
    pub fn note_off_track(&mut self, track: usize, key: u8) {
        for voice in &mut self.voices {
            if voice.track() == track && !voice.is_stopping() && voice.midi_key() == key {
                voice.note_off();
                break;
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
        let mut mixer = Mixer::default();
        mixer.add_voice(constant_voice(50, 0));
        let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
        mixer.mix_frame(&mut out);
        // env gain 254, contribution (254*50)>>8 = 49, /128.
        let expected = (((254 * 50) >> 8) as f32) / 128.0;
        assert!((out[0] - expected).abs() < 1e-6);
    }

    #[test]
    fn two_voices_sum() {
        let mut mixer = Mixer::default();
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
        let mut mixer = Mixer::default();
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
        let mut mixer = Mixer::default();
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
    fn voice_cap_drops_extra_notes() {
        let mut mixer = Mixer::new(DEFAULT_MASTER_VOLUME, 2);
        assert!(mixer.add_voice(constant_voice(10, 0)));
        assert!(mixer.add_voice(constant_voice(10, 1)));
        assert!(!mixer.add_voice(constant_voice(10, 2)));
        assert_eq!(mixer.voice_count(), 2);
    }
}
