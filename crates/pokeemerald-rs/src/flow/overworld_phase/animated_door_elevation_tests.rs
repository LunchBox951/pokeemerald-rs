//! Pre-movement animated-door elevation handling tests.

use super::step::PreMovementFieldInput;
use super::test_support::held;
use super::OverworldPhase;
use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_NORMAL};
use engine::overworld::{Direction, MapRuntime, PlayerState, WarpTrigger, WALK_FRAMES_PER_TILE};
use platform::Buttons;

const CAVE: assets::MapId = assets::MapId("MAP_GRANITE_CAVE_B1F");
const DOOR: (u16, u16) = (8, 5);

fn cave_runtime(scene: &crate::overworld::OverworldScene) -> MapRuntime<'_> {
    let header = assets::MapHeaderTable::new()
        .header(CAVE)
        .expect("Granite Cave B1F resolves in the generated map-header table");
    let events = assets::MapEventsTable::new()
        .resolve(CAVE)
        .expect("Granite Cave B1F resolves in the generated map-events table");
    scene.runtime(CAVE, header, events)
}

/// A transition landing then a multi-level landing keeps collision 0 and
/// retained 3 (`PlayerState::adopt_elevation`).
#[test]
fn the_animated_door_poll_resolves_at_the_retained_previous_elevation() {
    let events = assets::MapEventsTable::new()
        .resolve(CAVE)
        .expect("Granite Cave B1F resolves in the generated map-events table");
    assert!(
        events
            .warp_events
            .iter()
            .any(|w| (w.x, w.y) == (8, 5) && w.elevation == 3),
        "fixture precondition: the door tile carries a warp event stored at elevation 3"
    );

    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tiles_at_elevations(
            10,
            10,
            &[
                (DOOR, MB_ANIMATED_DOOR, 3),
                ((8, 6), MB_NORMAL, 15),
                ((8, 7), MB_NORMAL, 0),
            ],
        ),
        CAVE,
        PlayerState::new((8, 8), 3, Direction::North),
        None,
    );

    for _ in 0..2 * u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::UP));
    }
    assert_eq!(phase.player.position(), (8, 6));
    assert!(!phase.player.in_transit());
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 3)
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
            pre.animated_door_trigger,
            Some(WarpTrigger::Resolved { map, .. })
                if map == assets::MapId("MAP_GRANITE_CAVE_B2F")
        ),
        "the animated-door poll must resolve at PlayerGetElevation()'s retained 3 \
         (field_player_avatar.c:1192-1195) -- a lookup at the collision elevation 0 \
         misses the warp event stored at 3; got {:?}",
        pre.animated_door_trigger
    );
}
