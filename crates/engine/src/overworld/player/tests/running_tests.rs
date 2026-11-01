//! Held-B running: its gate, its eight-frame cadence, and the interactions it
//! must leave alone. Split from `tests` like `forced_movement_tests`
//! `(oop-boundaries)`.

use super::*;

const FLAG_SYS_B_DASH: u16 = 0x8C0;

fn shoes() -> EventData {
    let mut data = EventData::new();
    data.flag_set(FLAG_SYS_B_DASH).unwrap();
    data
}

/// A 9x9 runtime whose every cell (metatile 1, elevation 3) has `behavior`,
/// on a map whose header allows running only when `allow_run`.
fn runtime_with(behavior: u8, allow_run: bool, elevation: u8) -> MapRuntime<'static> {
    let (_, mut header, events) = flat_runtime(1, 1, |_, _| 0);
    header.allow_run = allow_run;
    let mut bytes = Vec::new();
    for _ in 0..81 {
        let raw = MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation,
        }
        .pack();
        bytes.extend_from_slice(&raw.to_le_bytes());
    }
    let attrs = [0u16.to_le_bytes(), u16::from(behavior).to_le_bytes()].concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 9,
        height: 9,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    MapRuntime::new(
        MapId("MAP_TEST"),
        Box::leak(Box::new(header)),
        Box::leak(Box::new(events)),
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    )
}

fn open() -> MapRuntime<'static> {
    runtime_with(MB_NORMAL, true, 3)
}

fn step(
    player: &mut PlayerState,
    direction: Option<Direction>,
    run: bool,
    runtime: &MapRuntime<'_>,
    data: &EventData,
) -> StepOutcome {
    player.step_with_run(direction, run, runtime, &no_connections, data)
}

fn duration_after_step(runtime: &MapRuntime<'_>, run: bool, data: &EventData) -> u8 {
    duration_after_step_at(runtime, 3, run, data)
}

fn duration_after_step_at(
    runtime: &MapRuntime<'_>,
    elevation: u8,
    run: bool,
    data: &EventData,
) -> u8 {
    let mut player = PlayerState::new((4, 4), elevation, Direction::South);
    let outcome = step(&mut player, Some(Direction::South), run, runtime, data);
    assert!(matches!(outcome, StepOutcome::Advanced { .. }));
    player.transit_duration()
}

#[test]
fn held_b_with_the_shoes_crosses_every_direction_in_eight_frames() {
    let runtime = open();
    let data = shoes();
    for direction in [
        Direction::South,
        Direction::North,
        Direction::West,
        Direction::East,
    ] {
        let mut player = PlayerState::new((4, 4), 3, direction);
        let (dx, dy) = direction.delta();
        assert_eq!(
            step(&mut player, Some(direction), true, &runtime, &data),
            StepOutcome::Advanced {
                from: (4, 4),
                to: (4 + dx, 4 + dy),
            }
        );
        assert_eq!(player.facing(), direction);
        assert_eq!(player.transit_duration(), RUN_FRAMES_PER_TILE);
        assert!(!player.transit_animation_disabled());
        for _ in 0..RUN_FRAMES_PER_TILE - 1 {
            player.tick();
            assert!(player.in_transit());
        }
        player.tick();
        assert!(!player.in_transit());
    }
}

#[test]
fn every_unmet_gate_walks() {
    let data = shoes();
    let walk = WALK_FRAMES_PER_TILE;
    assert_eq!(
        duration_after_step(&open(), false, &data),
        walk,
        "B released"
    );
    assert_eq!(
        duration_after_step(&open(), true, &EventData::new()),
        walk,
        "shoes not received"
    );
    assert_eq!(
        duration_after_step(&runtime_with(MB_NORMAL, false, 3), true, &data),
        walk,
        "header forbids running"
    );
}

#[test]
fn departure_behaviors_that_forbid_running_walk() {
    let data = shoes();
    for behavior in [0x0A, 0x03, 0x28, 0x74, 0x75, 0x76, 0x77] {
        assert_eq!(
            duration_after_step(&runtime_with(behavior, true, 3), true, &data),
            WALK_FRAMES_PER_TILE,
            "behavior {behavior:#x}"
        );
    }
    for behavior in [0x02, 0x09, 0x00] {
        assert_eq!(
            duration_after_step(&runtime_with(behavior, true, 3), true, &data),
            RUN_FRAMES_PER_TILE,
            "behavior {behavior:#x} permits running"
        );
    }
}

#[test]
fn the_fortree_bridge_forbids_running_only_at_even_elevation() {
    let data = shoes();
    assert_eq!(
        duration_after_step_at(&runtime_with(0x78, true, 4), 4, true, &data),
        WALK_FRAMES_PER_TILE
    );
    assert_eq!(
        duration_after_step(&runtime_with(0x78, true, 3), true, &data),
        RUN_FRAMES_PER_TILE
    );
}

#[test]
fn the_pace_is_fixed_when_the_crossing_starts() {
    let runtime = open();
    let data = shoes();
    let mut player = PlayerState::new((4, 4), 3, Direction::South);
    step(&mut player, Some(Direction::South), true, &runtime, &data);
    player.tick();
    // B released mid-crossing: ignored while busy.
    assert_eq!(
        step(&mut player, Some(Direction::South), false, &runtime, &data),
        StepOutcome::Idle
    );
    assert_eq!(player.transit_duration(), RUN_FRAMES_PER_TILE);
    for _ in 1..RUN_FRAMES_PER_TILE {
        player.tick();
    }
    // The next eligible poll reads B again and walks.
    assert!(matches!(
        step(&mut player, Some(Direction::South), false, &runtime, &data),
        StepOutcome::Advanced { .. }
    ));
    assert_eq!(player.transit_duration(), WALK_FRAMES_PER_TILE);
}

#[test]
fn b_alone_and_a_standstill_turn_do_not_run() {
    let runtime = open();
    let data = shoes();
    let mut player = PlayerState::new((4, 4), 3, Direction::South);
    assert_eq!(
        step(&mut player, None, true, &runtime, &data),
        StepOutcome::Idle
    );
    assert!(!player.in_transit());
    assert_eq!(
        step(&mut player, Some(Direction::East), true, &runtime, &data),
        StepOutcome::Turned(Direction::East)
    );
    assert!(!player.in_transit());
    assert_eq!(player.turn_frames_remaining(), TURN_IN_PLACE_FRAMES);
}

#[test]
fn a_blocked_run_keeps_the_blocked_outcome_and_facing() {
    let (bytes, header, events) = flat_runtime(9, 9, |_, y| u8::from(y == 5));
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 9,
        height: 9,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let bytes = Box::leak(bytes.into_boxed_slice());
    let runtime = MapRuntime::new(
        MapId("MAP_TEST"),
        Box::leak(Box::new(header)),
        Box::leak(Box::new(events)),
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(&[]),
        MetatileAttributeTable::new(&[]),
    );
    let mut player = PlayerState::new((4, 4), 3, Direction::South);
    assert_eq!(
        step(
            &mut player,
            Some(Direction::South),
            true,
            &runtime,
            &shoes()
        ),
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: Collision::Impassable,
        }
    );
    assert!(!player.in_transit());
    assert_eq!(player.position(), (4, 4));
}

#[test]
fn the_permission_is_read_from_saved_bytes_at_each_step() {
    let runtime = open();
    let data = shoes();
    let restored = EventData::from_saved_state(*data.flag_bytes(), *data.vars_raw());
    assert_eq!(
        duration_after_step(&runtime, true, &restored),
        RUN_FRAMES_PER_TILE
    );
    // A fresh player on a different map header re-reads the gate.
    assert_eq!(
        duration_after_step(&runtime_with(MB_NORMAL, false, 3), true, &restored),
        WALK_FRAMES_PER_TILE
    );
}
