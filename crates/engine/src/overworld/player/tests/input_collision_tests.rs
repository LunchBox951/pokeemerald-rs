//! Input handling, same-map collision, and elevation-adoption regressions.

use super::*;

#[test]
fn a_scripted_face_turns_the_player_without_moving_or_disturbing_a_step() {
    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    player.face(Direction::North);
    assert_eq!(player.facing(), Direction::North);
    assert_eq!(player.position(), (2, 2));
    assert!(!player.in_transit());
    assert_eq!(player.step_direction(), None);

    let runtime = flat_map_runtime(5, 5, |_, _| 0);
    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    let progress = player.step_progress();
    assert_eq!(player.step_direction(), Some(Direction::South));
    player.face(Direction::West);
    assert_eq!(player.facing(), Direction::West);
    assert_eq!(player.position(), (2, 3), "the committed step is untouched");
    assert!(player.in_transit());
    assert_eq!(player.step_progress(), progress);
    assert_eq!(
        player.step_direction(),
        Some(Direction::South),
        "facing does not replace the committed step direction"
    );
}

#[test]
fn fresh_pressing_the_facing_direction_steps_immediately() {
    let runtime = flat_map_runtime(5, 5, |_, _| 0);

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Advanced {
            from: (2, 2),
            to: (2, 3),
        }
    );
    assert_eq!(player.position(), (2, 3));
    assert!(player.in_transit());
}

#[test]
fn pressing_a_new_direction_from_standstill_turns_without_stepping() {
    let runtime = flat_map_runtime(5, 5, |_, _| 0);

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(outcome, StepOutcome::Turned(Direction::East));
    assert_eq!(
        player.position(),
        (2, 2),
        "a turn must not move the tile position"
    );
    assert_eq!(player.facing(), Direction::East);
    assert!(!player.in_transit());
    player.tick();

    // The turn frame itself spends one of TURN_IN_PLACE_FRAMES, so the
    // held direction stays swallowed through frames 2..=8.
    assert_eq!(player.turn_frames_remaining(), TURN_IN_PLACE_FRAMES - 1);
    for frame in 2..=8 {
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Idle,
            "frame {frame} is still inside the turn's busy window"
        );
        assert_eq!(player.position(), (2, 2), "frame {frame} must not move");
        player.tick();
        assert_eq!(
            player.turn_frames_remaining(),
            TURN_IN_PLACE_FRAMES - frame,
            "frame {frame} of the window leaves the rest of it to run"
        );
    }

    let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Advanced {
            from: (2, 2),
            to: (3, 2),
        },
        "the step begins only once the turn's busy window has drained"
    );
}

#[test]
fn changing_direction_mid_movement_steps_immediately_without_a_turn_frame() {
    let runtime = flat_map_runtime(5, 5, |_, _| 0);

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert!(matches!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced { .. }
    ));
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    assert!(!player.in_transit());

    let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Advanced {
            from: (2, 3),
            to: (3, 3),
        }
    );
}

#[test]
fn release_during_transit_does_not_end_the_movement_streak() {
    let runtime = flat_map_runtime(5, 5, |_, _| 0);

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert!(matches!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced { .. }
    ));

    for _ in 0..WALK_FRAMES_PER_TILE {
        assert_eq!(
            player.step(None, &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Idle
        );
        player.tick();
    }

    let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Advanced {
            from: (2, 3),
            to: (3, 3),
        }
    );
}

#[test]
fn releasing_input_resets_to_not_moving_so_the_next_direction_turns_first() {
    let runtime = flat_map_runtime(5, 5, |_, _| 0);

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert!(matches!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced { .. }
    ));
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    assert_eq!(
        player.step(None, &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Idle
    );
    let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(outcome, StepOutcome::Turned(Direction::East));
}

#[test]
fn in_transit_step_calls_are_a_no_op() {
    let runtime = flat_map_runtime(5, 5, |_, _| 0);

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    assert!(matches!(
        player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced { .. }
    ));
    assert_eq!(
        player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Idle
    );
    assert_eq!(
        player.position(),
        (2, 3),
        "position must not change while busy"
    );
}

#[test]
fn collision_bit_blocks_the_step_and_leaves_position_unchanged() {
    let runtime = flat_map_runtime(5, 5, |_, y| u8::from(y == 3));

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::Impassable,
        }
    );
    assert_eq!(player.position(), (2, 2));
    assert!(
        !player.in_transit(),
        "a blocked step must not start a transition"
    );
}

#[test]
fn elevation_mismatch_blocks_the_step() {
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

    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(
        outcome,
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: super::super::collision::Collision::ElevationMismatch,
        }
    );
    assert_eq!(player.position(), (2, 2));
}
