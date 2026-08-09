//! Tests for map-edge connection crossing and [`super::OverworldPhase::cross_connection`].

use super::test_support::*;
use super::OverworldPhase;
use engine::overworld::{Direction, PlayerState, StepOutcome, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

// -- Map-edge connection crossing (issue #177): coordinate translation -----

/// The straight case: an `offset: 0` south connection.
#[test]
fn advance_player_one_frame_crosses_a_zero_offset_connection_unchanged() {
    let runtime = connected_runtime(
        5,
        5,
        assets::Direction::South,
        0,
        assets::MapId("MAP_SOUTH"),
    );
    let maps = SingleConnectedMap::new(
        assets::MapId("MAP_SOUTH"),
        (5, 5),
        (3, 0),
        assets::MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    );
    let mut player = PlayerState::new((3, 4), 3, Direction::South);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        &runtime,
        &maps,
        &NO_FLAGS,
    );
    assert_eq!(
        outcome,
        StepOutcome::Crossed {
            to_map: assets::MapId("MAP_SOUTH"),
            to_position: (3, 0),
        }
    );
    assert_eq!(player.position(), (3, 0));
    assert!(player.in_transit(), "a crossing is a step like any other");
}

/// The offset case: a nonzero `offset` on an east/west connection.
#[test]
fn advance_player_one_frame_crosses_an_offset_connection_shifting_the_cross_axis() {
    let runtime = connected_runtime(6, 6, assets::Direction::East, -2, assets::MapId("MAP_EAST"));
    let maps = SingleConnectedMap::new(
        assets::MapId("MAP_EAST"),
        (10, 10),
        (0, 7),
        assets::MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    );
    let mut player = PlayerState::new((5, 5), 3, Direction::East);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::East),
        &runtime,
        &maps,
        &NO_FLAGS,
    );
    assert_eq!(
        outcome,
        StepOutcome::Crossed {
            to_map: assets::MapId("MAP_EAST"),
            to_position: (0, 7),
        }
    );
    assert_eq!(player.position(), (0, 7));
}

/// The same offset rule on the north/south axis.
#[test]
fn advance_player_one_frame_crosses_an_offset_north_connection_shifting_x() {
    let runtime = connected_runtime(
        6,
        6,
        assets::Direction::North,
        -2,
        assets::MapId("MAP_NORTH"),
    );
    let maps = SingleConnectedMap::new(
        assets::MapId("MAP_NORTH"),
        (10, 10),
        (7, 9),
        assets::MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    );
    let mut player = PlayerState::new((5, 0), 3, Direction::North);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::North),
        &runtime,
        &maps,
        &NO_FLAGS,
    );
    assert_eq!(
        outcome,
        StepOutcome::Crossed {
            to_map: assets::MapId("MAP_NORTH"),
            to_position: (7, 9),
        }
    );
    assert_eq!(player.position(), (7, 9));
}

/// Different lateral positions along the same edge each translate to
/// their own distinct landing tile.
#[test]
fn advance_player_one_frame_crossing_at_different_lateral_positions_lands_at_the_matching_column() {
    let runtime = connected_runtime(
        8,
        8,
        assets::Direction::North,
        0,
        assets::MapId("MAP_NORTH"),
    );

    for x in [0_i32, 3, 7] {
        let maps = SingleConnectedMap::new(
            assets::MapId("MAP_NORTH"),
            (8, 8),
            (x, 7),
            assets::MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 3,
            },
        );
        let mut player = PlayerState::new((x, 0), 3, Direction::North);
        let outcome = advance_player_one_frame(
            &mut player,
            Some(Direction::North),
            &runtime,
            &maps,
            &NO_FLAGS,
        );
        assert_eq!(
            outcome,
            StepOutcome::Crossed {
                to_map: assets::MapId("MAP_NORTH"),
                to_position: (x, 7),
            },
            "column x={x} must land at the neighbour's matching column, not a fixed spot"
        );
    }
}

/// A candidate connection whose perpendicular bounds don't cover the
/// stepped-off position is rejected outright.
#[test]
fn advance_player_one_frame_rejects_a_crossing_outside_the_neighbours_bounds() {
    let runtime = connected_runtime(
        10,
        3,
        assets::Direction::South,
        0,
        assets::MapId("MAP_NARROW"),
    );
    let maps = SingleConnectedMap::new(
        assets::MapId("MAP_NARROW"),
        (3, 3),
        (8, 0),
        assets::MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    );
    let mut player = PlayerState::new((8, 2), 3, Direction::South);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        &runtime,
        &maps,
        &NO_FLAGS,
    );
    assert_eq!(
        outcome,
        StepOutcome::Blocked {
            direction: Direction::South,
            collision: engine::overworld::Collision::Impassable,
        },
        "x=8 is outside the 3-wide neighbour's bounds, so the crossing must fail closed"
    );
    assert_eq!(
        player.position(),
        (8, 2),
        "a rejected crossing must not move the player"
    );
}

/// The real-pack acceptance test for issue #177: Littleroot Town's actual
/// north connection to Route 101.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
#[allow(clippy::too_many_lines)]
fn walking_off_littlerootss_north_edge_crosses_into_route_101_and_back() {
    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let route101 = assets::MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(littleroot).expect("run `cargo xtask extract` first");

    let player = PlayerState::new((10, 2), 3, Direction::North);
    let mut phase = OverworldPhase::for_test(scene, littleroot, player, None);

    for _ in 0..2 {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
        assert_eq!(phase.map_id, littleroot, "still on Littleroot's own grid");
    }
    assert_eq!(phase.player.position(), (10, 0));

    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.tick, 33,
        "a crossing is NOT a map load: upstream's LoadMapFromCameraTransition re-inits only \
         the secondary tileset counter (InitSecondaryTilesetAnimation, overworld.c:815), so \
         the primary counter this port models keeps running -- 32 walk frames plus the \
         crossing step's own increment, with no reset (contrast the warp test above)"
    );
    assert_eq!(
        phase.pending_landing,
        Some((10, 19)),
        "the crossing step's landing tile re-latches in the *entered* map's coordinate \
         space, so the drain-frame door check evaluates against Route 101"
    );
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.pending_landing, None,
        "the drain frame consumed the latched landing (Route 101's south edge is no door)"
    );

    assert_eq!(
        phase.map_id, route101,
        "walking off Littleroot's north edge must rebind to Route 101"
    );
    assert_eq!(
        phase.player.position(),
        (10, 19),
        "offset 0 carries x straight across, landing on Route 101's own south edge \
         (height 20, so y = 19)"
    );
    assert_eq!(
        phase.player.elevation(),
        3,
        "the landing cell's own elevation, adopted the same way an ordinary step's is"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a plain connection crossing does not alter facing -- the player was already \
         walking north when it fired"
    );
    assert!(
        !phase.player.in_transit(),
        "the walk animation the crossing step started has already fully drained by now"
    );

    let route101_header = assets::MapHeaderTable::new()
        .header(route101)
        .expect("Route 101 must resolve in the generated map-header table");
    assert_eq!(
        phase.save1().location.map_group,
        i8::try_from(route101_header.group).unwrap()
    );
    assert_eq!(
        phase.save1().location.map_num,
        i8::try_from(route101_header.num).unwrap()
    );
    assert_eq!(
        phase.save1().location.warp_id,
        -1,
        "a connection crossing names no warp event -- WARP_ID_NONE (overworld.c:633)"
    );
    assert_eq!(
        (phase.save1().location.x, phase.save1().location.y),
        (-1, -1),
        "LoadMapFromCameraTransition's own SetWarpDestination call shape (overworld.c:786)"
    );
    assert_eq!(
        (
            i32::from(phase.save1().pos.x),
            i32::from(phase.save1().pos.y)
        ),
        (10, 19),
        "save1.pos must mirror the post-crossing tile"
    );

    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }

    assert_eq!(
        phase.map_id, littleroot,
        "walking south off Route 101's own south edge must cross back into Littleroot"
    );
    assert_eq!(
        phase.player.position(),
        (10, 0),
        "landing back on Littleroot's own north edge, same x"
    );
    assert_eq!(phase.player.elevation(), 3);

    let littleroot_header = assets::MapHeaderTable::new()
        .header(littleroot)
        .expect("Littleroot Town must resolve in the generated map-header table");
    assert_eq!(
        phase.save1().location.map_group,
        i8::try_from(littleroot_header.group).unwrap()
    );
    assert_eq!(
        phase.save1().location.map_num,
        i8::try_from(littleroot_header.num).unwrap()
    );
    assert_eq!(phase.save1().location.warp_id, -1);
}

/// The claim at upstream's `LoadMapFromCameraTransition` call site: a
/// connection crossing restarts the wild encounter immunity window.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn crossing_a_map_connection_restarts_the_wild_encounter_immunity_window() {
    use engine::overworld::WILD_ENCOUNTER_IMMUNITY_STEPS;
    use engine::rng::Rng;

    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let route101 = assets::MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(littleroot).expect("run `cargo xtask extract` first");
    let mut phase = OverworldPhase::for_test(
        scene,
        littleroot,
        PlayerState::new((10, 4), 3, Direction::North),
        None,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);

    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    }
    assert_eq!(phase.player.position(), (10, 0));
    assert_eq!(phase.wild.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);
    assert_eq!(
        phase.rng.state(),
        Rng::new(IMMUNITY_SEED).state(),
        "a map with no wild table draws nothing, window or no window"
    );

    phase.step(held(Buttons::UP));
    assert_eq!(phase.map_id, route101, "the crossing must have landed");
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "a connection crossing calls RestartWildEncounterImmunitySteps too \
         (LoadMapFromCameraTransition, overworld.c:800)"
    );
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.wild.immunity_steps(),
        1,
        "the crossing step's own landing then spends the first of the four it was just \
         granted -- upstream reaches CheckStandardWildEncounter for that step too, the \
         crossing having happened inside it"
    );

    phase.player = PlayerState::new(
        ROUTE_101_GRASS_ROW[0],
        ROUTE_101_GRASS_ELEVATION,
        Direction::East,
    );
    phase.pending_landing = None;
    let remaining = usize::from(WILD_ENCOUNTER_IMMUNITY_STEPS - phase.wild.immunity_steps());
    for (index, tile) in ROUTE_101_GRASS_ROW
        .iter()
        .copied()
        .enumerate()
        .take(remaining + 1)
        .skip(1)
    {
        walk_one_tile_east(&mut phase);
        assert_eq!(phase.player.position(), tile);
        assert_eq!(
            phase.rng.state(),
            Rng::new(IMMUNITY_SEED).state(),
            "grass step {index} is inside the window the crossing restarted, so it must not \
             draw at all"
        );
        assert!(phase.wild_battle.is_none());
    }
    assert_eq!(phase.wild.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);

    walk_one_tile_east(&mut phase);
    assert_ne!(
        phase.rng.state(),
        Rng::new(IMMUNITY_SEED).state(),
        "the step past the window's end rolls for real"
    );
}
