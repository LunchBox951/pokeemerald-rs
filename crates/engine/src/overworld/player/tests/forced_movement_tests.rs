//! Table-driven regressions for every `MB_WALK_*`/`MB_SLIDE_*` forced
//! dispatch: no-input first dispatch, cadence, held-opposite input,
//! blocked-route fallback, and chaining. Split from `tests` for the same
//! reason `crate::flow::save_continue_tests` split from `tests`: one
//! module, one concept `(oop-boundaries)`.

use super::*;

/// The eight standing tiles [`supported_forced_mover`] dispatches, paired
/// with the fixed direction and per-tile frame count their own
/// `ForcedMovement_*` mover uses (`field_player_avatar.c:486-552`,
/// `event_object_movement.c:8233-8298`).
const FORCED_MOVERS: [(u8, Direction, u8); 8] = [
    (MB_WALK_EAST, Direction::East, WALK_FRAMES_PER_TILE),
    (MB_WALK_WEST, Direction::West, WALK_FRAMES_PER_TILE),
    (MB_WALK_NORTH, Direction::North, WALK_FRAMES_PER_TILE),
    (MB_WALK_SOUTH, Direction::South, WALK_FRAMES_PER_TILE),
    (MB_SLIDE_EAST, Direction::East, SLIDE_FRAMES_PER_TILE),
    (MB_SLIDE_WEST, Direction::West, SLIDE_FRAMES_PER_TILE),
    (MB_SLIDE_NORTH, Direction::North, SLIDE_FRAMES_PER_TILE),
    (MB_SLIDE_SOUTH, Direction::South, SLIDE_FRAMES_PER_TILE),
];

/// Returns the cardinal opposite of `direction`, for polling a forced
/// tile's own direction against a caller input guaranteed to differ.
const fn opposite(direction: Direction) -> Direction {
    match direction {
        Direction::East => Direction::West,
        Direction::West => Direction::East,
        Direction::North => Direction::South,
        Direction::South => Direction::North,
    }
}

/// A 5x5 runtime with `behavior` at its center `(2, 2)` and every other
/// tile plain ground, optionally with a collision bit set on the tile
/// `direction` of center -- the tile `behavior`'s own mover would
/// dispatch onto.
fn forced_mover_runtime(
    behavior: u8,
    direction: Direction,
    block_destination: bool,
) -> MapRuntime<'static> {
    let (dx, dy) = direction.delta();
    let destination: (i32, i32) = (2 + dx, 2 + dy);
    let mut bytes = Vec::new();
    for y in 0..5u16 {
        for x in 0..5u16 {
            let is_center = (x, y) == (2, 2);
            let is_destination = (i32::from(x), i32::from(y)) == destination;
            let raw = MetatileCell {
                metatile_id: u16::from(is_center),
                collision: u8::from(block_destination && is_destination),
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(behavior).to_le_bytes(),
    ]
    .concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    )
}

/// Manually enters the forced tile at `runtime`'s center from the side
/// opposite `direction`, ticks the entry step's crossing to completion,
/// and returns the player standing on the tile with the guard armed --
/// the shared setup every `FORCED_MOVERS` case starts from.
fn enter_forced_tile(runtime: &MapRuntime<'_>, direction: Direction) -> PlayerState {
    let (dx, dy) = direction.delta();
    let entry: TilePos = (2 - dx, 2 - dy);
    let mut player = PlayerState::new(entry, 3, direction);
    assert_eq!(
        player.step(Some(direction), runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: entry,
            to: (2, 2),
        },
        "fixture precondition: the forced tile is entered like ordinary ground"
    );
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    assert!(
        player.forced_movement_armed(),
        "fixture precondition: entering the tile arms the guard"
    );
    player
}

/// A no-input poll dispatches every supported standing forced tile in
/// its own fixed direction, going through the same `Advanced` outcome
/// an ordinary step uses (`field_player_avatar.c:332-349, 407-470`)
/// `(behavioral-fidelity)`.
#[test]
fn supported_forced_tiles_dispatch_on_no_input_in_the_tiles_own_direction() {
    for &(behavior, direction, _frames) in &FORCED_MOVERS {
        let runtime = forced_mover_runtime(behavior, direction, false);
        let mut player = enter_forced_tile(&runtime, direction);
        let (dx, dy) = direction.delta();
        let destination = (2 + dx, 2 + dy);

        assert_eq!(
            player.step(None, &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (2, 2),
                to: destination,
            },
            "behavior {behavior:#04x}: a no-input poll must dispatch the \
             standing tile's own forced direction"
        );
        assert_eq!(
            player.facing(),
            direction,
            "behavior {behavior:#04x}: the dispatched direction sets facing"
        );
        assert_eq!(
            player.step_direction(),
            Some(direction),
            "behavior {behavior:#04x}: the active crossing is the forced direction"
        );
    }
}

/// Each dispatched crossing drains in exactly its own family's frame
/// count: 16 for a walk tile, 8 for a slide tile
/// (`event_object_movement.c:8233-8298`) `(behavioral-fidelity)`.
#[test]
fn supported_forced_tiles_use_their_own_exact_crossing_cadence() {
    for &(behavior, direction, frames) in &FORCED_MOVERS {
        let runtime = forced_mover_runtime(behavior, direction, false);
        let mut player = enter_forced_tile(&runtime, direction);
        let (dx, dy) = direction.delta();

        assert_eq!(
            player.step(None, &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (2, 2),
                to: (2 + dx, 2 + dy),
            },
            "behavior {behavior:#04x}: fixture precondition, dispatch begins \
             the crossing"
        );

        for frame in 1..frames {
            player.tick();
            assert!(
                player.in_transit(),
                "behavior {behavior:#04x}: frame {frame} of {frames} must \
                 still be mid-crossing"
            );
        }
        player.tick();
        assert!(
            !player.in_transit(),
            "behavior {behavior:#04x}: the crossing must drain in exactly \
             {frames} frames"
        );
    }
}

/// A held direction opposite the tile's own never steers the crossing:
/// `DoForcedMovement` moves in the mover's own direction independent of
/// the keypad (`field_player_avatar.c:407-470`) `(behavioral-fidelity)`.
#[test]
fn supported_forced_tiles_ignore_a_held_opposing_direction() {
    for &(behavior, direction, _frames) in &FORCED_MOVERS {
        let runtime = forced_mover_runtime(behavior, direction, false);
        let mut player = enter_forced_tile(&runtime, direction);
        let (dx, dy) = direction.delta();
        let destination = (2 + dx, 2 + dy);

        assert_eq!(
            player.step(
                Some(opposite(direction)),
                &runtime,
                &no_connections,
                &NO_FLAGS
            ),
            StepOutcome::Advanced {
                from: (2, 2),
                to: destination,
            },
            "behavior {behavior:#04x}: a held opposing direction must not \
             steer away from the tile's own forced direction"
        );
        assert_eq!(
            player.facing(),
            direction,
            "behavior {behavior:#04x}: facing follows the forced direction, \
             not the caller's held opposite"
        );
    }
}

/// A blocked forced route falls through to a held keypad direction on
/// the very poll that found it blocked, immediately stepping rather
/// than turning first, since the movement streak the entry step started
/// never ended (`field_player_avatar.c:344-349, 443-462`)
/// `(behavioral-fidelity)`.
#[test]
fn supported_forced_tiles_blocked_route_falls_through_to_held_keypad() {
    for &(behavior, direction, _frames) in &FORCED_MOVERS {
        let runtime = forced_mover_runtime(behavior, direction, true);
        let mut player = enter_forced_tile(&runtime, direction);
        let (dx, dy) = direction.delta();
        let entry: TilePos = (2 - dx, 2 - dy);

        assert_eq!(
            player.step(
                Some(opposite(direction)),
                &runtime,
                &no_connections,
                &NO_FLAGS
            ),
            StepOutcome::Advanced {
                from: (2, 2),
                to: entry,
            },
            "behavior {behavior:#04x}: the forced route is blocked, so the \
             held opposite direction must move the player back off the tile \
             immediately rather than turning first"
        );
        assert!(
            !player.forced_movement_armed(),
            "behavior {behavior:#04x}: the guard must clear once the forced \
             route is found blocked"
        );
    }
}

/// A blocked forced route with no input idles, but keeps the guard armed
/// while the player still stands on the tile -- only leaving it or a
/// successful dispatch consumes the guard, so a later poll retries the
/// dispatch too (`field_player_avatar.c:344-349, 409-470`). The retried
/// dispatch is still blocked and, like `DoForcedMovement`'s collision
/// branch, never touches the movement streak the idle poll already ended
/// (`:443-462`), so that later poll turns in place first rather than
/// stepping immediately `(behavioral-fidelity)`.
#[test]
fn blocked_forced_route_after_idle_turns_before_stepping() {
    for &(behavior, direction, _frames) in &FORCED_MOVERS {
        let runtime = forced_mover_runtime(behavior, direction, true);
        let mut player = enter_forced_tile(&runtime, direction);

        assert_eq!(
            player.step(None, &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Idle,
            "behavior {behavior:#04x}: the forced route is blocked, so a \
             no-input poll must idle rather than move"
        );
        assert!(
            player.forced_movement_armed(),
            "behavior {behavior:#04x}: the guard must stay armed while the \
             player still stands on the tile, so a later poll can retry \
             once whatever blocked it no longer does"
        );

        assert_eq!(
            player.step(
                Some(opposite(direction)),
                &runtime,
                &no_connections,
                &NO_FLAGS
            ),
            StepOutcome::Turned(opposite(direction)),
            "behavior {behavior:#04x}: the retried dispatch is still \
             blocked and never touched the movement streak the idle poll \
             ended, so this poll turns in place like any other direction \
             change from standstill"
        );
    }
}

/// Two consecutive tiles of `behavior` at `(2, 2)` and `(3, 2)`, open
/// ground otherwise, so a dispatched eastward crossing chains directly
/// into the next tile's own dispatch without further input.
fn chained_forced_mover_runtime(behavior: u8) -> MapRuntime<'static> {
    let mut bytes = Vec::new();
    for y in 0..5u16 {
        for x in 0..5u16 {
            let is_forced = (x, y) == (2, 2) || (x, y) == (3, 2);
            let raw = MetatileCell {
                metatile_id: u16::from(is_forced),
                collision: 0,
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(behavior).to_le_bytes(),
    ]
    .concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    )
}

/// Consecutive supported forced landings chain: the second tile
/// re-arms the guard and dispatches its own crossing with no
/// intervening input, matching upstream's per-frame dispatch running
/// again on every subsequent standing tile
/// (`field_player_avatar.c:332-349`) `(behavioral-fidelity)`.
fn assert_forced_tiles_chain_without_input(behavior: u8, frames: u8) {
    let runtime = chained_forced_mover_runtime(behavior);
    let mut player = enter_forced_tile(&runtime, Direction::East);

    assert_eq!(
        player.step(None, &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: (2, 2),
            to: (3, 2),
        },
        "the first forced dispatch must land on the next forced tile"
    );
    assert!(
        player.forced_movement_armed(),
        "the second tile re-arms the guard"
    );
    for _ in 0..frames {
        player.tick();
    }
    assert_eq!(
        player.step(None, &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: (3, 2),
            to: (4, 2),
        },
        "the second forced tile must dispatch again without any input, \
         chaining the crossing onward"
    );
}

#[test]
fn walk_tiles_chain_across_consecutive_landings() {
    assert_forced_tiles_chain_without_input(MB_WALK_EAST, WALK_FRAMES_PER_TILE);
}

#[test]
fn slide_tiles_chain_across_consecutive_landings() {
    assert_forced_tiles_chain_without_input(MB_SLIDE_EAST, SLIDE_FRAMES_PER_TILE);
}

/// A slide-east tile whose eastward destination is blocked only by a
/// visible object event (rather than a wall), so it can become passable
/// again once that object hides.
fn slide_east_runtime_with_blocking_object(flag: &'static str) -> MapRuntime<'static> {
    let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 3, 2, 3, flag)]));
    let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: objects,
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));
    let mut bytes = Vec::new();
    for y in 0..5u16 {
        for x in 0..5u16 {
            let raw = MetatileCell {
                metatile_id: u16::from((x, y) == (2, 2)),
                collision: 0,
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(MB_SLIDE_EAST).to_le_bytes(),
    ]
    .concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let (_, header, _) = flat_runtime(1, 1, |_, _| 0);
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    )
}

/// A forced route blocked only by an object event retries on later polls:
/// `GetForcedMovementByMetatileBehavior` re-reads the standing tile's
/// behavior fresh every poll rather than latching a one-time verdict
/// (`field_player_avatar.c:409-470`), so the guard must stay armed while
/// the player still stands on the tile -- only leaving it or a successful
/// dispatch consumes it. Once the blocking object is hidden, the next
/// no-input poll dispatches `(behavioral-fidelity)`.
#[test]
fn blocked_forced_route_retries_once_the_blocking_object_disappears() {
    const HIDE: &str = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM";
    let runtime = slide_east_runtime_with_blocking_object(HIDE);
    let mut player = enter_forced_tile(&runtime, Direction::East);
    let mut data = EventData::new();

    assert_eq!(
        player.step(None, &runtime, &no_connections, &data),
        StepOutcome::Idle,
        "the visible object blocks the forced route"
    );
    assert!(
        player.forced_movement_armed(),
        "the guard must stay armed while the player still stands on the tile"
    );

    data.flag_set(assets::object_event_flags::resolve(HIDE).unwrap())
        .unwrap();
    assert_eq!(
        player.step(None, &runtime, &no_connections, &data),
        StepOutcome::Advanced {
            from: (2, 2),
            to: (3, 2),
        },
        "once the blocking object is hidden, the still-standing forced tile \
         must retry its dispatch on the next no-input poll"
    );
}
