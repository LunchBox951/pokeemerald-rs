//! Object-event collision regressions.

use super::*;
use crate::overworld::object_event::ObjectEventCollection;
use crate::overworld::object_event_harness::{admitted, template, Rig};
use assets::MovementType;

#[test]
fn a_visible_object_event_blocks_the_step_and_leaves_the_player_facing_it() {
    let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 3, "0")]));
    let runtime = runtime_with_objects(objects);

    let mut player = PlayerState::new((2, 2), 3, Direction::North);
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Turned(Direction::South)
    );
    player.tick();

    // Drain the turn's busy window (`TURN_IN_PLACE_FRAMES` doc comment)
    // before the held direction can be attempted as a step at all.
    for _ in 2..=8 {
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Idle,
            "still inside the turn's busy window"
        );
        player.tick();
    }

    assert_eq!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::ObjectEvent,
        }
    );
    assert_eq!(
        player.position(),
        (2, 2),
        "a step into an occupied tile must not move the player"
    );
    assert_eq!(player.facing(), Direction::South);
    assert!(
        !player.in_transit(),
        "a blocked step must not start a transition"
    );

    // The held bump swallows the still-blocked repeats; the next attempt
    // lands once its 32 frames are spent.
    for _ in 0..BUMP_IN_PLACE_FRAMES {
        player.tick();
        assert_eq!(player.position(), (2, 2));
    }
    assert!(matches!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked {
            collision: super::super::collision::Collision::ObjectEvent,
            ..
        }
    ));
}

#[test]
fn a_hidden_object_event_does_not_block_the_step() {
    let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(
        1,
        2,
        3,
        3,
        "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
    )]));
    let runtime = runtime_with_objects(objects);
    let mut data = EventData::new();
    let hidden_npc_flag = assets::object_event_flags::resolve(
        "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
    )
    .expect("a bundled hide flag must resolve");
    data.flag_set(hidden_npc_flag).unwrap();

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &no_connections, &data),
        StepOutcome::Advanced {
            from: (2, 2),
            to: (2, 3),
        },
        "a hidden template does not occupy the map"
    );
    assert_eq!(player.position(), (2, 3));
}

#[test]
fn object_event_collision_respects_the_elevation_wildcard() {
    let upstairs: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 5, "0")]));
    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert!(matches!(
        player.step(
            Some(Direction::South),
            &runtime_with_objects(upstairs),
            &no_connections,
            &NO_FLAGS
        ),
        StepOutcome::Advanced { .. }
    ));

    let transitional: &'static [ObjectEvent] = Box::leak(Box::new([object(
        1,
        2,
        3,
        super::super::collision::ELEVATION_TRANSITION,
        "0",
    )]));
    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert_eq!(
        player.step(
            Some(Direction::South),
            &runtime_with_objects(transitional),
            &no_connections,
            &NO_FLAGS
        ),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::ObjectEvent,
        }
    );
}

#[test]
fn a_wall_outranks_an_object_event_on_the_same_tile() {
    let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 3, "0")]));
    let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: objects,
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));
    let runtime = runtime_with_events(events, |x, y| u8::from(x == 2 && y == 3));

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::Impassable,
        }
    );
}

#[test]
fn elevation_mismatch_outranks_an_object_event_on_the_same_tile() {
    let width = 5u16;
    let height = 5u16;
    let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
    for y in 0..height {
        for x in 0..width {
            let elevation = if x == 2 && y == 3 { 7 } else { 3 };
            let raw = MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width,
        height,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let grid = layout.grid(&bytes).unwrap();
    let header = MapHeader {
        id: assets::MapId("MAP_TEST"),
        group: 0,
        num: 0,
        name: "MapTest",
        layout: assets::LayoutId("MAP_TEST"),
        music: assets::MusicId(0),
        region_map_section: RegionMapSectionId("MAPSEC_NONE"),
        requires_flash: false,
        weather: Weather::None,
        map_type: MapType::Route,
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal,
        map_scripts: None,
        connections: &[] as &'static [MapConnection],
    };
    let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 3, "0")]));
    let events = MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: objects,
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    };
    let runtime = MapRuntime::new(
        assets::MapId("MAP_TEST"),
        &header,
        &events,
        grid,
        MetatileAttributeTable::new(&[]),
        MetatileAttributeTable::new(&[]),
    );

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::ElevationMismatch,
        },
        "elevation mismatch must outrank an object event on the same \
         tile, the same order GetCollisionAtCoords enforces"
    );
    assert_eq!(player.position(), (2, 2));
}

#[test]
fn a_hidden_first_stack_blocks_only_while_some_template_on_the_tile_is_visible() {
    let objects: &'static [ObjectEvent] = Box::leak(Box::new([
        object(
            1,
            2,
            3,
            3,
            "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL",
        ),
        object(
            2,
            2,
            3,
            3,
            "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE",
        ),
    ]));
    let runtime = runtime_with_objects(objects);

    let mut data = EventData::new();
    let cyndaquil = assets::object_event_flags::resolve(
        "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL",
    )
    .expect("a real FLAG_HIDE_* name must resolve");
    let totodile = assets::object_event_flags::resolve(
        "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE",
    )
    .expect("a real FLAG_HIDE_* name must resolve");
    data.flag_set(cyndaquil).unwrap();

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &no_connections, &data),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::ObjectEvent,
        },
        "the visible second template occupies the tile even though the \
         first one declared there is hidden"
    );

    data.flag_set(totodile).unwrap();
    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert!(matches!(
        player.step(Some(Direction::South), &runtime, &no_connections, &data),
        StepOutcome::Advanced { .. }
    ));
}

#[test]
fn mom_blocks_a_step_into_her_tile_in_brendans_house_1f() {
    let events = assets::MapEventsTable::new()
        .resolve(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"))
        .expect("a bundled map must resolve in the generated table");
    let mom = events
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_MOM")
        .expect("1F's object events include Mom");
    assert_eq!(
        (mom.x, mom.y, mom.elevation),
        (2, 6, 3),
        "fixture precondition: Mom's real map.json position"
    );

    let mut data = EventData::new();
    for &id in assets::RESET_MAP_FLAGS {
        data.flag_set(id).unwrap();
    }
    assert!(
        super::super::object_event::object_event_is_visible(mom, &data),
        "fixture precondition: a fresh save does not hide Mom"
    );

    let runtime = runtime_with_events(events, |_, _| 0);
    let mut player = PlayerState::new((2, 7), 3, Direction::North);
    assert_eq!(
        player.step(Some(Direction::North), &runtime, &no_connections, &data),
        StepOutcome::Blocked {
            direction: Direction::North,
            collision: super::super::collision::Collision::ObjectEvent,
        }
    );
    assert_eq!(
        player.position(),
        (2, 7),
        "the player must stop on the tile adjacent to Mom"
    );
    assert_eq!(player.facing(), Direction::North);
}

fn moved_east_runtime() -> MapRuntime<'static> {
    // Authored at (2, 3); walked east to the live tile (3, 3).
    let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 3, "0")]));
    let mut collection = admitted(objects, (2, 3));
    collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::East);
    let collection: &'static ObjectEventCollection = Box::leak(Box::new(collection));
    runtime_with_objects(objects).with_object_events(collection)
}

#[test]
fn a_moved_object_event_no_longer_blocks_its_authored_tile() {
    let runtime = moved_east_runtime();
    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: (2, 2),
            to: (2, 3),
        }
    );
}

#[test]
fn a_moved_object_event_blocks_its_live_tile() {
    let runtime = moved_east_runtime();
    let mut player = PlayerState::new((3, 2), 3, Direction::South);
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::ObjectEvent,
        }
    );
    assert_eq!(player.position(), (3, 2));
    assert!(!player.in_transit());
}

const HIDE: &str = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM";

/// Steps a player at (2, 2) south toward the `Rig`'s lone entry at (2, 3).
fn step_south_into(rig: &Rig) -> StepOutcome {
    let templates: &'static [ObjectEvent] =
        std::slice::from_ref(rig.collection.get(1).unwrap().template());
    let runtime = runtime_with_objects(templates).with_object_events(&rig.collection);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    player.step(
        Some(Direction::South),
        &runtime,
        &no_connections,
        &rig.event_data,
    )
}

#[test]
fn an_object_the_scheduler_never_admitted_does_not_block_after_its_flag_clears() {
    let mut rig = Rig::new(vec![template(1, 2, 3, MovementType::FaceUp, HIDE)], (2, 2));
    let flag = assets::object_event_flags::resolve(HIDE).unwrap();
    rig.event_data.flag_set(flag).unwrap();
    rig.tick(1);
    rig.event_data.flag_clear(flag).unwrap();
    rig.tick(10);
    assert!(!rig.collection.get(1).unwrap().is_active());
    assert_eq!(
        step_south_into(&rig),
        StepOutcome::Advanced {
            from: (2, 2),
            to: (2, 3),
        },
        "no spawned object stands there while the camera has not moved"
    );
}

#[test]
fn an_admitted_object_keeps_blocking_after_its_flag_is_set() {
    let mut rig = Rig::new(vec![template(1, 2, 3, MovementType::FaceUp, HIDE)], (2, 2));
    rig.tick(1);
    assert!(matches!(
        step_south_into(&rig),
        StepOutcome::Blocked {
            collision: super::super::collision::Collision::ObjectEvent,
            ..
        }
    ));
    rig.event_data
        .flag_set(assets::object_event_flags::resolve(HIDE).unwrap())
        .unwrap();
    rig.tick(10);
    assert!(matches!(
        step_south_into(&rig),
        StepOutcome::Blocked {
            collision: super::super::collision::Collision::ObjectEvent,
            ..
        }
    ));
}
