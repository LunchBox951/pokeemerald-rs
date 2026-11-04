//! The hardware envelope timer counts the shared sequencer's phase-7 events
//! wherever a render buffer ends, and only while its channel is playing
//! (`mgba/src/gb/audio.c:710-732`).

use super::fixture::*;
use super::*;

/// Hardware period one downward from the goal's nibble 15.
const FALLING_EVERY_EVENT: CgbAdsr = CgbAdsr {
    attack: 0,
    decay: 1,
    sustain: 0,
    release: 0,
};
const TIED_LENGTH: u8 = 60;
const FRAMES: usize = 6;

fn falling_square() -> CgbVoice {
    square_voice_with_adsr(
        CgbChannelNumber::Square2,
        None,
        FALLING_EVERY_EVENT,
        TestNote::default(),
    )
}

fn falling_noise() -> CgbVoice {
    noise_voice(FALLING_EVERY_EVENT, WIDE_NOISE, TestNote::default())
}

/// Renders `total` samples of one software frame's worth of state, split into
/// `chunks` at the buffer boundaries, never beginning another software frame.
fn render_in_chunks(voice: &mut CgbVoice, chunks: &[usize]) -> Vec<StereoAcc> {
    let mut clock = SharedClock::new();
    voice.set_sequencer_step(FrameSequencer::INITIAL_STEP);
    voice.begin_frame(false);
    chunks
        .iter()
        .flat_map(|&samples| clock.chunk(voice, samples))
        .collect()
}

/// Boundaries at 25 and 26 (the first event leads a chunk), 224, and 234 and
/// 235 (the second event does).
const SPLIT: [usize; 7] = [25, 1, 198, 10, 1, 208, 157];

#[test]
fn a_buffer_boundary_beside_an_event_does_not_move_it() {
    let whole_length: usize = SPLIT.iter().sum();
    for (name, mut whole, mut split) in [
        ("square", falling_square(), falling_square()),
        ("noise", falling_noise(), falling_noise()),
    ] {
        let whole_acc = render_in_chunks(&mut whole, &[whole_length]);
        let split_acc = render_in_chunks(&mut split, &SPLIT);

        assert_eq!(whole_acc, split_acc, "{name}");
        assert_eq!(whole.hardware_volume(), 12, "{name}: three events counted");
        assert_eq!(split.hardware_volume(), 12, "{name}");
        assert_eq!(whole.noise_output_latch(), split.noise_output_latch());
        assert_eq!(whole.noise_lfsr(), split.noise_lfsr());
    }
}

#[test]
fn a_length_stopped_channel_does_not_clock_its_envelope() {
    for channel in [CgbChannelNumber::Square1, CgbChannelNumber::Square2] {
        let mut clock = SharedClock::new();
        let mut stopped =
            square_voice_with_adsr(channel, None, FALLING_EVERY_EVENT, TestNote::default())
                .with_length(TIED_LENGTH);
        let mut running =
            square_voice_with_adsr(channel, None, FALLING_EVERY_EVENT, TestNote::default());
        let mut running_clock = SharedClock::new();

        for _ in 0..FRAMES {
            clock.frame(&mut stopped, false);
            running_clock.frame(&mut running, false);
        }

        assert!(stopped.is_length_stopped(), "{channel:?}");
        assert_eq!(
            stopped.hardware_volume(),
            14,
            "{channel:?}: only the event before the length expiry at 156 counted"
        );
        assert!(
            running.hardware_volume() < 14,
            "{channel:?}: sanity: the same envelope otherwise keeps falling"
        );
    }
}

#[test]
fn a_noise_channel_stopped_by_length_holds_its_latch_through_events() {
    let mut clock = SharedClock::new();
    let mut voice =
        noise_voice(FALLING_EVERY_EVENT, WIDE_NOISE, TestNote::default()).with_length(TIED_LENGTH);
    for _ in 0..FRAMES {
        clock.frame(&mut voice, false);
    }
    assert!(voice.is_length_stopped());
    assert_eq!(voice.hardware_volume(), 14);
}

#[test]
fn a_sweep_muted_channel_does_not_clock_its_envelope() {
    const OVERFLOW_KEY: u8 = 120;
    let mut clock = SharedClock::new();
    let mut voice = square_voice_with_adsr(
        CgbChannelNumber::Square1,
        Some(upward_sweep(3, 1)),
        FALLING_EVERY_EVENT,
        TestNote::at_key(OVERFLOW_KEY),
    );
    for _ in 0..FRAMES {
        clock.frame(&mut voice, false);
    }
    assert!(voice.hardware_muted, "sanity: the note overflowed at once");
    assert_eq!(voice.hardware_volume(), 15, "no event reached the timer");
}

#[test]
fn a_period_zero_envelope_holds_its_level_through_every_event() {
    let mut clock = SharedClock::new();
    let mut voice = square_voice(CgbChannelNumber::Square1, None, TestNote::default());
    let first = clock.frame(&mut voice, false);
    for _ in 1..FRAMES {
        let next = clock.frame(&mut voice, false);
        assert_eq!(peak_magnitude(&next), peak_magnitude(&first));
    }
    assert_eq!(voice.hardware_volume(), 14);
}

/// A square an event saturates at zero stops running its duty from that
/// sample on, as one that begins its buffer dead does
/// (`mgba/src/gb/audio.c:493-510`), so the sweep that retunes it afterwards
/// leaves the same duty position wherever the buffers end.
#[test]
fn a_square_an_event_silences_defers_its_duty_however_the_buffer_is_split() {
    let sweeping_note = TestNote {
        track_right: 10,
        track_left: 10,
        ..TestNote::at_key(48)
    };
    let dies_at_first_event = CgbAdsr {
        attack: 0,
        decay: 17,
        sustain: 0,
        release: 0,
    };
    let render = |chunks: &[usize]| {
        let mut clock = SharedClock::new();
        let mut voice = square_voice_with_adsr(
            CgbChannelNumber::Square1,
            Some(0x19),
            dies_at_first_event,
            sweeping_note,
        );
        voice.set_sequencer_step(FrameSequencer::INITIAL_STEP);
        voice.begin_frame(false);
        for &samples in chunks {
            clock.chunk(&mut voice, samples);
        }
        assert_eq!(voice.hardware_volume(), 0, "sanity: the event silenced it");
        voice.set_track_volume(10, 10);
        clock.frame(&mut voice, false)
    };

    assert_eq!(render(&[224]), render(&[27, 197]));
}
