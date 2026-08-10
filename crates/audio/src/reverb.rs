//! The master-mix reverb/pseudo-echo stage (S-3, issue #185): a feedback
//! delay line seeding [`crate::mixer::Mixer`]'s per-frame accumulator before
//! voices mix additively on top of it.
//!
//! # Upstream behaviour
//!
//! Behavioural port of `SoundMainRAM`'s reverb arm
//! (`pokeemerald/src/m4a_1.s:88`-`:119`, `SoundMainRAM_Reverb`): when a
//! song's `SongHeader::reverb` is nonzero, the mixer does **not** clear its
//! output buffer before mixing a frame's channels. Instead, for every
//! sample position it reads the stereo bytes still sitting in the buffer
//! from a past frame — one tap at the position about to be overwritten,
//! and a second tap `pcmSamplesPerVBlank` samples away (or, once every
//! `pcmDmaPeriod`-th V-blank, the buffer's absolute start, an
//! address-alignment correction for the hardware's ping-pong DMA buffer,
//! `m4a_1.s:90`-`:100`) — sums all four bytes (both taps, both channels),
//! multiplies by `reverb`, and shifts right 9 (`/512`), writing the
//! **same** result into both the left and right byte for that position.
//! Channel mixing then adds each voice's own panned contribution on top
//! (`SoundMainRAM_ChanLoop`, `m4a_1.s:153`), so what plays is wet (reverb)
//! plus dry (voices), and — because the wet term is derived from a past
//! frame's own wet+dry sum — the effect compounds into a decaying echo.
//!
//! `reverb == 0` skips all of this and zero-fills the buffer instead
//! (`SoundMainRAM_NoReverb`), i.e. reverb is off.
//!
//! # Deliberate simplification `(behavioral-fidelity, no-verbatim)`
//!
//! This module does not reproduce the hardware ping-pong DMA buffer's exact
//! address geometry — `(behavioral-fidelity)` explicitly does not chase
//! byte-for-byte hardware parity, only the player-audible result. The two
//! upstream taps differ by at most `pcmSamplesPerVBlank` samples (~17 ms) out
//! of a buffer that is otherwise [`DELAY_SAMPLES`] (~118 ms) long, and the
//! rare buffer-start correction exists purely to keep the *hardware's*
//! address arithmetic from drifting off the end of its physical ring — not
//! an audible feature in its own right. [`Reverb`] collapses both taps to a
//! single read at the same delay-line position (equivalent to reading that
//! position twice), which folds upstream's `sum-of-four-bytes >> 9` into
//! `sum-of-two-bytes >> 8` (see [`wet_sample`]) — the same gain curve and
//! the same ~118 ms echo spacing, reimplemented idiomatically rather than
//! transliterated `(no-verbatim)`.
//!
//! # Domain
//!
//! [`crate::mixer::Mixer`] already accumulates DirectSound/CGB voice output
//! in `i32` and **clips** (rather than wraps) to the `s8` range before
//! normalising to `f32` — a documented, deliberate departure from the
//! hardware's byte-wrapping accumulator (see `crate::mixer`'s module docs).
//! [`Reverb`] operates in that same clipped `i32` domain: its delay line
//! stores each frame's final, clipped `s8`-range samples (what upstream's
//! `pcmBuffer` would hold once mixing finished), and [`Reverb::seed_frame`]
//! seeds the *next* frame's accumulator from them before voices mix in.

use crate::voice::StereoAcc;

/// The upstream buffer length a reverb tap wraps around, in samples
/// (`PCM_DMA_BUF_SIZE`, `pokeemerald/include/gba/m4a_internal.h:169`). At
/// [`crate::pitch::MIXER_RATE`] (13379 Hz) this is a little under 118.4 ms —
/// the echo's audible repeat interval.
pub(crate) const DELAY_SAMPLES: usize = 1584;

/// The master-mix feedback delay line. See the module docs.
#[derive(Clone, Debug)]
pub(crate) struct Reverb {
    /// Circular buffer of past frames' final, clipped `(left, right)`
    /// samples, `s8`-range. Read by [`Self::seed_frame`], written by
    /// [`Self::commit_frame`].
    delay: Box<[StereoAcc]>,
    /// The next sample index [`Self::seed_frame`]/[`Self::commit_frame`]
    /// will touch, wrapping at [`DELAY_SAMPLES`].
    cursor: usize,
    /// `SongHeader::reverb`, `0..=127` (`SOUND_MODE_REVERB_VAL`,
    /// `m4a_internal.h:12`). `0` disables the effect entirely (module docs).
    level: u8,
}

impl Reverb {
    /// A reverb stage at `level` (`0` disables it). The delay line starts
    /// silent, matching `CpuFill32(0, soundInfo, ...)` zeroing `pcmBuffer`
    /// at `SoundInit` (`m4a.c:381`).
    pub(crate) fn new(level: u8) -> Self {
        Self {
            delay: vec![(0, 0); DELAY_SAMPLES].into_boxed_slice(),
            cursor: 0,
            level,
        }
    }

    /// Whether this stage does anything (`level != 0`).
    pub(crate) fn is_enabled(&self) -> bool {
        self.level != 0
    }

    /// Seed one frame's mixer accumulator with this stage's wet
    /// contribution, one delay-line position per output sample — the same
    /// per-sample granularity `SoundMainRAM_Reverb` runs at, not a single
    /// per-frame scalar. Voice mixing then adds its own contribution on top
    /// of what this writes (module docs).
    ///
    /// When disabled, this is exactly the all-zero reset the mixer used to
    /// perform inline (`SoundMainRAM_NoReverb`): every existing test built
    /// against the pre-reverb mixer stays byte-identical.
    pub(crate) fn seed_frame(&self, scratch: &mut [StereoAcc]) {
        if !self.is_enabled() {
            for acc in scratch.iter_mut() {
                *acc = (0, 0);
            }
            return;
        }
        let level = i32::from(self.level);
        for (i, acc) in scratch.iter_mut().enumerate() {
            let (l, r) = self.delay[(self.cursor + i) % DELAY_SAMPLES];
            let wet = wet_sample(l, r, level);
            *acc = (wet, wet);
        }
    }

    /// After voice mixing and clipping, store this frame's final `s8`-range
    /// samples back into the delay line for a future frame's taps, and
    /// advance the cursor by the frame length.
    ///
    /// `clipped` must be the same length [`Self::seed_frame`] was just
    /// called with (a whole frame); the two share the same
    /// index-to-delay-line-position mapping.
    pub(crate) fn commit_frame(&mut self, clipped: &[StereoAcc]) {
        for (i, &sample) in clipped.iter().enumerate() {
            self.delay[(self.cursor + i) % DELAY_SAMPLES] = sample;
        }
        self.cursor = (self.cursor + clipped.len()) % DELAY_SAMPLES;
    }
}

/// One tap's contribution: `(left + right) * level >> 8`.
///
/// Upstream's own formula sums two stereo taps (four `s8` bytes) and shifts
/// right 9 (module docs). This module's single-tap simplification reads the
/// same position twice, i.e. `(l + r + l + r) * level >> 9`, which is
/// exactly `(l + r) * level >> 8` — the shift folds the doubled tap in
/// rather than needing a separate `* 2`.
fn wet_sample(left: i32, right: i32, level: i32) -> i32 {
    ((left + right) * level) >> 8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disabled stage must always seed silence, regardless of whatever a
    /// caller previously committed — the pre-reverb mixer's own reset
    /// behaviour (module docs), pinned so a future default-level regression
    /// cannot quietly turn reverb on for every existing song.
    #[test]
    fn disabled_reverb_always_seeds_silence() {
        let mut reverb = Reverb::new(0);
        let mut scratch = [(9, -9); 4];
        reverb.seed_frame(&mut scratch);
        assert_eq!(scratch, [(0, 0); 4]);

        // Committing loud content must not matter either -- disabled means
        // disabled, not "disabled until something is written".
        reverb.commit_frame(&[(120, -120); 4]);
        let mut scratch2 = [(9, -9); 4];
        reverb.seed_frame(&mut scratch2);
        assert_eq!(scratch2, [(0, 0); 4]);
    }

    /// [`wet_sample`]'s formula, pinned against hand-computed values so the
    /// `(l + r) * level >> 8` shift can never silently drift (e.g. to the
    /// unfolded `>> 9` upstream's own two-tap sum uses, which would halve
    /// every echo).
    #[test]
    fn wet_sample_matches_the_documented_formula() {
        // (100 + -60) * 50 = 2000; 2000 >> 8 = 7.
        assert_eq!(wet_sample(100, -60, 50), 7);
        // A zero level must silence everything, whatever the taps are.
        assert_eq!(wet_sample(127, 127, 0), 0);
        // Negative sums must shift arithmetically (toward negative
        // infinity), not wrap.
        assert_eq!(wet_sample(-100, -100, 50), (-200 * 50) >> 8);
        assert!(wet_sample(-100, -100, 50) < 0);
    }

    /// A one-shot impulse resurfaces as a mono-folded, scaled echo exactly
    /// one [`DELAY_SAMPLES`] later -- the ~118 ms repeat the module docs
    /// describe -- and nowhere else in between.
    #[test]
    fn an_impulse_echoes_after_exactly_one_delay_length() {
        let mut reverb = Reverb::new(50);
        let mut seed = [(0, 0)];

        // Nothing committed yet: silence.
        reverb.seed_frame(&mut seed);
        assert_eq!(seed, [(0, 0)]);

        // Commit a single loud stereo impulse at position 0.
        reverb.commit_frame(&[(100, -60)]);

        // Silence for every position in between.
        for i in 1..DELAY_SAMPLES {
            reverb.seed_frame(&mut seed);
            assert_eq!(seed, [(0, 0)], "unexpected echo at offset {i}");
            reverb.commit_frame(&[(0, 0)]);
        }

        // Exactly one delay length later, the impulse reappears as the
        // documented mono-folded, scaled echo.
        reverb.seed_frame(&mut seed);
        let expected = wet_sample(100, -60, 50);
        assert_eq!(seed, [(expected, expected)]);
        assert_ne!(expected, 0, "the echo must actually be audible");
    }

    /// Multi-sample frames map linearly onto consecutive delay-line
    /// positions (not all onto one position) -- the seed/commit index
    /// mapping [`an_impulse_echoes_after_exactly_one_delay_length`] pins one
    /// sample at a time.
    #[test]
    fn a_whole_frame_maps_onto_consecutive_delay_positions() {
        let mut reverb = Reverb::new(80);
        let burst: Vec<StereoAcc> = (0..16).map(|i| (i * 2, i * 3)).collect();
        reverb.commit_frame(&burst);

        // Skip ahead by the rest of the delay length minus this burst.
        let mut silence = vec![(0, 0); DELAY_SAMPLES - burst.len()];
        let mut scratch = silence.clone();
        reverb.seed_frame(&mut scratch);
        reverb.commit_frame(&silence);
        silence.clear(); // silence checked above; avoid an unused warning.

        let mut echoed = vec![(0, 0); burst.len()];
        reverb.seed_frame(&mut echoed);
        let expected: Vec<StereoAcc> = burst
            .iter()
            .map(|&(l, r)| {
                let wet = wet_sample(l, r, 80);
                (wet, wet)
            })
            .collect();
        assert_eq!(echoed, expected);
    }

    /// Documented spectral/RMS tolerance (Discussion #227's owner decision):
    /// once the delay line holds only mono (`left == right`) feedback
    /// content -- which every position does after one full echo cycle,
    /// since [`Self::seed_frame`] always writes equal left/right -- each
    /// further uncontested cycle's total energy should scale by very close
    /// to `level / 128` (the module docs' folded-tap gain). Measured as an
    /// RMS ratio (not a byte-exact comparison) because `>> 8` integer
    /// truncation perturbs individual samples by up to one LSB; **5%** is
    /// the documented tolerance -- derived here as roughly 2x the worst-case
    /// per-sample truncation error (~1/60, given this test's ~60-amplitude
    /// synthetic waveform) after RMS-averaging across a full
    /// [`DELAY_SAMPLES`] window, with headroom.
    #[test]
    fn feedback_energy_decays_at_the_documented_rms_ratio_within_tolerance() {
        const LEVEL: u8 = 100;
        let mut reverb = Reverb::new(LEVEL);

        // A full delay-length burst of a small alternating synthetic
        // waveform (not silence, not a pure DC impulse) -- already mono
        // (`left == right`) so every subsequent cycle's ratio is directly
        // comparable, matching how a real post-reverb signal is mono once
        // it has been through the wet path once (module docs).
        let burst: Vec<StereoAcc> = (0..DELAY_SAMPLES)
            .map(|i| {
                let v = 60 - i32::try_from(i % 8).expect("< 8") * 15;
                (v, v)
            })
            .collect();
        let mut scratch = vec![(0, 0); DELAY_SAMPLES];
        reverb.seed_frame(&mut scratch); // nothing committed yet: silence
        reverb.commit_frame(&burst);

        let rms = |xs: &[StereoAcc]| -> f64 {
            let sum_sq: f64 = xs.iter().map(|&(l, _)| f64::from(l) * f64::from(l)).sum();
            // `xs.len()` is `DELAY_SAMPLES` (1584): far below `f64`'s 52-bit
            // mantissa, so this loses no precision.
            #[allow(clippy::cast_precision_loss)]
            let len = xs.len() as f64;
            (sum_sq / len).sqrt()
        };

        let expected_ratio = f64::from(LEVEL) / 128.0;
        let mut previous_rms = rms(&burst);
        for cycle in 0..4 {
            reverb.seed_frame(&mut scratch);
            // No further dry input this cycle: whatever reverb seeded is
            // exactly what a silent mixer would clip and commit (these
            // magnitudes stay far inside the s8 clip range).
            reverb.commit_frame(&scratch);
            let cycle_rms = rms(&scratch);
            let ratio = cycle_rms / previous_rms;
            assert!(
                (ratio - expected_ratio).abs() < 0.05,
                "cycle {cycle}: RMS ratio {ratio:.4} outside the documented 5% tolerance of \
                 {expected_ratio:.4}"
            );
            previous_rms = cycle_rms;
        }
    }
}
