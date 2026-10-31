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
    routed_wave_voice(id, samples, FULL_TRACK_VOLUME, FULL_TRACK_VOLUME)
}

fn routed_wave_voice(id: &str, samples: [i8; 32], vol_mr: u8, vol_ml: u8) -> CgbVoice {
    CgbVoice::wave(
        Arc::from(id),
        samples,
        CgbAdsr::flat(),
        false,
        KEY,
        0,
        vol_mr,
        vol_ml,
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

/// The output a wave channel holds once it stops clocking: the latch the
/// still-sounding mixer presents at its next sample
/// (`mgba/src/gb/audio.c:246-253,768-774`).
fn held_output() -> Vec<f32> {
    let mut continued = sounding_mixer();
    let held = frame(&mut continued)[..2].to_vec();
    assert!(
        held.iter().any(|&s| s != 0.0),
        "sanity: the held nibble is off-centre"
    );
    held
}

fn assert_frame_holds(actual: &[f32], held: &[f32]) {
    for (index, pair) in actual.as_chunks::<2>().0.iter().enumerate() {
        assert_eq!(pair, held, "sample {index} is the held latch");
    }
}

#[test]
fn a_retired_wave_slot_keeps_the_rotation_through_idle_frames() {
    let held = held_output();
    let mut mixer = sounding_mixer();
    mixer.note_off_track(TRACK, KEY);
    assert_frame_holds(&frame(&mut mixer), &held);
    for _ in 0..IDLE_FRAMES {
        assert_frame_holds(&frame(&mut mixer), &held);
    }
    assert!(mixer.add_cgb_voice(wave_voice("ramp-a", ramp())));
    let actual = frame(&mut mixer);
    assert_continues_rotation(&actual);
    assert_ne!(actual, fresh_frame(ramp()));
}

#[test]
fn a_stopped_wave_slot_keeps_the_rotation_through_idle_frames() {
    let held = held_output();
    let mut mixer = sounding_mixer();
    mixer.stop_track(TRACK);
    for _ in 0..IDLE_FRAMES {
        assert_frame_holds(&frame(&mut mixer), &held);
    }
    assert!(mixer.add_cgb_voice(wave_voice("ramp-a", ramp())));
    assert_continues_rotation(&frame(&mut mixer));
}

/// mGBA keeps mixing channel 3's held sample through NR51 after `NR30 = 0`
/// (`mgba/src/gb/audio.c:246-253,768-774`; `pokeemerald/src/m4a.c:869-871`),
/// so stopping a wave track must not step its output to zero.
#[test]
fn a_stopped_wave_slot_keeps_emitting_its_held_sample() {
    let held = held_output();
    let mut stopped = sounding_mixer();
    stopped.stop_track(TRACK);
    let idle = frame(&mut stopped);
    assert_eq!(
        idle[..2],
        held[..],
        "the first idle sample is the held latch"
    );
}

/// The off-write leaves NR51 alone, so the held sample reaches only the sides
/// the voice last committed.
#[test]
fn a_retired_wave_slot_holds_its_sample_on_the_committed_route_only() {
    let mut continued = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(continued.add_cgb_voice(routed_wave_voice("ramp-a", ramp(), FULL_TRACK_VOLUME, 0)));
    frame(&mut continued);
    let held = frame(&mut continued)[..2].to_vec();
    assert_eq!(held[0].to_bits(), 0, "sanity: the left side is not routed");
    assert_ne!(
        held[1].to_bits(),
        0,
        "sanity: the right side carries the nibble"
    );

    let mut stopped = Mixer::new(MAX_MASTER_VOLUME, 1);
    assert!(stopped.add_cgb_voice(routed_wave_voice("ramp-a", ramp(), FULL_TRACK_VOLUME, 0)));
    frame(&mut stopped);
    stopped.stop_track(TRACK);
    assert_frame_holds(&frame(&mut stopped), &held);
}

/// A held level does not rotate the table: the next voice sees the head it
/// left when the wave stopped clocking.
#[test]
fn a_held_wave_slot_does_not_advance_its_rotation() {
    let mut mixer = sounding_mixer();
    mixer.stop_track(TRACK);
    for _ in 0..IDLE_FRAMES * 4 {
        frame(&mut mixer);
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
