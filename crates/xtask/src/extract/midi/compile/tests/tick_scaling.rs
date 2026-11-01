use super::super::{compile, MidiError, SongEvent};
use super::support::*;

#[test]
fn convert_ticks_evaluates_the_multiply_in_u64() {
    use crate::extract::midi::translate::convert_ticks;

    assert_eq!(
        convert_ticks(MAX_STANDARD_VLQ, 24).unwrap(),
        MAX_STANDARD_VLQ
    );
    assert_eq!(
        convert_ticks(MAX_STANDARD_VLQ, 1).unwrap_err(),
        MidiError::TickOverflow(MAX_STANDARD_VLQ)
    );
}

#[test]
fn a_tick_that_overflows_once_scaled_is_an_error_not_a_panic() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, MAX_STANDARD_VLQ, note_off(0, 60));
    let midi = single_track_midi(1, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::TickOverflow(MAX_STANDARD_VLQ)
    );
}

#[test]
fn a_grid_mark_past_u32_max_is_an_error_not_a_hang() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 0x0F, note_off(0, 60));
    push_max_delta_empty_text_events(&mut body, 16);
    push_timed(&mut body, 0, time_signature(4, 2));
    push_timed(&mut body, 0, control_change(0, VOLUME_CONTROLLER, 100));
    let midi = single_track_midi(24, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::TickOverflow(u32::MAX)
    );
}

#[test]
fn a_unit_grid_period_reaches_a_distant_event_without_iterating() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, time_signature(1, 6));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 0x0F, note_off(0, 60));
    push_max_delta_empty_text_events(&mut body, 16);
    push_timed(&mut body, 0, control_change(0, VOLUME_CONTROLLER, 100));
    let midi = single_track_midi(24, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::TickOverflow(u32::MAX)
    );
}

#[test]
fn channel_events_are_scaled_to_the_output_timebase() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 12, program_change(0, 3));
    push_timed(&mut body, 12, control_change(0, MODULATION_CONTROLLER, 7));
    push_timed(&mut body, 12, pitch_bend(0, 0, 80));
    push_timed(&mut body, 12, note_off(0, 60));
    let midi = single_track_midi(48, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 24,
            },
            SongEvent::Wait(6),
            SongEvent::Voice(3),
            SongEvent::Wait(6),
            SongEvent::Modulation(7),
            SongEvent::Wait(6),
            SongEvent::Bend(16),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}
