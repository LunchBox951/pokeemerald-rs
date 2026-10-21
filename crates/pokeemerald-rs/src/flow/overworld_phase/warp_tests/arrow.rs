//! Arrow-warp triggers (`field_control_avatar.c:155-168`), ordered before movement.

use super::super::test_support::*;
use engine::overworld::metatile_behavior::MB_SOUTH_ARROW_WARP;
use engine::overworld::{Direction, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

// Arrow-warp contract: field_control_avatar.c:155-168,688-699,767-780.
// Field input precedes movement: overworld.c:1438-1455.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_the_doormat_holding_south_exits_on_the_landing_call() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    let mut phase = one_f_phase((8, 7), Direction::South);

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the step onto the doormat must commit on the frame it begins"
    );
    assert_eq!(
        phase.map_id, ONE_F,
        "the arrow warp must not fire on the frame the step begins"
    );

    for frame in 2..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.map_id, ONE_F,
            "the warp must not fire mid-animation (frame {frame} of {WALK_FRAMES_PER_TILE})"
        );
    }

    phase.step(held(Buttons::DOWN));

    let destination = assets::MapId("MAP_LITTLEROOT_TOWN");
    assert_eq!(
        phase.map_id, destination,
        "the doormat's arrow-warp trigger must fire on the first call whose \
         field input observes the completed step with South still held"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the destination front door faces South"
    );
    assert!(
        !phase.player.in_transit(),
        "the warp lands the player at rest, not mid-step"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn releasing_south_mid_step_does_not_exit_through_the_doormat() {
    let mut phase = one_f_phase((8, 7), Direction::South);

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the step onto the doormat must still commit"
    );

    for _ in 2..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
    }
    assert!(!phase.player.in_transit(), "the walk animation has drained");
    phase.step(ButtonState::new());
    assert!(
        !phase.mid_step(),
        "the landing call consumes the completed step"
    );
    assert_eq!(
        phase.map_id, ONE_F,
        "a released direction must not warp: upstream needs heldDirection on \
         the frame the check runs, not merely a completed step"
    );
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the player stays standing on the doormat"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn releasing_south_on_the_landing_call_does_not_exit_through_the_doormat() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    let mut phase = one_f_phase((8, 7), Direction::South);

    for frame in 1..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.map_id, ONE_F,
            "the warp must not fire while the step is still outstanding \
             (frame {frame} of {WALK_FRAMES_PER_TILE})"
        );
    }
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the crossing must have landed on the doormat"
    );
    assert!(
        !phase.player.in_transit(),
        "frame {WALK_FRAMES_PER_TILE} drains the walk animation"
    );
    assert!(
        phase.mid_step(),
        "animation drained; landing remains pending until the next call"
    );

    phase.step(ButtonState::new());

    assert!(
        !phase.mid_step(),
        "the landing call consumes the completed step"
    );
    assert_eq!(
        phase.map_id, ONE_F,
        "no direction is held on the frame the arrow-warp gate is open, so \
         the doormat must not exit the house"
    );
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the player stays standing on the doormat"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_one_frame_down_tap_on_the_doormat_facing_north_turns_without_warping() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    let mut phase = one_f_phase((8, 8), Direction::North);

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id, ONE_F,
        "held South does not match the pre-movement North facing; this call only turns"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the tap turned the player"
    );
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "still standing on the doormat"
    );

    phase.step(ButtonState::new());
    assert_eq!(phase.map_id, ONE_F, "released South does not warp");

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "holding Down against the South facing fires the doormat normally"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn holding_south_on_the_north_facing_doormat_turns_then_exits() {
    let mut phase = one_f_phase((0, 0), Direction::South);

    phase.warp_to(ONE_F, 1);
    assert_eq!(phase.player.position(), (8, 8), "1F's warp #1: the doormat");
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a warp-in onto a south-arrow tile faces back out of it"
    );

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id, ONE_F,
        "the turn frame must not warp: the poll reads the pre-turn facing"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "frame 1 turned the player to face the held direction"
    );

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "the second held-South call matches the pre-movement South facing and exits"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_eq!(phase.player.facing(), Direction::South);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn arriving_on_the_doormat_without_held_direction_does_not_retrigger() {
    let mut phase = one_f_phase((0, 0), Direction::South);
    phase.warp_to(ONE_F, 1);

    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) * 2 {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.map_id, ONE_F,
            "arrival without a held direction must not retrigger"
        );
    }
    assert_eq!(phase.player.position(), (8, 8));
    assert_eq!(phase.player.facing(), Direction::North);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_the_south_arrow_doormat_holding_east_does_not_exit() {
    let mut phase = one_f_phase((7, 8), Direction::East);

    phase.step(held(Buttons::RIGHT));
    assert_eq!(phase.player.position(), (8, 8));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert!(!phase.player.in_transit(), "the walk animation has drained");
    assert!(
        phase.mid_step(),
        "the drained step still awaits the landing call"
    );
    assert_eq!(phase.player.position(), (8, 8));

    phase.step(held(Buttons::RIGHT));
    assert_ne!(
        phase.pending_landing,
        Some((8, 8)),
        "the landing call consumes the doormat landing"
    );

    assert_eq!(
        phase.map_id, ONE_F,
        "held East matches facing East, but not MB_SOUTH_ARROW_WARP"
    );
}

/// Pack-free guard for movement preemption, not proof of arrival.
/// Destination loading needs a pack; [`a_legal_step_in_the_arrow_direction_lands_the_warp`]
/// proves arrival.
#[test]
fn a_legal_step_in_the_arrow_direction_is_preempted_without_starting_a_walk() {
    let mut phase = south_facing_doormat_phase_with_walkable_exit();

    phase.step(held(Buttons::DOWN));

    assert_ne!(
        phase.player.position(),
        (8, 9),
        "the held-South arrow check preempts the legal step to (8, 9)"
    );
    assert!(
        !phase.player.in_transit(),
        "no walk animation was ever started -- the step never ran"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_legal_step_in_the_arrow_direction_lands_the_warp() {
    let mut phase = south_facing_doormat_phase_with_walkable_exit();

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "the preempting arrow warp must actually land, not merely eat the step"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_ne!(
        phase.player.position(),
        (8, 9),
        "and the step in the arrow direction still never happened"
    );
}
