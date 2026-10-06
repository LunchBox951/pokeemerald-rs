//! Landing-call arrow-warp elevation tests against the pre-movement field input.

use super::step::PreMovementFieldInput;
use super::test_support::held;
use super::OverworldPhase;
use engine::overworld::metatile_behavior::MB_SOUTH_ARROW_WARP;
use engine::overworld::{Direction, MapRuntime, PlayerState, WarpTrigger, WALK_FRAMES_PER_TILE};
use platform::Buttons;

const CENTER: assets::MapId = assets::MapId("MAP_OLDALE_TOWN_POKEMON_CENTER_1F");
const DOORMAT: (u16, u16) = (7, 8);

fn center_runtime(scene: &crate::overworld::OverworldScene) -> MapRuntime<'_> {
    let header = assets::MapHeaderTable::new()
        .header(CENTER)
        .expect("Oldale Town's Pokemon Center resolves in the generated map-header table");
    let events = assets::MapEventsTable::new()
        .resolve(CENTER)
        .expect("Oldale Town's Pokemon Center resolves in the generated map-events table");
    scene.runtime(CENTER, header, events)
}

/// Asserted on [`super::step::PreMovementFieldInput`], since pack-free
/// `warp_to` is a no-op.
///
/// The frame identity is issue #1039's: the call whose `PlayerState::tick`
/// drains the walk animation is upstream's last CB2 animation frame, and
/// the arrow poll for the tile that step landed on belongs to the
/// *next* call's CB1, where the player begins at rest.
#[test]
fn the_landing_call_resolves_the_arrow_warp_at_the_retained_previous_elevation() {
    let events = assets::MapEventsTable::new()
        .resolve(CENTER)
        .expect("Oldale Town's Pokemon Center resolves in the generated map-events table");
    let doormat = events.warp_events[0];
    assert_eq!((doormat.x, doormat.y), (7, 8));
    assert_eq!(
        doormat.elevation, 3,
        "fixture precondition: the doormat's warp event is stored at elevation 3"
    );

    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tiles_at_elevations(
            10,
            10,
            &[(DOORMAT, MB_SOUTH_ARROW_WARP, 0)],
        ),
        CENTER,
        PlayerState::new((7, 7), 3, Direction::South),
        None,
    );

    // The whole crossing, drain call included. Nothing may have polled
    // the doormat yet: that call is upstream's last CB2 animation frame.
    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
    }
    assert_eq!(phase.player.position(), (7, 8));
    assert!(!phase.player.in_transit());
    assert!(
        phase.pending_landing.is_some(),
        "the completed step is still unobserved after the drain call (issue #1039)"
    );
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 3)
    );

    // The landing call's CB1. `landing_claimed` is false here for the
    // same reason `step` computes it false: the doormat is
    // MB_SOUTH_ARROW_WARP, which `is_warp_trigger` fails closed on, and
    // a Pokemon Center has no wild table.
    let runtime = center_runtime(&phase.scene);
    let pre: PreMovementFieldInput = phase.resolve_pre_movement_field_input(
        held(Buttons::DOWN),
        Some(Direction::South),
        &runtime,
        false,
    );
    let trigger = pre.arrow_trigger;
    assert!(
        matches!(
            trigger,
            Some(WarpTrigger::Resolved { map, .. })
                if map == assets::MapId("MAP_OLDALE_TOWN")
        ),
        "the landing call's arrow poll must resolve at PlayerGetElevation()'s \
         retained 3 (field_player_avatar.c:1192-1195) -- a lookup at the collision \
         elevation 0 misses the warp event stored at 3; got {trigger:?}"
    );
}
