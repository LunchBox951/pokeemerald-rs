//! Warp execution: arrival facing and map-load state (`overworld.c:846-850,929-951`).

use super::super::test_support::*;
use super::super::OverworldPhase;
use crate::new_game;
use assets::MapId;
use engine::overworld::metatile_behavior::{
    MB_ANIMATED_DOOR, MB_NON_ANIMATED_DOOR, MB_SOUTH_ARROW_WARP,
};
use engine::overworld::{
    warp_in_facing, Direction, PlayerState, WALK_FRAMES_PER_TILE, WILD_ENCOUNTER_IMMUNITY_STEPS,
};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

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

    for frame in 2..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.map_id, bedroom,
            "the warp must not fire mid-animation (frame {frame} of \
             {WALK_FRAMES_PER_TILE})"
        );
        assert!(
            phase.mid_step(),
            "the step must still be outstanding on frame {frame}"
        );
    }
    assert!(
        !phase.player.in_transit(),
        "frame {WALK_FRAMES_PER_TILE} drains the walk animation"
    );

    phase.step(ButtonState::new());

    let destination = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
    assert_eq!(
        phase.map_id, destination,
        "the frame after the walk animation drains must rebind to the 1F map"
    );
    assert_eq!(
        phase.player.position(),
        (8, 2),
        "the player must arrive at 1F's warp #2 position"
    );
    let (dest_pos, dest_behavior) = warp_tile_behavior(destination, 2);
    assert_eq!(dest_pos, (8, 2));
    assert_eq!(
        dest_behavior, MB_NON_ANIMATED_DOOR,
        "1F's warp #2 is the staircase's non-animated-door tile"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the non-animated-door destination tile determines South-facing arrival"
    );

    let dest_header = assets::MapHeaderTable::new()
        .header(destination)
        .expect("1F must resolve in the generated map-header table");
    assert_eq!(
        phase.save1().location.map_group,
        i8::try_from(dest_header.group).unwrap(),
        "SaveBlock1 location map group must be 1F's"
    );
    assert_eq!(
        phase.save1().location.map_num,
        i8::try_from(dest_header.num).unwrap(),
        "SaveBlock1 location map number must be 1F's"
    );
    assert_eq!(
        phase.save1().location.warp_id,
        2,
        "SaveBlock1 location must record arrival through 1F's warp-event index 2"
    );
    assert_eq!(
        (phase.save1().location.x, phase.save1().location.y),
        (-1, -1),
        "event-index arrivals store (-1, -1) as the location coordinates"
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

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn stepping_onto_an_ordinary_tile_does_not_warp() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let starting_map = phase.map_id;

    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert!(
        !phase.player.in_transit(),
        "{WALK_FRAMES_PER_TILE} frames must fully drain the step"
    );
    phase.step(ButtonState::new());

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
        !phase.mid_step(),
        "the call after the animation drains must consume the completed landing"
    );
}

#[test]
fn warp_data_index_narrows_the_generated_map_indices() {
    let header = assets::MapHeaderTable::new()
        .header(new_game::SPAWN_MAP_ID)
        .expect("SPAWN_MAP_ID must resolve in the generated map-header table");
    assert_eq!(
        warp_data_index(header.group, "MAP_GROUP"),
        new_game::SPAWN_MAP_GROUP,
        "generated map group must narrow to the spawn map group constant"
    );
    assert_eq!(
        warp_data_index(header.num, "MAP_NUM"),
        new_game::SPAWN_MAP_NUM,
        "generated map number must narrow to the spawn map number constant"
    );
    assert_eq!(
        warp_data_index(0, "warp id"),
        0,
        "0 is the lowest valid i8 index"
    );
    assert_eq!(
        warp_data_index(127, "warp id"),
        127,
        "127 is the highest valid i8 index"
    );
}

#[test]
fn warp_data_index_rejects_128_instead_of_clamping_to_127() {
    let result = std::panic::catch_unwind(|| warp_data_index(128, "MAP_GROUP"));
    let payload = result.expect_err("128 must be rejected, not clamped to 127");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or_default();
    assert!(
        message.contains("does not fit the i8"),
        "rejection must name the i8 overflow, got: {message:?}"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_front_door_warp_faces_north_from_the_destination_tiles_behavior() {
    let (source_pos, source_behavior) = warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN"), 1);
    assert_eq!(source_pos, (5, 8), "Littleroot's warp #1 is the house door");
    assert_eq!(source_behavior, MB_ANIMATED_DOOR);
    assert_eq!(
        warp_in_facing(source_behavior),
        Direction::South,
        "the source door tile alone would face South"
    );

    let (dest_pos, dest_behavior) =
        warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"), 1);
    assert_eq!(dest_pos, (8, 8), "1F's warp #1 is the doormat inside");
    assert_eq!(dest_behavior, MB_SOUTH_ARROW_WARP);
    assert_eq!(
        warp_in_facing(dest_behavior),
        Direction::North,
        "the destination doormat's south-arrow behavior faces North"
    );
}

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
        "1F's warp #1 is the doormat inside the front door"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "arrival must face North from the destination doormat behavior"
    );
    assert_eq!(phase.tick, 0, "warp_to must reset the frame tick");
    let fresh = crate::overworld::load_room(
        one_f,
        crate::overworld::PlayerCharacter::Brendan,
        &phase.save1.event_data,
    )
    .expect("1F must load from the extracted pack");
    assert!(
        phase.compose_frame()[..]
            == fresh.compose_frame(&phase.player, &phase.save1.event_data, 0)[..],
        "arrival scene must render the destination map"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_clears_temp_field_event_data_but_not_persistent_flags() {
    const FLAG_TEMP_12: u16 = 0x12;
    const VAR_TEMP_3: u16 = 0x4003;
    const FLAG_HIDE_ROUTE_103_RIVAL: u16 = 0x2D3;

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    phase.save1.event_data.flag_set(FLAG_TEMP_12).unwrap();
    phase.save1.event_data.var_set(VAR_TEMP_3, 7).unwrap();
    phase
        .save1
        .event_data
        .flag_set(FLAG_HIDE_ROUTE_103_RIVAL)
        .unwrap();

    phase.warp_to(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"), 1);

    assert_eq!(
        phase.save1.event_data.flag_get(FLAG_TEMP_12),
        Ok(false),
        "the temporary flag must clear on warp"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_TEMP_3),
        Ok(0),
        "the temporary variable must reset to zero on warp"
    );
    assert_eq!(
        phase.save1.event_data.flag_get(FLAG_HIDE_ROUTE_103_RIVAL),
        Ok(true),
        "the persistent flag must survive the warp"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_restarts_the_wild_encounter_immunity_window() {
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(10, 10),
        MapId("MAP_ROUTE101"),
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
        "a warp must restart the immunity counter"
    );

    let mut post_warp_grass_rng = Rng::new(IMMUNITY_SEED);
    for step in 1..=WILD_ENCOUNTER_IMMUNITY_STEPS {
        assert!(
            !grass_step_draws(&mut phase.wild, &mut post_warp_grass_rng),
            "Route 101 grass step {step} after a warp must be immune and RNG-silent"
        );
    }
    assert!(
        grass_step_draws(&mut phase.wild, &mut post_warp_grass_rng),
        "the fifth Route 101 grass step is outside the window and must roll"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_successful_warp_replaces_the_live_object_events_with_the_destinations() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let destination = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

    phase.warp_to(destination, 2);

    assert_eq!(phase.map_id, destination);
    let templates = phase
        .scene
        .map_events(destination)
        .expect("destination events resolve")
        .object_events;
    for template in templates {
        let entry = phase
            .object_events
            .get(template.local_id)
            .expect("every destination template is seeded");
        assert!(
            std::ptr::eq(entry.template(), template),
            "live object event must use the destination scene's resolved template"
        );
    }
}
