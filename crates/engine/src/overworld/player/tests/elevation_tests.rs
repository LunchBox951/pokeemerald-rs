//! Directional impassability and elevation-transition regressions.

use super::*;

#[test]
fn the_beds_pillow_tile_cannot_be_left_northward() {
    let runtime = bed_pillow_runtime(3);
    let mut player = PlayerState::new((1, 4), 3, Direction::North);

    let outcome = player.step(Some(Direction::North), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Blocked {
            direction: Direction::North,
            collision: super::super::collision::Collision::Impassable,
        }
    );
    assert_eq!(player.position(), (1, 4));
    assert!(
        !player.in_transit(),
        "a blocked step must not start a transition"
    );
}

#[test]
fn the_beds_pillow_tile_cannot_be_entered_from_the_north() {
    let runtime = bed_pillow_runtime(3);
    let mut player = PlayerState::new((1, 3), 3, Direction::South);

    let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::Impassable,
        }
    );
    assert_eq!(player.position(), (1, 3));
}

#[test]
fn the_beds_pillow_tile_stays_crossable_east_to_west() {
    let runtime = bed_pillow_runtime(3);

    let mut player = PlayerState::new((0, 4), 3, Direction::East);
    assert_eq!(
        player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: (0, 4),
            to: (1, 4),
        }
    );

    let mut player = PlayerState::new((1, 4), 3, Direction::East);
    assert_eq!(
        player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: (1, 4),
            to: (2, 4),
        }
    );
}

#[test]
fn directional_impassability_outranks_the_elevation_mismatch() {
    let runtime = bed_pillow_runtime(7);
    let mut player = PlayerState::new((1, 3), 3, Direction::South);

    assert_eq!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::Impassable,
        }
    );
}

#[test]
fn transition_tile_lets_the_next_step_cross_between_elevations() {
    let width = 5u16;
    let height = 5u16;
    let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
    for y in 0..height {
        for x in 0..width {
            let elevation = match (x, y) {
                (2, 2) => 0,
                (2, 3) => 5,
                _ => 3,
            };
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
        connections: &[] as &'static [MapConnection],
    };
    let events = MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: &[],
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

    let mut player = PlayerState::new((2, 1), 3, Direction::South);
    assert_eq!(
        player.previous_elevation(),
        3,
        "the constructor must align collision and render elevations"
    );
    let onto_transition = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert!(matches!(onto_transition, StepOutcome::Advanced { .. }));
    assert_eq!(player.elevation(), 0);
    assert_eq!(
        player.previous_elevation(),
        3,
        "previous_elevation must retain the last non-transition elevation \
         while standing on the transition wildcard"
    );
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    let onto_upper = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert!(matches!(onto_upper, StepOutcome::Advanced { .. }));
    assert_eq!(player.position(), (2, 3));
    assert_eq!(player.elevation(), 5);
    assert_eq!(
        player.previous_elevation(),
        5,
        "landing on a real (non-transition) elevation updates \
         previous_elevation too"
    );
}

#[test]
fn previous_elevation_survives_a_raised_tile_flanked_by_transitions_on_both_sides() {
    fn step_south(player: &mut PlayerState, runtime: &MapRuntime<'_>) {
        let outcome = player.step(Some(Direction::South), runtime, &no_connections, &NO_FLAGS);
        assert!(
            matches!(outcome, StepOutcome::Advanced { .. }),
            "every step down this column is collision-legal: {outcome:?}"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
    }

    let width = 5u16;
    let height = 6u16;
    let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
    for y in 0..height {
        for x in 0..width {
            let elevation = match (x, y) {
                (2, 2 | 4) => 0,
                (2, 3) => 4,
                _ => 3,
            };
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
        connections: &[] as &'static [MapConnection],
    };
    let events = MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: &[],
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

    let mut player = PlayerState::new((2, 1), 3, Direction::South);

    step_south(&mut player, &runtime);
    assert_eq!((player.elevation(), player.previous_elevation()), (0, 3));

    step_south(&mut player, &runtime);
    assert_eq!((player.elevation(), player.previous_elevation()), (4, 4));

    step_south(&mut player, &runtime);
    assert_eq!(
        (player.elevation(), player.previous_elevation()),
        (0, 4),
        "previous_elevation must still read the raised tile's 4, not \
         reset by standing on the wildcard"
    );

    step_south(&mut player, &runtime);
    assert_eq!((player.elevation(), player.previous_elevation()), (3, 3));
}

#[test]
fn a_multi_level_origin_defers_the_elevation_update_to_the_finished_step() {
    let width = 5u16;
    let height = 5u16;
    let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
    for y in 0..height {
        for x in 0..width {
            let elevation = if (x, y) == (2, 2) {
                super::super::collision::ELEVATION_MULTI_LEVEL
            } else {
                7
            };
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
        connections: &[] as &'static [MapConnection],
    };
    let events = MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: &[],
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

    let mut player = PlayerState::new((2, 2), 0, Direction::South);
    let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert!(
        matches!(outcome, StepOutcome::Advanced { .. }),
        "a multi-level tile is never itself a mismatch source: {outcome:?}"
    );
    assert_eq!(
        (player.elevation(), player.previous_elevation()),
        (0, 0),
        "the begin-step pass skips the whole adoption while the origin is \
         ELEVATION_MULTI_LEVEL, even though the destination (7) is ordinary"
    );
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    assert!(!player.in_transit());
    assert_eq!(
        (player.elevation(), player.previous_elevation()),
        (7, 7),
        "the finish-step pass runs over the landing cell alone, so both \
         fields adopt the ordinary destination once the crossing ends"
    );
}

/// A multi-level cell crossed between a transition cell and an ordinary
/// one leaves the settled pair on the landing cell's elevation, the pair
/// a save then holds and a continue accepts on an ordinary cell.
#[test]
fn settled_elevation_follows_the_landing_cell_after_crossing_multi_level() {
    // Column x=2: y=1 elev 3, y=2 elev 0, y=3 elev 15, y=4 elev 7.
    let (_, header, events) = flat_runtime(5, 6, |_, _| 0);
    let mut bytes = Vec::new();
    for y in 0..6u16 {
        for x in 0..5u16 {
            let elevation = match (x, y) {
                (2, 2) => 0,
                (2, 3) => super::super::collision::ELEVATION_MULTI_LEVEL,
                (2, 4) => 7,
                _ => 3,
            };
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
        width: 5,
        height: 6,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let grid = layout.grid(&bytes).unwrap();
    let runtime = MapRuntime::new(
        assets::MapId("MAP_TEST"),
        &header,
        &events,
        grid,
        MetatileAttributeTable::new(&[]),
        MetatileAttributeTable::new(&[]),
    );
    let mut player = PlayerState::new((2, 1), 3, Direction::South);
    let mut settled = Vec::new();
    for _ in 0..3 {
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced { .. }
        ));
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        settled.push((player.elevation(), player.previous_elevation()));
    }
    assert_eq!(player.position(), (2, 4));
    assert!(!player.in_transit());
    assert_eq!(
        settled,
        vec![(0, 3), (0, 3), (7, 7)],
        "transition keeps the retained 3, multi-level retains both, and the \
         ordinary landing settles both halves on 7"
    );
}
