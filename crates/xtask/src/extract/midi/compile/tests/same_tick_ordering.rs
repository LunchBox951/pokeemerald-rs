use super::super::{compile, SongEvent};
use super::support::*;

#[test]
fn an_over_long_zero_delta_preserves_the_prior_absolute_tick() {
    use crate::extract::midi::parse::{parse_track, RawEvent};

    let mut body = Vec::new();
    push_timed(&mut body, 1, note_on(0, 60, 100));
    body.extend(OVERLONG_ZERO_VLQ);
    body.extend(program_change(0, 5));
    push_timed(&mut body, 24, note_off(0, 60));

    let mut parse_body = body.clone();
    push_timed(&mut parse_body, 0, meta_event(END_OF_TRACK_META_EVENT, &[]));
    let parsed = parse_track(&parse_body).unwrap();
    assert_eq!(
        parsed.events[1],
        (
            1,
            RawEvent::ProgramChange {
                channel: 0,
                program: 5,
            },
        )
    );

    let compiled = compile(&single_track_midi(24, body), &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::Wait(1),
            SongEvent::KeyShift(0),
            SongEvent::Voice(5),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 24,
            },
            SongEvent::Wait(24),
            SongEvent::Fine,
        ]
    );
}

#[test]
fn a_same_tick_velocity_zero_note_on_precedes_an_extended_command_selector() {
    // Upstream sorts the retained type-zero note-on before controllers at the same tick
    // (`tools/mid2agb/midi.cpp:553-557,565-585`; `tools/mid2agb/midi.h:34-50`).
    let mut body = Vec::new();
    push_timed(&mut body, 0, note_on(0, 60, 100));
    push_timed(
        &mut body,
        4,
        control_change(0, EXTENDED_COMMAND_SELECTOR, PSEUDO_ECHO_VOLUME_COMMAND),
    );
    push_timed(&mut body, 0, note_on(0, 60, 0));
    push_timed(&mut body, 10, note_on(0, 64, 100));
    push_timed(&mut body, 6, note_off(0, 64));

    let compiled = compile(&single_track_midi(24, body), &cfg()).unwrap();
    assert_eq!(
        compiled.tracks[0],
        vec![
            SongEvent::Volume(127),
            SongEvent::KeyShift(0),
            SongEvent::Note {
                key: 60,
                velocity: 100,
                gate: 4,
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
