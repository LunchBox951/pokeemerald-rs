//! Map-connection crossing regressions.

use super::*;

#[test]
fn stepping_off_the_edge_with_a_connection_crosses_maps() {
    let runtime = south_connected_runtime();
    let maps = SingleConnectedMap {
        id: MapId("MAP_SOUTH"),
        dimensions: (5, 5),
        landing_position: (2, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 0,
        },
        landing_behavior: MB_NORMAL,
    };

    let mut player = PlayerState::new((2, 4), 3, Direction::South);
    let outcome = player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Crossed {
            to_map: MapId("MAP_SOUTH"),
            to_position: (2, 0),
        }
    );
    assert_eq!(player.position(), (2, 0));
    assert_eq!(player.elevation(), 0);
}

#[test]
fn connected_map_collision_bit_blocks_crossing() {
    let runtime = south_connected_runtime();
    let maps = SingleConnectedMap {
        id: MapId("MAP_SOUTH"),
        dimensions: (5, 5),
        landing_position: (2, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 1,
            elevation: 3,
        },
        landing_behavior: MB_NORMAL,
    };
    let mut player = PlayerState::new((2, 4), 3, Direction::South);

    assert_eq!(
        player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::Impassable,
        }
    );
    assert_eq!(player.position(), (2, 4));
    assert_eq!(player.elevation(), 3);
}

#[test]
fn connected_map_elevation_mismatch_blocks_crossing() {
    let runtime = south_connected_runtime();
    let maps = SingleConnectedMap {
        id: MapId("MAP_SOUTH"),
        dimensions: (5, 5),
        landing_position: (2, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 4,
        },
        landing_behavior: MB_NORMAL,
    };
    let mut player = PlayerState::new((2, 4), 3, Direction::South);

    assert_eq!(
        player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::ElevationMismatch,
        }
    );
    assert_eq!(player.position(), (2, 4));
    assert_eq!(player.elevation(), 3);
}

#[test]
fn connection_collision_bit_outranks_elevation_mismatch() {
    let runtime = south_connected_runtime();
    let maps = SingleConnectedMap {
        id: MapId("MAP_SOUTH"),
        dimensions: (5, 5),
        landing_position: (2, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 1,
            elevation: 4,
        },
        landing_behavior: MB_NORMAL,
    };
    let mut player = PlayerState::new((2, 4), 3, Direction::South);

    assert_eq!(
        player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::Impassable,
        },
        "collision bits must outrank elevation mismatch on a crossing \
         landing, the same order the same-map branch enforces"
    );
    assert_eq!(player.position(), (2, 4));
    assert_eq!(player.elevation(), 3);
}

#[test]
fn connection_crossing_does_not_check_local_object_events() {
    let (bytes, mut header, _) = flat_runtime(5, 5, |_, _| 0);
    header.connections = &[MapConnection {
        direction: assets::Direction::South,
        offset: 0,
        target: MapId("MAP_SOUTH"),
    }];
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 0, 3, "0")]));
    let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: objects,
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));
    let bytes = Box::leak(bytes.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let runtime = MapRuntime::new(
        MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(&[]),
        MetatileAttributeTable::new(&[]),
    );
    let maps = SingleConnectedMap {
        id: MapId("MAP_SOUTH"),
        dimensions: (5, 5),
        landing_position: (2, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
        landing_behavior: MB_NORMAL,
    };

    let mut player = PlayerState::new((2, 4), 3, Direction::South);
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
        StepOutcome::Crossed {
            to_map: MapId("MAP_SOUTH"),
            to_position: (2, 0),
        },
        "a connection crossing must never consult this map's own object events"
    );
}

#[test]
fn stepping_off_the_edge_without_a_connection_is_blocked() {
    let runtime = flat_map_runtime(5, 5, |_, _| 0);

    let mut player = PlayerState::new((2, 4), 3, Direction::South);
    let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::Impassable,
        }
    );
    assert_eq!(player.position(), (2, 4));
}

#[test]
fn a_crossing_whose_landing_tile_cannot_be_classified_is_refused() {
    let runtime = south_connected_runtime();
    let maps = UnclassifiedConnectedMap(SingleConnectedMap {
        id: MapId("MAP_SOUTH"),
        dimensions: (5, 5),
        landing_position: (2, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
        landing_behavior: MB_NORMAL,
    });

    let mut player = PlayerState::new((2, 4), 3, Direction::South);
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: Collision::Impassable,
        }
    );
    assert_eq!(player.position(), (2, 4));
}
