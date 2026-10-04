//! Baseline compilation, configuration, playable-channel filtering, and invalid-input boundaries.

use super::super::{compile, MidiError, SongEvent};
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

#[test]
fn a_loop_end_with_no_matching_loop_begin_is_an_error() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(&mut body, 0, text_event(b"]"));
    let midi = single_track_midi(24, body);

    let err = compile(&midi, &cfg()).unwrap_err();
    assert_eq!(err, MidiError::DanglingLoopEnd);
}

#[test]
fn a_zero_period_time_signature_is_an_error() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, time_signature(1, 7));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    let midi = single_track_midi(24, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::ZeroTimeSignature
    );
}

#[test]
fn a_zero_period_time_signature_with_no_playable_channel_is_an_error() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, time_signature(1, 7));
    let midi = single_track_midi(24, body);

    assert_eq!(
        compile(&midi, &cfg()).unwrap_err(),
        MidiError::ZeroTimeSignature
    );
}

#[test]
fn memacc_controllers_are_a_hard_error() {
    for controller in MEMACC_CONTROLLERS {
        let mut body = Vec::new();
        push_timed(&mut body, 0, note_on(0, 60, 100));
        push_timed(&mut body, 4, control_change(0, controller, 1));
        push_timed(&mut body, 0, note_off(0, 60));
        let midi = single_track_midi(24, body);
        let err = compile(&midi, &cfg()).unwrap_err();
        assert_eq!(err, MidiError::UnsupportedMemAccController(controller));
    }
}

#[test]
fn an_unterminated_note_is_an_error() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    let midi = single_track_midi(24, body);
    let err = compile(&midi, &cfg()).unwrap_err();
    assert_eq!(
        err,
        MidiError::UnterminatedNote {
            channel: 0,
            key: 60
        }
    );
}

#[test]
fn non_exact_gate_time_is_rejected() {
    let midi = single_note_midi(24, 1);
    let mut entry = cfg();
    entry.exact_gate_time = false;
    let err = compile(&midi, &entry).unwrap_err();
    assert_eq!(err, MidiError::NonExactGateTime);
}

#[test]
fn unsupported_clocks_per_beat_is_rejected() {
    let midi = single_note_midi(24, 1);
    let mut entry = cfg();
    entry.clocks_per_beat = 2;
    let err = compile(&midi, &entry).unwrap_err();
    assert_eq!(err, MidiError::UnsupportedClocksPerBeat(2));
}

#[test]
fn tempo_only_reaches_the_first_agb_track() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, tempo(500_000));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 4, running_note_off(60));
    push_timed(&mut body, 0, note_on(1, 64, 100));
    push_timed(&mut body, 4, note_off(1, 64));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(compiled.tracks.len(), 2);
    assert!(compiled.tracks[0].contains(&SongEvent::Tempo(120)));
    assert!(!compiled.tracks[1]
        .iter()
        .any(|e| matches!(e, SongEvent::Tempo(_))));
}

#[test]
fn song_level_metadata_is_carried_from_cfg() {
    let midi = single_note_midi(24, 1);
    let mut entry = cfg();
    entry.voicegroup_label = "title".to_owned();
    entry.priority = 3;
    entry.reverb = Some(50);
    let compiled = compile(&midi, &entry).unwrap();
    assert_eq!(compiled.voicegroup_label, "title");
    assert_eq!(compiled.priority, 3);
    assert_eq!(compiled.reverb, Some(50));
}

#[test]
fn a_channel_of_zero_duration_notes_emits_no_track() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, tempo(500_000));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 0, note_off(0, 60));
    push_timed(&mut body, 0, note_on(1, 64, 100));
    push_timed(&mut body, 4, note_off(1, 64));
    let midi = single_track_midi(24, body);

    let compiled = compile(&midi, &cfg()).unwrap();
    assert_eq!(compiled.tracks.len(), 1);
    assert!(compiled.tracks[0].contains(&SongEvent::Tempo(120)));
    assert!(compiled.tracks[0].contains(&SongEvent::Note {
        key: 64,
        velocity: 100,
        gate: 4,
    }));
}

#[test]
fn a_channel_with_no_notes_emits_no_track() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, control_change(0, VOLUME_CONTROLLER, 100));
    push_timed(&mut body, 0, note_on(1, 64, 100));
    push_timed(&mut body, 4, note_off(1, 64));
    let midi = single_track_midi(24, body);

    assert_eq!(compile(&midi, &cfg()).unwrap().tracks.len(), 1);
}

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
fn malformed_tempo_and_velocity_are_errors_at_the_compile_boundary() {
    let mut body = Vec::new();
    push_timed(&mut body, 0, tempo(0));
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(&mut body, 1, note_off(0, 60));
    let zero_tempo = single_track_midi(24, body);
    assert_eq!(
        compile(&zero_tempo, &cfg()).unwrap_err(),
        MidiError::ZeroTempo
    );

    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 128));
    push_timed(&mut body, 1, note_off(0, 60));
    let invalid_velocity = single_track_midi(24, body);
    assert_eq!(
        compile(&invalid_velocity, &cfg()).unwrap_err(),
        MidiError::InvalidDataByte(128)
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
