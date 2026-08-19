//! Frame-mixing tests for [`Mixer`]: envelope scaling, summing, clipping and
//! `EOT` note-off selection across the DirectSound pool and the CGB slots.
//! The note-on channel search has its own sibling, [`super::priority_tests`].

use std::sync::Arc;

use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;
use crate::envelope::Adsr;
use crate::pitch::{DIV_FREQ, FRAC_BITS};
use crate::sample::WaveData;

fn unity_freq() -> u32 {
    (1 << FRAC_BITS) / DIV_FREQ
}

/// A tied square-1 CGB voice on `track` at `key`, for cross-kind
/// end-of-tie tests. Its hardware slot is [`CgbChannelNumber::Square1`].
fn cgb_keyed_voice(track: usize, key: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square1,
        2,
        None,
        CgbAdsr::flat(),
        key,
        0,
        0xFF,
        0xFF,
        127,
        0,
        key,
        track,
        0,
        0,
        0,
    )
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
fn note_off_releases_newest_match_across_voice_kinds_pcm_then_cgb() {
    // Same track, same key: an older DirectSound voice then a newer CGB
    // voice (as a `VOICE` change between overlapping ties would produce).
    // Upstream chains both kinds together newest-first, so an `EOT` must
    // release the newer CGB voice — not the older PCM one. Before the fix
    // `note_off_track` scanned the whole PCM pool first and wrongly
    // released the older PCM voice.
    let mut mixer = Mixer::default();
    mixer.add_voice(keyed_voice(50, 0, 60, 0xFF, 0xFF)); // older PCM, seq 0
    mixer.add_cgb_voice(cgb_keyed_voice(0, 60)); // newer CGB, seq 1
    mixer.note_off_track(0, 60);
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
    // The mirror case: an older CGB voice then a newer DirectSound voice.
    // The `EOT` must release the newer PCM voice, leaving the CGB sounding.
    let mut mixer = Mixer::default();
    mixer.add_cgb_voice(cgb_keyed_voice(0, 60)); // older CGB, seq 0
    mixer.add_voice(keyed_voice(50, 0, 60, 0xFF, 0xFF)); // newer PCM, seq 1
    mixer.note_off_track(0, 60);
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
