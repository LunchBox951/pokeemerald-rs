//! Tests for [`super::input::advance_player_one_frame`] and
//! [`super::input::held_direction`].

use super::test_support::*;
use engine::overworld::{Direction, PlayerState};
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
        &runtime,
        &no_connections,
        &NO_FLAGS,
    );
    assert_eq!(player.facing(), Direction::East, "must have turned");
    assert_eq!(player.position(), (2, 2), "a turn must not move the tile");
    assert!(!player.in_transit());
    assert_eq!(player.step_progress(), 0);
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
