//! The master-mix reverb/pseudo-echo stage (S-3, issue #185): a **two-tap
//! feedback comb filter** seeding [`crate::mixer::Mixer`]'s per-frame
//! accumulator before voices mix additively on top of it.
//!
//! # Upstream behaviour
//!
//! Behavioural port of `SoundMainRAM`'s reverb arm
//! (`pokeemerald/src/m4a_1.s:88`-`:119`, `SoundMainRAM_Reverb`), together
//! with the write-pointer arithmetic `SoundMain` hands it
//! (`m4a_1.s:61`-`:72`).
//!
//! ## Where the two taps come from
//!
//! `SoundMain` picks the buffer slot this V-blank is about to fill:
//! `r5 = pcmBuffer + (pcmDmaPeriod - (pcmDmaCounter - 1)) * pcmSamplesPerVBlank`
//! (`m4a_1.s:64`-`:72`). Consecutive V-blanks therefore walk `pcmDmaPeriod`
//! consecutive `pcmSamplesPerVBlank`-sample slots and then wrap — a ring of
//! `pcmDmaPeriod * pcmSamplesPerVBlank` samples, i.e. [`DELAY_SAMPLES`].
//!
//! `SoundMainRAM_Reverb` then reads **two** taps per sample position
//! (`m4a_1.s:97`-`:107`):
//!
//! - **tap A** at `r5` itself — the slot about to be overwritten, holding
//!   what was written one whole ring cycle ago, i.e. [`DELAY_SAMPLES`]
//!   (1568) samples back;
//! - **tap B** at `r7 = r5 + pcmSamplesPerVBlank` — the *next* slot, one
//!   frame further round the ring and therefore
//!   `DELAY_SAMPLES - SAMPLES_PER_FRAME` (1344) samples back. On the
//!   `pcmDmaCounter == 2` V-blank that address would run past the ring's
//!   end, so `r7` is reset to `pcmBuffer` (`m4a_1.s:97`-`:98`) — the ring's
//!   own wrap, not a separate feature.
//!
//! Both taps are stereo (the right channel lives `PCM_DMA_BUF_SIZE` bytes
//! further on, `ldrsb r0, [r5, r6]`), so the loop sums **four** `s8` bytes,
//! multiplies by `SongHeader::reverb`, arithmetic-shifts right 9 (`/512`),
//! applies the round-toward-zero correction below, and stores the **same**
//! byte into both the left and right channel for that position
//! (`m4a_1.s:102`-`:114`).
//!
//! Channel mixing then adds each voice's own panned contribution on top
//! (`SoundMainRAM_ChanLoop`, `m4a_1.s:153`), so what plays is wet (reverb)
//! plus dry (voices) — and because the wet term is derived from past frames'
//! own wet+dry sums, it recirculates.
//!
//! `reverb == 0` skips all of this and zero-fills the buffer instead
//! (`SoundMainRAM_NoReverb`, `m4a_1.s:120`), i.e. reverb is off.
//!
//! ## Why both taps matter
//!
//! The two taps are 1568 and 1344 samples deep — [`SAMPLES_PER_FRAME`] (224)
//! apart — and they sit *inside* the feedback loop. Summing them is a comb
//! filter: content whose period divides 224 samples arrives at both taps in
//! phase and is reinforced (response peaks at multiples of
//! `MIXER_RATE / 224` ~= 59.7 Hz), while content halfway between two peaks
//! arrives in antiphase and cancels outright (nulls every ~59.7 Hz too,
//! interleaved with the peaks, the first at ~29.9 Hz). That
//! frequency-dependent decay — some pitches ringing on, others swallowed
//! immediately — is the audible signature of this stage, so [`Reverb`]
//! models both taps rather than collapsing them into one.
//!
//! ## The round-toward-zero correction
//!
//! `asr 9` rounds toward negative infinity, which would leave every negative
//! wet sample one LSB too low; recirculated, that accumulates into a
//! standing DC offset. Upstream corrects it with
//! `tst r0, 0x80 / addne r0, r0, 1` (`m4a_1.s:111`-`:112`): the shifted
//! result is about to be stored as a signed byte, so bit 7 of its low byte
//! being set means "negative", and such results get `+1`.
//! [`wet_sample`] reproduces that test bit-for-bit rather than reaching for
//! a generic `div_euclid`-style rounding helper, because the two disagree on
//! exact multiples of 512 (upstream biases those toward zero as well) and
//! `(behavioral-fidelity)` follows upstream, not the tidier rule.
//!
//! # Deliberate simplification `(behavioral-fidelity, no-verbatim)`
//!
//! This module does not reproduce the hardware ping-pong DMA buffer's exact
//! byte geometry — `(behavioral-fidelity)` explicitly does not chase
//! byte-for-byte hardware parity, only the player-audible result. It models
//! the ring as [`DELAY_SAMPLES`] stereo sample slots rather than two
//! `PCM_DMA_BUF_SIZE`-byte mono halves, and derives the wrap from
//! `% DELAY_SAMPLES` rather than from a `pcmDmaCounter` countdown — the same
//! tap delays, reimplemented idiomatically instead of transliterated
//! `(no-verbatim)`.
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
//! Note that this puts the clip *inside* the feedback loop: where the
//! pre-reverb mixer clipped once per frame on the way out, a clipped sample
//! now recirculates and is re-read by both taps, so a sustained overload
//! decays as clipped content rather than as the (wrapped) values hardware
//! would have fed back. That is the same trade-off `crate::mixer`'s clip
//! already made, now audible for one more frame's worth of history.

use crate::pitch::SAMPLES_PER_FRAME;
use crate::voice::StereoAcc;

/// How many `SAMPLES_PER_FRAME`-sized slots the reverb ring holds:
/// `pcmDmaPeriod = PCM_DMA_BUF_SIZE / pcmSamplesPerVBlank = 1584 / 224 = 7`
/// (`pokeemerald/src/m4a.c:407`).
const RING_FRAMES: usize = 7;

/// The reverb ring's length in samples: `pcmDmaPeriod * pcmSamplesPerVBlank`
/// `= 7 * 224 = 1568` — the delay of tap A (module docs).
///
/// Deliberately **not** `PCM_DMA_BUF_SIZE` (1584,
/// `pokeemerald/include/gba/m4a_internal.h:169`): `pcmDmaPeriod` is a
/// truncating division (`m4a.c:407`), so the last 1584 - 1568 = 16 samples of
/// the physical buffer are never the start of a V-blank's slot and the write
/// pointer wraps at 1568. 1568 is also an exact multiple of
/// [`SAMPLES_PER_FRAME`], which is what makes a whole number of mixer frames
/// fit the ring.
///
/// At [`crate::pitch::MIXER_RATE`] (13379 Hz) this is a little under
/// 117.2 ms — the longer of the two taps' repeat intervals.
pub(crate) const DELAY_SAMPLES: usize = RING_FRAMES * SAMPLES_PER_FRAME;

/// Tap B's delay, in samples: one mixer frame shallower than tap A (module
/// docs), i.e. 1344 samples. Expressed as the *forward* offset applied to a
/// ring index, which is how [`Reverb::seed_frame`] reads it.
const TAP_B_OFFSET: usize = SAMPLES_PER_FRAME;

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

    /// Whether an enabled stage still holds any delayed audio that must be
    /// allowed to feed back before playback is complete.
    pub(crate) fn has_pending_samples(&self) -> bool {
        self.is_enabled() && self.delay.iter().any(|&sample| sample != (0, 0))
    }

    /// Seed one frame's mixer accumulator with this stage's wet
    /// contribution, one delay-line position per output sample — the same
    /// per-sample granularity `SoundMainRAM_Reverb` runs at, not a single
    /// per-frame scalar. Both taps (module docs) are read for every
    /// position, and the mono result is written to both channels, exactly
    /// as `m4a_1.s:113`-`:114` stores the same byte twice. Voice mixing then
    /// adds its own contribution on top of what this writes.
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
            let tap_a = self.delay[(self.cursor + i) % DELAY_SAMPLES];
            let tap_b = self.delay[(self.cursor + i + TAP_B_OFFSET) % DELAY_SAMPLES];
            let wet = wet_sample(tap_a, tap_b, level);
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

/// One position's wet sample: upstream's `((la + ra + lb + rb) * level) >> 9`
/// plus the round-toward-zero correction (`m4a_1.s:102`-`:112`, module docs).
///
/// Both taps are summed *before* scaling, which is what makes the pair a comb
/// filter rather than two independent echoes: in-phase content sees twice the
/// gain of a lone tap, antiphase content cancels to zero.
///
/// Every input is a clipped `s8`-range sample and `level` is `0..=127`, so the
/// product lands in `-65_024..=64_516` and the shifted result in
/// `-127..=126` — the same range upstream's `strb` stores, which is what makes
/// the `& 0x80` sign test a faithful reproduction of `tst r0, 0x80`.
fn wet_sample(tap_a: StereoAcc, tap_b: StereoAcc, level: i32) -> i32 {
    let sum = tap_a.0 + tap_a.1 + tap_b.0 + tap_b.1;
    let scaled = (sum * level) >> 9;
    // `tst r0, 0x80 / addne r0, r0, 1`: bit 7 of the byte about to be stored
    // is its sign bit, so a negative result is nudged one LSB back toward
    // zero instead of staying rounded toward negative infinity.
    if scaled & 0x80 == 0 {
        scaled
    } else {
        scaled + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Root-mean-square of a slice's left channel (the wet path writes both
    /// channels identically, so one is enough).
    fn rms(xs: &[StereoAcc]) -> f64 {
        let sum_sq: f64 = xs.iter().map(|&(l, _)| f64::from(l) * f64::from(l)).sum();
        // `xs.len()` is at most a few thousand: far below `f64`'s 52-bit
        // mantissa, so this loses no precision.
        #[allow(clippy::cast_precision_loss)]
        let len = xs.len() as f64;
        (sum_sq / len).sqrt()
    }

    /// Fill the ring with one cycle of a mono sinusoid of the given integer
    /// `period` and amplitude, then let it ring down with no further dry
    /// input, returning each subsequent ring cycle's [`DELAY_SAMPLES`]
    /// samples.
    ///
    /// The excitation *replaces* whatever [`Reverb::seed_frame`] would have
    /// produced (rather than being added to it, as the mixer's dry path
    /// does), so the delay line's starting contents are exactly the intended
    /// waveform. Everything is driven one [`SAMPLES_PER_FRAME`] frame at a
    /// time, exactly as `crate::mixer::Mixer::mix_frame` drives it, so the
    /// feedback path really is recursive: the last frame of each cycle's
    /// tap B reads the *same* cycle's first frame.
    fn ring_down(period: usize, amplitude: f64, level: u8, cycles: usize) -> Vec<Vec<StereoAcc>> {
        use std::f64::consts::TAU;

        let mut reverb = Reverb::new(level);
        let mut scratch = vec![(0, 0); SAMPLES_PER_FRAME];
        for frame in 0..RING_FRAMES {
            let excitation: Vec<StereoAcc> = (0..SAMPLES_PER_FRAME)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let n = (frame * SAMPLES_PER_FRAME + i) as f64;
                    #[allow(clippy::cast_precision_loss)]
                    let phase = TAU * n / period as f64;
                    #[allow(clippy::cast_possible_truncation)]
                    let v = (amplitude * phase.sin()).round() as i32;
                    (v, v)
                })
                .collect();
            reverb.seed_frame(&mut scratch);
            reverb.commit_frame(&excitation);
        }

        let mut per_cycle = Vec::with_capacity(cycles);
        for _ in 0..cycles {
            let mut cycle = vec![(0, 0); DELAY_SAMPLES];
            for frame in 0..RING_FRAMES {
                reverb.seed_frame(&mut scratch);
                // No dry input: whatever reverb seeded is exactly what a
                // silent mixer would clip and commit (these magnitudes stay
                // inside the s8 clip range).
                reverb.commit_frame(&scratch);
                cycle[frame * SAMPLES_PER_FRAME..(frame + 1) * SAMPLES_PER_FRAME]
                    .copy_from_slice(&scratch);
            }
            per_cycle.push(cycle);
        }
        per_cycle
    }

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

    /// The ring geometry the two taps are measured against (module docs):
    /// 1568 samples, an exact whole number of mixer frames, with tap B one
    /// frame shallower. Pinned so nobody "fixes" [`DELAY_SAMPLES`] back to
    /// `PCM_DMA_BUF_SIZE` (1584), which is not a multiple of the frame size
    /// and is not where upstream's write pointer wraps.
    #[test]
    fn the_ring_is_seven_whole_mixer_frames() {
        assert_eq!(DELAY_SAMPLES, 1568);
        assert_eq!(DELAY_SAMPLES % SAMPLES_PER_FRAME, 0);
        assert_eq!(DELAY_SAMPLES / SAMPLES_PER_FRAME, RING_FRAMES);
        assert_eq!(DELAY_SAMPLES - TAP_B_OFFSET, 1344);
    }

    /// [`wet_sample`]'s formula, pinned against hand-computed values so
    /// neither the four-byte sum, the `>> 9` scale nor the round-toward-zero
    /// correction can silently drift.
    #[test]
    fn wet_sample_matches_the_documented_formula() {
        // (100 + -60 + 0 + 0) * 50 = 2000; 2000 >> 9 = 3.
        assert_eq!(wet_sample((100, -60), (0, 0), 50), 3);
        // Both taps in phase: double the sum, double the result.
        assert_eq!(wet_sample((100, -60), (100, -60), 50), 7);
        // Both taps in antiphase: exact cancellation, the comb's null.
        assert_eq!(wet_sample((100, -60), (-100, 60), 50), 0);
        // A zero level must silence everything, whatever the taps are.
        assert_eq!(wet_sample((127, 127), (127, 127), 0), 0);
        // Negative results are nudged back toward zero by the correction:
        // (-100 + -100) * 50 = -10000; -10000 >> 9 = -20 (floor), + 1 = -19.
        assert_eq!((-10_000) >> 9, -20);
        assert_eq!(wet_sample((-100, -100), (0, 0), 50), -19);
        // ... including results that the shift alone would leave at -1, the
        // value a recirculating loop would otherwise get stuck on.
        assert_eq!(wet_sample((-1, -1), (-1, -1), 100), 0);
    }

    /// A one-shot impulse comes back **twice**, once per tap: at
    /// `DELAY_SAMPLES - SAMPLES_PER_FRAME` (tap B, 1344 samples) and again at
    /// [`DELAY_SAMPLES`] (tap A, 1568 samples), each with a lone tap's gain
    /// and nothing in between. This is the two-tap structure the module docs
    /// describe, and the direct regression against collapsing them into a
    /// single echo.
    #[test]
    fn one_impulse_returns_as_two_echoes_one_frame_apart() {
        let mut reverb = Reverb::new(50);
        let mut seed = [(0, 0)];

        // Nothing committed yet: silence.
        reverb.seed_frame(&mut seed);
        assert_eq!(seed, [(0, 0)]);

        // Commit a single loud stereo impulse at position 0.
        reverb.commit_frame(&[(100, -60)]);

        let echo = wet_sample((100, -60), (0, 0), 50);
        assert_ne!(echo, 0, "the echo must actually be audible");
        for offset in 1..=DELAY_SAMPLES {
            reverb.seed_frame(&mut seed);
            let expected = if offset == DELAY_SAMPLES - SAMPLES_PER_FRAME || offset == DELAY_SAMPLES
            {
                echo
            } else {
                0
            };
            assert_eq!(
                seed,
                [(expected, expected)],
                "wrong output at offset {offset}"
            );
            reverb.commit_frame(&[(0, 0)]);
        }
    }

    /// The comb's audible signature (module docs): content at a response
    /// **peak** (period 224 samples = `MIXER_RATE / 224` ~= 59.7 Hz, arriving
    /// at both taps in phase) rings on at the doubled per-tap gain, while
    /// content at the adjacent **null** (period 448 samples ~= 29.9 Hz,
    /// arriving in antiphase) is annihilated by the tap sum.
    ///
    /// A single-tap stage cannot pass this: with one tap there is no
    /// cancellation, so both frequencies would decay at the identical rate.
    ///
    /// Note that the null does not decay to *silence*, and this test does not
    /// pretend it does: only the [`DELAY_SAMPLES`] - [`TAP_B_OFFSET`] region
    /// where both taps still hold the original waveform cancels exactly. The
    /// remaining [`TAP_B_OFFSET`] samples of each lap are where tap B has
    /// already wrapped onto this lap's own fresh output, and that residue is
    /// a broadband half-cycle burst rather than a null-frequency tone, so it
    /// rings down at the ordinary rate — from a starting energy some five
    /// times below the peak case's.
    #[test]
    fn a_comb_null_dies_far_faster_than_a_comb_peak() {
        const LEVEL: u8 = 100;
        const CYCLES: usize = 4;
        const AMPLITUDE: f64 = 100.0;

        let peak = ring_down(SAMPLES_PER_FRAME, AMPLITUDE, LEVEL, CYCLES);
        let null = ring_down(2 * SAMPLES_PER_FRAME, AMPLITUDE, LEVEL, CYCLES);

        // Where both taps still hold the excitation, antiphase content
        // cancels to *exactly* zero -- the comb null, sample-exact.
        let cancelling = DELAY_SAMPLES - TAP_B_OFFSET;
        assert!(
            null[0][..cancelling].iter().all(|&s| s == (0, 0)),
            "antiphase taps must cancel exactly, not merely attenuate"
        );

        // In-phase content over the same region is reinforced instead: both
        // taps add, so the lap keeps `2 * level / 256` = `level / 128` of the
        // excitation's amplitude.
        let expected_peak_ratio = f64::from(LEVEL) / 128.0;
        let excitation_rms = AMPLITUDE / f64::sqrt(2.0);
        let peak_first = rms(&peak[0][..cancelling]);
        assert!(
            peak_first > 0.95 * expected_peak_ratio * excitation_rms,
            "the comb peak must ring on at roughly level/128 per lap, got {peak_first:.2} from \
             {excitation_rms:.2}"
        );

        // Across whole laps the null stays a small fraction of the peak, and
        // the gap keeps widening rather than closing.
        let mut previous_ratio = f64::INFINITY;
        for (cycle, (n, p)) in null.iter().zip(&peak).enumerate() {
            let (n_rms, p_rms) = (rms(n), rms(p));
            let ratio = n_rms / p_rms;
            assert!(
                ratio < 0.25,
                "lap {cycle}: null RMS {n_rms:.3} is not far below peak RMS {p_rms:.3}"
            );
            assert!(
                ratio < previous_ratio,
                "lap {cycle}: the null must keep falling behind the peak, ratio {ratio:.4}"
            );
            previous_ratio = ratio;
        }
        assert!(
            rms(&peak[CYCLES - 1]) > 0.25 * excitation_rms,
            "the comb peak must still be clearly audible after {CYCLES} laps"
        );
    }

    /// Upstream's round-toward-zero correction (`m4a_1.s:111`-`:112`, module
    /// docs) is what keeps the recirculating loop from parking on a standing
    /// DC offset: negative content must decay to **exactly** zero and stay
    /// there. Without the correction, `asr 9`'s floor pins every `-1` sample
    /// at `-1` forever (`(-1 * 4 * 100) >> 9 == -1`), and the mix acquires a
    /// permanent negative bias.
    #[test]
    fn negative_content_decays_to_exactly_zero_instead_of_a_dc_offset() {
        for start in [-100, -1, 100, 1] {
            let mut reverb = Reverb::new(100);
            let mut scratch = vec![(0, 0); SAMPLES_PER_FRAME];
            for _ in 0..RING_FRAMES {
                reverb.seed_frame(&mut scratch);
                reverb.commit_frame(&vec![(start, start); SAMPLES_PER_FRAME]);
            }

            // Comfortably more cycles than the ~15 an amplitude-100 tail
            // needs at level 100 (each cycle scales by 100/128).
            for _ in 0..40 * RING_FRAMES {
                reverb.seed_frame(&mut scratch);
                reverb.commit_frame(&scratch);
            }
            assert!(
                scratch.iter().all(|&s| s == (0, 0)),
                "a tail starting at {start} left a standing offset: {:?}",
                scratch[0]
            );

            // And it stays silent: no slow re-accumulation.
            for _ in 0..RING_FRAMES {
                reverb.seed_frame(&mut scratch);
                assert!(scratch.iter().all(|&s| s == (0, 0)));
                reverb.commit_frame(&scratch);
            }
        }
    }

    /// Multi-sample frames map linearly onto consecutive delay-line
    /// positions (not all onto one position) -- the seed/commit index
    /// mapping [`one_impulse_returns_as_two_echoes_one_frame_apart`] pins one
    /// sample at a time.
    #[test]
    fn a_whole_frame_maps_onto_consecutive_delay_positions() {
        let mut reverb = Reverb::new(80);
        let burst: Vec<StereoAcc> = (0..16).map(|i| (i * 2, i * 3)).collect();
        reverb.commit_frame(&burst);

        // Skip ahead to just before tap B reaches the burst, i.e. one frame
        // short of a full lap, so only that tap sees it and the expected
        // gain is a lone tap's.
        let silence = vec![(0, 0); DELAY_SAMPLES - TAP_B_OFFSET - burst.len()];
        let mut scratch = silence.clone();
        reverb.seed_frame(&mut scratch);
        reverb.commit_frame(&silence);

        let mut echoed = vec![(0, 0); burst.len()];
        reverb.seed_frame(&mut echoed);
        let expected: Vec<StereoAcc> = burst
            .iter()
            .map(|&tap| {
                let wet = wet_sample(tap, (0, 0), 80);
                (wet, wet)
            })
            .collect();
        assert_eq!(echoed, expected);
    }

    /// Documented spectral/RMS tolerance (Discussion #227's owner decision):
    /// once the delay line holds only mono (`left == right`) feedback
    /// content -- which every position does after one full echo cycle,
    /// since [`Reverb::seed_frame`] always writes equal left/right -- a
    /// waveform whose period divides [`SAMPLES_PER_FRAME`] hits both taps in
    /// phase, so each further uncontested cycle's total energy should scale
    /// by very close to `2 * level / 256` = `level / 128`. Measured as an RMS
    /// ratio (not a byte-exact comparison) because `>> 9` integer truncation
    /// perturbs individual samples by up to one LSB; **5%** is the documented
    /// tolerance -- derived here as roughly 2x the worst-case per-sample
    /// truncation error (~1/60, given this test's ~60-amplitude synthetic
    /// waveform) after RMS-averaging across a full [`DELAY_SAMPLES`] window,
    /// with headroom.
    #[test]
    fn feedback_energy_decays_at_the_documented_rms_ratio_within_tolerance() {
        const LEVEL: u8 = 100;
        let mut reverb = Reverb::new(LEVEL);

        // A full delay-length burst of a small alternating synthetic
        // waveform (not silence, not a pure DC impulse) -- already mono
        // (`left == right`) so every subsequent cycle's ratio is directly
        // comparable, matching how a real post-reverb signal is mono once
        // it has been through the wet path once (module docs). Its 8-sample
        // period divides [`SAMPLES_PER_FRAME`], so it sits on a comb peak.
        let burst: Vec<StereoAcc> = (0..DELAY_SAMPLES)
            .map(|i| {
                let v = 60 - i32::try_from(i % 8).expect("< 8") * 15;
                (v, v)
            })
            .collect();
        let mut scratch = vec![(0, 0); SAMPLES_PER_FRAME];
        for frame in 0..RING_FRAMES {
            reverb.seed_frame(&mut scratch); // nothing committed yet: silence
            reverb.commit_frame(&burst[frame * SAMPLES_PER_FRAME..(frame + 1) * SAMPLES_PER_FRAME]);
        }

        let expected_ratio = f64::from(LEVEL) / 128.0;
        let mut previous_rms = rms(&burst);
        let mut cycle_buffer = vec![(0, 0); DELAY_SAMPLES];
        for cycle in 0..4 {
            for frame in 0..RING_FRAMES {
                reverb.seed_frame(&mut scratch);
                // No further dry input this cycle: whatever reverb seeded is
                // exactly what a silent mixer would clip and commit (these
                // magnitudes stay far inside the s8 clip range).
                reverb.commit_frame(&scratch);
                cycle_buffer[frame * SAMPLES_PER_FRAME..(frame + 1) * SAMPLES_PER_FRAME]
                    .copy_from_slice(&scratch);
            }
            let cycle_rms = rms(&cycle_buffer);
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
