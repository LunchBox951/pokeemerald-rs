//! Tests for `ON_TRANSITION` map scripts, decoration-placeholder visibility,
//! and object-event flag state at map entry.

use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// Every decoration flag id must be one `EventData::flag_set` accepts.
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

/// The transcribed map decoration list must cover every bundled map.
#[test]
fn the_decoration_flag_map_list_covers_every_bundled_map_with_placeholders() {
    const BUNDLED_MAPS: [&str; 6] = [
        "MAP_LITTLEROOT_TOWN",
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        "MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
    ];

    let table = assets::MapEventsTable::new();
    let mut with_placeholders: Vec<assets::MapId> = BUNDLED_MAPS
        .iter()
        .map(|m| assets::MapId(m))
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

/// The finding-5 regression: on a fresh save the player's bedroom must be
/// walkable, not fenced in by its own decoration placeholders.
#[test]
fn the_bedrooms_decoration_placeholders_do_not_block_a_fresh_save() {
    const BEDROOM: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
    let events = assets::MapEventsTable::new().resolve(BEDROOM).unwrap();

    let placeholder = events
        .object_events
        .iter()
        .find(|o| (o.x, o.y) == (1, 2))
        .expect("the bedroom declares a decoration placeholder at (1, 2)");
    assert_eq!(
        (placeholder.graphics_id, placeholder.flag),
        ("OBJ_EVENT_GFX_VAR_8", "FLAG_DECORATION_9"),
        "fixture precondition: its real map.json graphics id and flag"
    );

    let phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        BEDROOM,
        PlayerState::new((1, 3), 3, Direction::North),
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
        (1, 2),
        "the placeholder's tile must be walkable on a fresh save"
    );
}

/// A decoration placeholder would block if its flag were clear.
#[test]
fn a_decoration_placeholder_would_block_if_its_flag_were_clear() {
    const BEDROOM: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
    let events = assets::MapEventsTable::new().resolve(BEDROOM).unwrap();
    let placeholder = events
        .object_events
        .iter()
        .find(|o| (o.x, o.y) == (1, 2))
        .expect("the bedroom declares a decoration placeholder at (1, 2)");

    let data = EventData::new();
    assert!(
        engine::overworld::object_event_is_visible(placeholder, &data),
        "with its flag clear the placeholder is spawned -- upstream would \
         resolve OBJ_EVENT_GFX_VAR_8 through VAR_OBJ_GFX_ID_8 and draw a real \
         decoration there"
    );

    let header = assets::MapHeaderTable::new().header(BEDROOM).unwrap();
    let scene = crate::overworld::tests::synthetic_scene(10, 10);
    let runtime = scene.runtime(BEDROOM, header, events);
    let mut player = PlayerState::new((1, 3), 3, Direction::North);
    let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };
    assert!(
        matches!(
            player.step(Some(Direction::North), &runtime, &no_connections, &data),
            engine::overworld::StepOutcome::Blocked { .. }
        ),
        "a spawned decoration is a solid object event -- upstream never \
         exempts one from DoesObjectCollideWithObjectAt, so a future \
         decoration-placement slice inherits this blocking behaviour"
    );
}

/// The production pin: `load_default` hides decoration placeholders.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn load_default_hides_the_spawn_bedrooms_decoration_placeholders() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
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

    phase.player = PlayerState::new((1, 3), 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        (1, 2),
        "an empty decoration slot's tile must be walkable in the real room"
    );
}

/// The `warp_to` pin: every subsequent entry into a bedroom hides
/// its decoration placeholders.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_into_a_bedroom_hides_its_decoration_placeholders() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");

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

    phase.player = PlayerState::new((1, 3), 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(phase.player.position(), (1, 2));
}

/// The reported regression on the production path: a fresh run must not
/// have the rival's Poké Ball staged as an invisible collider.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_spawn_bedroom_has_no_invisible_poke_ball_collider() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");

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

/// After taking the stairs, the player's own house must not contain
/// the rival's family.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn taking_the_stairs_does_not_land_in_a_house_full_of_the_rivals_family() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");

    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    phase.step(held(Buttons::UP));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
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
