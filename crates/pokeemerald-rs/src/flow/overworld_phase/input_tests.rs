//! Tests for [`super::input::advance_player_one_frame`] and
//! [`super::input::held_direction`].

use super::test_support::*;
use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};

/// Senior review round 3 regression.
#[test]
fn advance_player_one_frame_shows_progress_1_on_the_frame_a_step_begins_and_takes_16_frames_per_tile(
) {
    let runtime = flat_runtime(5, 5);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);

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

/// A turn-in-place never enters transit.
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
