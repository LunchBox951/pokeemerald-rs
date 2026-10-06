//! The forced-movement-tile field-input gate
//! (`field_control_avatar.c:95-113`) for the arrow-warp poll, the
//! animated-door poll, and the same-frame interaction; `start_menu`'s
//! own tests already pin its `START` half.

use super::interaction::InteractionOutcome;
use super::step::PreMovementFieldInput;
use super::test_support::{held, pressed, runtime_for, ONE_F};
use super::OverworldPhase;
use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_MUDDY_SLOPE, MB_SOUTH_ARROW_WARP};
use engine::overworld::{Direction, MapRuntime, PlayerState, WarpTrigger, WALK_FRAMES_PER_TILE};
use platform::Buttons;

/// Ticks the player directly rather than through `OverworldPhase::step`,
/// which would dispatch the still-armed forced slope instead of leaving
/// the player at rest for the next-call probe.
fn clear_landing_frame_without_forced_dispatch(phase: &mut OverworldPhase) {
    phase.player.tick();
    assert!(
        !phase.player.field_input_suppressed(),
        "fixture precondition: the landing call's own tick clears the one-frame flag"
    );
}

/// A synthetic [`MB_ANIMATED_DOOR`] over 1F's real warp #2 coordinate,
/// `(8, 2)` -- same substitution as
/// [`super::test_support::littleroot_lab_door_scene`]'s.
fn slope_below_the_upstairs_door_phase() -> OverworldPhase {
    let scene = crate::overworld::tests::synthetic_scene_with_special_tiles(
        10,
        10,
        &[((8, 3), MB_MUDDY_SLOPE), ((8, 2), MB_ANIMATED_DOOR)],
    );
    OverworldPhase::for_test(
        scene,
        ONE_F,
        PlayerState::new((8, 4), 3, Direction::North),
        None,
    )
}

/// The animated-door half of the gate: `poll_open`'s own
/// `!field_input_suppressed()` term (`step.rs:624-625`), since the door
/// poll is itself gated behind `poll_open`.
#[test]
fn a_forced_landing_suppresses_the_animated_door_poll_until_the_next_call() {
    let events = assets::MapEventsTable::new()
        .resolve(ONE_F)
        .expect("ONE_F must resolve in the generated map-events table");
    let door = events.warp_events[2];
    assert_eq!(
        (door.x, door.y),
        (8, 2),
        "fixture precondition: 1F's warp #2 is the upstairs door"
    );

    let mut phase = slope_below_the_upstairs_door_phase();
    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::UP));
    }
    assert_eq!(
        phase.player.position(),
        (8, 3),
        "setup: the held step must have crossed onto the muddy slope"
    );
    assert!(
        !phase.player.in_transit(),
        "setup: the crossing must have drained, so this is this port's first \
         T_TILE_CENTER CB1 for the landing"
    );
    assert!(
        phase.player.field_input_suppressed(),
        "fixture precondition: a forced-movement tile's own landing call is suppressed"
    );

    let landing_pre: PreMovementFieldInput = {
        let runtime = runtime_for(&phase);
        phase.resolve_pre_movement_field_input(
            held(Buttons::UP),
            Some(Direction::North),
            &runtime,
            false,
        )
    };
    assert_eq!(
        landing_pre.animated_door_trigger, None,
        "the landing call must never reach TryDoorWarp -- FieldGetPlayerInput \
         never sets heldDirection2 while forcedMove holds (field_control_avatar.c:95-113)"
    );

    clear_landing_frame_without_forced_dispatch(&mut phase);

    let at_rest_pre: PreMovementFieldInput = {
        let runtime = runtime_for(&phase);
        phase.resolve_pre_movement_field_input(
            held(Buttons::UP),
            Some(Direction::North),
            &runtime,
            false,
        )
    };
    assert!(
        matches!(
            at_rest_pre.animated_door_trigger,
            Some(WarpTrigger::Resolved { map, .. })
                if map == assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F")
        ),
        "the very next call is T_NOT_MOVING, where the same held Up must resolve \
         the door warp; got {:?}",
        at_rest_pre.animated_door_trigger
    );
}

/// A synthetic forced-movement slope one tile east of Brendan's House
/// 1F's own real Mom object event, `(2, 6)` (`map_events.rs`, visible on
/// a fresh save) -- so a forced landing sits immediately beside a real,
/// pack-free interaction target.
fn slope_beside_mom_phase() -> OverworldPhase {
    let scene =
        crate::overworld::tests::synthetic_scene_with_special_tile(10, 10, (3, 6), MB_MUDDY_SLOPE);
    OverworldPhase::for_test(
        scene,
        ONE_F,
        PlayerState::new((4, 6), 3, Direction::West),
        None,
    )
}

/// The same-frame interaction half of the gate: `interaction`'s own
/// `!field_input_suppressed()` term (`step.rs:645-648`), independent of
/// `poll_open`.
#[test]
fn a_forced_landing_suppresses_the_same_frame_interaction_until_the_next_call() {
    let events = assets::MapEventsTable::new()
        .resolve(ONE_F)
        .expect("ONE_F must resolve in the generated map-events table");
    let mom = events.object_events[0];
    assert_eq!(
        (mom.x, mom.y),
        (2, 6),
        "fixture precondition: Mom stands at (2, 6)"
    );
    assert_ne!(
        mom.script, "0x0",
        "fixture precondition: Mom's script is a real one, so A interacts"
    );

    let mut phase = slope_beside_mom_phase();
    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::LEFT));
    }
    assert_eq!(
        phase.player.position(),
        (3, 6),
        "setup: the held step must have crossed onto the muddy slope"
    );
    assert!(
        !phase.player.in_transit(),
        "setup: the crossing must have drained"
    );
    assert!(
        phase.player.field_input_suppressed(),
        "fixture precondition: a forced-movement tile's own landing call is suppressed"
    );

    let landing_pre: PreMovementFieldInput = {
        let runtime = runtime_for(&phase);
        phase.resolve_pre_movement_field_input(pressed(Buttons::A), None, &runtime, false)
    };
    assert_eq!(
        landing_pre.interaction, None,
        "the landing call must never reach TryStartInteractionScript -- \
         FieldGetPlayerInput never sets pressedAButton while forcedMove holds \
         (field_control_avatar.c:95-113)"
    );

    clear_landing_frame_without_forced_dispatch(&mut phase);

    // A fresh `ButtonState` models a fresh key edge, same as the
    // landing-call press above: two independent presses, not one held
    // across both probed frames.
    let at_rest_pre: PreMovementFieldInput = {
        let runtime = runtime_for(&phase);
        phase.resolve_pre_movement_field_input(pressed(Buttons::A), None, &runtime, false)
    };
    assert!(
        matches!(at_rest_pre.interaction, Some(InteractionOutcome::Dialog(_))),
        "the very next call is T_NOT_MOVING, where the same fresh A press must \
         admit Mom's dialog; got {:?}",
        at_rest_pre.interaction
    );
}

/// A forced-movement slope over 1F's real front-doormat warp
/// coordinate, `(9, 8)`, paired with [`arrow_probe_runtime`]'s
/// arrow-tile probe scene at the same position.
fn slope_onto_the_southward_warp_phase() -> OverworldPhase {
    let scene =
        crate::overworld::tests::synthetic_scene_with_special_tile(10, 10, (9, 8), MB_MUDDY_SLOPE);
    OverworldPhase::for_test(
        scene,
        ONE_F,
        PlayerState::new((9, 7), 3, Direction::South),
        None,
    )
}

/// [`runtime_for`]'s own construction, but over an arbitrary probe
/// scene rather than `phase.scene` -- Brendan's House 1F's real
/// header/events stay the same either way.
fn arrow_probe_runtime(scene: &crate::overworld::OverworldScene) -> MapRuntime<'_> {
    let header = assets::MapHeaderTable::new().header(ONE_F).unwrap();
    let events = assets::MapEventsTable::new().resolve(ONE_F).unwrap();
    scene.runtime(ONE_F, header, events)
}

/// The arrow-warp half of the gate: `poll_open`'s own
/// `!field_input_suppressed()` term, same as the door poll above but
/// through the probe-runtime substitution this data shape forces (see
/// [`slope_onto_the_southward_warp_phase`]'s own doc comment).
#[test]
fn a_forced_landing_suppresses_the_arrow_poll_until_the_next_call() {
    let events = assets::MapEventsTable::new()
        .resolve(ONE_F)
        .expect("ONE_F must resolve in the generated map-events table");
    let doormat = events.warp_events[0];
    assert_eq!(
        (doormat.x, doormat.y),
        (9, 8),
        "fixture precondition: 1F's warp #0 is the front doormat"
    );

    let mut phase = slope_onto_the_southward_warp_phase();
    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
    }
    assert_eq!(
        phase.player.position(),
        (9, 8),
        "setup: the held step must have crossed onto the muddy slope"
    );
    assert!(
        !phase.player.in_transit(),
        "setup: the crossing must have drained"
    );
    assert!(
        phase.player.field_input_suppressed(),
        "fixture precondition: a forced-movement tile's own landing call is suppressed"
    );

    let probe_scene = crate::overworld::tests::synthetic_scene_with_special_tile(
        10,
        10,
        (9, 8),
        MB_SOUTH_ARROW_WARP,
    );

    let landing_pre: PreMovementFieldInput = {
        let runtime = arrow_probe_runtime(&probe_scene);
        phase.resolve_pre_movement_field_input(
            held(Buttons::DOWN),
            Some(Direction::South),
            &runtime,
            false,
        )
    };
    assert_eq!(
        landing_pre.arrow_trigger, None,
        "the landing call must never reach TryArrowWarp -- FieldGetPlayerInput \
         never sets heldDirection while forcedMove holds (field_control_avatar.c:95-113)"
    );

    clear_landing_frame_without_forced_dispatch(&mut phase);

    let at_rest_pre: PreMovementFieldInput = {
        let runtime = arrow_probe_runtime(&probe_scene);
        phase.resolve_pre_movement_field_input(
            held(Buttons::DOWN),
            Some(Direction::South),
            &runtime,
            false,
        )
    };
    assert!(
        matches!(
            at_rest_pre.arrow_trigger,
            Some(WarpTrigger::Resolved { map, .. })
                if map == assets::MapId("MAP_LITTLEROOT_TOWN")
        ),
        "the very next call is T_NOT_MOVING, where the same held Down against \
         the probe's arrow tile must resolve the warp; got {:?}",
        at_rest_pre.arrow_trigger
    );
}
