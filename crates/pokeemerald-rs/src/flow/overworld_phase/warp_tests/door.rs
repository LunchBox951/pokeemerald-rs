//! Animated-door warps: the pre-movement facing-tile check on the Littleroot lab door.

use super::super::test_support::*;
use assets::MapId;
use engine::overworld::{Direction, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// Animated doors are solid, so a completed-step check can never reach one.
/// The door must be looked up against the tile the player *faces* before
/// movement, and a held press toward it must preempt the step onto it.
///
/// The fixture makes the lab door's `(7, 16)` tile walkable, so a missing
/// pre-movement check shows up as the player stepping onto it. Asserting only
/// the negative half keeps this test pack-free; that the warp lands is pinned
/// by [`facing_the_lab_door_and_holding_north_enters_birchs_lab`].
#[test]
fn facing_the_lab_door_and_holding_north_preempts_movement_instead_of_stepping() {
    let mut phase = facing_littleroot_lab_door_phase(Direction::North);

    phase.step(held(Buttons::UP));

    assert_ne!(
        phase.player.position(),
        (7, 16),
        "the door check must preempt the legal step onto the walkable door tile \
         (field_control_avatar.c:170-178)"
    );
    assert!(
        !phase.player.in_transit(),
        "no walk animation was started -- the step never ran"
    );
}

/// The frame that turns the player toward the door must not warp: the door
/// check compares the held direction with the facing read before the turn, so
/// only the next held frame, already facing the door, can satisfy it.
#[test]
fn turning_to_face_the_lab_door_does_not_warp_on_the_turning_frame() {
    let mut phase = facing_littleroot_lab_door_phase(Direction::East);

    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "holding Up while facing East must turn the player to face North"
    );
    assert_eq!(
        phase.player.position(),
        (7, 17),
        "a turning frame must never also move or warp"
    );
    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN"),
        "a turning frame must not warp"
    );

    phase.step(held(Buttons::UP));
    assert_ne!(
        phase.player.position(),
        (7, 16),
        "now facing North, the second held-Up frame must preempt the step onto the \
         walkable door tile"
    );
}

/// Pack-free half of [`walking_up_to_the_lab_door_enters_it_on_the_next_input_frame`]:
/// the door must claim the first input read with the player at rest in front
/// of it, ahead of that frame's step. Only the negative half is asserted, as in
/// [`facing_the_lab_door_and_holding_north_preempts_movement_instead_of_stepping`].
#[test]
fn walking_up_to_the_lab_door_preempts_the_final_step_onto_it() {
    let mut phase = approaching_littleroot_lab_door_phase();

    for _ in 0..=2 * u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::UP));
    }

    assert_ne!(
        phase.player.position(),
        (7, 16),
        "the door check must preempt the final step onto the walkable door tile \
         on the first frame that sees the player at rest in front of it"
    );
    assert!(
        !phase.player.in_transit(),
        "the second tile crossing must have drained before held frame {}",
        2 * u32::from(WALK_FRAMES_PER_TILE) + 1
    );
}

/// Pack-gated half of
/// [`facing_the_lab_door_and_holding_north_preempts_movement_instead_of_stepping`]:
/// the preempting door warp must actually land, so a regression that skips
/// movement without warping fails here.
///
/// Needs a local pack because `warp_to` loads the destination room.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn facing_the_lab_door_and_holding_north_enters_birchs_lab() {
    let mut phase = on_checkout_pack(facing_littleroot_lab_door_phase(Direction::North));

    phase.step(held(Buttons::UP));

    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
        "holding Up in front of the lab's animated door must fire TryDoorWarp \
         (field_control_avatar.c:170-178, 833-856)"
    );
    assert_eq!(
        phase.player.position(),
        (6, 12),
        "arriving at the lab's own warp #0"
    );
}

/// Pack-gated half of [`walking_up_to_the_lab_door_preempts_the_final_step_onto_it`].
///
/// [`approaching_littleroot_lab_door_phase`] starts three tiles south of the
/// door, facing North. The walk to the tile in front of it takes two crossings
/// of `WALK_FRAMES_PER_TILE` held frames each. The door must stay shut on that
/// last drain frame and open on the next held frame (the input-read ordering
/// at `overworld.c:1438-1455`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_up_to_the_lab_door_enters_it_on_the_next_input_frame() {
    let mut phase = on_checkout_pack(approaching_littleroot_lab_door_phase());

    for _ in 0..2 * u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::UP));
    }
    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN"),
        "the drain frame reads no field input, so the door must not fire on held frame {}",
        2 * u32::from(WALK_FRAMES_PER_TILE)
    );

    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
        "held frame {} is the first input read with the player at rest, so TryDoorWarp \
         fires there",
        2 * u32::from(WALK_FRAMES_PER_TILE) + 1
    );
    assert_eq!(
        phase.player.position(),
        (6, 12),
        "arriving at the lab's own warp #0"
    );
}

/// Releasing Up on the frame after the walk drains leaves the door shut and
/// the player standing in front of it.
///
/// Pack-free, an erroneous warp attempt is indistinguishable from none:
/// `warp_to` fails to load the destination and leaves both asserted values
/// untouched. The decision itself is observed by `step`'s
/// `the_drain_call_of_a_walked_approach_resolves_no_door_warp`.
#[test]
fn releasing_up_after_the_walked_approach_drains_leaves_the_lab_door_shut() {
    let mut phase = approaching_littleroot_lab_door_phase();

    for _ in 0..2 * u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::UP));
    }

    phase.step(ButtonState::new());

    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN"),
        "Up was released on the only input read that could fire TryDoorWarp, \
         so the door must not open"
    );
    assert_eq!(
        phase.player.position(),
        (7, 17),
        "and the player must still be standing in front of it"
    );
}
