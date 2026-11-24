use assets::MovementType;

use super::*;
use crate::overworld::object_event::ObjectEventState;
use crate::overworld::object_event_harness::template;

fn state_at(x: i16, y: i16) -> ObjectEventState {
    let event = template(1, x, y, MovementType::FaceDown, "0");
    ObjectEventState::from_template(&event)
}

/// Runs the action until it reports done, returning how many `advance_step` calls it took.
fn frames_to_finish(state: &mut ObjectEventState) -> usize {
    (1..=usize::from(NORMAL_STEP_FRAMES))
        .find(|_| state.advance_step())
        .expect("the action finishes within its length")
}

#[test]
fn a_normal_step_is_sixteen_frames_counting_the_one_that_starts_it() {
    assert_eq!(NORMAL_STEP_FRAMES, 16);
    let mut state = state_at(5, 5);
    state.begin_normal_walk(Direction::East);

    assert_eq!(
        frames_to_finish(&mut state),
        15,
        "the starting frame is the first of sixteen"
    );
}

#[test]
fn a_walk_commits_the_destination_on_its_first_frame_and_settles_on_its_last() {
    let mut state = state_at(5, 5);
    state.begin_normal_walk(Direction::East);

    assert_eq!(state.position(), (6, 5), "the destination commits at once");
    assert_eq!(
        state.previous_position(),
        (5, 5),
        "the tile it left stays held"
    );
    assert_eq!(state.facing(), Direction::East);
    let step = state.step().unwrap();
    assert_eq!(step.kind(), StepKind::Walk);
    assert_eq!(step.frames_elapsed(), 1);

    for frame in 2..NORMAL_STEP_FRAMES {
        assert!(!state.advance_step(), "frame {frame} is mid-step");
        assert_eq!(state.step().unwrap().frames_elapsed(), frame);
        assert_eq!(
            state.previous_position(),
            (5, 5),
            "frame {frame} still holds the source"
        );
    }
    assert!(state.advance_step(), "frame 16 completes the action");
    assert_eq!(state.step(), None);
    assert_eq!(state.position(), (6, 5));
    assert_eq!(
        state.previous_position(),
        (6, 5),
        "the sixteenth frame shifts previous onto current"
    );
}

#[test]
fn a_walk_closes_one_pixel_a_frame_toward_its_committed_tile() {
    let offsets = |direction: Direction, frames: &[u8]| {
        let mut state = state_at(5, 5);
        state.begin_normal_walk(direction);
        let mut seen = Vec::new();
        for frame in 1..NORMAL_STEP_FRAMES {
            if frames.contains(&frame) {
                seen.push(state.step().unwrap().pixel_offset());
            }
            state.advance_step();
        }
        seen
    };

    assert_eq!(
        offsets(Direction::East, &[1, 8, 15]),
        [(-15, 0), (-8, 0), (-1, 0)]
    );
    assert_eq!(
        offsets(Direction::West, &[1, 8, 15]),
        [(15, 0), (8, 0), (1, 0)]
    );
    assert_eq!(
        offsets(Direction::South, &[1, 8, 15]),
        [(0, -15), (0, -8), (0, -1)]
    );
    assert_eq!(
        offsets(Direction::North, &[1, 8, 15]),
        [(0, 15), (0, 8), (0, 1)]
    );
}

#[test]
fn a_walk_in_place_turns_the_object_without_moving_or_offsetting_it() {
    let mut state = state_at(5, 5);
    state.begin_normal_walk_in_place(Direction::West);

    assert_eq!(state.facing(), Direction::West);
    assert_eq!(state.position(), (5, 5));
    assert_eq!(state.previous_position(), (5, 5));
    let step = state.step().unwrap();
    assert_eq!(step.kind(), StepKind::WalkInPlace);
    assert_eq!(step.pixel_offset(), (0, 0));

    assert_eq!(
        frames_to_finish(&mut state),
        15,
        "it lasts as long as a walk"
    );
    assert_eq!(state.position(), (5, 5));
    assert_eq!(state.previous_position(), (5, 5));
}

#[test]
fn the_started_direction_is_kept_when_facing_changes_mid_step() {
    let mut state = state_at(5, 5);
    state.begin_normal_walk(Direction::East);
    state.face(Direction::North);

    let step = state.step().unwrap();
    assert_eq!(step.direction(), Direction::East);
    assert_eq!(step.pixel_offset(), (-15, 0));
}

#[test]
fn advancing_with_no_action_reports_done_and_changes_nothing() {
    let mut state = state_at(5, 5);
    state.walk(Direction::South);
    let before = state;

    assert!(state.advance_step());
    assert_eq!(state, before, "a bare walk is not a timed action");
}

#[test]
fn settling_releases_the_previous_tile_without_ending_the_action() {
    let mut state = state_at(5, 5);
    state.begin_normal_walk(Direction::South);
    state.settle_previous_position();

    assert_eq!(state.previous_position(), (5, 6));
    assert!(state.step().is_some());
}

#[test]
fn a_respawn_drops_a_running_action() {
    let mut state = state_at(5, 5);
    state.begin_normal_walk(Direction::South);
    state.reinit_from_template_snapshots();

    assert_eq!(state.step(), None);
    assert_eq!(state.position(), (5, 5));
}

fn commands_over_a_step(state: &ObjectEventState) -> Vec<u8> {
    let mut state = *state;
    let mut seen = vec![state.step().unwrap().go_command()];
    while !state.advance_step() {
        seen.push(state.step().unwrap().go_command());
    }
    seen
}

#[test]
fn a_fresh_walker_starts_a_step_on_command_zero_for_eight_frames_then_the_neutral_pose() {
    let mut state = state_at(5, 5);
    assert_eq!(state.go_command(), 0);
    state.begin_normal_walk(Direction::South);

    // Elapsed frames 1 to 8 show command 0 and 9 to 15 show command 1; the sixteenth frame
    // ends the action in the same callback, so it is never observed as a running step.
    let mut expected = vec![0; 8];
    expected.extend([1; 7]);
    assert_eq!(commands_over_a_step(&state), expected);

    assert!(frames_to_finish(&mut state) > 0);
    assert_eq!(state.go_command(), 1, "the step rests on the neutral pose");
}

#[test]
fn consecutive_steps_alternate_the_foot() {
    let mut state = state_at(5, 5);
    let mut starts = Vec::new();
    for _ in 0..4 {
        state.begin_normal_walk(Direction::East);
        starts.push(state.step().unwrap().go_command());
        frames_to_finish(&mut state);
    }
    assert_eq!(starts, [0, 2, 0, 2]);
    assert_eq!(state.go_command(), 3);
}

#[test]
fn a_walk_in_place_alternates_like_a_walk_and_does_not_move() {
    let mut state = state_at(5, 5);
    state.begin_normal_walk_in_place(Direction::North);
    assert_eq!(state.step().unwrap().go_command(), 0);
    frames_to_finish(&mut state);
    state.begin_normal_walk(Direction::North);
    assert_eq!(state.step().unwrap().go_command(), 2, "the other foot");
}

#[test]
fn a_face_action_keeps_the_foot_that_follows_but_a_fresh_object_moves_to_neutral() {
    // A wanderer's face action before its first walk normalises command 0 to 1, so that
    // first walk takes the second foot (`SetStepAnim`, `event_object_movement.c:4601-4618`).
    let mut state = state_at(5, 5);
    state.face_in_place();
    assert_eq!(state.go_command(), 1);
    state.begin_normal_walk(Direction::South);
    assert_eq!(state.step().unwrap().go_command(), 2);
    frames_to_finish(&mut state);
    assert_eq!(state.go_command(), 3);

    // After that step the face action keeps command 3, so the next walk takes foot 0 again.
    state.face_in_place();
    assert_eq!(state.go_command(), 3);
    state.begin_normal_walk(Direction::South);
    assert_eq!(state.step().unwrap().go_command(), 0);
}

#[test]
fn a_plain_facing_change_leaves_the_animation_alone() {
    let mut state = state_at(5, 5);
    state.begin_normal_walk(Direction::East);
    frames_to_finish(&mut state);
    state.face(Direction::West);
    assert_eq!(state.go_command(), 1);
}

#[test]
fn faceplayer_keeps_the_foot_that_follows_like_any_face_action() {
    // `faceplayer` holds a face movement action, whose `SetStepAnim` maps 0 and 1 to 1 and
    // 2 and 3 to 3 (`event_object_movement.c:4975-4978,5048-5055,4601-4618`); it does not
    // restart the animation.
    let mut state = state_at(5, 5);
    state.begin_normal_walk(Direction::East);
    frames_to_finish(&mut state);
    assert_eq!(state.go_command(), 1);

    state.face_with_animation(Direction::West);
    assert_eq!(state.facing(), Direction::West);
    assert_eq!(state.go_command(), 1);
    state.begin_normal_walk(Direction::East);
    assert_eq!(
        state.step().unwrap().go_command(),
        2,
        "the second foot, not command 0"
    );
    frames_to_finish(&mut state);

    state.face_with_animation(Direction::North);
    assert_eq!(state.go_command(), 3);
    state.begin_normal_walk(Direction::East);
    assert_eq!(state.step().unwrap().go_command(), 0);
}

#[test]
fn clearing_a_running_step_freezes_the_command_it_had_reached() {
    let mut early = state_at(5, 5);
    early.begin_normal_walk(Direction::East);
    early.clear_step();
    assert_eq!(early.go_command(), 0);

    let mut late = state_at(5, 5);
    late.begin_normal_walk(Direction::East);
    for _ in 0..9 {
        late.advance_step();
    }
    assert_eq!(late.step().unwrap().frames_elapsed(), 10);
    late.clear_step();
    assert_eq!(late.go_command(), 1);
}

#[test]
fn a_respawn_returns_the_animation_to_command_zero() {
    let mut state = state_at(5, 5);
    state.begin_normal_walk(Direction::South);
    frames_to_finish(&mut state);
    assert_eq!(state.go_command(), 1);
    state.reinit_from_template_snapshots();
    assert_eq!(state.go_command(), 0);
}
