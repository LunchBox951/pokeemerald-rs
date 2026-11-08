//! Random-facing object events (`MOVEMENT_TYPE_LOOK_AROUND`, `MOVEMENT_TYPE_FACE_DOWN_AND_RIGHT`)
//! driven by the production frame: [`OverworldPhase::step`] runs the live collection's
//! scheduler (`MovementType_LookAround`, `event_object_movement.c:2844-2892`;
//! `MovementType_FaceDownAndRight`, `:3434-3481`) after the frame's input, drawing on the
//! phase's field stream.
//!
//! Each placement runs alone in the phase, since the maps hold several random-facing objects
//! that legitimately interleave their draws.

use assets::{MapEventsTable, MapId, MovementType};
use engine::overworld::object_event::ObjectEventCollection;
use engine::overworld::{select_interaction_target, Direction, InteractionTarget, PlayerState};
use engine::rng::Rng;

use platform::Buttons;

use crate::flow::tests::held;

use super::fixed_facing_tests::{idle_frames, phase_over, stand_near, FLOOR_ELEVATION};
use super::OverworldPhase;

const ROUTE_101: MapId = MapId("MAP_ROUTE101");

use MovementType::{FaceDownAndRight as FDR, LookAround as LA};

type Placements = &'static [(u8, MovementType)];

/// The checkpoint maps' random-facing placements (`LOOK_AROUND` in both 2F bedrooms, Route 101 and
/// Route 103; `FACE_DOWN_AND_RIGHT` on Route 103) as
/// `(local_id, movement type)`, read off `data/maps/*/map.json`. The bedrooms' ids 2 to 13 are
/// the decoration placeholders, hidden by `FLAG_DECORATION_*` until placed.
const CHECKPOINT_INVENTORY: [(&str, Placements); 4] = [
    (
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
        &[
            (2, LA),
            (3, LA),
            (4, LA),
            (5, LA),
            (6, LA),
            (7, LA),
            (8, LA),
            (9, LA),
            (10, LA),
            (11, LA),
            (12, LA),
            (13, LA),
            (15, LA),
            (16, LA),
        ],
    ),
    (
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        &[
            (2, LA),
            (3, LA),
            (4, LA),
            (5, LA),
            (6, LA),
            (7, LA),
            (8, LA),
            (9, LA),
            (10, LA),
            (11, LA),
            (12, LA),
            (13, LA),
            (16, LA),
        ],
    ),
    ("MAP_ROUTE101", &[(1, LA), (3, LA), (5, LA)]),
    (
        "MAP_ROUTE103",
        &[(13, LA), (14, LA), (15, LA), (20, LA), (3, FDR)],
    ),
];

/// A phase running `local_id` alone over `map`'s real template, on a field stream seeded with
/// `seed`, with the player standing clear of its row and column.
fn lone_placement(map: &'static str, local_id: u8, seed: u32) -> OverworldPhase {
    let events = MapEventsTable::new().resolve(MapId(map)).unwrap();
    let mut phase = phase_over(MapId(map), events);
    let template = phase.object_events.get(local_id).unwrap().template();
    phase.object_events = ObjectEventCollection::from_templates(std::slice::from_ref(template));
    phase.rng = Rng::new(seed);
    stand_near(&mut phase, local_id);
    phase
}

fn facing(phase: &OverworldPhase, local_id: u8) -> Direction {
    phase.object_events.get(local_id).unwrap().state().facing()
}

/// The stream state after a fresh `seed` generator has made `draws` draws.
fn rng_state_after(seed: u32, draws: usize) -> u32 {
    let mut rng = Rng::new(seed);
    for _ in 0..draws {
        engine::rng::RandomSource::next_u16(&mut rng);
    }
    rng.state()
}

fn assert_never_displaced(phase: &OverworldPhase, local_id: u8, what: &str) {
    let entry = phase.object_events.get(local_id).unwrap();
    let tile = (i32::from(entry.template().x), i32::from(entry.template().y));
    assert!(entry.is_active(), "{what}: admitted");
    assert_eq!(
        entry.state().position(),
        tile,
        "{what}: never leaves its tile"
    );
    assert_eq!(entry.state().previous_position(), tile, "{what}");
    assert_eq!(
        entry.state().elevation(),
        entry.template().elevation,
        "{what}"
    );
    assert!(
        phase.dialog.is_none() && phase.sight_approach.is_none() && !phase.in_battle(),
        "{what}: the loop under test was not preempted"
    );
}

#[test]
fn the_inventory_matches_each_maps_authored_random_facing_placements() {
    for (map, placements) in CHECKPOINT_INVENTORY {
        let events = MapEventsTable::new().resolve(MapId(map)).unwrap();
        let mut authored: Vec<(u8, MovementType)> = events
            .object_events
            .iter()
            .filter(|e| matches!(e.movement_type, LA | FDR))
            .map(|e| (e.local_id, e.movement_type))
            .collect();
        let mut expected = placements.to_vec();
        authored.sort_by_key(|&(id, _)| id);
        expected.sort_by_key(|&(id, _)| id);
        assert_eq!(authored, expected, "{map}");
    }
}

#[test]
fn every_checkpoint_placement_follows_the_seeded_oracle_through_the_production_loop() {
    // Seed 0 draws 0, 59774, 21105, 12720, 36418: delay 32 on frame 1; on frame 33 a facing
    // (`59774 & 3 = 2`, West; `& 1 = 0`, South) and `21105 % 4 = 1` (delay 64 or 48); on the
    // turn after, `12720 & 3 = 0` and `& 1 = 0` are South again.
    for (map, placements) in CHECKPOINT_INVENTORY {
        for &(local_id, movement_type) in placements {
            let what = format!("{map} local {local_id} {movement_type:?}");
            let mut phase = lone_placement(map, local_id, 0);

            idle_frames(&mut phase, 1);
            assert_eq!(phase.rng.state(), rng_state_after(0, 1), "{what}: frame 1");
            assert_eq!(facing(&phase, local_id), Direction::South, "{what}");

            idle_frames(&mut phase, 31);
            assert_eq!(
                phase.rng.state(),
                rng_state_after(0, 1),
                "{what}: frame 32 waits"
            );
            idle_frames(&mut phase, 1);
            assert_eq!(
                phase.rng.state(),
                rng_state_after(0, 3),
                "{what}: frame 33 turns"
            );
            let turned = match movement_type {
                LA => Direction::West,
                _ => Direction::South,
            };
            assert_eq!(facing(&phase, local_id), turned, "{what}");
            assert_never_displaced(&phase, local_id, &what);
        }
    }
}

#[test]
fn a_long_run_only_ever_turns_within_each_types_direction_set_and_never_moves() {
    for (map, placements) in CHECKPOINT_INVENTORY {
        for &(local_id, movement_type) in placements {
            let what = format!("{map} local {local_id} {movement_type:?}");
            let mut phase = lone_placement(map, local_id, 0x1234);
            let mut seen = Vec::new();
            for _ in 0..1200 {
                idle_frames(&mut phase, 1);
                let direction = facing(&phase, local_id);
                if !seen.contains(&direction) {
                    seen.push(direction);
                }
            }
            if movement_type == LA {
                assert!(seen.len() >= 3, "{what}: {seen:?}");
            } else {
                assert!(
                    seen.iter()
                        .all(|d| matches!(d, Direction::South | Direction::East)),
                    "{what}: {seen:?}"
                );
                assert_eq!(seen.len(), 2, "{what}: both of its facings occur");
            }
            assert_never_displaced(&phase, local_id, &what);
        }
    }
}

#[test]
fn a_facing_changed_by_the_scheduler_is_read_back_from_the_one_live_collection() {
    // Route 101's youngster, seed 0x1234: delay 128, then North on frame 129.
    let mut phase = lone_placement("MAP_ROUTE101", 1, 0x1234);
    idle_frames(&mut phase, 128);
    assert_eq!(facing(&phase, 1), Direction::South);
    idle_frames(&mut phase, 1);
    assert_eq!(facing(&phase, 1), Direction::North);
    assert_eq!(phase.rng.state(), rng_state_after(0x1234, 3));
    assert_never_displaced(&phase, 1, "Route 101 youngster");
}

mod rendered {
    use super::*;
    use engine::event_data::EventData;

    const SPRITES: [&str; 1] = ["youngster"];
    const YOUNGSTER: (i32, i32) = (16, 8);

    /// Route 101's real youngster alone (the synthetic scene binds only people sheets, not the
    /// bag's) on a roomy scene, the player one tile south of him looking at him.
    fn route_101_beside_the_youngster(seed: u32) -> OverworldPhase {
        let route_101 = MapEventsTable::new().resolve(ROUTE_101).unwrap();
        let youngster = std::slice::from_ref(route_101.object_events.first().unwrap());
        assert_eq!(youngster[0].local_id, 1);
        let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
            object_events: youngster,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
            ..*route_101
        }));
        let mut phase = OverworldPhase::for_test(
            crate::overworld::test_support::synthetic_scene_with_events(100, 100, events, &SPRITES),
            ROUTE_101,
            PlayerState::new(
                (YOUNGSTER.0, YOUNGSTER.1 + 1),
                FLOOR_ELEVATION,
                Direction::North,
            ),
            None,
        );
        phase.rng = Rng::new(seed);
        phase
    }

    fn selected(phase: &OverworldPhase) -> Option<u8> {
        let runtime = phase
            .scene
            .runtime()
            .with_object_events(&phase.object_events);
        match select_interaction_target(&phase.player, &runtime, &EventData::new(), &[]) {
            Some(InteractionTarget::Object(object)) => Some(object.local_id),
            _ => None,
        }
    }

    #[test]
    fn turns_change_facing_but_leave_the_interaction_target_and_the_composed_frame_alone() {
        let mut phase = route_101_beside_the_youngster(0);
        idle_frames(&mut phase, 1);
        let first_frame = phase.compose_frame();
        assert_eq!(facing(&phase, 1), Direction::South);
        assert_eq!(selected(&phase), Some(1));

        // Seed 0 turns him West on frame 33.
        idle_frames(&mut phase, 32);
        assert_eq!(facing(&phase, 1), Direction::West);
        assert_eq!(
            selected(&phase),
            Some(1),
            "still where the player is facing"
        );
        assert!(
            phase.compose_frame() == first_frame,
            "a turn on the spot repaints the same placement"
        );
        assert_never_displaced(&phase, 1, "Route 101 youngster");
    }

    #[test]
    fn a_scheduler_turn_repaints_the_live_objects_new_facing_frame() {
        let route_101 = MapEventsTable::new().resolve(ROUTE_101).unwrap();
        let youngster = std::slice::from_ref(route_101.object_events.first().unwrap());
        let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
            object_events: youngster,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
            ..*route_101
        }));
        let mut phase = OverworldPhase::for_test(
            crate::overworld::test_support::synthetic_scene_with_events_and_distinct_side_frame(
                100, 100, events, &SPRITES,
            ),
            ROUTE_101,
            PlayerState::new(
                (YOUNGSTER.0, YOUNGSTER.1 + 1),
                FLOOR_ELEVATION,
                Direction::North,
            ),
            None,
        );
        phase.rng = Rng::new(0);
        idle_frames(&mut phase, 1);
        let south_frame = phase.compose_frame();
        assert_eq!(facing(&phase, 1), Direction::South);
        idle_frames(&mut phase, 32);
        assert_eq!(facing(&phase, 1), Direction::West);
        assert!(
            phase.compose_frame() != south_frame,
            "the production frame paints the live object's new facing"
        );
        assert_never_displaced(&phase, 1, "Route 101 youngster");
    }

    #[test]
    fn a_displaced_object_is_not_found_and_does_paint_elsewhere() {
        let mut phase = route_101_beside_the_youngster(0);
        idle_frames(&mut phase, 1);
        let first_frame = phase.compose_frame();

        phase
            .object_events
            .get_mut(1)
            .unwrap()
            .state_mut()
            .walk(Direction::East);
        assert_eq!(selected(&phase), None);
        assert!(
            phase.compose_frame() != first_frame,
            "the control: a one-tile walk is visible in the same frame"
        );
    }
}

mod running_nearby {
    use super::*;

    const DAISY: u8 = 3;
    const FLAG_SYS_B_DASH: u16 = 0x8C0;

    /// Daisy (`MOVEMENT_TYPE_FACE_DOWN_AND_RIGHT`, a Normal trainer of sight range 3, at
    /// (71, 11)) after her first frame, with the shoes and the player four tiles east of her
    /// and two south, off her row and column so no cone reaches.
    fn daisy_waiting() -> OverworldPhase {
        let mut phase = lone_placement("MAP_ROUTE103", DAISY, 0);
        phase.save1.event_data.flag_set(FLAG_SYS_B_DASH).unwrap();
        phase.player = PlayerState::new((75, 13), FLOOR_ELEVATION, Direction::West);
        idle_frames(&mut phase, 1);
        assert_eq!(phase.rng.state(), rng_state_after(0, 1));
        phase
    }

    #[test]
    fn a_run_into_range_turns_her_early_toward_the_player_without_a_facing_draw() {
        let mut phase = daisy_waiting();
        phase.step(held(Buttons::B | Buttons::LEFT));
        assert_eq!(phase.player.position(), (74, 13));
        assert!(phase.player.dashing());

        // Offset (3, 2): the horizontal axis dominates, East. The wake draws only the next delay.
        assert_eq!(facing(&phase, DAISY), Direction::East);
        assert_eq!(phase.rng.state(), rng_state_after(0, 2));
        assert_never_displaced(&phase, DAISY, "Daisy");
    }

    #[test]
    fn a_walk_into_the_same_range_leaves_her_waiting_out_her_delay() {
        let mut phase = daisy_waiting();
        phase.step(held(Buttons::LEFT));
        assert_eq!(phase.player.position(), (74, 13));
        assert!(!phase.player.dashing());

        assert_eq!(facing(&phase, DAISY), Direction::South);
        assert_eq!(phase.rng.state(), rng_state_after(0, 1));
    }

    #[test]
    fn a_run_outside_the_range_square_does_not_wake_her() {
        let mut phase = daisy_waiting();
        phase.player = PlayerState::new((76, 13), FLOOR_ELEVATION, Direction::West);
        phase.step(held(Buttons::B | Buttons::LEFT));
        assert_eq!(phase.player.position(), (75, 13));
        assert!(phase.player.dashing());

        assert_eq!(facing(&phase, DAISY), Direction::South);
        assert_eq!(phase.rng.state(), rng_state_after(0, 1));
    }
}
