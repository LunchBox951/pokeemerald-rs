//! Malformed or unsupported input surfacing as a compile error rather than output.

use super::super::{compile, MidiError};
use super::support::*;

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
