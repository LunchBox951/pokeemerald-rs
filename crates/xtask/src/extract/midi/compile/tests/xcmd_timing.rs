//! XCMD selector suppression, timing-grid boundaries, and retained silent events.

use super::super::{compile, SongEvent};
use super::support::*;

#[test]
fn xcmd_pseudo_echo_volume_round_trips_and_unknown_subcommands_are_silent() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(
        &mut body,
        4,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(
        &mut body,
        0,
        control_change(0, EXTENDED_COMMAND_TRIGGER, 10),
    );
    push_timed(
        &mut body,
        0,
        control_change(0, EXTENDED_COMMAND_SELECTOR, 10),
    );
    push_timed(
        &mut body,
        0,
        control_change(0, ALTERNATE_EXTENDED_COMMAND_TRIGGER, 5),
    );
    push_timed(&mut body, 4, note_off(0, 60));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert!(compiled.tracks[0].contains(&SongEvent::PseudoEchoVolume(10)));
    assert_eq!(
        compiled.tracks[0]
            .iter()
            .filter(|e| matches!(
                e,
                SongEvent::PseudoEchoVolume(_) | SongEvent::PseudoEchoLength(_)
            ))
            .count(),
        1
    );
}

#[test]
fn an_extended_command_selector_discards_a_fixed_point_gap() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(
        &mut body,
        4,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(
        &mut body,
        24,
        running_control_change(EXTENDED_COMMAND_TRIGGER, 10),
    );
    push_timed(&mut body, 6, note_off(0, 60));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 34
            },
            SongEvent::Wait(4),
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn an_extended_command_selector_keeps_an_off_grid_remainder() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(
        &mut body,
        4,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(
        &mut body,
        27,
        running_control_change(EXTENDED_COMMAND_TRIGGER, 10),
    );
    push_timed(&mut body, 6, note_off(0, 60));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 37
            },
            SongEvent::Wait(7), // the two rests merge: canonical waits
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn an_extended_command_selector_gap_stops_at_the_timing_grid() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(
        &mut body,
        15,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_LENGTH_COMMAND),
    );
    push_timed(
        &mut body,
        100,
        running_control_change(ALTERNATE_EXTENDED_COMMAND_TRIGGER, 12),
    );
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4
            },
            SongEvent::Wait(43), // the two rests merge: canonical waits
            SongEvent::PseudoEchoLength(12),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn an_extended_command_selector_near_a_grid_line_preserves_later_ticks() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(
        &mut body,
        86,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(
        &mut body,
        20,
        running_control_change(EXTENDED_COMMAND_TRIGGER, 10),
    );
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4
            },
            SongEvent::Wait(104), // the two rests merge: canonical waits
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_velocity_zero_note_on_bounds_an_extended_command_selector_gap() {
    // Upstream retains a velocity-zero note-on as a silent, type-zero track
    // event even though it also ends the note it matches, but drops an
    // explicit note-off entirely (`tools/mid2agb/midi.cpp:471-514`). That
    // silent event still bounds an extended-command selector's suppressed
    // gap, exactly like any other retained item does.
    fn midi_with_note_end(note_end: [u8; 3]) -> Vec<u8> {
        let mut body = Vec::new();
        push_timed(&mut body, 0, note_on(0, 60, 100));
        push_timed(
            &mut body,
            4,
            control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
        );
        push_timed(&mut body, 10, note_end);
        push_timed(&mut body, 10, note_on(0, 64, 100));
        push_timed(&mut body, 6, note_off(0, 64));
        single_track_midi(24, body)
    }

    // The note-end at tick 14 is a velocity-zero note-on: it is retained as
    // a silent boundary, so the selector's suppressed gap stops there and
    // the remaining 10 ticks to the next note surface as an ordinary wait.
    let velocity_zero = compile(&midi_with_note_end(note_on(0, 60, 0)), &cfg()).unwrap();
    assert_eq!(
        velocity_zero.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 14,
            },
            SongEvent::Wait(14), // 4 (pre-selector) + 10 (selector to the silent boundary): canonical waits
            SongEvent::Note {
                key: 64,
                velocity: 100,
                gate: 6,
            },
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );

    // The same note-end as an explicit note-off leaves no boundary behind,
    // so the selector's suppression reaches all the way to the next note.
    let explicit_off = compile(&midi_with_note_end(note_off(0, 60)), &cfg()).unwrap();
    assert_eq!(
        explicit_off.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 14,
            },
            SongEvent::Wait(4),
            SongEvent::Note {
                key: 64,
                velocity: 100,
                gate: 6,
            },
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_time_signature_rephases_the_extended_command_timing_grid() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, time_signature(2, 2));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(
        &mut body,
        36,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(
        &mut body,
        30,
        running_control_change(EXTENDED_COMMAND_TRIGGER, 10),
    );
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4
            },
            SongEvent::Wait(62), // the two rests merge: canonical waits
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_silent_controller_after_an_extended_command_selector_keeps_its_wait() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(
        &mut body,
        4,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(&mut body, 10, running_control_change(UNKNOWN_CONTROLLER, 3));
    push_timed(
        &mut body,
        10,
        running_control_change(EXTENDED_COMMAND_TRIGGER, 10),
    );
    push_timed(&mut body, 6, note_off(0, 60));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 30
            },
            SongEvent::Wait(14), // the two rests merge: canonical waits
            SongEvent::PseudoEchoVolume(10),
            SongEvent::Wait(6),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn waits_not_adjacent_to_an_extended_command_selector_are_unaffected() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(&mut body, 10, control_change(0, LFO_SPEED_CONTROLLER, 7));
    push_timed(
        &mut body,
        5,
        running_control_change(EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_LENGTH_COMMAND),
    );
    push_timed(
        &mut body,
        40,
        running_control_change(ALTERNATE_EXTENDED_COMMAND_TRIGGER, 12),
    );
    push_timed(
        &mut body,
        300,
        running_control_change(MODULATION_CONTROLLER, 55),
    );
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4
            },
            SongEvent::Wait(14),
            SongEvent::LfoSpeed(7),
            SongEvent::Wait(5),
            SongEvent::PseudoEchoLength(12),
            SongEvent::Wait(u8::MAX),
            SongEvent::Wait(45),
            SongEvent::Modulation(55),
            SongEvent::Fine,
        ]
    );
}
