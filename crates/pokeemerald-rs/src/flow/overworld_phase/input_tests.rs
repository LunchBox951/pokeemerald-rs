//! Tests for [`super::input::advance_player_one_frame`],
//! [`super::input::held_direction`], and [`super::input::run_held`].

use super::input::run_held;
use super::test_support::*;
use super::OverworldPhase;
use engine::overworld::{Direction, PlayerState, StepOutcome};
use platform::{ButtonState, Buttons};

const FLAG_SYS_B_DASH: u16 = 0x8C0;

// Expected cadences, fixed here rather than imported so that a change to the
// engine's constants fails these tests instead of moving the oracle with them.
// Walking is 16 frames per tile (`sStepTimes[MOVE_SPEED_NORMAL]`, and the
// 16-entry `sStep1Funcs`, `event_object_movement.c`); sliding and running are
// 8, and current movers 6 (`field_player_avatar.c:473-552`,
// `event_object_movement.c:8233-8298`). A turn in place holds input for 8
// frames.
const WALK_FRAMES: u8 = 16;
const SLIDE_FRAMES: u8 = 8;
const RUN_FRAMES: u8 = 8;
const CURRENT_FRAMES: u8 = 6;
const TURN_BUSY_FRAMES: u8 = 8;

/// The frame that begins a step already shows `step_progress() == 1`, and a
/// tile crossing takes exactly `WALK_FRAMES` frames, because upstream
/// applies the first animation frame in the call that starts the step.
#[test]
fn advance_player_one_frame_shows_progress_1_on_the_frame_a_step_begins_and_takes_walk_frames_per_tile(
) {
    let runtime = flat_runtime(5, 5);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);

    // Already facing South, so a held South poll steps without turning first.
    advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        false,
        &runtime,
        &no_connections,
        &NO_FLAGS,
    );
    assert_eq!(player.position(), (2, 3), "the step must have landed");
    assert!(player.in_transit());
    assert_eq!(
        player.step_progress(),
        1,
        "the frame that just started a step is already 1px into the \
         walk animation"
    );

    // Each following frame advances the timer by 1 while the input stays held.
    for expected in 2..WALK_FRAMES {
        advance_player_one_frame(
            &mut player,
            Some(Direction::South),
            false,
            &runtime,
            &no_connections,
            &NO_FLAGS,
        );
        assert_eq!(player.step_progress(), expected);
    }
    assert!(
        player.in_transit(),
        "still mid-transit one frame before settling"
    );

    // The transit settles on the `WALK_FRAMES`th frame, not one later.
    advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        false,
        &runtime,
        &no_connections,
        &NO_FLAGS,
    );
    assert!(
        !player.in_transit(),
        "the transit must settle on exactly the last frame of the crossing"
    );
}

#[test]
fn advance_player_one_frame_turning_in_place_never_enters_transit() {
    let runtime = flat_runtime(5, 5);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);

    advance_player_one_frame(
        &mut player,
        Some(Direction::East),
        false,
        &runtime,
        &no_connections,
        &NO_FLAGS,
    );
    assert_eq!(player.facing(), Direction::East, "must have turned");
    assert_eq!(player.position(), (2, 2), "a turn must not move the tile");
    assert!(!player.in_transit());
    assert_eq!(player.step_progress(), 0);
}

/// A held direction is swallowed through the turn's busy window; the first
/// step begins on the frame after `TURN_BUSY_FRAMES` have drained.
#[test]
fn advance_player_one_frame_swallows_input_through_the_turns_busy_window_before_stepping() {
    let runtime = flat_runtime(5, 5);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::East),
        false,
        &runtime,
        &no_connections,
        &NO_FLAGS,
    );
    assert_eq!(outcome, StepOutcome::Turned(Direction::East));

    for frame in 2..=TURN_BUSY_FRAMES {
        let outcome = advance_player_one_frame(
            &mut player,
            Some(Direction::East),
            false,
            &runtime,
            &no_connections,
            &NO_FLAGS,
        );
        assert_eq!(
            outcome,
            StepOutcome::Idle,
            "frame {frame} is still inside the turn's busy window"
        );
        assert_eq!(player.position(), (2, 2), "frame {frame} must not move");
    }

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::East),
        false,
        &runtime,
        &no_connections,
        &NO_FLAGS,
    );
    assert_eq!(
        outcome,
        StepOutcome::Advanced {
            from: (2, 2),
            to: (3, 2),
        },
        "the step begins only once the turn's busy window has drained"
    );
}

#[test]
fn held_direction_prioritizes_up_over_every_other_direction() {
    assert_eq!(
        held_direction(held(
            Buttons::UP | Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT
        )),
        Some(Direction::North)
    );
    assert_eq!(
        held_direction(held(Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT)),
        Some(Direction::South)
    );
    assert_eq!(
        held_direction(held(Buttons::LEFT | Buttons::RIGHT)),
        Some(Direction::West)
    );
    assert_eq!(held_direction(held(Buttons::RIGHT)), Some(Direction::East));
    assert_eq!(held_direction(ButtonState::new()), None);
}

#[test]
fn run_held_follows_the_held_b_bit_not_the_press_edge() {
    assert!(run_held(held(Buttons::B | Buttons::UP)));
    assert!(!run_held(held(Buttons::UP)));
    assert!(!run_held(ButtonState::new()));
}

#[test]
fn held_b_with_the_shoes_flag_crosses_a_tile_in_run_frames_per_tile() {
    let runtime = flat_runtime(5, 5);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    let mut shoes = engine::event_data::EventData::new();
    shoes.flag_set(FLAG_SYS_B_DASH).unwrap();
    advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        true,
        &runtime,
        &no_connections,
        &shoes,
    );
    assert_eq!(player.step_progress(), 1);
    for expected in 2..RUN_FRAMES {
        advance_player_one_frame(
            &mut player,
            Some(Direction::South),
            true,
            &runtime,
            &no_connections,
            &shoes,
        );
        assert_eq!(player.step_progress(), expected);
    }
    advance_player_one_frame(&mut player, None, true, &runtime, &no_connections, &shoes);
    assert!(
        !player.in_transit(),
        "the last frame of the crossing drains the run"
    );
}

/// Each forced-movement behavior with the direction it moves the player and
/// the frames its crossing takes.
fn forced_movers() -> [(u8, Direction, u8); 11] {
    use engine::overworld::metatile_behavior::*;
    [
        (MB_WALK_EAST, Direction::East, WALK_FRAMES),
        (MB_SLIDE_SOUTH, Direction::South, SLIDE_FRAMES),
        (MB_ICE, Direction::East, SLIDE_FRAMES),
        (MB_ICE, Direction::North, SLIDE_FRAMES),
        (MB_TRICK_HOUSE_PUZZLE_8_FLOOR, Direction::West, SLIDE_FRAMES),
        (
            MB_TRICK_HOUSE_PUZZLE_8_FLOOR,
            Direction::South,
            SLIDE_FRAMES,
        ),
        (MB_SOUTHWARD_CURRENT, Direction::South, CURRENT_FRAMES),
        (MB_NORTHWARD_CURRENT, Direction::North, CURRENT_FRAMES),
        (MB_WESTWARD_CURRENT, Direction::West, CURRENT_FRAMES),
        (MB_EASTWARD_CURRENT, Direction::East, CURRENT_FRAMES),
        (MB_SLIDE_WEST, Direction::West, SLIDE_FRAMES),
    ]
}

/// Every forced mover reaches the player through `advance_player_one_frame`
/// on a `None` poll and on a held-opposite poll, at its own cadence
/// (`field_player_avatar.c:473-552`, `event_object_movement.c:8233-8298`). The
/// triggering call shows progress 1 and the crossing settles on call `frames`.
#[test]
fn advance_player_one_frame_drives_every_forced_mover_at_its_own_cadence() {
    for (behavior, direction, frames) in forced_movers() {
        for held_opposite in [false, true] {
            let scene = crate::overworld::test_support::synthetic_scene_with_special_tile(
                10,
                10,
                (4, 3),
                behavior,
            );
            let phase = OverworldPhase::for_test(
                scene,
                ONE_F,
                PlayerState::new((0, 0), 3, direction),
                None,
            );
            let runtime = runtime_for(&phase);
            let (dx, dy) = direction.delta();
            let mut player = PlayerState::new((4 - dx, 3 - dy), 3, direction);
            advance_player_one_frame(
                &mut player,
                Some(direction),
                false,
                &runtime,
                &no_connections,
                &NO_FLAGS,
            );
            for _ in 1..WALK_FRAMES {
                advance_player_one_frame(
                    &mut player,
                    None,
                    false,
                    &runtime,
                    &no_connections,
                    &NO_FLAGS,
                );
            }
            assert_eq!(player.position(), (4, 3), "setup: {behavior} entered");
            assert!(!player.in_transit(), "setup: entry drained");

            let input = held_opposite.then_some(match direction {
                Direction::East => Direction::West,
                Direction::West => Direction::East,
                Direction::North => Direction::South,
                Direction::South => Direction::North,
            });
            let outcome = advance_player_one_frame(
                &mut player,
                input,
                false,
                &runtime,
                &no_connections,
                &NO_FLAGS,
            );
            assert!(
                matches!(outcome, StepOutcome::Advanced { from: (4, 3), .. }),
                "behavior {behavior} {direction:?} opp {held_opposite}: {outcome:?} {:?}",
                player.position()
            );
            assert_eq!(player.position(), (4 + dx, 3 + dy));
            assert_eq!(player.step_progress(), 1);
            for expected in 2..frames {
                advance_player_one_frame(
                    &mut player,
                    input,
                    false,
                    &runtime,
                    &no_connections,
                    &NO_FLAGS,
                );
                assert_eq!(player.step_progress(), expected, "behavior {behavior}");
                assert!(player.in_transit());
            }
            advance_player_one_frame(
                &mut player,
                input,
                false,
                &runtime,
                &no_connections,
                &NO_FLAGS,
            );
            assert!(
                !player.in_transit(),
                "behavior {behavior} {direction:?} settles on frame {frames}"
            );
        }
    }
}
