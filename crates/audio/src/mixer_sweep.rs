//! Mixer-level sweep tick cadence, overflow muting, and tick-buffer bounds.

use super::test_support::*;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;

const PERIOD_7_UPWARD_SHIFT_1: u8 = 0x71;

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
