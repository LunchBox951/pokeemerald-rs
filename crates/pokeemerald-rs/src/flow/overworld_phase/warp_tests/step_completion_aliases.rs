//! Completed-step warps on the non-animated-door aliases
//! (`MetatileBehavior_IsNonAnimDoor`, `metatile_behavior.c:262-270`).

use super::super::test_support::*;
use super::super::OverworldPhase;
use engine::overworld::metatile_behavior::{
    MB_CAVE, MB_DEEP_SOUTH_WARP, MB_NON_ANIMATED_DOOR, MB_WATER_DOOR,
};
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use engine::rng::Rng;
use platform::Buttons;

/// A step onto a warp tile whose behavior is one of `IsNonAnimDoor`'s ids must
/// end the frame in `TryStartWarpEventScript` (`field_control_avatar.c:155-161,
/// 751-765`), ahead of `CheckStandardWildEncounter`.
///
/// Observed through the suppressed encounter roll, as
/// `a_door_warp_is_looked_up_at_the_retained_previous_elevation` does: the
/// warp's destination is outside this port's bundled layouts, so `warp_to`
/// leaves the phase in place, but a roll that saw the tile would record its
/// behavior in `sPrevMetatileBehavior`.
fn assert_step_onto_warp_tile_fires_the_warp(warp_behavior: u8) {
    const CAVE: assets::MapId = assets::MapId("MAP_GRANITE_CAVE_B1F");
    const FLOOR: (u16, u16) = (7, 5);
    const WARP: (u16, u16) = (8, 5);

    let events = assets::MapEventsTable::new()
        .resolve(CAVE)
        .expect("Granite Cave B1F resolves in the generated map-events table");
    assert!(
        events
            .warp_events
            .iter()
            .any(|w| (w.x, w.y) == (8, 5) && w.elevation == 3),
        "fixture precondition: the warp tile carries a warp event stored at elevation 3"
    );

    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_special_tiles(
            10,
            10,
            &[(FLOOR, MB_CAVE), (WARP, warp_behavior)],
        ),
        CAVE,
        PlayerState::new((6, 5), 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);
    // Same screen override as `a_door_warp_frame_never_reaches_the_encounter_roll`.
    phase.wild_table_screen = Some((CAVE, true));

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (7, 5));
    // The floor tile's landing call, which also starts the next crossing.
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_CAVE,
        "fixture precondition: an unsuppressed roll really does overwrite this"
    );
    assert_eq!(phase.player.position(), (8, 5));

    for _ in 0..WALK_FRAMES_PER_TILE - 1 {
        phase.step(held(Buttons::RIGHT));
    }
    assert!(!phase.player.in_transit());
    // The warp tile's own landing call.
    phase.step(held(Buttons::RIGHT));

    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_CAVE,
        "{warp_behavior:#04x}: IsWarpMetatileBehavior accepts it, so \
         ProcessPlayerFieldInput returns before CheckStandardWildEncounter and the \
         tile's own behavior is never recorded"
    );
}

#[test]
fn a_non_animated_door_step_fires_the_completed_step_warp() {
    assert_step_onto_warp_tile_fires_the_warp(MB_NON_ANIMATED_DOOR);
}

#[test]
fn a_water_door_step_fires_the_completed_step_warp() {
    assert_step_onto_warp_tile_fires_the_warp(MB_WATER_DOOR);
}

#[test]
fn a_deep_south_warp_step_fires_the_completed_step_warp() {
    assert_step_onto_warp_tile_fires_the_warp(MB_DEEP_SOUTH_WARP);
}
