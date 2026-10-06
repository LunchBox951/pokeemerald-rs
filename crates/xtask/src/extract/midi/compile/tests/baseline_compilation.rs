//! Valid input compiling to the expected event stream: notes, ties, waits, and loops.

use super::super::{compile, SongEvent};
use super::support::*;

#[test]
fn a_single_note_compiles_to_the_expected_event_stream() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 24, running_note_off(60));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(compiled.tracks.len(), 1);
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 24
            },
            SongEvent::Wait(24),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_volume_controller_before_the_note_suppresses_the_synthetic_preamble() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, control_change(0, VOLUME_CONTROLLER, 100));
    push_timed(&mut body, 0, note_on(0, 60, 96));
    push_timed(&mut body, 10, note_off(0, 60));
    let midi = single_track_midi(24, body);

    let mut entry = cfg();
    entry.master_volume = 90;
    let compiled = compile(&midi, &entry).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::KeyShift(0),
            SongEvent::Volume(70),
            SongEvent::Note {
                key: 60,
                velocity: 96,
                gate: 10
            },
            SongEvent::Wait(10),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_long_note_is_tie_split_with_an_explicit_end_of_tie_key() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 67, 80));
    push_timed(&mut body, 120, running_note_off(67));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 67,
                velocity: 80,
                gate: 0
            },
            SongEvent::Wait(120),
            SongEvent::EndOfTie { key: 67 },
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_gap_over_255_ticks_is_split_into_multiple_waits() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 1, running_note_off(60));
    push_timed(&mut body, 300, note_on(0, 62, 100));
    push_timed(&mut body, 1, running_note_off(62));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    let waits: Vec<u8> = compiled.tracks[0]
        .iter()
        .filter_map(|e| match e {
            SongEvent::Wait(t) => Some(*t),
            _ => None,
        })
        .collect();
    assert!(waits.contains(&255));
    assert_eq!(
        waits.iter().map(|&w| u32::from(w)).sum::<u32>(),
        1 + 300 + 1
    );
}

#[test]
fn loop_markers_compile_to_a_backward_goto() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, text_event(b"["));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(&mut body, 0, text_event(b"]"));
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
            SongEvent::Wait(4),
            SongEvent::Goto(2),
            SongEvent::Fine,
        ]
    );
}
