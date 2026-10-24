//! Mixer silence, voice lifetime, scaling, clipping, and admission regressions.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::envelope::Adsr;
use crate::sample::WaveData;

#[test]
fn empty_mixer_renders_silence() {
    let mut mixer = Mixer::default();
    let mut out = vec![9.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    assert!(out.iter().all(|&s| s == 0.0));
    assert!(mixer.is_idle());
}

#[test]
fn empty_one_shot_retires_after_one_mix_frame() {
    const ECHO_VOLUME: u8 = 128;
    const ECHO_LENGTH: u8 = 60;

    let voice = Voice::new(
        Arc::new(WaveData::one_shot(0, vec![])),
        Adsr::flat(),
        unity_freq(),
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        60,
        0,
        ECHO_VOLUME,
        ECHO_LENGTH,
    );
    let mut mixer = Mixer::default();
    assert!(mixer.add_voice(voice));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    assert_eq!(
        mixer.voice_count(),
        0,
        "an empty one-shot must not occupy a mixer slot"
    );
}

#[test]
fn single_voice_is_scaled_and_normalised() {
    const SAMPLE: i8 = 50;

    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES);
    mixer.add_voice(constant_voice(SAMPLE, 0));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let contribution =
        FLAT_ENVELOPE_GAIN_AT_MAX_MASTER_VOLUME * i32::from(SAMPLE) / SAMPLE_GAIN_DIVISOR;
    let expected = (contribution as f32) / OUTPUT_SCALE;
    assert!((out[0] - expected).abs() < ASSERTION_TOLERANCE);
}

#[test]
fn two_voices_sum() {
    const FIRST_SAMPLE: i8 = 40;
    const SECOND_SAMPLE: i8 = 30;

    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES);
    mixer.add_voice(constant_voice(FIRST_SAMPLE, 0));
    mixer.add_voice(constant_voice(SECOND_SAMPLE, 1));
    assert_eq!(mixer.voice_count(), 2);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let first_contribution =
        FLAT_ENVELOPE_GAIN_AT_MAX_MASTER_VOLUME * i32::from(FIRST_SAMPLE) / SAMPLE_GAIN_DIVISOR;
    let second_contribution =
        FLAT_ENVELOPE_GAIN_AT_MAX_MASTER_VOLUME * i32::from(SECOND_SAMPLE) / SAMPLE_GAIN_DIVISOR;
    let expected = ((first_contribution + second_contribution) as f32) / OUTPUT_SCALE;
    assert!((out[0] - expected).abs() < ASSERTION_TOLERANCE);
}

#[test]
fn loud_sum_clips_to_full_scale() {
    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES);
    for track in 0..4 {
        mixer.add_voice(constant_voice(i8::MAX, track));
    }
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let positive_full_scale = f32::from(i8::MAX) / OUTPUT_SCALE;
    assert!((out[0] - positive_full_scale).abs() < ASSERTION_TOLERANCE);
}

#[test]
fn negative_sum_clips_to_minus_one() {
    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES);
    for track in 0..4 {
        mixer.add_voice(constant_voice(i8::MIN, track));
    }
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    assert!((out[0] - (-1.0)).abs() < ASSERTION_TOLERANCE);
}

#[test]
fn cgb_voice_output_is_unaffected_by_the_direct_sound_master_volume() {
    // The rationale is on `CgbVoice::begin_frame`.
    let mut default_mixer = Mixer::new(DEFAULT_MASTER_VOLUME, DEFAULT_MAX_VOICES);
    assert!(default_mixer.add_cgb_voice(cgb_keyed_voice(0, 60)));
    let mut at_default = vec![0.0; SAMPLES_PER_FRAME * 2];
    default_mixer.mix_frame(&mut at_default);

    let mut full_scale_mixer = Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES);
    assert!(full_scale_mixer.add_cgb_voice(cgb_keyed_voice(0, 60)));
    let mut at_full_scale = vec![0.0; SAMPLES_PER_FRAME * 2];
    full_scale_mixer.mix_frame(&mut at_full_scale);

    assert!(
        at_default.iter().any(|&sample| sample != 0.0),
        "sanity: the CGB voice under test must be audible"
    );
    assert_eq!(
        at_default, at_full_scale,
        "the Direct Sound master volume must not attenuate CGB output"
    );
}

#[test]
fn voice_cap_drops_extra_notes() {
    let mut mixer = Mixer::new(DEFAULT_MASTER_VOLUME, 2);
    assert!(mixer.add_voice(constant_voice(10, 0)));
    assert!(mixer.add_voice(constant_voice(10, 1)));
    assert!(!mixer.add_voice(constant_voice(10, 2)));
    assert_eq!(mixer.voice_count(), 2);
}
