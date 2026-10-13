//! `OldaleTown_OnTransition` flag-gates the footprints-man and mart-employee
//! `setobjectxyperm`/`setobjectmovementtype` pair on entry (`data/maps/OldaleTown/scripts.inc:2,8-25`).

use assets::{MapId, MovementType};
use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState};
use platform::Buttons;

use crate::flow::tests::held;
use crate::overworld::oldale_town_npc_reposition;

use super::OverworldPhase;

const OLDALE_TOWN: MapId = MapId("MAP_OLDALE_TOWN");

/// `setobjectxyperm LOCALID_FOOTPRINTS_MAN, 1, 11` (`scripts.inc:18-21`).
const FOOTPRINTS_MAN_NEW_TILE: (i32, i32) = (1, 11);

/// Authored footprints-man tile (`data/maps/OldaleTown/map.json:61-66`).
const FOOTPRINTS_MAN_OLD_TILE: (i32, i32) = (8, 9);

/// `setobjectxyperm LOCALID_OLDALE_MART_EMPLOYEE, 13, 14` (`scripts.inc:23-26`).
const MART_EMPLOYEE_NEW_TILE: (i32, i32) = (13, 14);

/// Authored mart-employee tile (`data/maps/OldaleTown/map.json:47-52`).
const MART_EMPLOYEE_OLD_TILE: (i32, i32) = (13, 7);

/// `FLAG_ADVENTURE_STARTED` (`include/constants/flags.h:136`).
const FLAG_ADVENTURE_STARTED: u16 = 0x74;

/// `FLAG_RECEIVED_POTION_OLDALE` (`include/constants/flags.h:154`).
const FLAG_RECEIVED_POTION_OLDALE: u16 = 0x84;

const PLAYER_PALETTE_BANK: u8 = 0;

fn fresh_save_oldale_on_open_grid(player: PlayerState) -> OverworldPhase {
    oldale_on_open_grid_with_entry_flags(player, &EventData::new())
}

fn oldale_on_open_grid_with_entry_flags(
    player: PlayerState,
    event_data: &EventData,
) -> OverworldPhase {
    let events = oldale_town_npc_reposition::resolve_map_events(OLDALE_TOWN, event_data)
        .expect("MAP_OLDALE_TOWN must resolve");
    let events: &'static assets::MapEvents = Box::leak(Box::new(events));
    OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_events(
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

#[test]
fn fresh_save_resolves_both_oldale_npc_tiles_and_facings() {
    let map_events = oldale_town_npc_reposition::resolve_map_events(OLDALE_TOWN, &EventData::new())
        .expect("MAP_OLDALE_TOWN must resolve");

    let footprints_man = map_events
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_MANIAC")
        .expect("Oldale Town's object events include the footprints man");
    assert_eq!(
        (i32::from(footprints_man.x), i32::from(footprints_man.y)),
        FOOTPRINTS_MAN_NEW_TILE,
        "the footprints man's resolved tile"
    );
    assert_eq!(
        footprints_man.movement_type,
        MovementType::FaceLeft,
        "the footprints man's resolved facing"
    );

    let mart_employee = map_events
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_MART_EMPLOYEE")
        .expect("Oldale Town's object events include the mart employee");
    assert_eq!(
        (i32::from(mart_employee.x), i32::from(mart_employee.y)),
        MART_EMPLOYEE_NEW_TILE,
        "the mart employee's resolved tile"
    );
    assert_eq!(
        mart_employee.movement_type,
        MovementType::FaceDown,
        "the mart employee's resolved facing"
    );
}

#[test]
fn the_footprints_mans_new_tile_blocks_the_player() {
    let (fx, fy) = FOOTPRINTS_MAN_NEW_TILE;
    let mut phase =
        fresh_save_oldale_on_open_grid(PlayerState::new((fx - 1, fy), 3, Direction::East));

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

#[test]
fn the_footprints_mans_old_tile_is_walkable() {
    let (fx, fy) = FOOTPRINTS_MAN_OLD_TILE;
    let mut phase =
        fresh_save_oldale_on_open_grid(PlayerState::new((fx - 1, fy), 3, Direction::East));

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

#[test]
fn the_mart_employees_new_tile_blocks_the_player() {
    let (ex, ey) = MART_EMPLOYEE_NEW_TILE;
    let mut phase =
        fresh_save_oldale_on_open_grid(PlayerState::new((ex, ey - 1), 3, Direction::South));

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.player.position(),
        (ex, ey - 1),
        "the mart employee's new tile must block the step"
    );
    assert!(
        !phase.player.in_transit(),
        "a blocked step must not start a walk animation"
    );
}

#[test]
fn the_mart_employees_old_tile_is_walkable() {
    let (ex, ey) = MART_EMPLOYEE_OLD_TILE;
    let mut phase =
        fresh_save_oldale_on_open_grid(PlayerState::new((ex, ey - 1), 3, Direction::South));

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.player.position(),
        (ex, ey),
        "nothing stands on the mart employee's vacated map.json tile"
    );
    assert!(
        phase.player.in_transit(),
        "a walkable step starts a walk animation"
    );
}

#[test]
fn mid_visit_flags_preserve_entry_collision_interaction_and_rendering() {
    let (ex, ey) = MART_EMPLOYEE_NEW_TILE;
    let mut phase =
        fresh_save_oldale_on_open_grid(PlayerState::new((ex, ey - 1), 3, Direction::South));
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
        let player = *oam
            .iter()
            .find(|entry| entry.palette_bank() == PLAYER_PALETTE_BANK)
            .expect("the player is found by palette bank, independent of depth-ordered OAM slot");
        oam.into_iter().find(|entry| {
            entry.palette_bank() != PLAYER_PALETTE_BANK
                && entry.x() == player.x()
                && i32::from(entry.y()) == i32::from(player.y()) + 16
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
    let mut phase =
        fresh_save_oldale_on_open_grid(PlayerState::new((fx - 1, fy), 3, Direction::East));
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

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_the_repositioned_npcs_block_and_vacate_their_real_map_tiles() {
    let _ = crate::overworld::load_room(
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

#[test]
fn the_phase_seeds_live_object_events_from_the_transitioned_placements() {
    let phase = fresh_save_oldale_on_open_grid(PlayerState::new((5, 5), 3, Direction::South));
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
            assert!(
                std::ptr::eq(entry.template(), template),
                "each live entry must point at the scene's resolved template"
            );
            entry.state().position()
        })
        .collect();
    let resolved_positions: Vec<_> = resolved
        .object_events
        .iter()
        .map(|template| (i32::from(template.x), i32::from(template.y)))
        .collect();
    assert_eq!(
        seeded, resolved_positions,
        "live entries must start at the transitioned template positions, in declaration order"
    );

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

#[test]
fn a_flag_set_mid_visit_does_not_reseed_the_live_object_events() {
    let mut phase = fresh_save_oldale_on_open_grid(PlayerState::new((5, 5), 3, Direction::South));
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
        1,
        "mid-visit flags must preserve the footprints man's entry placement"
    );
}

#[test]
fn failed_warp_preserves_live_object_event_edits() {
    let mut phase = fresh_save_oldale_on_open_grid(PlayerState::new((5, 5), 3, Direction::South));
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
        (FOOTPRINTS_MAN_NEW_TILE.0, FOOTPRINTS_MAN_NEW_TILE.1 - 1),
        "a failed warp must keep the live edit, not reseed"
    );
}

#[test]
fn successful_warp_and_connection_replace_live_object_events() {
    use crate::pack_source::PackSource;

    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-live-object-events-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    crate::overworld::test_support::write_oldale_layout_pack(
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

    let mut phase = fresh_save_oldale_on_open_grid(PlayerState::new((5, 5), 3, Direction::South));
    phase.pack_source = PackSource::Test(leaked_path);
    let local_id = walk_footprints_man(&mut phase);
    phase.warp_to(OLDALE_TOWN, 0);
    assert_replaced(&phase, local_id, "the warp");

    let mut phase = fresh_save_oldale_on_open_grid(PlayerState::new((5, 5), 3, Direction::South));
    phase.pack_source = PackSource::Test(leaked_path);
    let local_id = walk_footprints_man(&mut phase);
    assert!(phase.cross_connection(OLDALE_TOWN, (5, 5)));
    assert_replaced(&phase, local_id, "the connection crossing");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn continue_uses_saved_flags_to_restore_scene_and_live_npc_placements() {
    use crate::flow::save_continue_support::save_from_the_start_menu;
    use crate::flow::tests::TempSave;
    use crate::pack_source::PackSource;

    let temp = TempSave::new("oldale-continue-flags");
    let mut slot = temp.slot();
    let mut entry_flags = EventData::new();
    for flag in [FLAG_ADVENTURE_STARTED, FLAG_RECEIVED_POTION_OLDALE] {
        entry_flags.flag_set(flag).unwrap();
    }
    let mut phase = oldale_on_open_grid_with_entry_flags(
        PlayerState::new((5, 5), 3, Direction::South),
        &entry_flags,
    );
    phase.save1.event_data = entry_flags;
    let header = assets::MapHeaderTable::new()
        .header(OLDALE_TOWN)
        .expect("MAP_OLDALE_TOWN must have a header");
    phase.save1.location = engine::save::WarpData {
        map_group: i8::try_from(header.group).unwrap(),
        map_num: i8::try_from(header.num).unwrap(),
        warp_id: -1,
        x: 5,
        y: 5,
    };
    save_from_the_start_menu(&mut phase, &mut slot);

    let saved = slot.load();
    assert!(saved.status.menu_shows_continue());
    for flag in [FLAG_ADVENTURE_STARTED, FLAG_RECEIVED_POTION_OLDALE] {
        assert!(saved.block1.event_data.flag_get(flag).unwrap());
    }
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-oldale-continue-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    crate::overworld::test_support::write_oldale_layout_pack(
        &path,
        &["girl_3", "mart_employee", "maniac"],
    );
    let leaked_path: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());
    let resumed = OverworldPhase::continue_saved_game(
        PackSource::Test(leaked_path),
        saved.block1,
        saved.block2,
    )
    .expect("the synthetic Oldale pack must load");
    assert_eq!(resumed.map_id, OLDALE_TOWN);

    let events = resumed.scene.map_events(OLDALE_TOWN).unwrap();
    for (graphics_id, tile) in [
        ("OBJ_EVENT_GFX_MANIAC", FOOTPRINTS_MAN_OLD_TILE),
        ("OBJ_EVENT_GFX_MART_EMPLOYEE", MART_EMPLOYEE_OLD_TILE),
    ] {
        let template = events
            .object_events
            .iter()
            .find(|event| event.graphics_id == graphics_id)
            .expect("the Oldale NPC must exist");
        assert_eq!(
            (i32::from(template.x), i32::from(template.y)),
            tile,
            "{graphics_id}: continue must resolve the saved flags"
        );
        assert_eq!(
            resumed
                .object_events
                .get(template.local_id)
                .unwrap()
                .state()
                .position(),
            tile,
            "{graphics_id}: the live placement must match the resumed scene"
        );
    }

    let _ = std::fs::remove_file(&path);
}
