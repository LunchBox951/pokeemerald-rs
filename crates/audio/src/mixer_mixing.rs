use std::sync::Arc;

use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;
use crate::envelope::Adsr;
use crate::pitch::{DIV_FREQ, FRAC_BITS};
use crate::sample::WaveData;

const MAX_MASTER_VOLUME: u8 = 15;
const FULL_TRACK_VOLUME: u8 = u8::MAX;
const MUTED_TRACK_VOLUME: u8 = 0;
const TEST_VELOCITY: u8 = 127;
const TIED_GATE_TIME: u16 = 0;
const SAMPLE_GAIN_DIVISOR: i32 = 256;
const OUTPUT_SCALE: f32 = 128.0;
const ASSERTION_TOLERANCE: f32 = 1e-6;
const FLAT_ENVELOPE_GAIN_AT_MAX_MASTER_VOLUME: i32 = 254;
const PERIOD_7_UPWARD_SHIFT_1: u8 = 0x71;

fn unity_freq() -> u32 {
    (1 << FRAC_BITS) / DIV_FREQ
}

fn cgb_keyed_voice(track: usize, key: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        None,
        CgbAdsr::flat(),
        key,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        key,
        track,
        0,
        0,
        0,
    )
}

fn constant_voice(level: i8, track: usize) -> Voice {
    keyed_voice(level, track, 60, FULL_TRACK_VOLUME, FULL_TRACK_VOLUME)
}

fn keyed_voice(level: i8, track: usize, key: u8, right_volume: u8, left_volume: u8) -> Voice {
    let frame_long_samples = vec![level; SAMPLES_PER_FRAME + 4];
    let wave = Arc::new(WaveData::one_shot(0, frame_long_samples));
    Voice::new(
        wave,
        Adsr::flat(),
        unity_freq(),
        right_volume,
        left_volume,
        TEST_VELOCITY,
        TIED_GATE_TIME,
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
fn note_off_track_stops_only_the_newest_matching_voice() {
    const TRACK: usize = 0;
    const REPEATED_KEY: u8 = 60;
    const OTHER_KEY: u8 = 64;

    let mut mixer = Mixer::default();
    // Panned apart so the assertions can tell WHICH matching voice stopped.
    mixer.add_voice(keyed_voice(
        50,
        TRACK,
        REPEATED_KEY,
        MUTED_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
    ));
    mixer.add_voice(keyed_voice(
        50,
        TRACK,
        REPEATED_KEY,
        FULL_TRACK_VOLUME,
        MUTED_TRACK_VOLUME,
    ));
    mixer.add_voice(keyed_voice(
        50,
        TRACK,
        OTHER_KEY,
        MUTED_TRACK_VOLUME,
        MUTED_TRACK_VOLUME,
    ));
    mixer.note_off_track(TRACK, REPEATED_KEY);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let left_energy: f32 = out.iter().step_by(2).map(|sample| sample.abs()).sum();
    let right_energy: f32 = out
        .iter()
        .skip(1)
        .step_by(2)
        .map(|sample| sample.abs())
        .sum();
    assert_eq!(mixer.voice_count(), 2);
    assert!(left_energy > 0.0, "the older matching voice keeps sounding");
    assert_eq!(
        right_energy, 0.0,
        "the newest matching voice is the one that stops"
    );
}

#[test]
fn note_off_track_matches_the_requested_key() {
    const TRACK: usize = 0;
    const LEFT_KEY: u8 = 60;
    const RIGHT_KEY: u8 = 64;

    let mut mixer = Mixer::default();
    mixer.add_voice(keyed_voice(
        60,
        TRACK,
        LEFT_KEY,
        MUTED_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
    ));
    mixer.add_voice(keyed_voice(
        60,
        TRACK,
        RIGHT_KEY,
        FULL_TRACK_VOLUME,
        MUTED_TRACK_VOLUME,
    ));
    mixer.note_off_track(TRACK, RIGHT_KEY);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let left_energy: f32 = out.iter().step_by(2).map(|sample| sample.abs()).sum();
    let right_energy: f32 = out
        .iter()
        .skip(1)
        .step_by(2)
        .map(|sample| sample.abs())
        .sum();
    assert_eq!(mixer.voice_count(), 1);
    assert!(
        left_energy > 0.0,
        "the voice on the unrequested key must keep sounding"
    );
    assert_eq!(
        right_energy, 0.0,
        "the voice on the requested key must stop"
    );
}

#[test]
fn note_off_track_releases_the_newest_matching_voice() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut mixer = Mixer::default();
    let older_left_voice = keyed_voice(60, TRACK, KEY, MUTED_TRACK_VOLUME, FULL_TRACK_VOLUME);
    let newer_right_voice = keyed_voice(60, TRACK, KEY, FULL_TRACK_VOLUME, MUTED_TRACK_VOLUME);
    mixer.add_voice(older_left_voice);
    mixer.add_voice(newer_right_voice);
    mixer.note_off_track(TRACK, KEY);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    let left_energy: f32 = out.iter().step_by(2).map(|sample| sample.abs()).sum();
    let right_energy: f32 = out
        .iter()
        .skip(1)
        .step_by(2)
        .map(|sample| sample.abs())
        .sum();
    assert_eq!(mixer.voice_count(), 1);
    assert!(left_energy > 0.0, "the older voice must keep sounding");
    assert_eq!(right_energy, 0.0, "the newer voice must be released");
}

#[test]
fn note_off_releases_newest_match_across_voice_kinds_pcm_then_cgb() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut mixer = Mixer::default();
    let older_direct_sound_voice =
        keyed_voice(50, TRACK, KEY, FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
    let newer_cgb_voice = cgb_keyed_voice(TRACK, KEY);
    mixer.add_voice(older_direct_sound_voice);
    mixer.add_cgb_voice(newer_cgb_voice);
    mixer.note_off_track(TRACK, KEY);
    let cgb = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("cgb voice present");
    assert!(cgb.is_stopping(), "newer CGB voice must be released");
    assert!(
        !mixer.voices()[0].is_stopping(),
        "older PCM voice must keep sounding"
    );
}

#[test]
fn note_off_releases_newest_match_across_voice_kinds_cgb_then_pcm() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut mixer = Mixer::default();
    let older_cgb_voice = cgb_keyed_voice(TRACK, KEY);
    let newer_direct_sound_voice =
        keyed_voice(50, TRACK, KEY, FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
    mixer.add_cgb_voice(older_cgb_voice);
    mixer.add_voice(newer_direct_sound_voice);
    mixer.note_off_track(TRACK, KEY);
    assert!(
        mixer.voices()[0].is_stopping(),
        "newer PCM voice must be released"
    );
    let cgb = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("cgb voice present");
    assert!(!cgb.is_stopping(), "older CGB voice must keep sounding");
}

#[test]
fn voice_cap_drops_extra_notes() {
    let mut mixer = Mixer::new(DEFAULT_MASTER_VOLUME, 2);
    assert!(mixer.add_voice(constant_voice(10, 0)));
    assert!(mixer.add_voice(constant_voice(10, 1)));
    assert!(!mixer.add_voice(constant_voice(10, 2)));
    assert_eq!(mixer.voice_count(), 2);
}

fn cgb_sweep_voice(sweep_byte: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        Some(sweep_byte),
        CgbAdsr::flat(),
        0,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        0,
        0,
        0,
        0,
        0,
    )
}

fn square1_sweep_frequency(mixer: &Mixer) -> Option<u16> {
    mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .map(|voice| {
            voice
                .sweep_frequency()
                .expect("the square-1 slot holds a sweeping voice")
        })
}

#[test]
fn mix_frame_sweeps_at_128hz_across_frame_boundaries() {
    const FRAMES_BEFORE_FIFTH_SWEEP_STEP: usize = 16;
    const FREQUENCY_AFTER_FOUR_STEPS: u16 = 222;
    const FREQUENCY_AFTER_FIVE_STEPS: u16 = 333;

    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_sweep_voice(PERIOD_7_UPWARD_SHIFT_1)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];

    for _ in 0..FRAMES_BEFORE_FIFTH_SWEEP_STEP {
        mixer.mix_frame(&mut out);
    }
    assert_eq!(
        square1_sweep_frequency(&mixer),
        Some(FREQUENCY_AFTER_FOUR_STEPS),
        "16 frames must be exactly 34 ticks, i.e. 4 period-7 steps"
    );

    mixer.mix_frame(&mut out);
    assert_eq!(
        square1_sweep_frequency(&mixer),
        Some(FREQUENCY_AFTER_FIVE_STEPS),
        "the 17th frame must cross tick 35 and take the 5th step"
    );
}

#[test]
fn mix_frame_retires_an_overflowing_sweep_on_the_hardware_tick_count() {
    const FRAMES_BEFORE_OVERFLOW: usize = 29;
    const FREQUENCY_BEFORE_OVERFLOW: u16 = 1122;

    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(cgb_sweep_voice(PERIOD_7_UPWARD_SHIFT_1)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];

    for _ in 0..FRAMES_BEFORE_OVERFLOW {
        mixer.mix_frame(&mut out);
    }
    assert_eq!(
        square1_sweep_frequency(&mixer),
        Some(FREQUENCY_BEFORE_OVERFLOW),
        "29 frames must be 62 ticks: 8 steps, one tick short of the overflow"
    );

    mixer.mix_frame(&mut out);
    assert!(
        square1_sweep_frequency(&mixer).is_none(),
        "the 30th frame must reach tick 63, overflow, and free the slot"
    );
    assert_eq!(mixer.voice_count(), 0);
}

#[test]
fn a_frames_sweep_tick_buffer_never_has_to_grow() {
    const FRAMES_TO_SAMPLE: usize = 1000;

    assert_eq!(MAX_SWEEP_TICKS_PER_FRAME, 3);

    let mut clock = FrameSequencer128Hz::default();
    let mut ticks = Vec::new();
    let mut seen_three = false;
    for _ in 0..FRAMES_TO_SAMPLE {
        clock.advance_into(SAMPLES_PER_FRAME, &mut ticks);
        assert!(
            ticks.len() <= MAX_SWEEP_TICKS_PER_FRAME,
            "a frame produced {} ticks",
            ticks.len()
        );
        seen_three |= ticks.len() == MAX_SWEEP_TICKS_PER_FRAME;
    }
    assert!(seen_three, "the fractional carry must reach three ticks");
}
