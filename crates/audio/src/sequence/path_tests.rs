use super::{decode_track, Event};

fn notes(events: &[Event], count: usize) -> Vec<(u8, u8, u8)> {
    let mut result = Vec::new();
    let mut cursor = 0;
    let mut returns = Vec::new();
    for _ in 0..1000 {
        if result.len() == count {
            return result;
        }
        match &events[cursor] {
            Event::Note {
                key,
                velocity,
                gate,
            } => result.push((*key, *velocity, *gate)),
            Event::Goto(target) => {
                cursor = *target;
                continue;
            }
            Event::Pattern(target) => {
                returns.push(cursor + 1);
                cursor = *target;
                continue;
            }
            Event::PatternEnd => {
                if let Some(ret) = returns.pop() {
                    cursor = ret;
                    continue;
                }
            }
            Event::Fine => return result,
            _ => (),
        }
        cursor += 1;
    }
    panic!("test track did not produce its expected notes");
}

#[test]
fn pattern_calls_inherit_each_callers_note() {
    let bytes = [
        0xbd, 0, 0xcf, 60, 127, 0xb3, 19, 0, 0, 0, 0xcf, 72, 127, 0xb3, 19, 0, 0, 0, 0xb1, 0xcf,
        0xb4,
    ];
    let events = decode_track(&bytes).unwrap();
    assert_eq!(
        notes(&events, 4),
        [(60, 127, 0), (60, 127, 0), (72, 127, 0), (72, 127, 0)]
    );
}

#[test]
fn loop_reentry_inherits_the_executed_key_and_velocity() {
    let bytes = [
        0xcf, 60, 100, 0x81, 0xcf, 0x81, 0xcf, 72, 80, 0x81, 0xb2, 4, 0, 0, 0, 0xb1,
    ];
    let events = decode_track(&bytes).unwrap();
    assert_eq!(
        notes(&events, 6),
        [
            (60, 100, 0),
            (60, 100, 0),
            (72, 80, 0),
            (72, 80, 0),
            (72, 80, 0),
            (72, 80, 0)
        ]
    );
}

#[test]
fn a_jump_skips_a_running_status_change() {
    let bytes = [
        0xd0, 60, 100, 0x81, 0xb2, 11, 0, 0, 0, 0xbd, 1, 64, 0x81, 0xb1,
    ];
    let events = decode_track(&bytes).unwrap();
    assert_eq!(notes(&events, 2), [(60, 100, 1), (64, 100, 1)]);
}

#[test]
fn a_patterns_explicit_tie_end_changes_the_callers_inherited_key() {
    let bytes = [
        0xcf, 60, 90, 0xb3, 11, 0, 0, 0, 0xcf, 0x81, 0xb1, 0xce, 72, 0xb4,
    ];
    let events = decode_track(&bytes).unwrap();
    assert_eq!(notes(&events, 2), [(60, 90, 0), (72, 90, 0)]);
    assert!(events.contains(&Event::EndOfTie { key: Some(72) }));
}

#[test]
fn patterns_carry_running_status_back_to_the_caller() {
    let bytes = [
        0xd0, 60, 90, 0xb3, 11, 0, 0, 0, 3, 0x81, 0xb1, 0xbd, 2, 0xb4,
    ];
    assert_eq!(
        decode_track(&bytes).unwrap(),
        [
            Event::Note {
                key: 60,
                velocity: 90,
                gate: 1
            },
            Event::Voice(2),
            Event::Voice(3),
            Event::Wait(1),
            Event::Fine,
        ]
    );
}

#[test]
fn repeat_reentry_inherits_changed_notes_and_resets_its_counter() {
    let bytes = [
        0xd0, 60, 90, 0xd0, 0x81, 0xd0, 72, 80, 0xb5, 3, 3, 0, 0, 0, 0xd0, 84, 100, 0x81, 0xb5, 2,
        14, 0, 0, 0, 0xb1,
    ];
    let events = decode_track(&bytes).unwrap();
    assert_eq!(
        notes(&events, 20),
        [
            (60, 90, 1),
            (60, 90, 1),
            (72, 80, 1),
            (72, 80, 1),
            (72, 80, 1),
            (72, 80, 1),
            (72, 80, 1),
            (84, 100, 1),
            (84, 100, 1),
        ]
    );
    assert_eq!(events.last(), Some(&Event::Fine));
}

#[test]
fn repeat_sites_share_one_track_counter() {
    // The inner repeat resets the counter before the outer repeat can finish.
    let bytes = [
        0xd0, 60, 90, 0xd0, 0x81, 0xd0, 72, 80, 0xb5, 3, 3, 0, 0, 0, 0xb5, 2, 3, 0, 0, 0, 0xb1,
    ];
    let events = decode_track(&bytes).unwrap();
    let mut expected = vec![(60, 90, 1), (60, 90, 1)];
    expected.extend([(72, 80, 1); 18]);
    assert_eq!(notes(&events, 20), expected);
    assert!(matches!(events.last(), Some(Event::Goto(_))));
}

#[test]
fn zero_repeat_count_closes_a_finite_event_loop() {
    let bytes = [0xd0, 60, 90, 0x81, 0xb5, 0, 0, 0, 0, 0, 0xb1];
    let events = decode_track(&bytes).unwrap();
    assert_eq!(
        events,
        [
            Event::Note {
                key: 60,
                velocity: 90,
                gate: 1
            },
            Event::Wait(1),
            Event::Goto(0)
        ]
    );
    assert_eq!(notes(&events, 3), [(60, 90, 1); 3]);
}

#[test]
fn fourth_nested_pattern_ends_the_track() {
    let bytes = [0xb3, 0, 0, 0, 0, 0xd0, 60, 90, 0xb1];
    assert_eq!(decode_track(&bytes).unwrap(), [Event::Fine]);
}

#[test]
fn an_empty_pattern_stack_falls_through() {
    assert_eq!(
        decode_track(&[0xb4, 0xbd, 3, 0xb1]).unwrap(),
        [Event::Voice(3), Event::Fine]
    );
}

#[test]
fn a_zero_time_jump_cycle_decodes_without_expanding_forever() {
    assert_eq!(decode_track(&[0xb2, 0, 0, 0, 0]).unwrap(), [Event::Goto(0)]);
}

#[test]
fn a_conditional_target_can_have_different_operand_boundaries() {
    let bytes = [
        0xd0, 60, 100, 0xb9, 6, 0, 1, 20, 0, 0, 0, 0xbd, 2, 0xb2, 20, 0, 0, 0, 0xb1, 0xb1, 64, 80,
        0x81, 0xb1,
    ];
    let events = decode_track(&bytes).unwrap();
    let Event::MemAcc {
        target: Some(target),
        ..
    } = events[1]
    else {
        panic!("missing dynamic condition")
    };
    assert_eq!(
        events[2..5],
        [Event::Voice(2), Event::Voice(64), Event::Voice(80)]
    );
    assert_eq!(
        events[target],
        Event::Note {
            key: 64,
            velocity: 80,
            gate: 1
        }
    );
    assert_eq!(notes(&events[target..], 1), [(64, 80, 1)]);
}

#[test]
fn an_unterminated_branch_does_not_fall_into_a_different_context() {
    let bytes = [
        0xcf, 60, 100, 0xb9, 6, 0, 1, 14, 0, 0, 0, 0xcf, 72, 80, 0xcf,
    ];
    let events = decode_track(&bytes).unwrap();
    let Event::MemAcc {
        target: Some(target),
        ..
    } = events[1]
    else {
        panic!("missing dynamic condition")
    };
    assert_eq!(notes(&events, 8), [(60, 100, 0), (72, 80, 0), (72, 80, 0)]);
    assert_eq!(
        events[target],
        Event::Note {
            key: 60,
            velocity: 100,
            gate: 0
        }
    );
}

#[test]
fn jumps_to_the_byte_after_the_program_are_invalid() {
    assert!(matches!(
        decode_track(&[0xb2, 5, 0, 0, 0]),
        Err(super::DecodeError::UnresolvedJump {
            offset: 0,
            target: 5
        })
    ));
}

#[test]
fn a_taken_branch_to_an_eventless_tail_has_a_terminal_target() {
    let bytes = [0xb9, 6, 0, 1, 9, 0, 0, 0, 0xb1, 0xb4];
    let events = decode_track(&bytes).unwrap();
    let Event::MemAcc {
        target: Some(target),
        ..
    } = events[0]
    else {
        panic!("missing dynamic condition")
    };
    assert_eq!(events[1], Event::Fine);
    assert_eq!(events[target], Event::Fine);
}
