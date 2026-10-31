//! Channel-3 wave-RAM rotation through replacement, retirement, and reload.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::psg::WaveChannel;

const TRACK: usize = 0;
const KEY: u8 = 60;
const IDLE_FRAMES: usize = 3;
/// Whole wave-RAM steps one rendered frame advances at [`KEY`], unfixed.
const ROTATION_PER_FRAME: usize = 6;

/// Ascending then descending nibbles, so every rotation is distinct.
const RAMP_RAM: [u8; 16] = [
    0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54, 0x32, 0x10,
];

fn wave_voice(id: &str, samples: [i8; 32]) -> CgbVoice {
    CgbVoice::wave(
        Arc::from(id),
        samples,
        CgbAdsr::flat(),
        false,
        KEY,
        0,
        FULL_TRACK_VOLUME,
        FULL_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        KEY,
        TRACK,
        0,
        0,
        0,
    )
}

fn ramp() -> [i8; 32] {
    WaveChannel::decode_wave_ram(&RAMP_RAM)
}

fn rotated_ramp() -> [i8; 32] {
    let mut samples = ramp();
    samples.rotate_left(ROTATION_PER_FRAME);
    samples
}

fn other_ramp() -> [i8; 32] {
    let mut samples = ramp();
    samples.reverse();
    samples
}

fn frame(mixer: &mut Mixer) -> Vec<f32> {
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    out
}

fn fresh_frame(samples: [i8; 32]) -> Vec<f32> {
    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(mixer.add_cgb_voice(wave_voice("oracle", samples)));
    frame(&mut mixer)
}

fn sounding_mixer() -> Mixer {
    let mut mixer = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(mixer.add_cgb_voice(wave_voice("ramp-a", ramp())));
    frame(&mut mixer);
    mixer
}

/// A replacement keeps the rotation but holds the nibble the previous clock
/// removed until its first clock (`mgba/src/gb/audio.c:557-568`).
fn assert_continues_rotation(actual: &[f32]) {
    let oracle = fresh_frame(rotated_ramp());
    let first_clock = actual
        .iter()
        .position(|s| s.to_bits() != actual[0].to_bits())
        .expect("the first wave clock lands within one frame");
    assert!(first_clock > 0);
    assert_ne!(
        actual[0].to_bits(),
        oracle[0].to_bits(),
        "the retained latch is the removed nibble"
    );
    let bits = |samples: &[f32]| samples.iter().map(|s| s.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&actual[first_clock..]), bits(&oracle[first_clock..]));
}

#[test]
fn an_immediate_same_wave_replacement_keeps_the_rotation() {
    let mut mixer = sounding_mixer();
    assert!(mixer.add_cgb_voice(wave_voice("ramp-a", ramp())));
    let actual = frame(&mut mixer);
    assert_continues_rotation(&actual);
    assert_ne!(actual, fresh_frame(ramp()));
}

#[test]
fn a_retired_wave_slot_keeps_the_rotation_through_idle_frames() {
    let mut mixer = sounding_mixer();
    mixer.note_off_track(TRACK, KEY);
    assert!(frame(&mut mixer).iter().all(|&s| s == 0.0));
    for _ in 0..IDLE_FRAMES {
        assert!(frame(&mut mixer).iter().all(|&s| s == 0.0));
    }
    assert!(mixer.add_cgb_voice(wave_voice("ramp-a", ramp())));
    let actual = frame(&mut mixer);
    assert_continues_rotation(&actual);
    assert_ne!(actual, fresh_frame(ramp()));
}

#[test]
fn a_stopped_wave_slot_keeps_the_rotation_through_idle_frames() {
    let mut mixer = sounding_mixer();
    mixer.stop_track(TRACK);
    for _ in 0..IDLE_FRAMES {
        assert!(frame(&mut mixer).iter().all(|&s| s == 0.0));
    }
    assert!(mixer.add_cgb_voice(wave_voice("ramp-a", ramp())));
    assert_continues_rotation(&frame(&mut mixer));
}

/// A changed asset reloads the unrotated table but keeps the held output
/// nibble until the first clock (`mgba/src/gb/audio.c:300-337`).
fn assert_reloads_after_first_clock(actual: &[f32], oracle: &[f32]) {
    let first_clock = actual
        .iter()
        .position(|s| s.to_bits() != actual[0].to_bits())
        .expect("the first wave clock lands within one frame");
    assert!(first_clock > 0);
    assert_ne!(
        actual[0].to_bits(),
        oracle[0].to_bits(),
        "the retained latch is the previous slot's nibble"
    );
    let bits = |samples: &[f32]| samples.iter().map(|s| s.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&actual[first_clock..]), bits(&oracle[first_clock..]));
}

#[test]
fn a_changed_wave_reloads_the_unrotated_table() {
    let mut mixer = sounding_mixer();
    assert!(mixer.add_cgb_voice(wave_voice("ramp-b", other_ramp())));
    assert_reloads_after_first_clock(&frame(&mut mixer), &fresh_frame(other_ramp()));
}

#[test]
fn a_changed_wave_id_reloads_even_with_identical_bytes() {
    let mut mixer = sounding_mixer();
    assert!(mixer.add_cgb_voice(wave_voice("ramp-copy", ramp())));
    let actual = frame(&mut mixer);
    assert_reloads_after_first_clock(&actual, &fresh_frame(ramp()));
    assert_ne!(actual, fresh_frame(rotated_ramp()));
}
