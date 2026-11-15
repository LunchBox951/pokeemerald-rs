//! A DirectSound voice seeded from the XCMD 0x0D sample progress starts at
//! `data + count` with a cleared phase and `size - count` samples left
//! (`m4a_1.s:184`..`:196`, `:1791`..`:1797`).

use std::sync::Arc;

use crate::envelope::Adsr;
use crate::pitch;
use crate::sample::WaveData;
use crate::voice::{StereoAcc, Voice};

/// Hand-indexed, nonuniform and signed: every index reads back a different
/// value, so a wrong start, step or lifetime cannot coincide with the right
/// one.
const SAMPLES: [i8; 8] = [10, -20, 30, -40, 50, -60, 70, -80];
/// Per-sample gain at full channel volume and a flat envelope
/// (`voice.rs` `FULL_SCALE_FRAME_GAIN`, 254, shifted by `SAMPLE_GAIN_BITS`).
const FRAME_GAIN: i32 = 254;
const GAIN_SHIFT: u32 = 8;
const MASTER_VOLUME: u8 = 15;

fn gained(sample: i32) -> i32 {
    (FRAME_GAIN * sample) >> GAIN_SHIFT
}

fn voice_over(wave: WaveData, frequency: u32, fixed: bool) -> Voice {
    Voice::new(
        Arc::new(wave),
        Adsr::flat(),
        frequency,
        u8::MAX,
        u8::MAX,
        127,
        0,
        60,
        0,
        0,
        0,
    )
    .fixed_rate(fixed)
}

fn render(voice: &mut Voice, outputs: usize) -> Vec<StereoAcc> {
    let mut acc = vec![(0, 0); outputs];
    voice.begin_frame(MASTER_VOLUME);
    voice.render(&mut acc);
    acc
}

/// Mirror of `SoundMainRAM`'s read: `current + (frac * (next - current)) >> 23`
/// for a 23-bit fraction, over a hand-indexed position in 23-bit fixed point.
fn reference_output(samples: &[i8], position: u64) -> i32 {
    let index = usize::try_from(position >> pitch::FRAC_BITS).unwrap();
    let frac = i64::try_from(position & u64::from(pitch::FRAC_ONE - 1)).unwrap();
    let current = i64::from(samples[index]);
    let next = i64::from(samples[index + 1]);
    i32::try_from(current + ((frac * (next - current)) >> pitch::FRAC_BITS)).unwrap()
}

#[test]
fn fixed_rate_voice_starts_at_the_progress_and_retires_after_the_remaining_samples() {
    let progress = 5;
    let mut voice = voice_over(WaveData::one_shot(0, SAMPLES.to_vec()), 0, true)
        .with_sample_progress(progress)
        .expect("5 is inside an 8-sample wave");
    assert_eq!(voice.source_index(), 5);

    let acc = render(&mut voice, 6);

    // size - count = 3 samples remain: indices 5, 6, 7, then nothing.
    let expected: Vec<StereoAcc> = [-60, 70, -80]
        .into_iter()
        .map(|s| (gained(s), gained(s)))
        .chain([(0, 0); 3])
        .collect();
    assert_eq!(acc, expected);
    assert!(
        !voice.is_active(),
        "the voice must retire on the 8th sample"
    );
}

#[test]
fn a_zero_progress_voice_is_the_unseeded_voice() {
    let plain = voice_over(WaveData::one_shot(0, SAMPLES.to_vec()), 0, true);
    let mut seeded = plain.clone().with_sample_progress(0).unwrap();
    let mut plain = plain;
    assert_eq!(seeded.source_index(), 0);
    assert_eq!(render(&mut seeded, 10), render(&mut plain, 10));
    assert_eq!(seeded.is_active(), plain.is_active());
}

#[test]
fn pitched_integer_step_lands_on_whole_samples_from_the_progress() {
    // 627 * 2^23 wraps to 115 * 2^23: a pitched step of exactly 115 samples.
    let frequency = 1 << pitch::FRAC_BITS;
    assert_eq!(pitch::phase_step(frequency), 115 * pitch::FRAC_ONE);
    let samples: Vec<i8> = (0..300)
        .map(|i: i32| i8::try_from((i * 7) % 101 - 50).unwrap())
        .collect();
    let mut voice = voice_over(WaveData::one_shot(0, samples.clone()), frequency, false)
        .with_sample_progress(40)
        .unwrap();

    let acc = render(&mut voice, 5);

    // 40, 155, 270 are playable; 385 is past the 300-sample wave.
    let expected: Vec<StereoAcc> = [40_usize, 155, 270]
        .into_iter()
        .map(|i| {
            let s = i32::from(samples[i]);
            (gained(s), gained(s))
        })
        .chain([(0, 0); 2])
        .collect();
    assert_eq!(acc, expected);
    assert!(!voice.is_active());
}

#[test]
fn pitched_fractional_step_starts_with_a_zero_phase_and_follows_the_reference_model() {
    let frequency = (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ * 3 / 4;
    let step = u64::from(pitch::phase_step(frequency));
    assert!(
        step > 0 && step < u64::from(pitch::FRAC_ONE),
        "fractional step {step}"
    );
    let progress = 2_u64;
    let mut voice = voice_over(WaveData::one_shot(0, SAMPLES.to_vec()), frequency, false)
        .with_sample_progress(u32::try_from(progress).unwrap())
        .unwrap();

    let acc = render(&mut voice, 6);

    // The first output is the raw sample: the seed clears the fractional phase.
    assert_eq!(acc[0].0, gained(i32::from(SAMPLES[2])));
    // A one-shot with no retained guard repeats its final sample as the
    // lookahead past the buffer (`interpolated_sample`).
    let mut table = SAMPLES.to_vec();
    table.push(SAMPLES[7]);
    for (n, output) in acc.iter().enumerate() {
        let position = (progress << pitch::FRAC_BITS) + step * u64::try_from(n).unwrap();
        let expected = gained(reference_output(&table, position));
        assert_eq!(output.0, expected, "output {n}");
        assert_eq!(output.1, expected, "output {n}");
    }
    assert!(
        acc.iter().all(|&(l, _)| l != 0),
        "6 outputs at step < 1 stay inside the remaining 6 samples"
    );
}

#[test]
fn fractional_pitch_lifetime_counts_the_samples_remaining_after_the_progress() {
    let frequency = (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ * 3 / 4;
    let step = u64::from(pitch::phase_step(frequency));
    let remaining = (8 - 2_u64) << pitch::FRAC_BITS;
    // The output index n is playable while n * step < remaining.
    let playable = usize::try_from(remaining.div_ceil(step)).unwrap();
    let mut voice = voice_over(WaveData::one_shot(0, SAMPLES.to_vec()), frequency, false)
        .with_sample_progress(2)
        .unwrap();

    let acc = render(&mut voice, playable + 4);

    assert!(playable >= 1);
    assert_ne!(acc[playable - 1], (0, 0), "the last playable output sounds");
    assert!(acc[playable..].iter().all(|&o| o == (0, 0)));
    assert!(!voice.is_active());
}

#[test]
fn the_interpolation_guard_still_supplies_the_final_lookahead_after_a_seed() {
    // Logical length 4 over a 5-sample buffer: index 4 (value 50) is the
    // retained guard, not a playable sample.
    let wave = WaveData::one_shot(0, SAMPLES[..5].to_vec()).with_logical_len(4);
    let frequency = (1 << pitch::FRAC_BITS) / pitch::DIV_FREQ / 2;
    let step = u64::from(pitch::phase_step(frequency));
    let mut voice = voice_over(wave, frequency, false)
        .with_sample_progress(3)
        .unwrap();

    let acc = render(&mut voice, 4);

    assert_eq!(acc[0].0, gained(i32::from(SAMPLES[3])));
    // Index 3 blends toward the guard at index 4 (50); then the one-shot
    // retires once the position reaches the logical end (index 4).
    let blended = reference_output(&SAMPLES[..5], (3 << pitch::FRAC_BITS) + step);
    assert_eq!(acc[1].0, gained(blended));
    assert!(
        blended > i32::from(SAMPLES[3]),
        "the blend must move toward the guard sample"
    );
    let live = usize::try_from((1_u64 << pitch::FRAC_BITS).div_ceil(step)).unwrap();
    assert!(acc[live..].iter().all(|&o| o == (0, 0)));
    assert!(!voice.is_active());
}

#[test]
fn fixed_rate_ignores_the_guard_and_reads_raw_samples_from_the_progress() {
    let wave = WaveData::one_shot(0, SAMPLES[..5].to_vec()).with_logical_len(4);
    let mut voice = voice_over(wave, 0, true).with_sample_progress(3).unwrap();

    let acc = render(&mut voice, 3);

    assert_eq!(acc[0].0, gained(-40));
    assert_eq!(
        acc[1],
        (0, 0),
        "index 4 is the guard, not a playable sample"
    );
    assert!(!voice.is_active());
}

#[test]
fn a_looping_voice_seeded_inside_the_loop_wraps_to_the_loop_start() {
    let wave = WaveData::looping(0, 2, SAMPLES[..6].to_vec());
    let mut voice = voice_over(wave, 0, true).with_sample_progress(5).unwrap();

    let acc = render(&mut voice, 4);

    // 5, then the wrap to the loop start: 2, 3, 4.
    let expected: Vec<StereoAcc> = [-60, 30, -40, 50]
        .into_iter()
        .map(|s| (gained(s), gained(s)))
        .collect();
    assert_eq!(acc, expected);
    assert!(voice.is_active());
}

#[test]
fn the_last_sample_is_the_highest_supported_progress() {
    let voice = voice_over(WaveData::one_shot(0, SAMPLES.to_vec()), 0, true);
    let seeded = voice.with_sample_progress(7).expect("the last sample");
    assert_eq!(seeded.source_index(), 7);
}

#[test]
fn an_out_of_range_progress_fails_closed() {
    let base = voice_over(WaveData::one_shot(0, SAMPLES.to_vec()), 0, true);
    for progress in [8, 9, u32::MAX] {
        assert!(
            base.clone().with_sample_progress(progress).is_none(),
            "progress {progress} is outside an 8-sample wave"
        );
    }

    // The retained guard is not a playable start, and neither is any offset
    // into an empty wave.
    let guarded = voice_over(
        WaveData::one_shot(0, SAMPLES[..5].to_vec()).with_logical_len(4),
        0,
        true,
    );
    assert!(guarded.clone().with_sample_progress(3).is_some());
    assert!(guarded.with_sample_progress(4).is_none());
    let empty = voice_over(WaveData::one_shot(0, Vec::new()), 0, true);
    assert!(empty.with_sample_progress(1).is_none());

    // Looping waves share the bound: a start inside the loop is supported,
    // one at the logical end is not.
    let looping = voice_over(WaveData::looping(0, 2, SAMPLES[..6].to_vec()), 0, true);
    assert!(looping.clone().with_sample_progress(5).is_some());
    assert!(looping.with_sample_progress(6).is_none());
}
