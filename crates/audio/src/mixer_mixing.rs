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
    cgb_swept_voice(track, key, None)
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

/// A square-channel voice on `channel` with the given envelope and
/// pseudo-echo tail, on track 0 and key 60 like [`cgb_keyed_voice`].
fn cgb_endtie_voice(
    channel: CgbChannelNumber,
    adsr: CgbAdsr,
    echo_volume: u8,
    echo_length: u8,
) -> CgbVoice {
    CgbVoice::square(
        channel,
        2,
        None,
        adsr,
        60,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        60,
        0,
        0,
        echo_volume,
        echo_length,
    )
}

#[test]
fn end_tie_skips_a_cgb_channel_in_its_automatic_pseudo_echo_tail() {
    // A zero-sustain CGB envelope completes on its own into the automatic
    // pseudo-echo tail that `CgbEnvelope::is_end_tie_eligible`'s doc excludes.
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut mixer = Mixer::default();
    let older_sustaining_voice = cgb_endtie_voice(CgbChannelNumber::Square2, CgbAdsr::flat(), 0, 0);
    let newer_echo_voice = cgb_endtie_voice(
        CgbChannelNumber::Square1,
        CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: 0,
            release: 0,
        },
        128,
        60,
    );
    assert!(mixer.add_cgb_voice(older_sustaining_voice));
    assert!(mixer.add_cgb_voice(newer_echo_voice));

    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    assert!(
        mixer.cgb_voices()[CgbChannelNumber::Square1.slot()].is_some(),
        "the zero-sustain voice must have entered its pseudo-echo tail, not retired"
    );

    mixer.note_off_track(TRACK, KEY);

    let echo = mixer.cgb_voices()[CgbChannelNumber::Square1.slot()]
        .as_ref()
        .expect("echo voice present");
    assert!(
        !echo.is_stopping(),
        "the end-tie must walk past the IEC-only pseudo-echo channel"
    );
    let older = mixer.cgb_voices()[CgbChannelNumber::Square2.slot()]
        .as_ref()
        .expect("older voice present");
    assert!(
        older.is_stopping(),
        "the older same-key channel must be the one the end-tie stops"
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
fn mix_frame_mutes_an_overflowing_sweep_on_the_hardware_tick_count() {
    const FRAMES_BEFORE_OVERFLOW: usize = 29;
    const FREQUENCY_BEFORE_OVERFLOW: u16 = 1122;
    const FREQUENCY_AT_OVERFLOW: u16 = 1683;

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
    assert_eq!(
        square1_sweep_frequency(&mixer),
        Some(FREQUENCY_AT_OVERFLOW),
        "the 30th frame must reach tick 63, take the 9th step, and overflow"
    );
    assert_eq!(
        mixer.voice_count(),
        1,
        "the overflow mutes the hardware channel, keeping the slot for a later trigger"
    );

    mixer.mix_frame(&mut out);
    assert!(
        out.iter().all(|&sample| sample == 0.0),
        "a muted channel contributes nothing to the frames after its overflow"
    );
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

/// CGB output must never reach the DirectSound reverb ring
/// (`crate::reverb`'s module doc carries the upstream citation).
#[test]
fn a_cgb_only_mix_never_feeds_the_directsound_reverb_ring() {
    const REVERB_LEVEL: u8 = 100;
    const FRAMES: usize = 4;

    let mut mixer =
        Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES).with_reverb_level(REVERB_LEVEL);
    assert!(mixer.add_cgb_voice(cgb_keyed_voice(0, 60)));

    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut heard_cgb = false;
    for _ in 0..FRAMES {
        mixer.mix_frame(&mut out);
        heard_cgb |= out.iter().any(|&sample| sample != 0.0);
    }

    assert!(heard_cgb, "the CGB voice must actually be audible");
    assert!(
        !mixer.has_pending_reverb(),
        "CGB output must not enter the DirectSound reverb ring",
    );
}

/// A CGB voice sounding alongside DirectSound must still leave the reverb
/// ring exactly as a DirectSound-only mix would, once both retire: the tail
/// echoed back must match bit-for-bit against a twin mixer that never had a
/// CGB voice at all.
#[test]
fn a_cgb_voice_alongside_directsound_leaves_the_reverb_tap_directsound_only() {
    const REVERB_LEVEL: u8 = 100;
    const DIRECT_SAMPLE: i8 = 40;
    const DIRECT_TRACK: usize = 0;
    const CGB_TRACK: usize = 1;
    const CGB_KEY: u8 = 60;
    const MAX_SETTLE_FRAMES: usize = 32;

    let mut combined =
        Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES).with_reverb_level(REVERB_LEVEL);
    let mut direct_only =
        Mixer::new(MAX_MASTER_VOLUME, DEFAULT_MAX_VOICES).with_reverb_level(REVERB_LEVEL);
    assert!(combined.add_voice(constant_voice(DIRECT_SAMPLE, DIRECT_TRACK)));
    assert!(direct_only.add_voice(constant_voice(DIRECT_SAMPLE, DIRECT_TRACK)));
    assert!(combined.add_cgb_voice(cgb_keyed_voice(CGB_TRACK, CGB_KEY)));

    let mut combined_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut direct_out = vec![0.0; SAMPLES_PER_FRAME * 2];

    combined.mix_frame(&mut combined_out);
    direct_only.mix_frame(&mut direct_out);
    assert_ne!(
        combined_out, direct_out,
        "the CGB voice must audibly change the combined output"
    );

    combined.note_off_track(CGB_TRACK, CGB_KEY);

    let mut settled = false;
    for _ in 0..MAX_SETTLE_FRAMES {
        combined.mix_frame(&mut combined_out);
        direct_only.mix_frame(&mut direct_out);
        if combined.is_idle() && direct_only.is_idle() {
            settled = true;
            break;
        }
    }
    assert!(
        settled,
        "both mixers must fall silent within the settle window"
    );

    let ring_frames = crate::reverb::DELAY_SAMPLES.div_ceil(SAMPLES_PER_FRAME);
    for offset in 0..ring_frames {
        combined.mix_frame(&mut combined_out);
        direct_only.mix_frame(&mut direct_out);
        assert_eq!(
            combined_out, direct_out,
            "reverb tail must match a DirectSound-only mix at offset {offset}"
        );
    }
}

/// A note taking over an occupied square slot must pick the duty phase up
/// where the note it replaced left it (`SquareChannel::continue_duty_from`'s doc).
#[test]
fn a_square_note_on_continues_the_duty_phase_of_the_note_it_replaces() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;

    let mut retriggered = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut sustaining = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(retriggered.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    assert!(sustaining.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));

    let mut retriggered_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut sustaining_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    retriggered.mix_frame(&mut retriggered_out);
    sustaining.mix_frame(&mut sustaining_out);
    assert_eq!(
        retriggered_out, sustaining_out,
        "the two mixers must still be identical before the re-attack",
    );

    // Same track, same key, same priority: the slot is reusable, so this note
    // replaces the sounding one for the next frame.
    assert!(retriggered.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));

    retriggered.mix_frame(&mut retriggered_out);
    sustaining.mix_frame(&mut sustaining_out);
    assert_eq!(
        retriggered_out, sustaining_out,
        "a square note-on must carry the channel's duty phase forward instead \
         of restarting the duty table at index zero",
    );
}

/// A note landing on a square slot that `stop_track` vacated must continue
/// the duty index the slot's free-running register would have reached over
/// that silence, not restart at zero (`CgbVoice::advance_idle_duty`'s doc).
#[test]
fn a_square_note_on_a_slot_stop_track_vacated_continues_the_idle_duty_phase() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;
    const IDLE_FRAMES: usize = 3;

    let mut occupied = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut vacated = Mixer::new(MAX_MASTER_VOLUME, 1);
    // A slot's off-write truncates its frequency the instant it idles
    // (`CgbVoice::apply_hardware_off_write`'s doc); a slot rated the same
    // way and left occupied for the same span is the oracle.
    let mut idle_rate_reference = cgb_keyed_voice(TRACK, KEY);
    idle_rate_reference.apply_hardware_off_write();
    assert!(occupied.add_cgb_voice(idle_rate_reference));
    assert!(vacated.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    vacated.stop_track(TRACK);

    let mut occupied_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut vacated_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for frame in 0..IDLE_FRAMES {
        occupied.mix_frame(&mut occupied_out);
        vacated.mix_frame(&mut vacated_out);
        assert!(
            vacated_out.iter().all(|&sample| sample == 0.0),
            "frame {frame}: a vacated slot must render silence"
        );
    }

    assert!(occupied.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    assert!(vacated.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));

    occupied.mix_frame(&mut occupied_out);
    vacated.mix_frame(&mut vacated_out);
    assert_eq!(
        occupied_out, vacated_out,
        "a note on a stop_track-vacated slot must continue the duty index \
         the slot's free-running register would have reached, not reset it \
         to zero",
    );
}

/// The same continuation, this time through natural envelope retirement
/// instead of `stop_track`.
#[test]
fn a_square_note_on_a_naturally_retired_slot_continues_the_idle_duty_phase() {
    const TRACK: usize = 0;
    const KEY: u8 = 60;
    const IDLE_FRAMES: usize = 3;
    const MAX_FRAMES_TO_RETIRE: usize = 32;

    let mut occupied = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut retired = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(occupied.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    assert!(retired.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    retired.note_off_track(TRACK, KEY);

    let mut occupied_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut retired_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut frames_to_retire = 0;
    while retired.cgb_voices()[CgbChannelNumber::Square1.slot()].is_some() {
        retired.mix_frame(&mut retired_out);
        // The instant a slot idles, its off-write truncates the rate its
        // duty catch-up measures by; switch the still-sounding oracle to
        // that same rate, at the same instant, to track it.
        if retired.cgb_voices()[CgbChannelNumber::Square1.slot()].is_none() {
            let mut idle_rate_reference = cgb_keyed_voice(TRACK, KEY);
            idle_rate_reference.apply_hardware_off_write();
            assert!(occupied.add_cgb_voice(idle_rate_reference));
        }
        occupied.mix_frame(&mut occupied_out);
        frames_to_retire += 1;
        assert!(
            frames_to_retire < MAX_FRAMES_TO_RETIRE,
            "test setup must retire well within this bound"
        );
    }

    for frame in 0..IDLE_FRAMES {
        occupied.mix_frame(&mut occupied_out);
        retired.mix_frame(&mut retired_out);
        assert!(
            retired_out.iter().all(|&sample| sample == 0.0),
            "frame {frame}: a retired slot must render silence while idle"
        );
    }

    assert!(occupied.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));
    assert!(retired.add_cgb_voice(cgb_keyed_voice(TRACK, KEY)));

    occupied.mix_frame(&mut occupied_out);
    retired.mix_frame(&mut retired_out);
    assert_eq!(
        occupied_out, retired_out,
        "a note on a naturally retired slot must continue the duty index \
         the slot's free-running register would have reached, not reset it \
         to zero",
    );
}

// Overflows at trigger, muting the hardware from frame 0 while the
// envelope keeps running (`square1_upward_sweep_overflow_is_born_muted_and_revives_on_a_safe_trigger`,
// cgb_voice.rs).
const OVERFLOWING_SWEEP_AT_TRIGGER: u8 = 0x31;
const OVERFLOW_KEY: u8 = 120;

fn cgb_muted_at_trigger_voice(track: usize, key: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        Some(OVERFLOWING_SWEEP_AT_TRIGGER),
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

/// A hardware-muted voice's unconditional duty catch-up in `render` must
/// not also double-count the frame its envelope retires on top of the
/// idle-duty loop's own advance for that same frame.
#[test]
fn a_note_on_a_slot_a_muted_voice_vacated_does_not_double_count_the_retirement_frame() {
    const TRACK: usize = 0;
    const IDLE_FRAMES: usize = 3;
    const MAX_FRAMES_TO_RETIRE: usize = 32;

    let mut occupied = Mixer::new(MAX_MASTER_VOLUME, 1);
    let mut retired = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(retired.add_cgb_voice(cgb_muted_at_trigger_voice(TRACK, OVERFLOW_KEY)));
    retired.note_off_track(TRACK, OVERFLOW_KEY);

    let mut occupied_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut retired_out = vec![0.0; SAMPLES_PER_FRAME * 2];
    let mut frames_to_retire = 0;
    while retired.cgb_voices()[CgbChannelNumber::Square1.slot()].is_some() {
        retired.mix_frame(&mut retired_out);
        assert!(
            retired_out.iter().all(|&sample| sample == 0.0),
            "a muted voice must render silence while it still holds the slot"
        );
        if retired.cgb_voices()[CgbChannelNumber::Square1.slot()].is_none() {
            // The oracle idles through `stop_track`, which counts the frame
            // once, at the same vacated instant.
            assert!(occupied.add_cgb_voice(cgb_muted_at_trigger_voice(TRACK, OVERFLOW_KEY)));
            occupied.stop_track(TRACK);
        }
        occupied.mix_frame(&mut occupied_out);
        frames_to_retire += 1;
        assert!(
            frames_to_retire < MAX_FRAMES_TO_RETIRE,
            "test setup must retire well within this bound"
        );
    }

    for frame in 0..IDLE_FRAMES {
        occupied.mix_frame(&mut occupied_out);
        retired.mix_frame(&mut retired_out);
        assert!(
            retired_out.iter().all(|&sample| sample == 0.0),
            "frame {frame}: a retired slot must render silence while idle"
        );
    }

    assert!(occupied.add_cgb_voice(cgb_keyed_voice(TRACK, OVERFLOW_KEY)));
    assert!(retired.add_cgb_voice(cgb_keyed_voice(TRACK, OVERFLOW_KEY)));

    occupied.mix_frame(&mut occupied_out);
    retired.mix_frame(&mut retired_out);
    assert_eq!(
        occupied_out, retired_out,
        "a note on a slot a muted voice vacated must not double-count the \
         retirement frame's silence",
    );
}

fn cgb_swept_voice(track: usize, key: u8, sweep: Option<u8>) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        sweep,
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

/// Mixes `idle_frames` over a Square1 slot `stop_track` vacated from a voice
/// carrying `sweep`, then plays an unswept note on it and returns that frame.
fn frame_after_idling_a_stopped_square1(
    sweep: Option<u8>,
    key: u8,
    idle_frames: usize,
) -> Vec<f32> {
    const TRACK: usize = 0;

    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(mixer.add_cgb_voice(cgb_swept_voice(TRACK, key, sweep)));
    mixer.stop_track(TRACK);
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..idle_frames {
        mixer.mix_frame(&mut out);
    }
    assert!(mixer.add_cgb_voice(cgb_swept_voice(TRACK, key, None)));
    mixer.mix_frame(&mut out);
    out
}

/// A vacated Square1 slot's hardware sweep keeps ticking and retuning while
/// it idles, so the next note inherits a different duty phase than an
/// unswept slot's (`CgbVoice::advance_idle_duty`'s doc).
#[test]
fn verify_idle_square1_sweep_keeps_ticking_after_stop_track() {
    const KEY: u8 = 60;
    const DOWNWARD_PERIOD_1_SHIFT_1: u8 = 0x19;

    assert_ne!(
        frame_after_idling_a_stopped_square1(Some(DOWNWARD_PERIOD_1_SHIFT_1), KEY, 3),
        frame_after_idling_a_stopped_square1(None, KEY, 3),
        "an idle slot's sweep must retune the duty rate the next note continues",
    );
}
