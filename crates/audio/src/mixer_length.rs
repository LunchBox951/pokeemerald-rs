//! The mixer hands every CGB voice the shared frame sequencer's length clock.

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;

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
