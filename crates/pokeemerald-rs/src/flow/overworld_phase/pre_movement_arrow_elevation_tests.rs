//! Pre-movement arrow-warp elevation handling tests.

use super::step::PreMovementFieldInput;
use super::test_support::held;
use super::OverworldPhase;
use engine::overworld::metatile_behavior::MB_NORTH_ARROW_WARP;
use engine::overworld::{Direction, MapRuntime, PlayerState, WarpTrigger, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

const CAVE: assets::MapId = assets::MapId("MAP_GRANITE_CAVE_B1F");
const ARROW: (u16, u16) = (8, 5);

fn cave_runtime(scene: &crate::overworld::OverworldScene) -> MapRuntime<'_> {
    let header = assets::MapHeaderTable::new()
        .header(CAVE)
        .expect("Granite Cave B1F resolves in the generated map-header table");
    let events = assets::MapEventsTable::new()
        .resolve(CAVE)
        .expect("Granite Cave B1F resolves in the generated map-events table");
    scene.runtime(CAVE, header, events)
}

/// A frame inside the turn lock, where upstream already reaches `TryArrowWarp`.
#[test]
fn a_turning_frame_resolves_the_arrow_warp_at_the_retained_previous_elevation() {
    let events = assets::MapEventsTable::new()
        .resolve(CAVE)
        .expect("Granite Cave B1F resolves in the generated map-events table");
    assert!(
        events
            .warp_events
            .iter()
            .any(|w| (w.x, w.y) == (8, 5) && w.elevation == 3),
        "fixture precondition: the arrow tile carries a warp event stored at elevation 3"
    );

    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tiles_at_elevations(
            10,
            10,
            &[(ARROW, MB_NORTH_ARROW_WARP, 0)],
        ),
        CAVE,
        PlayerState::new((7, 5), 3, Direction::East),
        None,
    );

    // East never matches a north arrow; this only lands on the transition cell.
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (8, 5));
    assert!(!phase.player.in_transit());
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 3)
    );

    // A neutral frame ends the streak, so the next Up frame turns in place.
    phase.step(ButtonState::new());
    phase.step(held(Buttons::UP));
    assert_eq!(phase.player.facing(), Direction::North);
    assert_eq!(phase.player.position(), (8, 5), "the Up frame only turned");
    assert!(
        phase.player.turn_frames_remaining() > 0,
        "fixture precondition: the standstill turn's lock is still draining"
    );

    let pre: PreMovementFieldInput = {
        let runtime = cave_runtime(&phase.scene);
        phase.resolve_pre_movement_field_input(
            held(Buttons::UP),
            Some(Direction::North),
            &runtime,
            false,
        )
    };
    assert!(
        matches!(
            pre.arrow_trigger,
            Some(WarpTrigger::Resolved { map, .. })
                if map == assets::MapId("MAP_GRANITE_CAVE_B2F")
        ),
        "the at-rest arrow preempt must resolve at PlayerGetElevation()'s retained \
         3 (field_player_avatar.c:1192-1195) -- a lookup at the collision elevation \
         0 misses the warp event stored at 3; got {:?}",
        pre.arrow_trigger
    );
}
