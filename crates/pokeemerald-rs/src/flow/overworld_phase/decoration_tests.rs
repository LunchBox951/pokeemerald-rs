//! Tests for `ON_TRANSITION` map scripts, decoration-placeholder visibility,
//! and object-event flag state at map entry.

use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use assets::MapId;
use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState};
use platform::Buttons;

const BEDROOM: MapId = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");

const PLACEHOLDER_APPROACH: (i32, i32) = (1, 3);
const PLACEHOLDER_TILE: (i32, i32) = (1, 2);

fn bedroom_placeholder(events: &assets::MapEvents) -> &assets::ObjectEvent {
    events
        .object_events
        .iter()
        .find(|o| (i32::from(o.x), i32::from(o.y)) == PLACEHOLDER_TILE)
        .expect("the bedroom declares a decoration placeholder at (1, 2)")
}

#[test]
fn every_decoration_flag_id_is_settable() {
    let mut data = EventData::new();
    for &id in assets::object_event_flags::DECORATION_FLAGS {
        assert!(
            data.flag_set(id).is_ok(),
            "{id:#x} must be an ordinary flag id"
        );
    }
    assert_eq!(
        assets::object_event_flags::DECORATION_FLAGS.len(),
        14,
        "SecretBase_EventScript_SetDecorationFlags sets FLAG_DECORATION_1..14"
    );
}

#[test]
fn the_decoration_flag_map_list_covers_every_bundled_map_with_placeholders() {
    // Mirrors `crates/xtask/src/extract/scope.rs` `LAYOUTS` (no `xtask` dependency here);
    // `the_bundled_layout_set_is_pinned_for_the_tables_derived_from_it` pins that list.
    const BUNDLED_LAYOUTS: [&str; 10] = [
        "LAYOUT_LITTLEROOT_TOWN",
        "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        "LAYOUT_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
        "LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE",
        "LAYOUT_ROUTE101",
        "LAYOUT_OLDALE_TOWN",
        "LAYOUT_ROUTE103",
    ];

    let mut bundled_maps: Vec<MapId> = assets::MapHeaderTable::new()
        .iter()
        .filter(|header| BUNDLED_LAYOUTS.contains(&header.layout.name()))
        .map(|header| header.id)
        .collect();
    bundled_maps.sort_unstable_by_key(|m| m.0);
    assert_eq!(
        bundled_maps.len(),
        9,
        "the ten bundled layouts cover nine maps (the lab's `_WITH_TABLE` \
         variant is an alternate layout for a map already listed, not a map \
         of its own)"
    );

    let table = assets::MapEventsTable::new();
    let mut with_placeholders: Vec<MapId> = bundled_maps
        .iter()
        .copied()
        .filter(|map| {
            table.resolve(*map).is_ok_and(|events| {
                events
                    .object_events
                    .iter()
                    .any(|o| o.flag.starts_with("FLAG_DECORATION_"))
            })
        })
        .collect();
    with_placeholders.sort_unstable_by_key(|m| m.0);

    let mut listed = super::connections::MAPS_THAT_SET_DECORATION_FLAGS.to_vec();
    listed.sort_unstable_by_key(|m| m.0);
    assert_eq!(
        with_placeholders, listed,
        "every bundled map declaring FLAG_DECORATION_* object events must run \
         SecretBase_EventScript_SetDecorationFlags on transition, and no other"
    );
}

/// Upstream's `MAP_SCRIPT_ON_TRANSITION` hides the twelve `OBJ_EVENT_GFX_VAR_*` slots
/// (`data/maps/LittlerootTown_BrendansHouse_2F/scripts.inc:6-10`; slots at `map.json:32-185`).
#[test]
fn the_bedrooms_decoration_placeholders_do_not_block_a_fresh_save() {
    let events = assets::MapEventsTable::new().resolve(BEDROOM).unwrap();
    let placeholder = bedroom_placeholder(events);
    assert_eq!(
        (placeholder.graphics_id, placeholder.flag),
        ("OBJ_EVENT_GFX_VAR_8", "FLAG_DECORATION_9"),
        "fixture precondition: its real map.json graphics id and flag"
    );

    let phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(10, 10),
        BEDROOM,
        PlayerState::new(PLACEHOLDER_APPROACH, 3, Direction::North),
        None,
    );
    assert!(
        !engine::overworld::object_event_is_visible(placeholder, &phase.save1().event_data),
        "an empty decoration slot is the flag-SET state; entering the map \
         must have set it"
    );

    let mut phase = phase;
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        PLACEHOLDER_TILE,
        "the placeholder's tile must be walkable on a fresh save"
    );
}

#[test]
fn a_decoration_placeholder_would_block_if_its_flag_were_clear() {
    let events = assets::MapEventsTable::new().resolve(BEDROOM).unwrap();
    let placeholder = bedroom_placeholder(events);

    let decoration_flags_clear = EventData::new();
    assert!(
        engine::overworld::object_event_is_visible(placeholder, &decoration_flags_clear),
        "with its flag clear the placeholder is spawned -- upstream would \
         resolve OBJ_EVENT_GFX_VAR_8 through VAR_OBJ_GFX_ID_8 and draw a real \
         decoration there"
    );

    let scene = crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(BEDROOM);
    let runtime = scene.runtime();
    let mut player = PlayerState::new(PLACEHOLDER_APPROACH, 3, Direction::North);
    assert!(
        matches!(
            player.step(
                Some(Direction::North),
                &runtime,
                &no_connections,
                &decoration_flags_clear
            ),
            engine::overworld::StepOutcome::Blocked { .. }
        ),
        "a spawned decoration is a solid object event -- upstream never \
         exempts one from DoesObjectCollideWithObjectAt, so a future \
         decoration-placement slice inherits this blocking behaviour"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn load_default_hides_the_spawn_bedrooms_decoration_placeholders() {
    let mut phase = OverworldPhase::load(
        crate::pack_source::PackSource::Repo,
        crate::new_game::NewGameOptions::DEFAULT,
        crate::new_game::NewGameIdentity::DEFAULT,
    )
    .expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.map_id,
        new_game::SPAWN_MAP_ID,
        "fixture precondition: the spawn map is the bedroom with the placeholders"
    );

    for &id in assets::object_event_flags::DECORATION_FLAGS {
        assert_eq!(
            phase.save1().event_data.flag_get(id),
            Ok(true),
            "{id:#x} must be set on entering the bedroom"
        );
    }

    let events = assets::MapEventsTable::new()
        .resolve(new_game::SPAWN_MAP_ID)
        .unwrap();
    let placeholders: Vec<_> = events
        .object_events
        .iter()
        .filter(|o| o.graphics_id.starts_with("OBJ_EVENT_GFX_VAR_"))
        .collect();
    assert_eq!(
        placeholders.len(),
        12,
        "the bedroom declares twelve decoration slots (DECOR_MAX_PLAYERS_HOUSE)"
    );
    for placeholder in &placeholders {
        assert!(
            !engine::overworld::object_event_is_visible(placeholder, &phase.save1().event_data),
            "{} at ({}, {}) must be absent on a fresh save",
            placeholder.graphics_id,
            placeholder.x,
            placeholder.y
        );
    }

    // Direct placement: the spawn tile is the stair warp, so walking off and back leaves the room.
    phase.player = PlayerState::new(PLACEHOLDER_APPROACH, 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        PLACEHOLDER_TILE,
        "OverworldPhase::load must leave an empty decoration slot's tile walkable in the real room"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_into_a_bedroom_hides_its_decoration_placeholders() {
    let mut phase = OverworldPhase::load(
        crate::pack_source::PackSource::Repo,
        crate::new_game::NewGameOptions::DEFAULT,
        crate::new_game::NewGameIdentity::DEFAULT,
    )
    .expect("run `cargo xtask extract` first");

    for &id in assets::object_event_flags::DECORATION_FLAGS {
        phase.save1.event_data.flag_clear(id).unwrap();
    }
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(assets::object_event_flags::DECORATION_FLAGS[0]),
        Ok(false),
        "fixture precondition: the flags start clear for this transition"
    );

    phase.warp_to(new_game::SPAWN_MAP_ID, 0);
    assert_eq!(
        phase.map_id,
        new_game::SPAWN_MAP_ID,
        "the warp must have landed"
    );

    for &id in assets::object_event_flags::DECORATION_FLAGS {
        assert_eq!(
            phase.save1().event_data.flag_get(id),
            Ok(true),
            "{id:#x} must be set again by the arrival transition"
        );
    }

    phase.player = PlayerState::new(PLACEHOLDER_APPROACH, 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        PLACEHOLDER_TILE,
        "warp_to must restore hidden decoration slots after the flags were cleared"
    );
}

/// The male truck branch hides the bedroom's Poké Ball
/// (`data/maps/InsideOfTruck/scripts.inc:31`); `init_save_blocks` applies it.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_spawn_bedroom_has_no_invisible_poke_ball_collider() {
    let mut phase = OverworldPhase::load(
        crate::pack_source::PackSource::Repo,
        crate::new_game::NewGameOptions::DEFAULT,
        crate::new_game::NewGameIdentity::DEFAULT,
    )
    .expect("run `cargo xtask extract` first");

    let ball = assets::MapEventsTable::new()
        .resolve(new_game::SPAWN_MAP_ID)
        .unwrap()
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_ITEM_BALL")
        .expect("the bedroom declares the rival's Poké Ball");
    assert_eq!(
        (ball.x, ball.y, ball.elevation),
        (3, 4, 0),
        "fixture precondition: its real map.json position, at the \
         transition-wildcard elevation that collides with anything"
    );
    assert!(
        !engine::overworld::object_event_is_visible(ball, &phase.save1().event_data),
        "a fresh male save must not have the rival's Poké Ball spawned"
    );

    phase.player = PlayerState::new((3, 5), 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        (3, 4),
        "the Poké Ball's tile must be walkable on a fresh save"
    );
}

/// The male truck branch hides the rival's mother and sibling
/// (`data/maps/InsideOfTruck/scripts.inc:29-30`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn taking_the_stairs_does_not_land_in_a_house_full_of_the_rivals_family() {
    let mut phase = OverworldPhase::load(
        crate::pack_source::PackSource::Repo,
        crate::new_game::NewGameOptions::DEFAULT,
        crate::new_game::NewGameIdentity::DEFAULT,
    )
    .expect("run `cargo xtask extract` first");

    walk_one_tile(&mut phase, Buttons::DOWN);
    turn_and_walk_one_tile(&mut phase, Buttons::UP);
    assert_eq!(phase.map_id, ONE_F, "the stairs must have landed on 1F");

    let events = assets::MapEventsTable::new().resolve(ONE_F).unwrap();
    let find = |gfx: &str| {
        events
            .object_events
            .iter()
            .find(|o| o.graphics_id == gfx)
            .unwrap_or_else(|| panic!("1F must declare a {gfx}"))
    };
    let data = &phase.save1().event_data;

    let mom = find("OBJ_EVENT_GFX_MOM");
    assert!(
        engine::overworld::object_event_is_visible(mom, data),
        "the player's own mother must be home"
    );
    let rival_mom = find("OBJ_EVENT_GFX_WOMAN_4");
    assert!(
        !engine::overworld::object_event_is_visible(rival_mom, data),
        "the rival's mother must not be duplicated into the player's house"
    );
    let sibling = find("OBJ_EVENT_GFX_NINJA_BOY");
    assert!(
        !engine::overworld::object_event_is_visible(sibling, data),
        "the rival's sibling must not be an invisible blocker at ({}, {})",
        sibling.x,
        sibling.y
    );

    for tile in [(rival_mom.x, rival_mom.y), (sibling.x, sibling.y)] {
        let (x, y) = (i32::from(tile.0), i32::from(tile.1));
        phase.player = PlayerState::new((x, y + 1), 3, Direction::North);
        phase.step(held(Buttons::UP));
        assert_eq!(
            phase.player.position(),
            (x, y),
            "({x}, {y}) must be walkable once its object event is hidden"
        );
    }
}
