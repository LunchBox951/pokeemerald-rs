//! Tests for warp execution, arrow-warp triggers, and door transitions
//! ([`super::OverworldPhase::warp_to`]).

use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use engine::overworld::metatile_behavior::{
    MB_ANIMATED_DOOR, MB_NON_ANIMATED_DOOR, MB_SOUTH_ARROW_WARP,
};
use engine::overworld::{warp_in_facing, Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// The issue #163 acceptance test: stepping onto the bedroom's stair warp.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn stepping_onto_the_bedroom_stair_warp_transitions_to_the_1f_map() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let bedroom = phase.map_id;

    phase.player = PlayerState::new((7, 2), new_game::SPAWN_ELEVATION, Direction::North);

    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        new_game::SPAWN_POSITION,
        "the step commits the landing tile on the frame it begins"
    );
    assert_eq!(
        phase.map_id, bedroom,
        "the warp must not fire on the frame the step begins"
    );

    for frame in 2..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.map_id, bedroom,
            "the warp must not fire mid-animation (frame {frame} of \
             {WALK_FRAMES_PER_TILE})"
        );
        assert!(
            phase.player.in_transit(),
            "the walk animation must still be draining on frame {frame}"
        );
    }

    phase.step(ButtonState::new());

    let destination = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
    assert_eq!(
        phase.map_id, destination,
        "the completed step onto the stair warp must rebind to the 1F map \
         on the 16th frame"
    );
    assert_eq!(
        phase.player.position(),
        (8, 2),
        "the player must arrive at 1F's own warp #2 position"
    );
    let (dest_pos, dest_behavior) = warp_tile_behavior(destination, 2);
    assert_eq!(dest_pos, (8, 2));
    assert_eq!(
        dest_behavior, MB_NON_ANIMATED_DOOR,
        "1F's warp #2 is the staircase's own non-animated-door tile"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "GetAdjustedInitialDirection's IsNonAnimDoor||IsDoor branch \
         (overworld.c:935-936) applies to that tile"
    );

    let dest_header = assets::MapHeaderTable::new()
        .header(destination)
        .expect("1F must resolve in the generated map-header table");
    assert_eq!(
        phase.save1().location.map_group,
        i8::try_from(dest_header.group).unwrap()
    );
    assert_eq!(
        phase.save1().location.map_num,
        i8::try_from(dest_header.num).unwrap()
    );
    assert_eq!(
        phase.save1().location.warp_id,
        2,
        "arrived via 1F's own warp-event index 2 (new_game module docs)"
    );
    assert_eq!(
        (phase.save1().location.x, phase.save1().location.y),
        (-1, -1),
        "SetWarpDestinationToMapWarp always passes -1, -1 for x/y (overworld.c:638-641)"
    );
    assert_eq!(
        (
            i32::from(phase.save1().pos.x),
            i32::from(phase.save1().pos.y)
        ),
        (8, 2),
        "save1.pos must mirror the post-warp tile, not the pre-warp one"
    );
}

/// Regression: a completed landing on an ordinary tile must not transition the map.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn stepping_onto_an_ordinary_tile_does_not_warp() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let starting_map = phase.map_id;

    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }

    assert_eq!(
        phase.map_id, starting_map,
        "stepping onto an ordinary floor tile must not transition maps"
    );
    assert_eq!(
        phase.player.position(),
        (7, 2),
        "the step itself must still have landed"
    );
    assert!(
        !phase.player.in_transit(),
        "16 frames must fully drain the step this test relies on completing"
    );
}

/// [`warp_data_index`] narrows the generated tables' real indices.
#[test]
fn warp_data_index_narrows_the_generated_map_indices() {
    let header = assets::MapHeaderTable::new()
        .header(new_game::SPAWN_MAP_ID)
        .expect("SPAWN_MAP_ID must resolve in the generated map-header table");
    assert_eq!(
        warp_data_index(header.group, "MAP_GROUP"),
        new_game::SPAWN_MAP_GROUP
    );
    assert_eq!(
        warp_data_index(header.num, "MAP_NUM"),
        new_game::SPAWN_MAP_NUM
    );
    assert_eq!(warp_data_index(0, "warp id"), 0);
    assert_eq!(warp_data_index(127, "warp id"), 127);
}

/// The out-of-range case panics rather than fabricating a plausible index.
#[test]
#[should_panic(expected = "does not fit the i8")]
fn warp_data_index_refuses_to_fabricate_an_out_of_range_index() {
    let _ = warp_data_index(128, "MAP_GROUP");
}

/// Real-pack guard for the destination-tile rule.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_front_door_warp_faces_north_from_the_destination_tiles_behavior() {
    let (source_pos, source_behavior) = warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN"), 1);
    assert_eq!(source_pos, (5, 8), "Littleroot's warp #1: the house door");
    assert_eq!(source_behavior, MB_ANIMATED_DOOR);
    assert_eq!(
        warp_in_facing(source_behavior),
        Direction::South,
        "what a (wrong) source-tile rule would have produced"
    );

    let (dest_pos, dest_behavior) =
        warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"), 1);
    assert_eq!(dest_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(dest_behavior, MB_SOUTH_ARROW_WARP);
    assert_eq!(
        warp_in_facing(dest_behavior),
        Direction::North,
        "GetAdjustedInitialDirection's IsSouthArrowWarp branch (overworld.c:937-938)"
    );
}

/// Mutation guard for [`OverworldPhase::warp_to`] itself: the front-door
/// arrival from the test above, driven through `warp_to`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_to_the_front_doormat_faces_north_and_rebinds_the_scene() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let one_f = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

    phase.warp_to(one_f, 1);

    assert_eq!(phase.map_id, one_f);
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "1F's warp #1: the doormat inside the front door"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "the doormat's own MB_SOUTH_ARROW_WARP behavior must drive the \
         facing (overworld.c:937-938) -- South would mean the destination \
         behavior was never read"
    );
    let fresh = crate::overworld::load_room(one_f).expect("1F must load from the extracted pack");
    assert!(
        phase.compose_frame()[..]
            == fresh.compose_frame(&phase.player, &phase.save1.event_data, 0)[..],
        "warp_to must rebind `scene` to the destination map, not just `map_id` -- \
         `tick` is 0 on both sides since `warp_to` resets `phase.tick` and `fresh` \
         has never `step`ped"
    );
}

// -- Doormat / arrow warp tests --------------------------------------------

/// The issue #174 acceptance test: walking onto the doormat holding South
/// exits through the front door.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_the_doormat_holding_south_exits_through_the_front_door() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    let mut phase = one_f_phase((8, 7), Direction::South);

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the step onto the doormat must commit on the frame it begins"
    );
    assert_eq!(
        phase.map_id, ONE_F,
        "the arrow warp must not fire on the frame the step begins"
    );

    for frame in 2..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.map_id, ONE_F,
            "the warp must not fire mid-animation (frame {frame} of \
             {WALK_FRAMES_PER_TILE})"
        );
    }

    phase.step(held(Buttons::DOWN));

    let destination = assets::MapId("MAP_LITTLEROOT_TOWN");
    assert_eq!(
        phase.map_id, destination,
        "the doormat's arrow-warp trigger must fire on the first at-rest \
         frame with South still held"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the front door's MB_ANIMATED_DOOR behavior drives the arrival \
         facing (overworld.c:935-936)"
    );
    assert!(
        !phase.player.in_transit(),
        "the warp lands the player at rest, not mid-step"
    );
}

/// Senior-review regression (#174 finding 1b): releasing South mid-step
/// does not exit through the doormat.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn releasing_south_mid_step_does_not_exit_through_the_doormat() {
    let mut phase = one_f_phase((8, 7), Direction::South);

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the step onto the doormat must still commit"
    );

    for _ in 2..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
    }

    assert!(
        !phase.player.in_transit(),
        "the crossing must have fully drained -- otherwise this test proves nothing"
    );
    assert_eq!(
        phase.map_id, ONE_F,
        "a released direction must not warp: upstream needs heldDirection on \
         the frame the check runs, not merely a completed step"
    );
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the player stays standing on the doormat"
    );
}

/// Review regression (#191): a one-frame Down tap on the doormat facing
/// North turns without warping.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_one_frame_down_tap_on_the_doormat_facing_north_turns_without_warping() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    let mut phase = one_f_phase((8, 8), Direction::North);

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id, ONE_F,
        "the tap frame must only turn the player, not fire the arrow warp"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the tap turned the player"
    );
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "still standing on the doormat"
    );

    phase.step(ButtonState::new());
    assert_eq!(
        phase.map_id, ONE_F,
        "a released tap leaves the player standing on the doormat, as upstream does"
    );

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "holding Down against the South facing fires the doormat normally"
    );
}

/// Senior-review regression (#174 finding 1a): standing on the doormat
/// facing North and holding South exits.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn standing_on_the_doormat_facing_north_and_holding_south_exits() {
    let mut phase = one_f_phase((0, 0), Direction::South);

    phase.warp_to(ONE_F, 1);
    assert_eq!(phase.player.position(), (8, 8), "1F's warp #1: the doormat");
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a warp-in onto a south-arrow tile faces back out of it"
    );

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id, ONE_F,
        "the turn frame must not warp: the poll reads the pre-turn facing"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "frame 1 turned the player to face the held direction"
    );

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "holding Down while standing on the doormat must exit the house -- \
         the step south is blocked (off-map), so only the every-frame \
         heldDirection poll can ever fire this"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_eq!(phase.player.facing(), Direction::South);
}

/// The companion: a warp-in alone must not warp back out.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn arriving_on_the_doormat_does_not_immediately_warp_back_out() {
    let mut phase = one_f_phase((0, 0), Direction::South);
    phase.warp_to(ONE_F, 1);

    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) * 2 {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.map_id, ONE_F,
            "an arrival on an arrow tile must not re-fire it"
        );
    }
    assert_eq!(phase.player.position(), (8, 8));
    assert_eq!(phase.player.facing(), Direction::North);
}

/// Regression: walking onto the doormat facing East does not exit.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_the_doormat_facing_east_does_not_exit() {
    let mut phase = one_f_phase((7, 8), Direction::East);

    phase.step(held(Buttons::RIGHT));
    assert_eq!(phase.player.position(), (8, 8));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert!(
        !phase.player.in_transit(),
        "the crossing must have fully drained on the last frame Right was polled"
    );

    assert_eq!(
        phase.map_id, ONE_F,
        "the doormat must not fire while facing East -- only South matches \
         its MB_SOUTH_ARROW_WARP behavior"
    );
    assert_eq!(phase.player.position(), (8, 8));
}

/// The issue #194 acceptance test: a legal step in the arrow direction
/// warps instead of stepping.
#[test]
fn a_legal_step_in_the_arrow_direction_warps_instead_of_stepping() {
    let mut phase = walkable_south_arrow_phase();

    phase.step(held(Buttons::DOWN));

    assert_ne!(
        phase.player.position(),
        (8, 9),
        "a legal step in the arrow direction must never happen -- the warp preempts it \
         (overworld.c:1444-1455); the pre-#194 movement-first ordering would have stepped \
         the player onto (8, 9) here, before the poll ever got a chance to fire"
    );
    assert!(
        !phase.player.in_transit(),
        "no walk animation was ever started -- the step never ran"
    );
}

/// The positive half: the same scene, asserting the preempting warp landed.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_legal_step_in_the_arrow_direction_lands_the_warp() {
    let mut phase = walkable_south_arrow_phase();

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "the preempting arrow warp must actually land, not merely eat the step"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_ne!(
        phase.player.position(),
        (8, 9),
        "and the step in the arrow direction still never happened"
    );
}

/// Warp restarts the wild encounter immunity window (`LoadMapFromWarp`
/// call site).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_restarts_the_wild_encounter_immunity_window() {
    use engine::overworld::WILD_ENCOUNTER_IMMUNITY_STEPS;
    use engine::rng::Rng;

    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        assets::MapId("MAP_ROUTE101"),
        PlayerState::new((2, 5), 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);

    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        walk_one_tile_east(&mut phase);
    }
    assert_eq!(
        phase.wild.immunity_steps(),
        WILD_ENCOUNTER_IMMUNITY_STEPS,
        "four ordinary steps must exhaust the post-transition window"
    );

    phase.warp_to(ONE_F, 1);
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "a warp is a full map load -- LoadMapFromWarp calls \
         RestartWildEncounterImmunitySteps (overworld.c:850)"
    );

    let mut rng = Rng::new(IMMUNITY_SEED);
    for step in 1..=WILD_ENCOUNTER_IMMUNITY_STEPS {
        assert!(
            !grass_step_draws(&mut phase.wild, &mut rng),
            "grass step {step} after a warp must be immune, and immune means RNG-silent"
        );
    }
    assert!(
        grass_step_draws(&mut phase.wild, &mut rng),
        "the fifth step is out of the window and rolls for real"
    );
}
