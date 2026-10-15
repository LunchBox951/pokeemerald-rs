//! Tests for [`super::input::advance_player_one_frame`] and
//! [`super::input::held_direction`].

use super::input::run_held;
use super::test_support::*;
use super::OverworldPhase;
use engine::overworld::{Direction, PlayerState, StepOutcome};
use platform::{ButtonState, Buttons};

/// Senior review round 3 regression, correcting the prior (empirically
/// wrong -- see [`advance_player_one_frame`]'s own doc comment) "skip
/// the first tick" change: the first frame composed after a step begins
/// must see `step_progress() == 1`, not `0` -- upstream applies the
/// first walk-animation frame in the very call that starts the step
/// (`MovementAction_WalkNormalDown_Step0`'s `InitMovementNormal`
/// immediately followed by `Step1` -> `UpdateMovementNormal` ->
/// `NpcTakeStep`, `pokeemerald/src/event_object_movement.c:5354-5358`)
/// -- and a full tile crossing takes exactly
/// [`engine::overworld::WALK_FRAMES_PER_TILE`] (16) rendered frames.
#[test]
fn advance_player_one_frame_shows_progress_1_on_the_frame_a_step_begins_and_takes_16_frames_per_tile(
) {
    let runtime = flat_runtime(5, 5);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);

    // Facing South already; a held South poll steps immediately (no
    // turn-in-place first, since the direction already matches facing).
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
         walk animation, matching upstream's InitMovementNormal-then- \
         immediately-Step1 shape"
    );

    // Every following frame advances the timer by exactly 1 while the
    // input stays held.
    for expected in 2..engine::overworld::WALK_FRAMES_PER_TILE {
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

    // The 16th frame (this crossing's `WALK_FRAMES_PER_TILE`th) is the
    // one where the transit settles -- 16 rendered frames total to
    // cross one tile, not 17.
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
        "the transit must settle on exactly the 16th frame"
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

/// A held direction stays swallowed through the turn's busy window and
/// lands its first step on the 9th frame, not the 2nd.
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

    for frame in 2..=8 {
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
            "frame {frame} is still inside the turn's 8-frame busy window"
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
    // field_control_avatar.c's own if/else-if chain order (see
    // `held_direction`'s doc comment): up beats every simultaneous
    // combination.
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
fn held_b_with_the_shoes_flag_crosses_a_tile_in_eight_rendered_frames() {
    let runtime = flat_runtime(5, 5);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    let mut shoes = engine::event_data::EventData::new();
    shoes.flag_set(0x8C0).unwrap();
    advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        true,
        &runtime,
        &no_connections,
        &shoes,
    );
    assert_eq!(player.step_progress(), 1);
    for expected in 2..engine::overworld::RUN_FRAMES_PER_TILE {
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
    assert!(!player.in_transit(), "the eighth frame drains the run");
}

/// Every forced mover reaches the player through the production driver on a
/// `None` poll and on a held-opposite poll, with the cadence its own upstream
/// mover uses: walk 16, slide and slip 8, current 6 frames
/// (`field_player_avatar.c:473-552`, `event_object_movement.c:8233-8298`).
/// The triggering call shows progress 1 and the crossing settles on call N.
#[test]
fn advance_player_one_frame_drives_every_forced_mover_at_its_own_cadence() {
    use engine::overworld::metatile_behavior::*;
    let movers: [(u8, Direction, u8); 11] = [
        (MB_WALK_EAST, Direction::East, 16),
        (MB_SLIDE_SOUTH, Direction::South, 8),
        (MB_ICE, Direction::East, 8),
        (MB_ICE, Direction::North, 8),
        (MB_TRICK_HOUSE_PUZZLE_8_FLOOR, Direction::West, 8),
        (MB_TRICK_HOUSE_PUZZLE_8_FLOOR, Direction::South, 8),
        (MB_SOUTHWARD_CURRENT, Direction::South, 6),
        (MB_NORTHWARD_CURRENT, Direction::North, 6),
        (MB_WESTWARD_CURRENT, Direction::West, 6),
        (MB_EASTWARD_CURRENT, Direction::East, 6),
        (MB_SLIDE_WEST, Direction::West, 8),
    ];
    for (behavior, direction, frames) in movers {
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
            for _ in 1..engine::overworld::WALK_FRAMES_PER_TILE {
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
