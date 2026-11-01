//! Pattern calls and repeats preserve return sites and runtime note inheritance.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::sequence::decode_track;
use crate::song::ToneData;

#[test]
fn decoded_pattern_notes_inherit_each_callers_key() {
    let bytes = [
        0xBD, 0, 0xCF, 60, 127, 0xB3, 19, 0, 0, 0, 0xCF, 72, 127, 0xB3, 19, 0, 0, 0, 0xB0, 0xCF,
        0xB4,
    ];
    let events = decode_track(&bytes).unwrap();
    let wave = Arc::new(WaveData::looping(0, 0, vec![100]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let mut sequencer = Sequencer::new(Song::new(voices, vec![events], 150));
    let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];
    sequencer.render_frame(&mut output);

    let mut keys: Vec<_> = sequencer
        .mixer
        .voices()
        .iter()
        .map(|voice| voice.midi_key())
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, vec![60, 60, 72, 72]);
}

// --- Pattern execution (`PATT`/`PEND`/`REPT`) ---------------------------

fn render_track(track: Vec<Event>, frames: usize) -> Vec<f32> {
    let mut sequencer = Sequencer::new(test_song(vec![track], 150));
    let mut output = vec![0.0; Sequencer::FRAME_SAMPLES * frames];
    sequencer.mix_into(&mut output);
    output
}

#[test]
fn pattern_call_renders_identically_to_the_unrolled_track() {
    const PATTERN_BODY: usize = 5;

    let with_pattern = vec![
        Event::Voice(0),
        Event::Volume(127),
        Event::Pattern(PATTERN_BODY),
        Event::Wait(48),
        Event::Fine,
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 8,
        },
        Event::PatternEnd,
    ];
    let unrolled = vec![
        Event::Voice(0),
        Event::Volume(127),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 8,
        },
        Event::Wait(48),
        Event::Fine,
    ];

    assert_eq!(render_track(with_pattern, 80), render_track(unrolled, 80));
}

#[test]
fn nested_pattern_calls_render_identically_to_the_unrolled_track() {
    const OUTER_BODY: usize = 5;
    const INNER_BODY: usize = 9;

    let note = |key: u8| Event::Note {
        key,
        velocity: 127,
        gate: 4,
    };
    let nested = vec![
        Event::Voice(0),
        Event::Volume(127),
        Event::Pattern(OUTER_BODY),
        Event::Wait(12),
        Event::Fine,
        Event::Pattern(INNER_BODY),
        note(64),
        Event::Wait(12),
        Event::PatternEnd,
        note(60),
        Event::Wait(12),
        Event::PatternEnd,
    ];
    let unrolled = vec![
        Event::Voice(0),
        Event::Volume(127),
        note(60),
        Event::Wait(12),
        note(64),
        Event::Wait(12),
        Event::Wait(12),
        Event::Fine,
    ];

    assert_eq!(render_track(nested, 80), render_track(unrolled, 80));
}

#[test]
fn nested_pattern_calls_return_to_each_call_site() {
    let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
    sequencer.tracks[0].cursor = 2;

    apply_test_event(&mut sequencer, 0, &Event::Pattern(10));
    sequencer.tracks[0].cursor = 11;
    apply_test_event(&mut sequencer, 0, &Event::Pattern(20));
    apply_test_event(&mut sequencer, 0, &Event::PatternEnd);
    assert_eq!(sequencer.tracks[0].cursor, 11);

    apply_test_event(&mut sequencer, 0, &Event::PatternEnd);
    assert_eq!(sequencer.tracks[0].cursor, 2);
    assert_eq!(sequencer.tracks[0].pattern_depth, 0);
}

#[test]
fn fourth_nested_pattern_call_ends_the_track() {
    let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));

    for target in 1..=MAX_PATTERN_DEPTH {
        apply_test_event(&mut sequencer, 0, &Event::Pattern(target));
        assert!(!sequencer.tracks[0].ended);
    }
    apply_test_event(&mut sequencer, 0, &Event::Pattern(99));

    assert!(sequencer.tracks[0].ended);
    assert_eq!(sequencer.tracks[0].pattern_depth, MAX_PATTERN_DEPTH);
}

#[test]
fn pattern_end_without_a_call_is_a_no_op() {
    let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
    let original = sequencer.tracks[0].clone();

    apply_test_event(&mut sequencer, 0, &Event::PatternEnd);

    assert_eq!(sequencer.tracks[0], original);
}

#[test]
fn repeat_jumps_until_its_count_is_reached_then_falls_through() {
    const REPEAT_TARGET: usize = 3;
    const FALLTHROUGH: usize = 7;

    let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
    for pass in 1..=5 {
        sequencer.tracks[0].cursor = FALLTHROUGH;
        apply_test_event(
            &mut sequencer,
            0,
            &Event::Repeat {
                count: 5,
                target: REPEAT_TARGET,
            },
        );
        let expected_cursor = if pass < 5 { REPEAT_TARGET } else { FALLTHROUGH };
        assert_eq!(sequencer.tracks[0].cursor, expected_cursor, "pass {pass}");
    }
    assert_eq!(sequencer.tracks[0].repeat_counter, 0);
}

#[test]
fn repeat_count_one_falls_through_immediately() {
    let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
    sequencer.tracks[0].cursor = 7;

    apply_test_event(
        &mut sequencer,
        0,
        &Event::Repeat {
            count: 1,
            target: 3,
        },
    );

    assert_eq!(sequencer.tracks[0].cursor, 7);
    assert_eq!(sequencer.tracks[0].repeat_counter, 0);
}

#[test]
fn repeat_count_zero_jumps_without_incrementing() {
    let mut sequencer = Sequencer::new(test_song(vec![vec![Event::Fine]], 150));
    sequencer.tracks[0].cursor = 7;

    apply_test_event(
        &mut sequencer,
        0,
        &Event::Repeat {
            count: 0,
            target: 3,
        },
    );

    assert_eq!(sequencer.tracks[0].cursor, 3);
    assert_eq!(sequencer.tracks[0].repeat_counter, 0);
}

fn repeat_body() -> [Event; 2] {
    [
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 4,
        },
        Event::Wait(12),
    ]
}

/// A finite `REPT` track, built so `target` points at the first event of
/// [`repeat_body`].
fn repeat_track(count: u8) -> Vec<Event> {
    const REPEAT_TARGET: usize = 2;

    let mut track = vec![Event::Voice(0), Event::Volume(127)];
    track.extend(repeat_body());
    track.push(Event::Repeat {
        count,
        target: REPEAT_TARGET,
    });
    track.push(Event::Fine);
    track
}

#[test]
fn repeat_renders_identically_to_the_unrolled_track() {
    const REPEAT_COUNT: u8 = 3;

    let mut unrolled = vec![Event::Voice(0), Event::Volume(127)];
    for _ in 0..REPEAT_COUNT {
        unrolled.extend(repeat_body());
    }
    unrolled.push(Event::Fine);

    assert_eq!(
        render_track(repeat_track(REPEAT_COUNT), 80),
        render_track(unrolled, 80)
    );
}

#[test]
fn repeat_reaches_fine_through_the_rendered_track() {
    let mut sequencer = Sequencer::new(test_song(vec![repeat_track(3)], 150));
    let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];

    for _ in 0..200 {
        sequencer.render_frame(&mut output);
    }

    assert!(sequencer.is_finished());
}

#[test]
fn repeat_count_zero_loops_the_rendered_track_forever() {
    let mut sequencer = Sequencer::new(test_song(vec![repeat_track(0)], 150));
    let mut output = vec![0.0; Sequencer::FRAME_SAMPLES];

    for _ in 0..200 {
        sequencer.render_frame(&mut output);
    }

    assert!(!sequencer.is_finished());
}
