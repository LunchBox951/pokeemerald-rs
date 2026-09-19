//! Tests for map-edge connection crossing and
//! [`super::OverworldPhase::cross_connection`].

use super::test_support::*;
use super::OverworldPhase;
use assets::{MapId, MetatileCell};
use engine::overworld::{
    Direction, PlayerState, StepOutcome, WALK_FRAMES_PER_TILE, WILD_ENCOUNTER_IMMUNITY_STEPS,
};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

/// The straight case: an `offset: 0` south connection (Route 101's own
/// south connection to Littleroot Town, per `data/maps/Route101/map.json`,
/// is exactly this shape) carries the stepped-off x coordinate straight
/// across unchanged, landing on the neighbour's opposite (north) edge.
#[test]
fn advance_player_one_frame_crosses_a_zero_offset_connection_unchanged() {
    let runtime = connected_runtime(5, 5, assets::Direction::South, 0, MapId("MAP_SOUTH"));
    let maps = SingleConnectedMap {
        id: MapId("MAP_SOUTH"),
        dimensions: (5, 5),
        landing_position: (3, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    };
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
            to_map: MapId("MAP_SOUTH"),
            to_position: (3, 0),
        }
    );
    assert_eq!(player.position(), (3, 0));
    assert!(player.in_transit(), "a crossing is a step like any other");
}

/// The offset case: a nonzero `offset` on an east/west connection shifts
/// the *cross axis* (`y`, for an east/west edge) coordinate by
/// `-offset` when carrying it into the neighbour's own space -- upstream
/// `IsPosInConnectingMap`/`SetPositionFromConnection`
/// (`pokeemerald/src/fieldmap.c:578-598, 699-712`), already exercised at the
/// engine layer (`engine::overworld::map_runtime::tests::
/// resolve_connection_applies_perpendicular_offset`) -- pinned again here at
/// the actual integration entry point [`OverworldPhase`] wires
/// [`engine::overworld::PlayerState::step`] through
/// ([`advance_player_one_frame`]), not just the engine function underneath
/// it.
#[test]
fn advance_player_one_frame_crosses_an_offset_connection_shifting_the_cross_axis() {
    let runtime = connected_runtime(6, 6, assets::Direction::East, -2, MapId("MAP_EAST"));
    let maps = SingleConnectedMap {
        id: MapId("MAP_EAST"),
        dimensions: (10, 10),
        // Stepping east off x=5 (width) at y=5: target_y = y - offset =
        // 5 - (-2) = 7, landing at the neighbour's west edge (x=0).
        landing_position: (0, 7),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    };
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
            to_map: MapId("MAP_EAST"),
            to_position: (0, 7),
        }
    );
    assert_eq!(player.position(), (0, 7));
}

/// The same offset rule on the *north/south* axis -- the axis the
/// Littleroot -> Route 101 crossing the early playable slice actually uses
/// (both its real
/// connections happen to carry `offset: 0`, which is exactly why a sign
/// regression in `target_x = x - offset` would survive every real-map
/// test; this synthetic pin is the guard).
#[test]
fn advance_player_one_frame_crosses_an_offset_north_connection_shifting_x() {
    let runtime = connected_runtime(6, 6, assets::Direction::North, -2, MapId("MAP_NORTH"));
    let maps = SingleConnectedMap {
        id: MapId("MAP_NORTH"),
        dimensions: (10, 10),
        // Stepping north off y=0 at x=5: target_x = x - offset =
        // 5 - (-2) = 7, landing at the neighbour's south edge (y = 9).
        landing_position: (7, 9),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    };
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
            to_map: MapId("MAP_NORTH"),
            to_position: (7, 9),
        }
    );
    assert_eq!(player.position(), (7, 9));
}

/// Different lateral positions along the *same* edge each translate to
/// their own distinct landing tile (not, say, a single hardcoded landing
/// position regardless of where the player actually crossed) -- the
/// property that makes walking along Littleroot's whole north edge land on
/// the matching column of Route 101, not just one fixed spot.
#[test]
fn advance_player_one_frame_crossing_at_different_lateral_positions_lands_at_the_matching_column() {
    let runtime = connected_runtime(8, 8, assets::Direction::North, 0, MapId("MAP_NORTH"));

    for x in [0_i32, 3, 7] {
        let maps = SingleConnectedMap {
            id: MapId("MAP_NORTH"),
            dimensions: (8, 8),
            landing_position: (x, 7),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 3,
            },
        };
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
                to_map: MapId("MAP_NORTH"),
                to_position: (x, 7),
            },
            "column x={x} must land at the neighbour's matching column, not a fixed spot"
        );
    }
}

/// A candidate connection whose perpendicular bounds don't cover the
/// stepped-off position at all is rejected outright -- the step is simply
/// blocked, exactly as if there were no connection there (mirrors
/// `engine::overworld::map_runtime::tests::
/// resolve_connection_rejects_position_outside_target_bounds`, again pinned
/// through this module's own integration entry point).
#[test]
fn advance_player_one_frame_rejects_a_crossing_outside_the_neighbours_bounds() {
    // The neighbour is much narrower than this map: only x in 0..3 lands.
    let runtime = connected_runtime(10, 3, assets::Direction::South, 0, MapId("MAP_NARROW"));
    let maps = SingleConnectedMap {
        id: MapId("MAP_NARROW"),
        dimensions: (3, 3),
        landing_position: (8, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
    };
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
/// north connection to Route 101 (`offset: 0` in both directions --
/// `data/maps/LittlerootTown/map.json:15-21`,
/// `data/maps/Route101/map.json`'s own south connection back), crossed both
/// ways through the whole [`OverworldPhase`].
///
/// `x = 10` is real map data's own walkable path column: Littleroot's row
/// `y = 0` and Route 101's row `y = 19` (both maps are `20x20`, so `19` is
/// Route 101's own last row) are each open ground at `x` in `{10, 11}` --
/// confirmed by hand against `data/layouts/LittlerootTown/map.bin` and
/// `data/layouts/Route101/map.bin` -- flanked by collision-1 fence tiles
/// everywhere else along both edges, and no object event sits anywhere near
/// either row (both maps' own `map.json`).
///
/// **`(10, 19)` is also the Route 101 rescue coord-event trigger tile (issue
/// #231).** That is deliberate and left alone: this phase is built with no
/// `party_lead`, so the trigger fires on the crossing's drain frame and
/// `super::first_battle_trigger`'s own `begin_first_battle` takes its
/// documented "no party mon yet" arm -- it consumes the trigger
/// (`VAR_ROUTE101_STATE` goes to `2`, upstream's mid-cutscene ordering) and
/// logs, but builds no battle, so nothing freezes the frame and the walk back
/// south below proceeds exactly as it did before that slice existed. This
/// test's subject is the *connection crossing*, and giving it a lead to keep
/// the trigger quiet would only trade a logged no-op for a battle it would
/// then have to play out. The trigger's own behaviour on this tile is pinned
/// by `real_pack_crossing_into_route_101_lands_on_the_rescue_trigger_and_starts_the_battle`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
#[allow(clippy::too_many_lines)] // one continuous two-crossing walk; splitting would re-set-up the pack scene
fn walking_off_littlerootss_north_edge_crosses_into_route_101_and_back() {
    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let route101 = assets::MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    // Two ordinary tiles south of the north edge, already facing the
    // direction that will carry it there. No `party_lead` is assigned: see
    // the doc comment on the rescue trigger this walk lands on.
    let player = PlayerState::new((10, 2), 3, Direction::North);
    let mut phase = OverworldPhase::for_test(scene, littleroot, player, None);

    // Two ordinary steps north, each draining its own walk animation, land
    // on the map's own last interior row (y = 0).
    for _ in 0..2 {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
        assert_eq!(phase.map_id, littleroot, "still on Littleroot's own grid");
    }
    assert_eq!(phase.player.position(), (10, 0));

    // The third step walks off the grid's own top edge -- the crossing.
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.tick, 33,
        "a crossing is NOT a map load: upstream's LoadMapFromCameraTransition re-inits only \
         the secondary tileset counter (InitSecondaryTilesetAnimation, overworld.c:815), so \
         the primary counter this port models keeps running -- 32 walk frames plus the \
         crossing step's own increment, with no reset (contrast `step_tests`' warp tick-reset test)"
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

    // Cross right back south: (10, 19) is already Route 101's own last row,
    // so a single south step walks straight off that edge too.
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

/// The same claim at upstream's *other* map-transition call site,
/// `LoadMapFromCameraTransition` (`src/overworld.c:800`) -- and this one is
/// walked end to end, because a connection crossing really can land the
/// player in grass.
///
/// Deleting `self.wild.restart_immunity_steps()` from
/// [`OverworldPhase::cross_connection`] fails here: the counter is still
/// exhausted on arrival and the very first grass step draws.
///
/// The player is repositioned onto [`ROUTE_101_GRASS_ROW`] after the real
/// crossing rather than walked there: the crossing lands at `(10, 19)` on
/// Route 101's south edge and the nearest tall grass is eleven tiles away,
/// which would spend the whole window before reaching it. Everything the
/// test actually asserts -- the counter at the crossing, and the four silent
/// steps after it -- is untouched by where the player is standing.
///
/// **`VAR_ROUTE101_STATE` is pre-set to its spent value (issue #231).**
/// `(10, 19)` is also the rescue coord-event trigger tile, and a *live*
/// trigger there would legitimately keep this step from ever reaching
/// `CheckStandardWildEncounter` at all -- upstream's `TryStartStepBasedScript`
/// returns TRUE at `:155-161`, two lines above it, so the immunity counter
/// (decremented inside that function, `:668-686`) would not move. That is the
/// coord event's behaviour, pinned in `first_battle_trigger_tests`;
/// asserting it here too would only turn this test's own subject -- the
/// crossing's immunity restart -- into a hostage of an unrelated slice. The
/// var starts at the post-rescue `2` instead, exactly the state upstream
/// leaves behind once the Birch rescue has run, which makes `(10, 19)` the
/// ordinary landing tile this test has always meant it to be.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn crossing_a_map_connection_restarts_the_wild_encounter_immunity_window() {
    let littleroot = MapId("MAP_LITTLEROOT_TOWN");
    let route101 = MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let mut phase = OverworldPhase::for_test(
        scene,
        littleroot,
        PlayerState::new((10, 4), 3, Direction::North),
        None,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);
    // The rescue trigger on the landing tile is already spent (doc comment).
    phase
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, 2)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");

    // Four ordinary steps north up Littleroot's own walkable column. Its map
    // has no wild table, so they draw nothing -- but upstream's
    // sWildEncounterImmunitySteps counts steps, not encounters, so the
    // window is spent all the same.
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

    // The fifth step walks off Littleroot's north edge into Route 101 -- the
    // real crossing, through `PlayerState::step` and `cross_connection`.
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

    // Stand in Route 101's own tall grass (doc comment) and walk it: three
    // immune steps are left, and the fourth rolls.
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
