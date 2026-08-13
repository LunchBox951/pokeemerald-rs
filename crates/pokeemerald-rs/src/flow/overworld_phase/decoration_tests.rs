//! Tests for `ON_TRANSITION` map scripts, decoration-placeholder visibility,
//! and object-event flag state at map entry.

use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use assets::MapId;
use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

// -- ON_TRANSITION decoration flags -----------------------------------------

/// Every [`assets::object_event_flags::DECORATION_FLAGS`] id must be one
/// [`EventData::flag_set`] accepts -- what makes
/// [`super::connections::run_on_transition_map_script`]'s `expect` unreachable rather
/// than a latent panic.
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

/// The transcribed [`super::connections::MAPS_THAT_SET_DECORATION_FLAGS`] list must be
/// exactly the set of bundled maps that actually carry decoration
/// placeholders -- the tripwire against the list going stale if the
/// extraction pipeline ever bundles another bedroom or secret base.
#[test]
fn the_decoration_flag_map_list_covers_every_bundled_map_with_placeholders() {
    /// The maps `crates/xtask/src/extract/mod.rs`'s `LAYOUTS` bundles --
    /// mirrored here as `crate::overworld::npc`'s own tests mirror it (this
    /// crate cannot depend on `xtask`; that module's
    /// `the_bundled_layout_set_is_pinned_for_the_tables_derived_from_it` is
    /// the tripwire for the list itself growing).
    const BUNDLED_MAPS: [&str; 6] = [
        "MAP_LITTLEROOT_TOWN",
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        "MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
    ];

    let table = assets::MapEventsTable::new();
    let mut with_placeholders: Vec<MapId> = BUNDLED_MAPS
        .iter()
        .map(|m| MapId(m))
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
///
/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F` declares twelve
/// `OBJ_EVENT_GFX_VAR_*` placeholders down the room's left column
/// (`map.json:32-175`), `(1, 2)` among them. Nothing sets
/// `FLAG_DECORATION_*` at new-game time, so once object events became solid
/// these turned into invisible walls. Upstream hides them from the map's own
/// `MAP_SCRIPT_ON_TRANSITION` (see
/// [`super::connections::run_on_transition_map_script`]), which
/// [`OverworldPhase::load_default`] now mirrors.
#[test]
fn the_bedrooms_decoration_placeholders_do_not_block_a_fresh_save() {
    const BEDROOM: MapId = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
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

    // A phase entering the bedroom -- the same path `load_default` takes,
    // minus the pack.
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

    // And the tile is genuinely walkable: step north onto it.
    let mut phase = phase;
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        (1, 2),
        "the placeholder's tile must be walkable on a fresh save"
    );
}

/// The same map entered *without* the on-transition script would block --
/// pinning that the test above measures the fix rather than some unrelated
/// property of the fixture (e.g. the placeholder being unreachable anyway).
#[test]
fn a_decoration_placeholder_would_block_if_its_flag_were_clear() {
    const BEDROOM: MapId = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");
    let events = assets::MapEventsTable::new().resolve(BEDROOM).unwrap();
    let placeholder = events
        .object_events
        .iter()
        .find(|o| (o.x, o.y) == (1, 2))
        .expect("the bedroom declares a decoration placeholder at (1, 2)");

    // A store with the decoration flags deliberately left clear -- i.e. the
    // pre-fix state, and also the state upstream reaches for a slot that
    // really does hold a decoration.
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
    let no_connections = |_: MapId| -> Option<(u16, u16)> { None };
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

/// The **production** pin for the decoration-flag fix, on the acceptance
/// path itself: [`OverworldPhase::load_default`] spawns the player into
/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F` -- the very room carrying the
/// twelve `OBJ_EVENT_GFX_VAR_*` placeholders -- so that constructor's own
/// `run_on_transition_map_script` call is what makes the room walkable in
/// the real game, not just in the `for_test` fixtures above.
///
/// Deleting that call from `load_default` must fail *here*: the headless
/// tests all build their phase through `for_test`, which has its own call,
/// so none of them can catch it.
///
/// Real-pack, because `load_default` is the pack-loading constructor.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn load_default_hides_the_spawn_bedrooms_decoration_placeholders() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.map_id,
        new_game::SPAWN_MAP_ID,
        "fixture precondition: the spawn map is the bedroom with the placeholders"
    );

    // Every flag the map's own ON_TRANSITION script sets.
    for &id in assets::object_event_flags::DECORATION_FLAGS {
        assert_eq!(
            phase.save1().event_data.flag_get(id),
            Ok(true),
            "{id:#x} must be set on entering the bedroom"
        );
    }

    // ...and therefore every placeholder is absent, by the engine's own
    // visibility query rather than by restating the flag check.
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

    // And the observable consequence: the tile a placeholder stages on is
    // walkable. `(1, 2)` is `OBJ_EVENT_GFX_VAR_8`/`FLAG_DECORATION_9` at
    // elevation 3, on open floor in the real layout -- so nothing but the
    // object event could block it.
    //
    // Placed directly rather than walked from the spawn tile (which is the
    // stair warp at `SPAWN_POSITION`, so stepping off it and back would
    // leave the room): the same direct-placement shape, for the same
    // reason, as
    // `frame_tests::walking_downstairs_and_talking_to_mom_opens_and_closes_her_dialog`.
    phase.player = PlayerState::new((1, 3), 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        (1, 2),
        "an empty decoration slot's tile must be walkable in the real room"
    );
}

/// The same pin for the *other* production call site,
/// [`OverworldPhase::warp_to`] -- every subsequent entry into a bedroom,
/// not just the first.
///
/// Calls `warp_to` directly rather than walking the 1F stairs: it is the
/// production method (the one `step` invokes on a resolved warp), and
/// driving it straight makes the assertion about the transition itself
/// rather than about the route taken to reach it. The flags are cleared
/// first so the assertion cannot pass on the state `load_default` already
/// left behind.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_into_a_bedroom_hides_its_decoration_placeholders() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");

    // Clear what `load_default` set, so only `warp_to` can restore it.
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

    // Warp into the bedroom (its only warp event, id 0, is the stair tile).
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

    // Same walkability consequence as the spawn case.
    phase.player = PlayerState::new((1, 3), 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(phase.player.position(), (1, 2));
}

/// The reported regression on the **production** path: a fresh run spawned
/// straight into the bedroom must not have the rival's Poké Ball staged
/// there as an invisible collider.
///
/// `(3, 4)` is `OBJ_EVENT_GFX_ITEM_BALL` at elevation `0` -- the transition
/// wildcard, so it collides with a player at *any* elevation -- on open
/// floor in the real layout, and nothing in this port draws an item ball.
/// Its `FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F_POKE_BALL` is set only
/// by the male branch of the skipped truck sequence
/// (`data/maps/InsideOfTruck/scripts.inc:31`), which
/// [`crate::new_game::init_save_blocks`] now applies.
///
/// Real-pack and driven through [`OverworldPhase::load_default`] for the
/// same reason as this module's other two production pins: the headless
/// fixtures build their save through the same constructor, so only a test
/// that walks the real spawn path proves the acceptance route is clear.
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

    // The observable consequence: the tile is walkable. (3, 5) and (3, 4)
    // are both open floor in the real layout, so only the object event
    // could have blocked it.
    phase.player = PlayerState::new((3, 5), 3, Direction::North);
    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        (3, 4),
        "the Poké Ball's tile must be walkable on a fresh save"
    );
}

/// The 1F half of the same regression, on the production path: after taking
/// the stairs, the player's own house must not contain the *rival's* family
/// -- no duplicated mother beside the real one, and no undrawn ninja-boy
/// blocker.
///
/// Both flags come from the male truck branch
/// (`data/maps/InsideOfTruck/scripts.inc:29-30`). Walks the real 2F -> 1F
/// stair warp rather than assigning `map_id`, so the arrival goes through
/// [`OverworldPhase::warp_to`] exactly as play would.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn taking_the_stairs_does_not_land_in_a_house_full_of_the_rivals_family() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");

    // Step off the spawn tile (which *is* the stair warp) and back onto it,
    // to generate a fresh landing -- the same shape as the real-pack tests
    // in `warp_tests`.
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

    // The player's own mother is home, exactly once.
    let mom = find("OBJ_EVENT_GFX_MOM");
    assert!(
        engine::overworld::object_event_is_visible(mom, data),
        "the player's own mother must be home"
    );
    // The rival's mother is not -- this is the duplicated-mother bug.
    let rival_mom = find("OBJ_EVENT_GFX_WOMAN_4");
    assert!(
        !engine::overworld::object_event_is_visible(rival_mom, data),
        "the rival's mother must not be duplicated into the player's house"
    );
    // Nor the rival's sibling, whose sprite this port cannot draw at all --
    // so a spawned one is a pure invisible blocker.
    let sibling = find("OBJ_EVENT_GFX_NINJA_BOY");
    assert!(
        !engine::overworld::object_event_is_visible(sibling, data),
        "the rival's sibling must not be an invisible blocker at ({}, {})",
        sibling.x,
        sibling.y
    );

    // Their tiles are walkable, which is what an invisible blocker would
    // have broken. Both sit on open floor in the real 1F layout.
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
