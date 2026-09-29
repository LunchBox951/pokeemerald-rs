//! Tests for `oldale_town_npc_reposition`:
//! `OldaleTown_OnTransition`'s `setobjectxyperm`/`setobjectmovementtype`
//! pair, gated by `FLAG_ADVENTURE_STARTED`/`FLAG_RECEIVED_POTION_OLDALE`,
//! resolved once when [`crate::overworld::load_room`] loads Oldale Town and
//! held by the scene for the rest of the visit. This file's phases load
//! from a fresh save, so both flags start unset and both NPCs start at
//! their moved tiles.
//!
//! Mirrors `route103_rival_tests`' own "real events over a synthetic grid"
//! split (that module's doc comment): a fabricated flat, open room loaded
//! with the *real* resolved `MAP_OLDALE_TOWN` object events, so the tests
//! need no local pack -- plus one `#[ignore]`d real-pack walk that proves
//! the same claims against the bundled map's own real layout.

use assets::{MapId, MovementType};
use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState};
use platform::Buttons;

use crate::flow::tests::held;
use crate::overworld::oldale_town_npc_reposition;

use super::OverworldPhase;

/// `MAP_OLDALE_TOWN`, used throughout this file.
const OLDALE_TOWN: MapId = MapId("MAP_OLDALE_TOWN");

/// The footprints man's own post-`OldaleTown_OnTransition` tile --
/// `setobjectxyperm LOCALID_FOOTPRINTS_MAN, 1, 11`
/// (`oldale_town_npc_reposition`'s own module docs).
const FOOTPRINTS_MAN_NEW_TILE: (i32, i32) = (1, 11);

/// The footprints man's bare map.json tile, now vacated.
const FOOTPRINTS_MAN_OLD_TILE: (i32, i32) = (8, 9);

/// The mart employee's own post-`OldaleTown_OnTransition` tile --
/// `setobjectxyperm LOCALID_OLDALE_MART_EMPLOYEE, 13, 14`.
const MART_EMPLOYEE_NEW_TILE: (i32, i32) = (13, 14);

/// The mart employee's bare map.json tile, now vacated.
const MART_EMPLOYEE_OLD_TILE: (i32, i32) = (13, 7);

/// `FLAG_ADVENTURE_STARTED` (`include/constants/flags.h:136`).
const FLAG_ADVENTURE_STARTED: u16 = 0x74;

/// `FLAG_RECEIVED_POTION_OLDALE` (`include/constants/flags.h:154`).
const FLAG_RECEIVED_POTION_OLDALE: u16 = 0x84;

/// An [`OverworldPhase`] over a **synthetic** flat, open 20x20 room loaded
/// with the *real* `MAP_OLDALE_TOWN` object events a fresh save resolves
/// (module docs) -- large enough to hold every tile this file exercises,
/// with no interior collision of its own, so any blocked step below is
/// caused by an object event, never the grid.
fn oldale_phase(player: PlayerState) -> OverworldPhase {
    let events = oldale_town_npc_reposition::resolve_map_events(OLDALE_TOWN, &EventData::new())
        .expect("MAP_OLDALE_TOWN must resolve");
    let events: &'static assets::MapEvents = Box::leak(Box::new(events));
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_events(
            20,
            20,
            events,
            &["girl_3", "mart_employee", "maniac"],
        ),
        OLDALE_TOWN,
        player,
        None,
    )
}

// -- The reposition, pinned directly at the data source --------------------

/// The object events [`crate::overworld::load_room`] resolves on a fresh
/// save carry the footprints man's and mart employee's post-script
/// positions and facings -- the scene holds them for the visit, and both
/// the collision tests below and NPC rendering draw from them.
#[test]
fn the_footprints_man_and_mart_employee_resolve_at_their_post_transition_tiles() {
    let map_events = oldale_town_npc_reposition::resolve_map_events(OLDALE_TOWN, &EventData::new())
        .expect("MAP_OLDALE_TOWN must resolve");

    let footprints_man = map_events
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_MANIAC")
        .expect("Oldale Town's object events include the footprints man");
    assert_eq!(
        (i32::from(footprints_man.x), i32::from(footprints_man.y)),
        FOOTPRINTS_MAN_NEW_TILE
    );
    assert_eq!(footprints_man.movement_type, MovementType::FaceLeft);

    let mart_employee = map_events
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_MART_EMPLOYEE")
        .expect("Oldale Town's object events include the mart employee");
    assert_eq!(
        (i32::from(mart_employee.x), i32::from(mart_employee.y)),
        MART_EMPLOYEE_NEW_TILE
    );
    assert_eq!(mart_employee.movement_type, MovementType::FaceDown);
}

// -- Collision, over a synthetic grid ---------------------------------------

/// The footprints man's new tile, `(1, 11)`, blocks the player -- approached
/// from the west, facing east, the only direction that keeps the whole walk
/// inside the 20x20 fixture.
#[test]
fn the_footprints_mans_new_tile_blocks_the_player() {
    let (fx, fy) = FOOTPRINTS_MAN_NEW_TILE;
    let mut phase = oldale_phase(PlayerState::new((fx - 1, fy), 3, Direction::East));

    phase.step(held(Buttons::RIGHT));

    assert_eq!(
        phase.player.position(),
        (fx - 1, fy),
        "the footprints man's new tile must block the step"
    );
    assert!(
        !phase.player.in_transit(),
        "a blocked step must not start a walk animation"
    );
}

/// The complement: the footprints man's *old*, bare map.json tile,
/// `(8, 9)`, is vacated and therefore walkable.
#[test]
fn the_footprints_mans_old_tile_is_walkable() {
    let (fx, fy) = FOOTPRINTS_MAN_OLD_TILE;
    let mut phase = oldale_phase(PlayerState::new((fx - 1, fy), 3, Direction::East));

    phase.step(held(Buttons::RIGHT));

    assert_eq!(
        phase.player.position(),
        (fx, fy),
        "nothing stands on the footprints man's vacated map.json tile"
    );
    assert!(
        phase.player.in_transit(),
        "a walkable step starts a walk animation"
    );
}

/// The mart employee's new tile, `(13, 14)`, blocks the player -- approached
/// from the north, facing south.
#[test]
fn the_mart_employees_new_tile_blocks_the_player() {
    let (ex, ey) = MART_EMPLOYEE_NEW_TILE;
    let mut phase = oldale_phase(PlayerState::new((ex, ey - 1), 3, Direction::South));

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.player.position(),
        (ex, ey - 1),
        "the mart employee's new tile must block the step"
    );
    assert!(!phase.player.in_transit());
}

/// The complement: the mart employee's *old*, bare map.json tile,
/// `(13, 7)`, is vacated and therefore walkable.
#[test]
fn the_mart_employees_old_tile_is_walkable() {
    let (ex, ey) = MART_EMPLOYEE_OLD_TILE;
    let mut phase = oldale_phase(PlayerState::new((ex, ey - 1), 3, Direction::South));

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.player.position(),
        (ex, ey),
        "nothing stands on the mart employee's vacated map.json tile"
    );
    assert!(phase.player.in_transit());
}

// -- The entry-time placement holds for the visit --------------------------

/// `OldaleTown_OnTransition` runs on map entry only
/// (`data/maps/OldaleTown/scripts.inc:4-9`), so setting either flag while
/// Oldale Town stays loaded must leave collision, interaction, and rendering
/// on the placement resolved at entry until the next transition.
#[test]
fn a_flag_set_mid_visit_leaves_the_entry_time_placement_in_force() {
    let (ex, ey) = MART_EMPLOYEE_NEW_TILE;
    let mut phase = oldale_phase(PlayerState::new((ex, ey - 1), 3, Direction::South));
    let entry_oam = phase
        .scene
        .oam_entries_and_bg_scroll(&phase.player, &phase.save1.event_data)
        .0;

    phase
        .save1
        .event_data
        .flag_set(FLAG_ADVENTURE_STARTED)
        .unwrap();
    phase
        .save1
        .event_data
        .flag_set(FLAG_RECEIVED_POTION_OLDALE)
        .unwrap();

    let events = phase
        .scene
        .map_events(OLDALE_TOWN)
        .expect("MAP_OLDALE_TOWN must resolve");
    let header = assets::MapHeaderTable::new()
        .header(OLDALE_TOWN)
        .expect("MAP_OLDALE_TOWN must have a header");
    {
        let runtime = phase.scene.runtime(OLDALE_TOWN, header, &events);
        assert!(
            phase
                .interaction_tokens_this_frame(crate::flow::tests::pressed(Buttons::A), &runtime)
                .is_some(),
            "the mart employee must still answer A on her entry-time tile"
        );
    }

    let mart_employee_draw = |phase: &OverworldPhase| {
        let (oam, _) = phase
            .scene
            .oam_entries_and_bg_scroll(&phase.player, &phase.save1.event_data);
        let player = oam[0];
        oam.into_iter().skip(1).find(|entry| {
            entry.x() == player.x() && i32::from(entry.y()) == i32::from(player.y()) + 16
        })
    };
    assert_eq!(
        phase
            .scene
            .oam_entries_and_bg_scroll(&phase.player, &phase.save1.event_data)
            .0,
        entry_oam,
        "rendering must not change when a flag flips mid-visit"
    );
    assert!(
        mart_employee_draw(&phase).is_some(),
        "the mart employee must still draw one tile south of the player"
    );

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (ex, ey - 1),
        "the mart employee's entry-time tile must still block the step"
    );

    let (fx, fy) = FOOTPRINTS_MAN_OLD_TILE;
    let mut phase = oldale_phase(PlayerState::new((fx - 1, fy), 3, Direction::East));
    phase
        .save1
        .event_data
        .flag_set(FLAG_ADVENTURE_STARTED)
        .unwrap();
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.player.position(),
        (fx, fy),
        "the footprints man's map.json tile must stay vacated until the next transition"
    );
}

// -- The same four claims, against the real bundled map -------------------

/// The real-pack counterpart of the four synthetic-grid tests above,
/// walked over Oldale Town's own real layout (`cargo xtask extract`
/// required) rather than a fabricated flat grid -- proves the two new
/// blockers and the two vacated tiles are genuine, not an artifact of the
/// synthetic fixture's own unconditional walkability.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_the_repositioned_npcs_block_and_vacate_their_real_map_tiles() {
    let scene = crate::overworld::load_room(
        OLDALE_TOWN,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    let approach = |pos: (i32, i32), facing: Direction| {
        OverworldPhase::for_test(
            crate::overworld::load_room(
                OLDALE_TOWN,
                crate::overworld::PlayerCharacter::Brendan,
                &engine::event_data::EventData::new(),
            )
            .expect("run `cargo xtask extract` first"),
            OLDALE_TOWN,
            PlayerState::new(pos, 3, facing),
            None,
        )
    };
    // `scene` above is unused once `approach` builds its own per-call scene
    // (`OverworldPhase::for_test` takes ownership) -- kept only so a
    // pack-load failure surfaces with the same message before any of the
    // four sub-cases run.
    drop(scene);

    let (fx, fy) = FOOTPRINTS_MAN_NEW_TILE;
    let mut phase = approach((fx - 1, fy), Direction::East);
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.player.position(),
        (fx - 1, fy),
        "the footprints man's new real-map tile must block the step"
    );

    let (fx, fy) = FOOTPRINTS_MAN_OLD_TILE;
    let mut phase = approach((fx - 1, fy), Direction::East);
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.player.position(),
        (fx, fy),
        "the footprints man's vacated real-map tile must be walkable"
    );

    let (ex, ey) = MART_EMPLOYEE_NEW_TILE;
    let mut phase = approach((ex, ey - 1), Direction::South);
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (ex, ey - 1),
        "the mart employee's new real-map tile must block the step"
    );

    let (ex, ey) = MART_EMPLOYEE_OLD_TILE;
    let mut phase = approach((ex, ey - 1), Direction::South);
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (ex, ey),
        "the mart employee's vacated real-map tile must be walkable"
    );
}

// -- The phase's live object events ------------------------------------------

/// The phase seeds one live entry per resolved Oldale template, in
/// declaration order, from the scene's own transitioned placements
/// (`OldaleTown_OnTransition`) rather than the authored map.json tiles.
#[test]
fn the_phase_seeds_live_object_events_from_the_transitioned_placements() {
    let phase = oldale_phase(PlayerState::new((5, 5), 3, Direction::South));
    let resolved = phase
        .scene
        .map_events(OLDALE_TOWN)
        .expect("MAP_OLDALE_TOWN must resolve");

    let seeded: Vec<_> = resolved
        .object_events
        .iter()
        .map(|template| {
            let entry = phase
                .object_events
                .get(template.local_id)
                .expect("every template seeds a live entry");
            assert!(std::ptr::eq(entry.template(), template));
            entry.state().position()
        })
        .collect();
    let authored: Vec<_> = resolved
        .object_events
        .iter()
        .map(|template| (i32::from(template.x), i32::from(template.y)))
        .collect();
    assert_eq!(seeded, authored, "seeded in declaration order");

    let footprints_man = phase
        .object_events
        .object_events_at(FOOTPRINTS_MAN_NEW_TILE.0, FOOTPRINTS_MAN_NEW_TILE.1, 3)
        .count();
    let vacated = phase
        .object_events
        .object_events_at(FOOTPRINTS_MAN_OLD_TILE.0, FOOTPRINTS_MAN_OLD_TILE.1, 3)
        .count();
    assert_eq!((footprints_man, vacated), (1, 0));
    let mart_employee = phase
        .object_events
        .object_events_at(MART_EMPLOYEE_NEW_TILE.0, MART_EMPLOYEE_NEW_TILE.1, 3)
        .count();
    let mart_vacated = phase
        .object_events
        .object_events_at(MART_EMPLOYEE_OLD_TILE.0, MART_EMPLOYEE_OLD_TILE.1, 3)
        .count();
    assert_eq!((mart_employee, mart_vacated), (1, 0));
}

/// A flag set mid-visit does not reseed the collection: it is fixed at
/// entry, like the scene's own events.
#[test]
fn a_flag_set_mid_visit_does_not_reseed_the_live_object_events() {
    let mut phase = oldale_phase(PlayerState::new((5, 5), 3, Direction::South));
    phase
        .save1
        .event_data
        .flag_set(FLAG_ADVENTURE_STARTED)
        .unwrap();
    phase
        .save1
        .event_data
        .flag_set(FLAG_RECEIVED_POTION_OLDALE)
        .unwrap();
    assert_eq!(
        phase
            .object_events
            .object_events_at(FOOTPRINTS_MAN_NEW_TILE.0, FOOTPRINTS_MAN_NEW_TILE.1, 3)
            .count(),
        1
    );
}

/// A failed transition leaves the current collection, live edits
/// included, exactly as it was.
#[test]
fn a_failed_warp_leaves_the_live_object_events_intact() {
    let mut phase = oldale_phase(PlayerState::new((5, 5), 3, Direction::South));
    let local_id = phase
        .object_events
        .object_events_at(FOOTPRINTS_MAN_NEW_TILE.0, FOOTPRINTS_MAN_NEW_TILE.1, 3)
        .next()
        .expect("the footprints man is seeded")
        .template()
        .local_id;
    phase
        .object_events
        .get_mut(local_id)
        .expect("seeded")
        .state_mut()
        .walk(Direction::North);

    phase.warp_to(MapId("MAP_DOES_NOT_EXIST"), 0);

    assert_eq!(phase.map_id, OLDALE_TOWN);
    assert_eq!(
        phase
            .object_events
            .get(local_id)
            .expect("still seeded")
            .state()
            .position(),
        (FOOTPRINTS_MAN_NEW_TILE.0, FOOTPRINTS_MAN_NEW_TILE.1 - 1)
    );
}

/// A successful warp and a successful connection crossing each replace the
/// live collection with the destination's freshly seeded one: a live edit
/// made before the transition does not survive it, and every entry points
/// at the new scene's own resolved template.
#[test]
fn a_successful_transition_replaces_the_live_object_events() {
    use crate::pack_source::PackSource;

    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-live-object-events-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    crate::overworld::tests::write_oldale_layout_pack(
        &path,
        &["girl_3", "mart_employee", "maniac"],
    );
    let leaked_path: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());

    let walk_footprints_man = |phase: &mut OverworldPhase| {
        let local_id = phase
            .object_events
            .object_events_at(FOOTPRINTS_MAN_NEW_TILE.0, FOOTPRINTS_MAN_NEW_TILE.1, 3)
            .next()
            .expect("the footprints man is seeded")
            .template()
            .local_id;
        phase
            .object_events
            .get_mut(local_id)
            .unwrap()
            .state_mut()
            .walk(Direction::North);
        local_id
    };
    let assert_replaced = |phase: &OverworldPhase, local_id, what: &str| {
        assert_eq!(phase.map_id, OLDALE_TOWN, "{what} must complete");
        assert_eq!(
            phase
                .object_events
                .get(local_id)
                .unwrap()
                .state()
                .position(),
            FOOTPRINTS_MAN_NEW_TILE,
            "{what} must reseed, discarding the pre-transition live edit"
        );
        for template in phase.scene.map_events(OLDALE_TOWN).unwrap().object_events {
            let entry = phase.object_events.get(template.local_id).unwrap();
            assert!(
                std::ptr::eq(entry.template(), template),
                "{what}: entries must point at the new scene's templates"
            );
        }
    };

    let mut phase = oldale_phase(PlayerState::new((5, 5), 3, Direction::South));
    phase.pack_source = PackSource::Test(leaked_path);
    let local_id = walk_footprints_man(&mut phase);
    phase.warp_to(OLDALE_TOWN, 0);
    assert_replaced(&phase, local_id, "the warp");

    let mut phase = oldale_phase(PlayerState::new((5, 5), 3, Direction::South));
    phase.pack_source = PackSource::Test(leaked_path);
    let local_id = walk_footprints_man(&mut phase);
    assert!(phase.cross_connection(OLDALE_TOWN, (5, 5)));
    assert_replaced(&phase, local_id, "the connection crossing");

    let _ = std::fs::remove_file(&path);
}
