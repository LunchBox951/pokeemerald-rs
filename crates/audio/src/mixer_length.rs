//! The mixer hands every CGB voice the shared frame sequencer's length clock.

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;
use crate::psg::WaveChannel;
use std::sync::Arc;

fn length_voice(length: u8) -> CgbVoice {
    CgbVoice::square(
        CgbChannelNumber::Square2,
        2,
        None,
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
    .with_length(length)
}

fn mix(length: u8, frames: usize) -> Vec<f32> {
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(length_voice(length)));
    let mut samples = Vec::new();
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..frames {
        mixer.mix_frame(&mut out);
        samples.extend_from_slice(&out);
    }
    samples
}

#[test]
fn a_length_cut_note_goes_still_at_its_counter_expiry_in_the_mixed_output() {
    // Length 48 expires at sample 783 from the sequencer's epoch
    // (`cgb_voice_length.rs`'s `CUT_SAMPLE`).
    const EXPIRY_SAMPLE: usize = 783;
    let cut = mix(48, 8);
    let uncut = mix(0, 8);

    let bits = |samples: &[f32]| -> Vec<u32> { samples.iter().map(|s| s.to_bits()).collect() };
    let (cut, uncut) = (bits(&cut), bits(&uncut));
    assert_eq!(cut[..EXPIRY_SAMPLE * 2], uncut[..EXPIRY_SAMPLE * 2]);
    let held = cut[(EXPIRY_SAMPLE - 1) * 2];
    assert!(
        cut[(EXPIRY_SAMPLE - 1) * 2..]
            .iter()
            .step_by(2)
            .all(|&sample| sample == held),
        "the stopped channel holds one level"
    );
    assert!(
        uncut[EXPIRY_SAMPLE * 2..]
            .windows(2)
            .any(|pair| pair[0] != pair[1]),
        "sanity: the uncut note keeps oscillating"
    );
}

fn wave_length_voice(length: u8) -> CgbVoice {
    CgbVoice::wave(
        Arc::from("full-swing"),
        WaveChannel::decode_wave_ram(&[0x0F; 16]),
        CgbAdsr::flat(),
        false,
        0,
        0,
        MAX_TRACK_VOLUME,
        MAX_TRACK_VOLUME,
        TEST_VELOCITY,
        TIED_GATE_TIME,
        0,
        0,
        0,
        0,
        0,
    )
    .with_length(length)
}

fn mix_wave(length: u8, frames: usize) -> Vec<u32> {
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(wave_length_voice(length)));
    let mut samples = Vec::new();
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    for _ in 0..frames {
        mixer.mix_frame(&mut out);
        samples.extend(out.iter().map(|s| s.to_bits()));
    }
    samples
}

#[test]
fn a_wave_note_cut_by_its_length_holds_its_latch_in_the_mixed_output() {
    // Raw length 252 loads 4, 3 after the enable edge: the third 256 Hz clock
    // is sample 156 (`cgb_voice_length.rs`'s `WAVE_TIED_LENGTH`).
    const EXPIRY_SAMPLE: usize = 156;
    let cut = mix_wave(252, 4);
    let uncut = mix_wave(0, 4);

    assert_eq!(cut[..EXPIRY_SAMPLE * 2], uncut[..EXPIRY_SAMPLE * 2]);
    let held = uncut[EXPIRY_SAMPLE * 2];
    assert!(
        cut[EXPIRY_SAMPLE * 2..]
            .iter()
            .step_by(2)
            .all(|&sample| sample == held),
        "the stopped channel holds the latch its last clock left"
    );
    assert!(
        uncut[EXPIRY_SAMPLE * 2..]
            .windows(2)
            .any(|pair| pair[0] != pair[1]),
        "sanity: the uncut note keeps oscillating"
    );
}

#[test]
fn a_vacated_wave_slot_keeps_counting_its_length() {
    // `NR30 = 0` does not stop the counter (`mgba/src/gb/audio.c:690-696`).
    let wave_slot = CgbChannelNumber::Wave.slot();
    let mut mixer = Mixer::default();
    assert!(mixer.add_cgb_voice(wave_length_voice(192)));
    let mut out = vec![0.0; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    mixer.stop_track(0);

    let counter = |mixer: &Mixer| {
        mixer.idle_cgb_hardware[wave_slot]
            .as_ref()
            .expect("the vacated wave slot keeps its hardware")
            .length_counter()
    };
    let before = counter(&mixer);
    assert!(before.is_enabled());
    assert!(
        before.remaining() > 40,
        "sanity: one frame leaves most of 64"
    );
    for _ in 0..40 {
        mixer.mix_frame(&mut out);
    }
    assert_eq!(counter(&mixer).remaining(), 0);
}
