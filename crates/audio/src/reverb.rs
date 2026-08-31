//! Two-tap feedback reverb for the master mix.
//!
//! Each output sample sums both stereo channels from a 1,568-sample tap and a
//! 1,344-sample tap, scales the sum by the song's reverb level, and seeds both
//! mixer channels with the same wet sample before voices add dry audio.
//!
//! The hardware buffer holds 1,584 samples, but `pcmDmaPeriod` truncates that
//! to seven complete 224-sample mixer frames (`m4a.c:407`). The native ring
//! models those 1,568 reachable positions directly. Its two tap positions and
//! mono write match `SoundMainRAM_Reverb` (`m4a_1.s:97..114`).
//!
//! With clipped samples and the supported `0..=127` level, upstream
//! arithmetically shifts the scaled sum by nine and then adds one when the
//! stored byte's sign bit is set (`m4a_1.s:109..112`). This corrects fractional
//! negative results toward zero and also biases exact negative multiples one
//! step toward zero, so ordinary signed division is not equivalent.
//!
//! The mixer commits the final clipped wet-plus-dry frame back to the ring.
//! Clipping therefore occurs inside the feedback loop; unlike the hardware's
//! byte accumulator, the native mixer deliberately does not wrap overflow.

use crate::pitch::SAMPLES_PER_FRAME;
use crate::voice::StereoAcc;

const PHYSICAL_PCM_BUFFER_SAMPLES: usize = 1_584;
const RING_FRAMES: usize = PHYSICAL_PCM_BUFFER_SAMPLES / SAMPLES_PER_FRAME;
pub(crate) const DELAY_SAMPLES: usize = RING_FRAMES * SAMPLES_PER_FRAME;
const SHORT_TAP_DELAY_SAMPLES: usize = DELAY_SAMPLES - SAMPLES_PER_FRAME;
const WET_GAIN_SHIFT: u32 = 9;
const STORED_SAMPLE_SIGN_BIT: i32 = 1 << 7;

/// Master-mix feedback delay with long and short taps one mixer frame apart.
#[derive(Clone, Debug)]
pub(crate) struct Reverb {
    delay: Box<[StereoAcc]>,
    next_frame_start: usize,
    level: u8,
}

impl Reverb {
    pub(crate) fn new(level: u8) -> Self {
        Self {
            delay: vec![(0, 0); DELAY_SAMPLES].into_boxed_slice(),
            next_frame_start: 0,
            level,
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.level != 0
    }

    pub(crate) fn has_pending_samples(&self) -> bool {
        self.is_enabled() && self.delay.iter().any(|&sample| sample != (0, 0))
    }

    pub(crate) fn seed_frame(&self, mix: &mut [StereoAcc]) {
        if !self.is_enabled() {
            mix.fill((0, 0));
            return;
        }

        let level = i32::from(self.level);
        for (frame_offset, output) in mix.iter_mut().enumerate() {
            let long_tap_index = (self.next_frame_start + frame_offset) % DELAY_SAMPLES;
            let short_tap_index =
                (long_tap_index + DELAY_SAMPLES - SHORT_TAP_DELAY_SAMPLES) % DELAY_SAMPLES;
            let mono_wet_sample = wet_sample(
                self.delay[long_tap_index],
                self.delay[short_tap_index],
                level,
            );
            *output = (mono_wet_sample, mono_wet_sample);
        }
    }

    pub(crate) fn commit_frame(&mut self, clipped_mix: &[StereoAcc]) {
        for (frame_offset, &sample) in clipped_mix.iter().enumerate() {
            let write_index = (self.next_frame_start + frame_offset) % DELAY_SAMPLES;
            self.delay[write_index] = sample;
        }
        self.next_frame_start = (self.next_frame_start + clipped_mix.len()) % DELAY_SAMPLES;
    }
}

fn wet_sample(long_tap: StereoAcc, short_tap: StereoAcc, level: i32) -> i32 {
    let stereo_tap_sum = long_tap.0 + long_tap.1 + short_tap.0 + short_tap.1;
    let shifted_wet_sample = (stereo_tap_sum * level) >> WET_GAIN_SHIFT;
    let upstream_correction = i32::from(shifted_wet_sample & STORED_SAMPLE_SIGN_BIT != 0);
    shifted_wet_sample + upstream_correction
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONO_COMB_GAIN_DIVISOR: f64 = 128.0;

    fn rms(samples: &[StereoAcc]) -> f64 {
        let squared_sum: f64 = samples
            .iter()
            .map(|&(left, _)| f64::from(left) * f64::from(left))
            .sum();
        #[expect(
            clippy::cast_precision_loss,
            reason = "test vectors contain at most a few reverb rings"
        )]
        let sample_count = samples.len() as f64;
        (squared_sum / sample_count).sqrt()
    }

    fn mono_sine(period: usize, amplitude: f64) -> Vec<StereoAcc> {
        use std::f64::consts::TAU;

        (0..DELAY_SAMPLES)
            .map(|index| {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a reverb-ring index is exactly representable as f64"
                )]
                let index = index as f64;
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "the small integer period is exactly representable as f64"
                )]
                let phase = TAU * index / period as f64;
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "the bounded synthetic amplitude fits i32"
                )]
                let sample = (amplitude * phase.sin()).round() as i32;
                (sample, sample)
            })
            .collect()
    }

    fn load_ring(reverb: &mut Reverb, samples: &[StereoAcc]) {
        assert_eq!(samples.len(), DELAY_SAMPLES);
        for frame in samples.chunks_exact(SAMPLES_PER_FRAME) {
            reverb.commit_frame(frame);
        }
    }

    fn feedback_cycle(reverb: &mut Reverb) -> Vec<StereoAcc> {
        let mut cycle = vec![(0, 0); DELAY_SAMPLES];
        for frame in cycle.chunks_exact_mut(SAMPLES_PER_FRAME) {
            reverb.seed_frame(frame);
            reverb.commit_frame(frame);
        }
        cycle
    }

    fn ring_down(period: usize, amplitude: f64, level: u8, cycles: usize) -> Vec<Vec<StereoAcc>> {
        let mut reverb = Reverb::new(level);
        load_ring(&mut reverb, &mono_sine(period, amplitude));
        (0..cycles).map(|_| feedback_cycle(&mut reverb)).collect()
    }

    #[test]
    fn disabled_reverb_always_seeds_silence() {
        let mut reverb = Reverb::new(0);
        let mut output = [(9, -9); 4];
        reverb.seed_frame(&mut output);
        assert_eq!(output, [(0, 0); 4]);

        reverb.commit_frame(&[(120, -120); 4]);
        output.fill((9, -9));
        reverb.seed_frame(&mut output);
        assert_eq!(output, [(0, 0); 4]);
    }

    #[test]
    fn physical_buffer_truncates_to_seven_frames_and_two_tap_delays() {
        assert_eq!(RING_FRAMES, 7);
        assert_eq!(DELAY_SAMPLES, 1_568);
        assert_eq!(DELAY_SAMPLES % SAMPLES_PER_FRAME, 0);
        assert_eq!(SHORT_TAP_DELAY_SAMPLES, 1_344);
    }

    #[test]
    fn wet_sample_sums_both_stereo_taps_before_scaling() {
        assert_eq!(wet_sample((100, -60), (0, 0), 50), 3);
        assert_eq!(wet_sample((100, -60), (100, -60), 50), 7);
        assert_eq!(wet_sample((100, -60), (-100, 60), 50), 0);
        assert_eq!(wet_sample((127, 127), (127, 127), 0), 0);
    }

    #[test]
    fn negative_wet_samples_receive_upstream_post_shift_correction() {
        assert_eq!((-10_000) >> WET_GAIN_SHIFT, -20);
        assert_eq!(wet_sample((-100, -100), (0, 0), 50), -19);
        assert_eq!(wet_sample((-1, -1), (-1, -1), 100), 0);
        assert_eq!(wet_sample((-2, -2), (-2, -2), 64), 0);
    }

    #[test]
    fn one_impulse_returns_from_short_then_long_tap() {
        let mut reverb = Reverb::new(50);
        let mut output = [(0, 0)];
        reverb.seed_frame(&mut output);
        assert_eq!(output, [(0, 0)]);

        let impulse = (100, -60);
        reverb.commit_frame(&[impulse]);
        let expected_echo = wet_sample(impulse, (0, 0), 50);
        assert_ne!(expected_echo, 0);

        for offset in 1..=DELAY_SAMPLES {
            reverb.seed_frame(&mut output);
            let expected = match offset {
                SHORT_TAP_DELAY_SAMPLES | DELAY_SAMPLES => expected_echo,
                _ => 0,
            };
            assert_eq!(output, [(expected, expected)], "offset {offset}");
            reverb.commit_frame(&[(0, 0)]);
        }
    }

    #[test]
    fn comb_null_cancels_while_adjacent_peak_keeps_decaying() {
        const LEVEL: u8 = 100;
        const CYCLES: usize = 4;
        const AMPLITUDE: f64 = 100.0;
        const MAX_NULL_TO_PEAK_RMS_RATIO: f64 = 0.25;
        const MIN_FIRST_PEAK_GAIN_RATIO: f64 = 0.95;

        let peak = ring_down(SAMPLES_PER_FRAME, AMPLITUDE, LEVEL, CYCLES);
        let null = ring_down(2 * SAMPLES_PER_FRAME, AMPLITUDE, LEVEL, CYCLES);

        assert!(
            null[0][..SHORT_TAP_DELAY_SAMPLES]
                .iter()
                .all(|&sample| sample == (0, 0)),
            "antiphase taps must cancel sample-exactly"
        );

        let expected_peak_ratio = f64::from(LEVEL) / MONO_COMB_GAIN_DIVISOR;
        let excitation_rms = AMPLITUDE / f64::sqrt(2.0);
        let first_peak_rms = rms(&peak[0][..SHORT_TAP_DELAY_SAMPLES]);
        assert!(
            first_peak_rms > MIN_FIRST_PEAK_GAIN_RATIO * expected_peak_ratio * excitation_rms,
            "first peak RMS {first_peak_rms:.2} did not preserve the expected comb gain"
        );

        let mut previous_ratio = f64::INFINITY;
        for (cycle, (null_samples, peak_samples)) in null.iter().zip(&peak).enumerate() {
            let null_rms = rms(null_samples);
            let peak_rms = rms(peak_samples);
            let ratio = null_rms / peak_rms;
            assert!(
                ratio < MAX_NULL_TO_PEAK_RMS_RATIO,
                "cycle {cycle}: null RMS {null_rms:.3} is too close to peak RMS {peak_rms:.3}"
            );
            assert!(
                ratio < previous_ratio,
                "cycle {cycle}: null-to-peak ratio stopped falling at {ratio:.4}"
            );
            previous_ratio = ratio;
        }

        assert!(
            rms(&peak[CYCLES - 1]) > MAX_NULL_TO_PEAK_RMS_RATIO * excitation_rms,
            "comb peak became inaudible before the measured decay window ended"
        );
    }

    #[test]
    fn negative_feedback_tail_reaches_zero_and_stays_there() {
        const CYCLES_TO_REACH_SILENCE: usize = 40;

        for starting_sample in [-100, -1, 100, 1] {
            let mut reverb = Reverb::new(100);
            load_ring(
                &mut reverb,
                &vec![(starting_sample, starting_sample); DELAY_SAMPLES],
            );

            let mut last_cycle = Vec::new();
            for _ in 0..CYCLES_TO_REACH_SILENCE {
                last_cycle = feedback_cycle(&mut reverb);
            }
            assert!(
                last_cycle.iter().all(|&sample| sample == (0, 0)),
                "tail starting at {starting_sample} left a standing offset"
            );
            assert!(
                feedback_cycle(&mut reverb)
                    .iter()
                    .all(|&sample| sample == (0, 0)),
                "silent feedback tail accumulated a new offset"
            );
        }
    }

    #[test]
    fn a_whole_frame_maps_to_consecutive_delay_positions() {
        const BURST_SAMPLES: usize = 16;

        let mut reverb = Reverb::new(80);
        let burst: Vec<StereoAcc> = (0..BURST_SAMPLES)
            .map(|index| {
                let index = i32::try_from(index).expect("small burst index");
                (index * 2, index * 3)
            })
            .collect();
        reverb.commit_frame(&burst);

        let samples_before_short_tap = SHORT_TAP_DELAY_SAMPLES - burst.len();
        let silence = vec![(0, 0); samples_before_short_tap];
        let mut skipped_output = silence.clone();
        reverb.seed_frame(&mut skipped_output);
        reverb.commit_frame(&silence);

        let mut echoed = vec![(0, 0); burst.len()];
        reverb.seed_frame(&mut echoed);
        let expected: Vec<StereoAcc> = burst
            .iter()
            .map(|&short_tap| {
                let wet = wet_sample((0, 0), short_tap, 80);
                (wet, wet)
            })
            .collect();
        assert_eq!(echoed, expected);
    }

    #[test]
    fn comb_peak_energy_decay_matches_mono_feedback_gain() {
        const LEVEL: u8 = 100;
        const CYCLES: usize = 4;
        const WAVE_PERIOD: usize = 8;
        const WAVE_MAXIMUM: i32 = 60;
        const WAVE_STEP: i32 = 15;
        // Five percent is roughly twice the worst-case one-LSB error relative
        // to this waveform's 60-sample amplitude after full-ring RMS averaging.
        const RMS_RATIO_TOLERANCE: f64 = 0.05;

        let burst: Vec<StereoAcc> = (0..DELAY_SAMPLES)
            .map(|index| {
                let phase = i32::try_from(index % WAVE_PERIOD).expect("phase fits i32");
                let sample = WAVE_MAXIMUM - phase * WAVE_STEP;
                (sample, sample)
            })
            .collect();

        let mut reverb = Reverb::new(LEVEL);
        load_ring(&mut reverb, &burst);
        let expected_ratio = f64::from(LEVEL) / MONO_COMB_GAIN_DIVISOR;
        let mut previous_rms = rms(&burst);

        for cycle in 0..CYCLES {
            let cycle_rms = rms(&feedback_cycle(&mut reverb));
            let measured_ratio = cycle_rms / previous_rms;
            assert!(
                (measured_ratio - expected_ratio).abs() < RMS_RATIO_TOLERANCE,
                "cycle {cycle}: RMS ratio {measured_ratio:.4} differs from {expected_ratio:.4} by more than the one-LSB rounding tolerance"
            );
            previous_rms = cycle_rms;
        }
    }
}
