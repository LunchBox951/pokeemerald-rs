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

/// A zero-offset connection carries the stepped-off x across unchanged onto the
/// neighbour's opposite edge.
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
        landing_behavior: engine::overworld::metatile_behavior::MB_NORMAL,
    };
    let mut player = PlayerState::new((3, 4), 3, Direction::South);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        false,
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

/// Crossing onto a forced-movement tile arms both guards (`field_player_avatar.c:332-349`),
/// and once the entered map is bound the landing tile forces the next driver
/// step at its own cadence whatever the keypad holds: a current pushes south
/// in six frames, ice slips on in the movement direction in eight
/// (`field_player_avatar.c:473-524`, `event_object_movement.c:8233-8298`).
#[test]
fn crossing_onto_a_forced_landing_tile_arms_the_guards_through_this_lane() {
    use engine::overworld::metatile_behavior::{MB_ICE, MB_SOUTHWARD_CURRENT};
    for (behavior, frames) in [(MB_SOUTHWARD_CURRENT, 6), (MB_ICE, 8)] {
        for held in [None, Some(Direction::North)] {
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
                landing_behavior: behavior,
            };
            let mut player = PlayerState::new((3, 4), 3, Direction::South);

            advance_player_one_frame(
                &mut player,
                Some(Direction::South),
                false,
                &runtime,
                &maps,
                &NO_FLAGS,
            );

            assert!(
                player.forced_movement_armed(),
                "the movement guard must arm from the neighbour's landing behavior"
            );
            assert!(
                player.field_input_suppressed(),
                "and so must the field-input gate the landing call reads"
            );

            // The phase rebinds onto the entered map after the crossing
            // frame; from here on the driver reads that map's own tiles.
            let landed = OverworldPhase::for_test(
                crate::overworld::test_support::synthetic_scene_with_special_tile(
                    5,
                    5,
                    (3, 0),
                    behavior,
                ),
                ONE_F,
                PlayerState::new((0, 0), 3, Direction::South),
                None,
            );
            let landed = runtime_for(&landed);
            for _ in 1..WALK_FRAMES_PER_TILE {
                advance_player_one_frame(
                    &mut player,
                    None,
                    false,
                    &landed,
                    &no_connections,
                    &NO_FLAGS,
                );
            }
            assert!(!player.in_transit(), "setup: the crossing step drained");

            let outcome = advance_player_one_frame(
                &mut player,
                held,
                false,
                &landed,
                &no_connections,
                &NO_FLAGS,
            );
            assert_eq!(
                outcome,
                StepOutcome::Advanced {
                    from: (3, 0),
                    to: (3, 1),
                },
                "behavior {behavior} held {held:?}"
            );
            assert_eq!(player.step_progress(), 1);
            for expected in 2..frames {
                advance_player_one_frame(
                    &mut player,
                    held,
                    false,
                    &landed,
                    &no_connections,
                    &NO_FLAGS,
                );
                assert_eq!(player.step_progress(), expected, "behavior {behavior}");
                assert!(player.in_transit());
            }
            advance_player_one_frame(
                &mut player,
                held,
                false,
                &landed,
                &no_connections,
                &NO_FLAGS,
            );
            assert!(
                !player.in_transit(),
                "behavior {behavior} held {held:?} settles on frame {frames}"
            );
        }
    }
}

const ROUTE_101: MapId = MapId("MAP_ROUTE101");
const OLDALE_TOWN: MapId = MapId("MAP_OLDALE_TOWN");
/// Oldale's south-edge column the crossing lands on, clear of its coord events
/// (x 8-10) and its hidden rival object (x 11).
const LANDING_X: i32 = 13;
/// Oldale's own object-event sheets, so the destination room loads.
const OLDALE_SPRITES: [&str; 3] = ["girl_3", "mart_employee", "maniac"];
/// Route 101's own object-event sheets, so a crossing back can load it.
const ROUTE_101_SPRITES: [&str; 5] = [
    "youngster",
    "prof_birch",
    "birchs_bag",
    "zigzagoon_1",
    "boy_2",
];

/// A phase standing at rest on Route 101's north edge, facing Oldale Town,
/// whose pack carries `behavior` on Oldale's landing tile. Returns the pack's
/// path for removal.
fn route101_phase_facing_a_forced_oldale_landing(
    behavior: u8,
) -> (OverworldPhase, std::path::PathBuf) {
    use crate::pack_source::PackSource;

    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-forced-landing-{behavior}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    let sprites: Vec<&str> = OLDALE_SPRITES
        .iter()
        .chain(&ROUTE_101_SPRITES)
        .copied()
        .collect();
    crate::overworld::test_support::write_route101_oldale_layout_pack(
        &path,
        &[((13, 19), behavior)],
        &sprites,
    );
    let leaked: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(20, 20),
        ROUTE_101,
        PlayerState::new((LANDING_X, 0), 3, Direction::North),
        None,
    );
    phase.pack_source = PackSource::Test(leaked);
    (phase, path)
}

/// Crosses north into Oldale through [`OverworldPhase::step`], so the
/// production [`OverworldPhase::cross_connection`] rebinds the scene, then
/// drains the crossing walk.
fn cross_into_oldale_and_drain(phase: &mut OverworldPhase) {
    phase.step(held(Buttons::UP));
    assert_eq!(phase.map_id, OLDALE_TOWN, "the crossing must rebind");
    assert_eq!(phase.player.position(), (LANDING_X, 19));
    assert!(
        phase.player.forced_movement_armed(),
        "the movement guard must arm from the entered map's landing behavior"
    );
    assert!(phase.player.field_input_suppressed());
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert!(
        !phase.player.in_transit(),
        "setup: the crossing step drained"
    );
}

/// The same landing as
/// `crossing_onto_a_forced_landing_tile_arms_the_guards_through_this_lane`, but
/// crossed and stepped through [`OverworldPhase::step`] over a pack-loaded
/// destination: once [`OverworldPhase::cross_connection`] binds Oldale, its
/// own landing tile forces the next step whatever the keypad holds, a
/// northward current in six frames and ice in eight
/// (`field_player_avatar.c:473-524`, `event_object_movement.c:8233-8298`).
#[test]
fn a_rebound_connection_landing_forces_the_next_phase_step_at_its_cadence() {
    use engine::overworld::metatile_behavior::{MB_ICE, MB_NORTHWARD_CURRENT};
    for (behavior, frames) in [(MB_NORTHWARD_CURRENT, 6), (MB_ICE, 8)] {
        for buttons in [ButtonState::new(), held(Buttons::DOWN)] {
            let (mut phase, path) = route101_phase_facing_a_forced_oldale_landing(behavior);
            cross_into_oldale_and_drain(&mut phase);

            phase.step(buttons);
            assert_eq!(phase.map_id, OLDALE_TOWN);
            assert_eq!(
                phase.player.position(),
                (LANDING_X, 18),
                "behavior {behavior} must push north off the rebound landing"
            );
            assert_eq!(phase.player.step_progress(), 1, "behavior {behavior}");
            for expected in 2..frames {
                phase.step(buttons);
                assert_eq!(
                    phase.player.step_progress(),
                    expected,
                    "behavior {behavior}"
                );
            }
            phase.step(buttons);
            assert!(
                !phase.player.in_transit(),
                "behavior {behavior} settles on frame {frames}"
            );
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// A southward current on the rebound landing pushes the player straight back
/// over the edge it was entered by: the automatic no-input step returns
/// `Crossed` through the existing connection route, and the phase rebinds
/// Route 101 the ordinary way (`field_player_avatar.c:506-509`, `overworld.c:784-825`).
#[test]
fn a_forced_current_on_a_rebound_landing_crosses_back_on_its_automatic_step() {
    use engine::overworld::metatile_behavior::MB_SOUTHWARD_CURRENT;
    let (mut phase, path) = route101_phase_facing_a_forced_oldale_landing(MB_SOUTHWARD_CURRENT);
    cross_into_oldale_and_drain(&mut phase);

    phase.step(ButtonState::new());
    assert_eq!(phase.map_id, ROUTE_101, "the forced step must cross back");
    assert_eq!(phase.player.position(), (LANDING_X, 0));
    assert_eq!(phase.player.step_progress(), 1);
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "an ordinary crossing's rebind"
    );
    assert_eq!(phase.save1().location.warp_id, -1);
    for expected in 2..6 {
        phase.step(ButtonState::new());
        assert_eq!(phase.player.step_progress(), expected);
    }
    phase.step(ButtonState::new());
    assert!(!phase.player.in_transit(), "a current settles on frame 6");
    let _ = std::fs::remove_file(&path);
}

/// A nonzero east/west offset shifts the cross-axis y by `-offset` (`fieldmap.c:578-598,699-712`).
#[test]
fn advance_player_one_frame_crosses_an_offset_connection_shifting_the_cross_axis() {
    let runtime = connected_runtime(6, 6, assets::Direction::East, -2, MapId("MAP_EAST"));
    let maps = SingleConnectedMap {
        id: MapId("MAP_EAST"),
        dimensions: (10, 10),
        landing_position: (0, 7),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
        landing_behavior: engine::overworld::metatile_behavior::MB_NORMAL,
    };
    let mut player = PlayerState::new((5, 5), 3, Direction::East);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::East),
        false,
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

/// The same offset rule on a north/south edge, shifting x. The real Littleroot/Route 101
/// connections have offset 0, so only this case guards the sign (`fieldmap.c:578-598,699-712`).
#[test]
fn advance_player_one_frame_crosses_an_offset_north_connection_shifting_x() {
    let runtime = connected_runtime(6, 6, assets::Direction::North, -2, MapId("MAP_NORTH"));
    let maps = SingleConnectedMap {
        id: MapId("MAP_NORTH"),
        dimensions: (10, 10),
        landing_position: (7, 9),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
        landing_behavior: engine::overworld::metatile_behavior::MB_NORMAL,
    };
    let mut player = PlayerState::new((5, 0), 3, Direction::North);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::North),
        false,
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

/// Each lateral position along one edge lands on its own matching column.
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
            landing_behavior: engine::overworld::metatile_behavior::MB_NORMAL,
        };
        let mut player = PlayerState::new((x, 0), 3, Direction::North);
        let outcome = advance_player_one_frame(
            &mut player,
            Some(Direction::North),
            false,
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

/// A connection whose bounds do not cover the stepped-off position is rejected and blocks the step.
#[test]
fn advance_player_one_frame_rejects_a_crossing_outside_the_neighbours_bounds() {
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
        landing_behavior: engine::overworld::metatile_behavior::MB_NORMAL,
    };
    let mut player = PlayerState::new((8, 2), 3, Direction::South);

    let outcome = advance_player_one_frame(
        &mut player,
        Some(Direction::South),
        false,
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

/// Littleroot Town's real north connection to Route 101 (`offset: 0`), crossed both ways.
///
/// `(10, 19)` is also the Route 101 rescue trigger tile. With no lead the trigger
/// consumes itself without building a battle, so the walk is unaffected; see
/// `real_pack_crossing_into_route_101_lands_on_the_rescue_trigger_and_starts_the_battle`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
#[allow(clippy::too_many_lines)] // one continuous two-crossing walk; splitting would re-set-up the pack scene
fn walking_off_littlerootss_north_edge_crosses_into_route_101_and_back() {
    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let route101 = assets::MapId("MAP_ROUTE101");
    let source = crate::pack_source::PackSource::Repo;
    let scene = crate::overworld::load_room_from_source(
        &source,
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    let player = PlayerState::new((10, 2), 3, Direction::North);
    let mut phase = OverworldPhase::for_test_with_source(scene, littleroot, player, None, source);

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
         crossing step's own increment, with no reset (contrast `step_tests`' warp tick-reset test)"
    );
    assert_eq!(
        phase.pending_landing,
        Some((10, 19)),
        "the crossing step's landing tile re-latches in the *entered* map's coordinate \
         space, so the landing call's door check evaluates against Route 101"
    );
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.pending_landing,
        Some((10, 19)),
        "call 16 only drains the walk animation -- the landing is still outstanding \
         (issue #1039)"
    );
    phase.step(ButtonState::new());
    assert_eq!(
        phase.pending_landing, None,
        "call 17's field input consumed the latched landing (Route 101's south edge is \
         no door)"
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

/// A crossing restarts the wild-encounter immunity window (`overworld.c:800`), so the
/// grass steps taken after it are protected. Deleting `self.wild.restart_immunity_steps()`
/// from [`OverworldPhase::cross_connection`] fails here.
///
/// The player is moved onto [`ROUTE_101_GRASS_ROW`] after the crossing because the landing
/// is eleven tiles from grass. `VAR_ROUTE101_STATE` starts at its spent value so the rescue
/// trigger on the landing tile (pinned in `first_battle_trigger_tests`) cannot intercept.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn crossing_a_map_connection_restarts_the_wild_encounter_immunity_window() {
    let littleroot = MapId("MAP_LITTLEROOT_TOWN");
    let route101 = MapId("MAP_ROUTE101");
    let source = crate::pack_source::PackSource::Repo;
    let scene = crate::overworld::load_room_from_source(
        &source,
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let mut phase = OverworldPhase::for_test_with_source(
        scene,
        littleroot,
        PlayerState::new((10, 4), 3, Direction::North),
        None,
        source,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);
    phase
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, 2)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");

    // Littleroot has no wild table, so these steps draw nothing but still spend the window.
    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    }
    // Drain the last step's outstanding landing.
    phase.step(ButtonState::new());
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
        0,
        "call 16 only drains the crossing step's walk animation -- its \
         CheckStandardWildEncounter ordering has not run yet (issue #1039)"
    );
    phase.step(ButtonState::new());
    assert_eq!(
        phase.wild.immunity_steps(),
        1,
        "call 17's field input then spends the first of the four the crossing had just \
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
        assert!(!phase.is_wild_battle_active());
    }
    assert_eq!(phase.wild.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);

    walk_one_tile_east(&mut phase);
    assert_ne!(
        phase.rng.state(),
        Rng::new(IMMUNITY_SEED).state(),
        "the step past the window's end rolls for real"
    );
}

/// A refused connection rebind restores the exact pre-step player, including the retained
/// previous elevation and movement history (`overworld.c:784-815`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn refused_connection_rebind_restores_the_complete_pre_step_player() {
    let littleroot = assets::MapId("MAP_LITTLEROOT_TOWN");
    let source = crate::pack_source::PackSource::Repo;
    let scene = crate::overworld::load_room_from_source(
        &source,
        littleroot,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let player = PlayerState::with_saved_elevations((10, 2), 0, 3, Direction::North);
    let mut phase = OverworldPhase::for_test_with_source(scene, littleroot, player, None, source);

    for _ in 0..2 {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    }
    assert_eq!(phase.player.position(), (10, 0));
    assert!(phase.player.second_foot_leads());

    // Warm the connection cache so landing resolves, then break the source so the room reload is refused.
    assert!(phase
        .connection_pack
        .set(assets::pack::AssetPack::load_repo().expect("run `cargo xtask extract` first"))
        .is_ok());
    phase.pack_source = crate::pack_source::PackSource::Explicit(
        std::path::Path::new("/nonexistent/issue-2028-refused-connection.pack").into(),
    );
    assert!(phase.pack_source.load().is_err());

    let before = phase.player;
    let loads = crate::pack_source::pack_loads_on_this_thread();
    phase.step(held(Buttons::UP));

    assert!(
        crate::pack_source::pack_loads_on_this_thread() > loads,
        "the destination room reload must actually be attempted"
    );
    assert_eq!(phase.map_id, littleroot);
    assert_eq!(phase.player.position(), (10, 0));
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (before.elevation(), before.previous_elevation())
    );
    assert_eq!(phase.player, before, "movement history must survive too");
    assert!(!phase.player.in_transit());
}
