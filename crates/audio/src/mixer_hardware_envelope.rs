//! `Mixer`-level tests for the hardware envelope clock: every channel's
//! hardware envelope counts the one shared 64 Hz sequencer, wherever its note
//! began.

use super::test_support::OUTPUT_SCALE;
use super::*;
use crate::cgb_envelope::CgbAdsr;
use crate::cgb_voice::CgbChannelNumber;

/// Hardware attack 17 decodes to an upward period of one event.
const CLIMBING_EVERY_EVENT: CgbAdsr = CgbAdsr {
    attack: 17,
    decay: 0,
    sustain: 15,
    release: 0,
};

fn climbing_voice(channel: CgbChannelNumber, track: usize) -> CgbVoice {
    CgbVoice::square(
        channel,
        2,
        None,
        CLIMBING_EVERY_EVENT,
        60,
        0,
        0xFF,
        0xFF,
        127,
        0,
        60,
        track,
        0,
        0,
        0,
    )
}

/// The left lane of one mixed frame in 1/128 units: a hardware volume unit is
/// two of them.
fn left_magnitudes(mixer: &mut Mixer) -> Vec<i32> {
    let mut out = vec![0.0f32; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    out.chunks_exact(2)
        .map(|pair| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a mixed lane is a multiple of 1/128 within [-1, 1)"
            )]
            let units = (pair[0] * OUTPUT_SCALE).round().abs() as i32;
            units
        })
        .collect()
}

/// The hardware volume each sample of a frame holds when the envelope events
/// fall at `events`, starting at `start`, one level per event.
fn levels_climbing_from(start: i32, events: &[usize]) -> Vec<i32> {
    (0..SAMPLES_PER_FRAME)
        .map(|sample| {
            start + i32::try_from(events.iter().filter(|&&e| e <= sample).count()).unwrap()
        })
        .collect()
}

#[test]
fn a_frames_envelope_tick_buffer_never_has_to_grow() {
    const FRAMES_TO_SAMPLE: usize = 1000;

    assert_eq!(MAX_ENVELOPE_TICKS_PER_FRAME, 2);

    let mut clock = FrameSequencer::default();
    let mut clocks = FrameClocks::with_capacity(0, 0, 0);
    let mut seen_two = false;
    for _ in 0..FRAMES_TO_SAMPLE {
        clock.advance_clocks_into(SAMPLES_PER_FRAME, &mut clocks);
        assert!(
            clocks.envelope.len() <= MAX_ENVELOPE_TICKS_PER_FRAME,
            "a frame produced {} envelope clocks",
            clocks.envelope.len()
        );
        seen_two |= clocks.envelope.len() == MAX_ENVELOPE_TICKS_PER_FRAME;
    }
    assert!(seen_two, "the fractional carry must reach two clocks");
}

#[test]
fn a_note_added_after_an_empty_frame_steps_at_the_shared_clocks_events() {
    let mut mixer = Mixer::default();
    let mut out = vec![0.0f32; SAMPLES_PER_FRAME * 2];
    mixer.mix_frame(&mut out);
    assert!(mixer.add_cgb_voice(climbing_voice(CgbChannelNumber::Square1, 0)));

    // The second frame's events fall at 11 and 220, not at the 26 a clock
    // restarted by the note would give.
    let heard = left_magnitudes(&mut mixer);
    let expected: Vec<i32> = levels_climbing_from(0, &[11, 220])
        .into_iter()
        .map(|level| 2 * level)
        .collect();
    assert_eq!(heard, expected);
}

#[test]
fn channels_added_on_different_frames_share_one_clock() {
    let mut mixer = Mixer::default();
    let mut out = vec![0.0f32; SAMPLES_PER_FRAME * 2];
    assert!(mixer.add_cgb_voice(climbing_voice(CgbChannelNumber::Square1, 0)));
    mixer.mix_frame(&mut out);
    assert!(mixer.add_cgb_voice(climbing_voice(CgbChannelNumber::Square2, 1)));

    // Square1 is at level 1 and climbs on the second frame's events at 11 and
    // 220; Square2 starts at zero and climbs on the same two.
    mixer.mix_frame(&mut out);
    let hardware_volume = |channel: CgbChannelNumber| {
        mixer.cgb_voices()[channel.slot()]
            .as_ref()
            .map(CgbVoice::hardware_volume)
    };
    assert_eq!(hardware_volume(CgbChannelNumber::Square1), Some(3));
    assert_eq!(hardware_volume(CgbChannelNumber::Square2), Some(2));
}

#[test]
fn a_replacement_restarts_its_own_countdown_but_not_the_clock() {
    let mut mixer = Mixer::default();
    let mut out = vec![0.0f32; SAMPLES_PER_FRAME * 2];
    assert!(mixer.add_cgb_voice(climbing_voice(CgbChannelNumber::Square1, 0)));
    mixer.mix_frame(&mut out);
    assert!(mixer.add_cgb_voice(climbing_voice(CgbChannelNumber::Square1, 0)));

    let heard = left_magnitudes(&mut mixer);
    let expected: Vec<i32> = levels_climbing_from(0, &[11, 220])
        .into_iter()
        .map(|level| 2 * level)
        .collect();
    assert_eq!(
        heard, expected,
        "the replacement starts silent and climbs on the clock's own events"
    );
}
