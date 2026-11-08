//! Door-warp drain-call sequencing tests against the pre-movement field input.

use super::step::PreMovementFieldInput;
use super::test_support::{
    approaching_littleroot_lab_door_phase, held, littleroot_runtime,
    mom_standing_in_the_house_door_phase,
};
use engine::overworld::{Direction, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// Upstream reaches `TryDoorWarp` (`field_control_avatar.c:170-178`)
/// only *after* `TryStartInteractionScript` (`:172`) has already
/// returned FALSE: an A press on an NPC standing in a doorway talks to
/// them, it never enters the door.
///
/// The last assertion is the ratchet: `step` composes `warp_trigger`
/// out of the completed-step door check and these two fields alone, so
/// nothing downstream can resurrect a door the same-frame interaction
/// already outranked.
#[test]
fn an_interaction_outranks_the_animated_door_check() {
    let phase = mom_standing_in_the_house_door_phase();
    let runtime = littleroot_runtime(&phase.scene);

    // A fresh A press with Up held, standing at rest already facing the
    // door -- exactly what `step` feeds this method. Nothing has landed
    // this call, so `landing_claimed` is false.
    let mut buttons = ButtonState::new();
    buttons.update(Buttons::A | Buttons::UP);
    let pre: PreMovementFieldInput =
        phase.resolve_pre_movement_field_input(buttons, Some(Direction::North), &runtime, false);

    assert!(
        pre.interaction.is_some(),
        "fixture precondition: the A press must find Mom in the doorway"
    );
    assert!(
        pre.animated_door_trigger.is_none(),
        "the pre-movement door check already yields to the interaction"
    );

    assert_eq!(
        pre.arrow_trigger.or(pre.animated_door_trigger),
        None,
        "this call's whole warp decision is these two fields plus the completed-step \
         door check, and no step landed here -- upstream reaches TryDoorWarp only \
         after TryStartInteractionScript falls through (field_control_avatar.c:172-178)"
    );
}

/// The drain call of a walked approach is this port's counterpart to
/// the CB2 that finishes the walk, where upstream processes no field
/// input at all (`super::animated_door`'s module docs), so the
/// frame's warp decision must come back empty even though the player's
/// walk animation ends on it one tile below the door with Up still
/// held.
///
/// Asserted on the decision rather than through
/// [`super::OverworldPhase::step`], because a whole-frame test cannot
/// see this: a restored drain-call poll would resolve the lab warp, but
/// pack-free `warp_to` fails to load the destination and returns
/// leaving both `map_id` and player position untouched
/// (`connections.rs:274-281`), making the erroneous attempt
/// observationally identical to no attempt (issue #851 review).
#[test]
fn the_drain_call_of_a_walked_approach_resolves_no_door_warp() {
    let mut phase = approaching_littleroot_lab_door_phase();

    // One call short of the second crossing's drain.
    for _ in 0..2 * u32::from(WALK_FRAMES_PER_TILE) - 1 {
        phase.step(held(Buttons::UP));
    }
    assert!(
        phase.player.in_transit(),
        "fixture precondition: the second crossing must still be draining here"
    );
    assert_eq!(
        phase.player.position(),
        (7, 17),
        "fixture precondition: position commits at crossing start, one tile below the door"
    );

    // Replay the drain call itself in `step`'s own order: pre-movement
    // field input, then this frame's movement, then the warp decision.
    let pre = {
        let runtime = littleroot_runtime(&phase.scene);
        phase.resolve_pre_movement_field_input(
            held(Buttons::UP),
            Some(Direction::North),
            &runtime,
            false,
        )
    };
    assert_eq!(
        pre.arrow_trigger.or(pre.animated_door_trigger),
        None,
        "upstream cannot read this frame's held direction for TryDoorWarp -- \
         heldDirection2 is only set at T_TILE_CENTER/T_NOT_MOVING \
         (field_control_avatar.c:95-112), which the drain call is not"
    );
    phase.player.tick();
    assert!(
        !phase.player.in_transit(),
        "fixture precondition: this call drains the crossing"
    );
    assert!(
        phase.pending_landing.is_some(),
        "and the completed step stays latched for the next call's CB1 (issue #1039)"
    );
}
