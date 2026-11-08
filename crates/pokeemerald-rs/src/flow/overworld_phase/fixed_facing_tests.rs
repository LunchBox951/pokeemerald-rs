//! Fixed-facing object events (`MOVEMENT_TYPE_FACE_UP/DOWN/LEFT/RIGHT`) driven by the
//! production frame: [`OverworldPhase::step`] runs the live collection's scheduler
//! (`MovementType_FaceDirection`, `event_object_movement.c:3031-3057`) after the frame's input.

use assets::{MapEventsTable, MapId, MovementType};
use engine::event_data::EventData;
use engine::overworld::object_event::ObjectEventCollection;
use engine::overworld::{Direction, PlayerState};
use platform::ButtonState;

use super::OverworldPhase;
use crate::overworld::oldale_town_npc_reposition as oldale;

/// A grid roomy enough for every checkpoint placement plus the player's offset from it.
const GRID: u16 = 100;
const FLOOR_ELEVATION: u8 = 3;

const OLDALE_TOWN: MapId = MapId("MAP_OLDALE_TOWN");

type Placements = &'static [(u8, MovementType)];

use MovementType::{FaceDown as D, FaceLeft as L, FaceRight as R, FaceUp as U};

/// The checkpoint inventory of issue #3059: each map's fixed-facing placements as
/// `(local_id, movement type)`, read off `data/maps/*/map.json`.
const CHECKPOINT_INVENTORY: [(&str, Placements); 13] = [
    (
        "MAP_LITTLEROOT_TOWN",
        &[(4, U), (7, U), (8, U), (5, R), (6, R)],
    ),
    (
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        &[(7, U), (5, L), (1, R), (4, R)],
    ),
    (
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
        &[(7, U), (1, L), (4, L), (5, R)],
    ),
    ("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F", &[(1, D), (14, D)]),
    (
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        &[(1, D), (14, D), (15, D)],
    ),
    (
        "MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
        &[(2, D), (3, D), (4, D), (5, D), (6, D)],
    ),
    // Authored placements; the flag-resolved Oldale set is tested separately.
    ("MAP_OLDALE_TOWN", &[(4, U), (2, D), (1, L), (3, R)]),
    ("MAP_OLDALE_TOWN_HOUSE2", &[(2, L), (1, R)]),
    (
        "MAP_OLDALE_TOWN_POKEMON_CENTER_1F",
        &[(1, D), (2, D), (3, R), (4, R)],
    ),
    (
        "MAP_OLDALE_TOWN_POKEMON_CENTER_2F",
        &[(1, D), (2, D), (3, D), (4, D)],
    ),
    ("MAP_OLDALE_TOWN_MART", &[(1, R), (2, R)]),
    (
        "MAP_ROUTE103",
        &[(17, U), (4, D), (5, D), (16, D), (1, L), (2, R), (12, R)],
    ),
    ("MAP_INSIDE_OF_TRUCK", &[(1, D), (2, D), (3, D)]),
];

const fn authored_facing(movement_type: MovementType) -> Direction {
    match movement_type {
        MovementType::FaceUp => Direction::North,
        MovementType::FaceLeft => Direction::West,
        MovementType::FaceRight => Direction::East,
        _ => Direction::South,
    }
}

/// A phase over an open synthetic room whose live collection is seeded from `events`, as
/// `connections::seed_object_events` seeds it from the scene's own events, without the sprite
/// sheets binding the scene to those events would need.
fn phase_over(map: MapId, events: &'static assets::MapEvents) -> OverworldPhase {
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(GRID, GRID),
        map,
        PlayerState::new((0, 0), FLOOR_ELEVATION, Direction::South),
        None,
    );
    phase.object_events = ObjectEventCollection::from_templates(events.object_events);
    // Every placement is on screen for its own visit: nothing hidden by a fresh save.
    for event in events.object_events {
        if let Some(flag) = assets::object_event_flags::resolve(event.flag) {
            phase.save1.event_data.flag_clear(flag).unwrap();
        }
    }
    phase
}

fn idle_frames(phase: &mut OverworldPhase, frames: usize) {
    for _ in 0..frames {
        phase.step(ButtonState::new());
    }
}

/// Stands the player where `local_id` is in the spawn window but on neither its row nor its
/// column, so no sight cone or interaction can reach.
fn stand_near(phase: &mut OverworldPhase, local_id: u8) {
    let template = phase.object_events.get(local_id).unwrap().template();
    phase.player = PlayerState::new(
        (i32::from(template.x) + 3, i32::from(template.y) + 2),
        FLOOR_ELEVATION,
        Direction::South,
    );
}

fn assert_holds_authored_facing(
    phase: &mut OverworldPhase,
    local_id: u8,
    movement_type: MovementType,
    what: &str,
) {
    stand_near(phase, local_id);
    let rng_before = phase.rng.state();
    idle_frames(phase, 4);
    idle_frames(phase, 60);

    let entry = phase.object_events.get(local_id).unwrap();
    assert!(entry.is_active(), "{what}: local {local_id} admitted");
    let state = entry.state();
    assert_eq!(
        state.facing(),
        authored_facing(movement_type),
        "{what}: local {local_id} {movement_type:?}"
    );
    assert_eq!(state.movement_type(), movement_type);
    assert_eq!(
        state.position(),
        (i32::from(entry.template().x), i32::from(entry.template().y)),
        "{what}: local {local_id} never translates"
    );
    assert_eq!(state.previous_position(), state.position());
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "{what}: fixed facing draws no field RNG"
    );
    assert!(
        phase.dialog.is_none() && phase.sight_approach.is_none() && !phase.in_battle(),
        "{what}: the loop under test was not preempted"
    );
}

#[test]
fn the_inventory_matches_each_maps_authored_fixed_facing_placements() {
    for (map, placements) in CHECKPOINT_INVENTORY {
        let events = MapEventsTable::new().resolve(MapId(map)).unwrap();
        let mut authored: Vec<(u8, MovementType)> = events
            .object_events
            .iter()
            .filter(|e| matches!(e.movement_type, U | D | L | R))
            .map(|e| (e.local_id, e.movement_type))
            .collect();
        let mut expected = placements.to_vec();
        authored.sort_by_key(|&(id, _)| id);
        expected.sort_by_key(|&(id, _)| id);
        assert_eq!(authored, expected, "{map}");
    }
}

#[test]
fn checkpoint_fixed_facing_events_hold_their_authored_facing_through_the_production_loop() {
    for (map, placements) in CHECKPOINT_INVENTORY {
        let map_id = MapId(map);
        let events = MapEventsTable::new().resolve(map_id).unwrap();
        let mut phase = phase_over(map_id, events);
        for &(local_id, movement_type) in placements {
            assert_holds_authored_facing(&mut phase, local_id, movement_type, map);
        }
    }
}

#[test]
fn oldales_flag_resolved_placements_hold_their_facing_through_the_production_loop() {
    let resolved = oldale::resolve_map_events(OLDALE_TOWN, &EventData::new()).unwrap();
    let events: &'static assets::MapEvents = Box::leak(Box::new(resolved));
    let mut phase = phase_over(OLDALE_TOWN, events);
    let fixed: Vec<(u8, MovementType)> = events
        .object_events
        .iter()
        .filter(|e| matches!(e.movement_type, U | D | L | R))
        .map(|e| (e.local_id, e.movement_type))
        .collect();
    assert!(
        fixed.contains(&(3, L)) && fixed.contains(&(2, D)),
        "the footprints man faces left and the mart employee down: {fixed:?}"
    );
    for (local_id, movement_type) in fixed {
        assert_holds_authored_facing(&mut phase, local_id, movement_type, "resolved Oldale");
    }
}

#[test]
fn a_facing_a_script_set_survives_the_loop_until_the_object_leaves_the_view() {
    let events = MapEventsTable::new()
        .resolve(MapId("MAP_OLDALE_TOWN_MART"))
        .unwrap();
    let mut phase = phase_over(MapId("MAP_OLDALE_TOWN_MART"), events);
    stand_near(&mut phase, 1);
    idle_frames(&mut phase, 3);
    phase
        .object_events
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::West);

    idle_frames(&mut phase, 120);
    assert_eq!(
        phase.object_events.get(1).unwrap().state().facing(),
        Direction::West,
        "the face action re-faces the current direction, not the authored one"
    );

    phase.player = PlayerState::new((90, 90), FLOOR_ELEVATION, Direction::South);
    idle_frames(&mut phase, 1);
    assert!(!phase.object_events.get(1).unwrap().is_active());
    stand_near(&mut phase, 1);
    idle_frames(&mut phase, 2);
    assert_eq!(
        phase.object_events.get(1).unwrap().state().facing(),
        Direction::East,
        "a respawn restores the authored FACE_RIGHT facing"
    );
}

#[test]
fn objects_outside_the_spawn_window_are_not_scheduled() {
    let events = MapEventsTable::new()
        .resolve(MapId("MAP_LITTLEROOT_TOWN"))
        .unwrap();
    let mut phase = phase_over(MapId("MAP_LITTLEROOT_TOWN"), events);
    phase.player = PlayerState::new((90, 90), FLOOR_ELEVATION, Direction::South);

    idle_frames(&mut phase, 10);

    assert!(
        phase.object_events.iter().all(|entry| !entry.is_active()),
        "no Littleroot placement lies within 9 tiles of (90, 90)"
    );
}

#[test]
fn an_open_start_menu_freezes_the_first_callback_until_it_closes() {
    let temp = crate::flow::tests::TempSave::new("fixed-facing-start-menu");
    let mut save_slot = temp.slot();
    let map = MapId("MAP_OLDALE_TOWN_MART");
    let events = MapEventsTable::new().resolve(map).unwrap();
    let mut phase = phase_over(map, events);
    stand_near(&mut phase, 1);
    // Previous and current tiles differ until the first callback settles them.
    phase
        .object_events
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::West);
    let walked = phase.object_events.get(1).unwrap().state().position();
    phase.start_menu = Some(crate::start_menu::synthetic_start_menu());

    for _ in 0..5 {
        assert!(phase.advance_start_menu_frame(ButtonState::new(), &mut save_slot));
    }
    let state = phase.object_events.get(1).unwrap().state();
    assert_ne!(
        state.previous_position(),
        walked,
        "the menu froze the callback"
    );

    phase.start_menu = None;
    idle_frames(&mut phase, 1);
    let state = phase.object_events.get(1).unwrap().state();
    assert_eq!(state.previous_position(), walked);
}

#[test]
fn a_map_transition_reseeds_unadmitted_authored_facing_and_the_new_visit_is_scheduled() {
    use crate::pack_source::PackSource;

    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-fixed-facing-transition-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    crate::overworld::test_support::write_oldale_layout_pack(
        &path,
        &["girl_3", "mart_employee", "maniac"],
    );
    let leaked_path: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());

    let resolved = oldale::resolve_map_events(OLDALE_TOWN, &EventData::new()).unwrap();
    let events: &'static assets::MapEvents = Box::leak(Box::new(resolved));
    let turn_everyone_south_and_run = |phase: &mut OverworldPhase| {
        stand_near(phase, 3);
        idle_frames(phase, 5);
        for template in events.object_events {
            phase
                .object_events
                .get_mut(template.local_id)
                .unwrap()
                .state_mut()
                .face(Direction::South);
        }
        assert_eq!(
            phase.object_events.get(3).unwrap().state().facing(),
            Direction::South,
            "setup: the footprints man (FACE_LEFT) was turned"
        );
    };
    let assert_reseeded = |phase: &mut OverworldPhase, what: &str| {
        assert_eq!(phase.map_id, OLDALE_TOWN, "{what} completed");
        for template in phase.scene.map_events().object_events {
            let entry = phase.object_events.get(template.local_id).unwrap();
            assert!(
                !entry.is_active(),
                "{what}: local {} is unscheduled",
                template.local_id
            );
            assert_eq!(
                entry.state().facing(),
                authored_facing(template.movement_type),
                "{what}: local {}",
                template.local_id
            );
        }
        stand_near(phase, 3);
        let rng_before = phase.rng.state();
        idle_frames(phase, 5);
        let footprints_man = phase.object_events.get(3).unwrap();
        assert!(
            footprints_man.is_active(),
            "{what}: the new visit schedules"
        );
        assert_eq!(footprints_man.state().facing(), Direction::West, "{what}");
        assert_eq!(phase.rng.state(), rng_before);
    };

    let build = || {
        let mut phase = OverworldPhase::for_test(
            crate::overworld::test_support::synthetic_scene_with_events(
                20,
                20,
                events,
                &["girl_3", "mart_employee", "maniac"],
            ),
            OLDALE_TOWN,
            PlayerState::new((5, 5), FLOOR_ELEVATION, Direction::South),
            None,
        );
        phase.pack_source = PackSource::Test(leaked_path);
        phase
    };

    let mut phase = build();
    turn_everyone_south_and_run(&mut phase);
    phase.warp_to(OLDALE_TOWN, 0);
    assert_reseeded(&mut phase, "the warp");

    let mut phase = build();
    turn_everyone_south_and_run(&mut phase);
    assert!(phase.cross_connection(OLDALE_TOWN, (5, 5), (0, 0)));
    assert_reseeded(&mut phase, "the connection crossing");

    let _ = std::fs::remove_file(&path);
}
