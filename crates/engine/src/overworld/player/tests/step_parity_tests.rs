//! Walk-cycle parity across ordinary steps, turns, and rejected polls. Split
//! from `tests` for the same reason as `forced_movement_tests`: one module,
//! one concept `(oop-boundaries)`.

use super::*;

const DIRECTIONS: [Direction; 4] = [
    Direction::South,
    Direction::North,
    Direction::West,
    Direction::East,
];

/// A 9x9 runtime of plain ground with a collision bit on `blocked_row`.
fn open_runtime(blocked_row: Option<u16>) -> MapRuntime<'static> {
    let (bytes, header, events) = flat_runtime(9, 9, |_, y| u8::from(Some(y) == blocked_row));
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 9,
        height: 9,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let bytes = Box::leak(bytes.into_boxed_slice());
    MapRuntime::new(
        MapId("MAP_TEST"),
        Box::leak(Box::new(header)),
        Box::leak(Box::new(events)),
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(&[]),
        MetatileAttributeTable::new(&[]),
    )
}

fn finish_crossing(player: &mut PlayerState) {
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
}

/// A fresh idle face seeks command 1; the next crossing or turn selects
/// command 2 (`SetStepAnim`, `SetStepAnimHandleAlternation`)
/// `(behavioral-fidelity)`.
#[test]
fn idle_then_first_step_or_turn_leads_with_the_second_foot() {
    let runtime = open_runtime(None);

    let mut stepping = PlayerState::new((4, 4), 3, Direction::South);
    assert_eq!(
        stepping.step(None, &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Idle
    );
    assert_eq!(
        stepping.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: (4, 4),
            to: (4, 5),
        }
    );
    assert!(stepping.second_foot_leads());

    let mut turning = PlayerState::new((4, 4), 3, Direction::South);
    assert_eq!(
        turning.step(None, &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Idle
    );
    assert_eq!(
        turning.step(Some(Direction::North), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Turned(Direction::North)
    );
    assert!(turning.second_foot_leads());
}

/// `PlayerFreeze` and a scripted face both run `FaceDirection`, so a field
/// lock or face that claims the first frame before any idle poll still
/// normalises the fresh cycle (`field_player_avatar.c:1039-1046`,
/// `trainer_see.c:524-525`) `(behavioral-fidelity)`.
#[test]
fn a_field_lock_or_face_before_any_idle_poll_leaves_the_second_foot_to_lead() {
    let runtime = open_runtime(None);

    let mut locked = PlayerState::new((4, 4), 3, Direction::South);
    locked.clear_turn_lock();
    locked.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert!(locked.second_foot_leads());

    let mut faced = PlayerState::new((4, 4), 3, Direction::South);
    faced.face(Direction::North);
    faced.step(Some(Direction::North), &runtime, &no_connections, &NO_FLAGS);
    assert!(faced.second_foot_leads());

    let mut walked = PlayerState::new((4, 4), 3, Direction::South);
    walked.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    finish_crossing(&mut walked);
    walked.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    finish_crossing(&mut walked);
    walked.clear_turn_lock();
    walked.face(Direction::South);
    walked.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert!(!walked.second_foot_leads(), "a started cycle is not reset");
}

/// Idling after a completed second-foot step keeps command 3, so the next
/// step leads with the first foot: the idle normalisation is not a reset.
#[test]
fn idle_after_a_second_foot_step_does_not_reset_the_phase() {
    let runtime = open_runtime(None);
    let mut player = PlayerState::new((4, 4), 3, Direction::South);
    for expect_second in [false, true] {
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(player.second_foot_leads(), expect_second);
        finish_crossing(&mut player);
    }
    player.step(None, &runtime, &no_connections, &NO_FLAGS);
    player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert!(!player.second_foot_leads());
}

/// Consecutive completed steps in every facing alternate the leading foot
/// (`SetStepAnimHandleAlternation`, `event_object_movement.c:4582-4598`)
/// `(behavioral-fidelity)`.
#[test]
fn consecutive_steps_alternate_the_leading_foot_in_every_facing() {
    for facing in DIRECTIONS {
        let runtime = open_runtime(None);
        let mut player = PlayerState::new((4, 4), 3, facing);
        let mut feet = Vec::new();
        for _ in 0..4 {
            assert!(matches!(
                player.step(Some(facing), &runtime, &no_connections, &NO_FLAGS),
                StepOutcome::Advanced { .. }
            ));
            feet.push(player.second_foot_leads());
            finish_crossing(&mut player);
            assert_eq!(player.second_foot_leads(), *feet.last().unwrap());
        }
        assert_eq!(feet, [false, true, false, true], "{facing:?}");
    }
}

/// A standstill turn runs the same alternating `sAnim_GoFast*`, so it
/// consumes a half-cycle (`event_object_movement.c:5704-5721`).
#[test]
fn a_turn_in_place_consumes_a_half_cycle() {
    let runtime = open_runtime(None);
    let mut player = PlayerState::new((4, 4), 3, Direction::South);
    assert_eq!(
        player.step(Some(Direction::North), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Turned(Direction::North)
    );
    assert!(!player.second_foot_leads());
    for _ in 0..TURN_IN_PLACE_FRAMES {
        player.tick();
    }
    player.step(None, &runtime, &no_connections, &NO_FLAGS);
    player.step(Some(Direction::North), &runtime, &no_connections, &NO_FLAGS);
    assert!(player.second_foot_leads());
}

/// Rust models no bump animation for a collision-blocked step, so a
/// rejected or still-busy poll leaves the parity alone.
#[test]
fn blocked_and_busy_polls_leave_the_parity_alone() {
    let runtime = open_runtime(Some(5));
    let mut player = PlayerState::new((4, 4), 3, Direction::South);
    let before = player.second_foot_leads();
    assert!(matches!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked { .. }
    ));
    assert_eq!(player.second_foot_leads(), before);

    let mut player = PlayerState::new((4, 3), 3, Direction::West);
    player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS);
    let during = player.second_foot_leads();
    assert_eq!(
        player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Idle
    );
    assert_eq!(player.second_foot_leads(), during);
}

/// A keypad step off a held slide pose keeps the slide's paused foot
/// (`SetStepAnimHandleAlternation`, `event_object_movement.c:4582-4598`).
#[test]
fn a_step_off_a_held_slide_pose_keeps_the_slides_foot() {
    let runtime = slide_east_runtime();
    let mut player = PlayerState::new((2, 1), 3, Direction::South);
    player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    player.step(None, &runtime, &no_connections, &NO_FLAGS);
    for _ in 0..SLIDE_FRAMES_PER_TILE {
        player.tick();
    }
    assert!(player.slide_pose_held());
    let slide_foot = player.second_foot_leads();
    let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Advanced {
            from: (3, 2),
            to: (4, 2)
        }
    );
    assert_eq!(player.second_foot_leads(), slide_foot);
}
