use assets::MovementType;

use crate::overworld::direction::Direction;
use crate::overworld::object_event_harness::{template, Rig, ScriptedRng};

use super::Lifecycle;

const FIXED_FACING: [(MovementType, Direction); 4] = [
    (MovementType::FaceUp, Direction::North),
    (MovementType::FaceDown, Direction::South),
    (MovementType::FaceLeft, Direction::West),
    (MovementType::FaceRight, Direction::East),
];

const NEVER_HIDDEN: &str = "0";
const HIDDEN_FLAG: &str = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM";

fn rig_with(movement_type: MovementType) -> Rig {
    Rig::new(
        vec![template(1, 20, 20, movement_type, NEVER_HIDDEN)],
        (20, 20),
    )
}

fn phase(rig: &Rig, local_id: u8) -> u8 {
    rig.collection.get(local_id).unwrap().type_func_id()
}

#[test]
fn a_seeded_fixed_facing_entry_starts_inactive_at_its_authored_tile_and_facing() {
    for (movement_type, facing) in FIXED_FACING {
        let rig = rig_with(movement_type);
        let entry = rig.collection.get(1).unwrap();

        assert!(!entry.is_active(), "{movement_type:?}");
        assert_eq!(entry.state().facing(), facing, "{movement_type:?}");
        assert_eq!(entry.state().position(), (20, 20));
        assert_eq!(entry.state().previous_position(), (20, 20));
        assert_eq!(entry.state().movement_type(), movement_type);
    }
}

#[test]
fn the_first_frame_admits_and_runs_all_three_face_steps_then_idles() {
    for (movement_type, facing) in FIXED_FACING {
        let mut rig = rig_with(movement_type);
        rig.tick(1);

        let entry = rig.collection.get(1).unwrap();
        assert!(entry.is_active(), "{movement_type:?}");
        assert_eq!(entry.type_func_id(), 2, "{movement_type:?}");
        assert_eq!(entry.state().facing(), facing);
    }
}

#[test]
fn idle_frames_hold_facing_position_and_phase_and_draw_no_random_numbers() {
    for (movement_type, facing) in FIXED_FACING {
        let mut rig = rig_with(movement_type);
        rig.tick(1);
        let after_first = *rig.collection.get(1).unwrap().state();

        rig.tick(120);

        let entry = rig.collection.get(1).unwrap();
        assert_eq!(*entry.state(), after_first, "{movement_type:?}");
        assert_eq!(entry.state().facing(), facing);
        assert_eq!(entry.type_func_id(), 2);
        assert!(rig.rng.is_untouched(), "{movement_type:?} drew a number");
        assert_eq!(rig.rng.draws(), 0);
    }
}

#[test]
fn the_first_callback_settles_the_previous_tile_onto_the_current_one() {
    let mut rig = rig_with(MovementType::FaceUp);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::East);
    assert_eq!(
        rig.collection.get(1).unwrap().state().previous_position(),
        (20, 20),
        "fixture precondition: previous and current tiles differ"
    );

    rig.tick(1);

    let state = rig.collection.get(1).unwrap().state();
    assert_eq!(state.position(), (21, 20));
    assert_eq!(state.previous_position(), (21, 20));
}

#[test]
fn a_script_set_facing_persists_through_and_after_the_first_callback() {
    let mut rig = rig_with(MovementType::FaceUp);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::East);

    rig.tick(1);
    assert_eq!(
        rig.collection.get(1).unwrap().state().facing(),
        Direction::East,
        "the face action re-faces the current direction, not the authored one"
    );

    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::West);
    rig.tick(30);
    assert_eq!(
        rig.collection.get(1).unwrap().state().facing(),
        Direction::West
    );
}

#[test]
fn a_frozen_frame_skips_the_callback_but_still_admits_and_the_unfrozen_frame_runs_it() {
    let mut rig = rig_with(MovementType::FaceDown);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::East);
    rig.frozen = true;

    rig.tick(5);
    let entry = rig.collection.get(1).unwrap();
    assert!(entry.is_active());
    assert_eq!(entry.type_func_id(), 0);
    assert_eq!(entry.state().previous_position(), (20, 20));

    rig.frozen = false;
    rig.tick(1);
    let entry = rig.collection.get(1).unwrap();
    assert_eq!(entry.type_func_id(), 2);
    assert_eq!(entry.state().previous_position(), (21, 20));
}

#[test]
fn the_spawn_window_is_inclusive_at_each_edge_of_the_asymmetric_rectangle() {
    // Player (20, 20): x in 11..=30, y in 13..=29 (`TrySpawnObjectEvents`, `:1645-1675`).
    let edges = [
        (11, 20, true),
        (10, 20, false),
        (30, 20, true),
        (31, 20, false),
        (20, 13, true),
        (20, 12, false),
        (20, 29, true),
        (20, 30, false),
    ];
    for (x, y, admitted) in edges {
        let mut rig = Rig::new(
            vec![template(1, x, y, MovementType::FaceUp, NEVER_HIDDEN)],
            (20, 20),
        );
        rig.tick(1);
        assert_eq!(
            rig.collection.get(1).unwrap().is_active(),
            admitted,
            "tile ({x}, {y})"
        );
    }
}

#[test]
fn a_set_hide_flag_refuses_admission_until_it_clears() {
    let mut rig = Rig::new(
        vec![template(1, 20, 20, MovementType::FaceUp, HIDDEN_FLAG)],
        (20, 20),
    );
    let flag = assets::object_event_flags::resolve(HIDDEN_FLAG).unwrap();
    rig.event_data.flag_set(flag).unwrap();

    rig.tick(10);
    assert!(!rig.collection.get(1).unwrap().is_active());
    assert_eq!(phase(&rig, 1), 0);

    rig.event_data.flag_clear(flag).unwrap();
    rig.player = (21, 20);
    rig.tick(1);
    assert!(rig.collection.get(1).unwrap().is_active());
    rig.tick(1);
    assert_eq!(phase(&rig, 1), 2);
}

#[test]
fn clearing_a_hide_flag_while_the_camera_is_stationary_waits_for_the_next_camera_update() {
    let mut rig = Rig::new(
        vec![template(1, 20, 20, MovementType::FaceUp, HIDDEN_FLAG)],
        (20, 20),
    );
    let flag = assets::object_event_flags::resolve(HIDDEN_FLAG).unwrap();
    rig.event_data.flag_set(flag).unwrap();
    rig.tick(1);
    assert!(!rig.collection.get(1).unwrap().is_active());

    // `TrySpawnObjectEvents` runs only on a camera tile delta (`field_camera.c:413-416`).
    rig.event_data.flag_clear(flag).unwrap();
    rig.tick(10);
    assert!(
        !rig.collection.get(1).unwrap().is_active(),
        "admitted with no camera update"
    );

    rig.player = (21, 20);
    rig.tick(1);
    assert!(rig.collection.get(1).unwrap().is_active());
}

#[test]
fn setting_a_hide_flag_does_not_retire_an_already_active_entry() {
    let mut rig = Rig::new(
        vec![template(1, 20, 20, MovementType::FaceUp, HIDDEN_FLAG)],
        (20, 20),
    );
    rig.tick(1);
    let flag = assets::object_event_flags::resolve(HIDDEN_FLAG).unwrap();
    rig.event_data.flag_set(flag).unwrap();

    rig.tick(10);

    assert!(
        rig.collection.get(1).unwrap().is_active(),
        "upstream only consults the flag at spawn (`:1670`); removal is a script's job"
    );
}

#[test]
fn an_entry_the_camera_pass_admits_runs_its_first_callback_on_the_next_frame() {
    let mut rig = Rig::new(
        vec![template(1, 40, 20, MovementType::FaceLeft, NEVER_HIDDEN)],
        (20, 20),
    );
    rig.tick(3);
    assert!(!rig.collection.get(1).unwrap().is_active());

    rig.player = (30, 20);
    rig.tick(1);
    assert!(rig.collection.get(1).unwrap().is_active());
    assert_eq!(phase(&rig, 1), 0, "callbacks run before the camera update");

    rig.tick(1);
    assert_eq!(phase(&rig, 1), 2);
}

#[test]
fn an_entry_leaving_the_view_is_retired_and_respawns_with_its_authored_facing() {
    let mut rig = rig_with(MovementType::FaceUp);
    rig.tick(1);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::East);

    rig.player = (200, 200);
    rig.tick(1);
    assert!(!rig.collection.get(1).unwrap().is_active());

    rig.player = (20, 20);
    rig.tick(1);
    let entry = rig.collection.get(1).unwrap();
    assert!(entry.is_active());
    assert_eq!(entry.state().facing(), Direction::North);
    assert_eq!(entry.type_func_id(), 0, "a respawn restarts the callback");
    rig.tick(1);
    assert_eq!(phase(&rig, 1), 2);
    assert!(rig.rng.is_untouched());
}

#[test]
fn a_respawn_restores_the_template_snapshots_not_the_authored_template() {
    let mut rig = rig_with(MovementType::FaceUp);
    rig.tick(1);
    {
        let state = rig.collection.get_mut(1).unwrap().state_mut();
        state.walk(Direction::East);
        state.override_template_coords();
        state.override_template_movement_type(MovementType::FaceLeft);
    }
    rig.player = (200, 200);
    rig.tick(1);
    rig.player = (20, 20);
    rig.tick(1);

    let state = rig.collection.get(1).unwrap().state();
    assert_eq!(state.position(), (21, 20));
    assert_eq!(state.previous_position(), (21, 20));
    assert_eq!(state.movement_type(), MovementType::FaceLeft);
    assert_eq!(state.facing(), Direction::West);
}

#[test]
fn an_entry_stays_active_while_either_its_current_or_its_initial_tile_is_in_view() {
    let mut rig = rig_with(MovementType::FaceUp);
    rig.tick(1);
    for _ in 0..20 {
        rig.collection
            .get_mut(1)
            .unwrap()
            .state_mut()
            .walk(Direction::East);
    }

    // Current tile (40, 20) is out of view, initial (20, 20) is in.
    rig.tick(1);
    assert!(rig.collection.get(1).unwrap().is_active());

    // Player (40, 20): current is in view, initial is out.
    rig.player = (40, 20);
    rig.tick(1);
    assert!(rig.collection.get(1).unwrap().is_active());

    // Both out.
    rig.player = (100, 100);
    rig.tick(1);
    assert!(!rig.collection.get(1).unwrap().is_active());
}

#[test]
fn an_object_walked_out_of_view_survives_on_its_initial_tile_then_is_removed_and_respawns_at_its_template(
) {
    let mut rig = rig_with(MovementType::FaceUp);
    rig.tick(1);
    for _ in 0..20 {
        rig.collection
            .get_mut(1)
            .unwrap()
            .state_mut()
            .walk(Direction::East);
    }

    // The current tile (40, 20) is outside the window, the initial tile (20, 20) inside; the
    // player steps so the camera pass runs.
    rig.player = (20, 21);
    rig.tick(1);
    assert!(rig.collection.get(1).unwrap().is_active());

    // Both tiles outside.
    rig.player = (100, 100);
    rig.tick(1);
    assert!(!rig.collection.get(1).unwrap().is_active());

    // The window returns over the template tile; the walk is forgotten.
    rig.player = (20, 20);
    rig.tick(1);
    let entry = rig.collection.get(1).unwrap();
    assert!(entry.is_active());
    assert_eq!(entry.state().position(), (20, 20));
    assert_eq!(entry.state().previous_position(), (20, 20));
    assert_eq!(entry.state().facing(), Direction::North);
}

#[test]
fn movement_types_without_a_callback_yet_stay_inert() {
    let mut rig = rig_with(MovementType::WanderAround);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::East);

    rig.tick(30);

    let entry = rig.collection.get(1).unwrap();
    assert!(entry.is_active());
    assert_eq!(entry.type_func_id(), 0);
    assert_eq!(entry.state().position(), (21, 20));
    assert_eq!(entry.state().previous_position(), (20, 20));
    assert!(rig.rng.is_untouched());
}

#[test]
fn a_freshly_seeded_collection_forgets_every_prior_visit() {
    let templates = vec![template(1, 20, 20, MovementType::FaceUp, NEVER_HIDDEN)];
    let mut visit = Rig::new(templates.clone(), (20, 20));
    visit.tick(1);
    visit
        .collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::South);

    let revisit = Rig::new(templates, (20, 20));

    let entry = revisit.collection.get(1).unwrap();
    assert!(!entry.is_active());
    assert_eq!(entry.state().facing(), Direction::North);
}

/// Upstream has `OBJECT_EVENTS_COUNT` (16) object-event slots, one held by the player
/// (`overworld.c:2173-2176`), and `GetAvailableObjectEventId` refuses admission when all are
/// active (`event_object_movement.c:1367-1374`), so at most 15 NPCs can be active.
#[test]
fn admission_never_exceeds_the_fifteen_npc_slots_left_beside_the_player() {
    let templates = (1..=16u8)
        .map(|id| {
            template(
                id,
                10 + i16::from(id),
                20,
                MovementType::FaceUp,
                NEVER_HIDDEN,
            )
        })
        .collect();
    let mut rig = Rig::new(templates, (18, 20));
    rig.tick(1);

    let active = (1..=16u8)
        .filter(|&id| rig.collection.get(id).unwrap().is_active())
        .count();
    assert!(
        active <= 15,
        "{active} NPCs active; upstream admits at most 15"
    );
}

/// Fixed-facing NPCs `1..=count` on the player's tile, so all are in view.
fn crowd(count: u8) -> Vec<assets::ObjectEvent> {
    (1..=count)
        .map(|id| template(id, 20, 20, MovementType::FaceUp, NEVER_HIDDEN))
        .collect()
}

fn active_count(rig: &Rig) -> usize {
    rig.collection
        .entries()
        .filter(|entry| entry.is_active())
        .count()
}

fn object_slot(rig: &Rig, local_id: u8) -> Option<usize> {
    rig.collection.get(local_id).unwrap().object_slot()
}

fn sprite_slot(rig: &Rig, local_id: u8) -> Option<usize> {
    rig.collection
        .get(local_id)
        .unwrap()
        .lifecycle
        .sprite_slot()
}

fn is_unadmitted(rig: &Rig, local_id: u8) -> bool {
    rig.collection.get(local_id).unwrap().lifecycle == Lifecycle::Unadmitted
}

/// Fifteen fixed-facing NPCs fill every object slot the player leaves
/// (`global.h:46`, `field_player_avatar.c:1399-1401`), then `overflow` as local id 16, which
/// first walks east so any callback or reinitialisation it later ran would show.
fn full_pool_with_overflow(overflow: MovementType) -> Rig {
    let mut templates = crowd(15);
    templates.push(template(16, 20, 20, overflow, NEVER_HIDDEN));
    let mut rig = Rig::new(templates, (20, 20));
    rig.collection
        .get_mut(16)
        .unwrap()
        .state_mut()
        .walk(Direction::East);
    rig
}

const RANDOM_FACING: [(MovementType, i16); 2] = [
    // The first delay draw of each type, for `Random() = 1`
    // (`MovementType_LookAround_Step2`, `MovementType_FaceDownAndRight_Step2`,
    // `event_object_movement.c:2844-2892,3434-3481`).
    (MovementType::LookAround, 64),
    (MovementType::FaceDownAndRight, 48),
];

#[test]
fn a_full_pool_gives_each_of_fifteen_npcs_the_lowest_free_object_slot() {
    let mut rig = Rig::new(crowd(15), (20, 20));

    assert!(rig
        .collection
        .admit_on_map_load(rig.player, &rig.event_data));
    assert_eq!(active_count(&rig), 15);
    assert!(!rig
        .collection
        .admit_on_map_load(rig.player, &rig.event_data));
    rig.tick(1);

    for id in 1..=15u8 {
        // `GetAvailableObjectEventId` skips slot 0, the player's (`:1367-1374`).
        assert_eq!(object_slot(&rig, id), Some(usize::from(id)), "{id}");
        assert_eq!(sprite_slot(&rig, id), Some(usize::from(id) - 1), "{id}");
        assert_eq!(phase(&rig, id), 2, "{id}");
    }
    assert!(rig.rng.is_untouched());
}

#[test]
fn a_refused_template_runs_no_callback_and_spends_no_random_draw() {
    for (overflow, _) in RANDOM_FACING {
        let mut rig = full_pool_with_overflow(overflow);
        let before = rig.collection.get(16).unwrap().state().previous_position();
        assert_eq!(before, (20, 20));

        // An exhausted RNG panics on any draw.
        rig.tick(1);
        rig.tick(40);

        assert_eq!(active_count(&rig), 15, "{overflow:?}");
        assert!(is_unadmitted(&rig, 16), "{overflow:?}");
        assert_eq!(phase(&rig, 16), 0, "{overflow:?}");
        let state = rig.collection.get(16).unwrap().state();
        assert_eq!(state.position(), (21, 20), "{overflow:?}");
        assert_eq!(state.previous_position(), before, "{overflow:?}");
        assert!(rig.rng.is_untouched(), "{overflow:?}");
    }
}

#[test]
fn a_refused_template_is_admitted_on_the_camera_pass_after_a_removal() {
    for (overflow, expected_delay) in RANDOM_FACING {
        let mut templates = crowd(15);
        templates[0].flag = HIDDEN_FLAG;
        templates.push(template(16, 20, 20, overflow, NEVER_HIDDEN));
        let mut rig = Rig::new(templates, (20, 20));
        rig.tick(1);
        assert!(is_unadmitted(&rig, 16));

        // The script verb removes the object and sets its hide flag (`:1383-1397`), so the
        // earlier template does not reclaim the slot.
        assert!(rig.collection.remove_by_local_id(1));
        let flag = assets::object_event_flags::resolve(HIDDEN_FLAG).unwrap();
        rig.event_data.flag_set(flag).unwrap();

        // Admission is a camera pass; a still camera offers the freed slot to no one.
        rig.tick(5);
        assert_eq!(active_count(&rig), 14, "{overflow:?}");
        assert!(is_unadmitted(&rig, 16), "{overflow:?}");

        rig.player = (21, 20);
        rig.tick(1);
        assert_eq!(active_count(&rig), 15, "{overflow:?}");
        assert_eq!(object_slot(&rig, 16), Some(1), "{overflow:?}");
        assert_eq!(sprite_slot(&rig, 16), Some(0), "{overflow:?}");
        assert_eq!(
            phase(&rig, 16),
            0,
            "admission runs no callback, {overflow:?}"
        );
        assert!(rig.rng.is_untouched(), "{overflow:?}");

        rig.rng = ScriptedRng::new([1]);
        rig.tick(1);
        assert_eq!(phase(&rig, 16), 3, "{overflow:?}");
        assert_eq!(rig.rng.draws(), 1, "{overflow:?}");
        let Lifecycle::Active { delay, .. } = rig.collection.get(16).unwrap().lifecycle else {
            panic!("{overflow:?} is not active");
        };
        assert_eq!(delay, expected_delay, "{overflow:?}");
    }
}

#[test]
fn an_object_leaving_the_view_frees_its_slot_for_a_later_pass_not_its_own() {
    // Upstream spawns before it removes (`:1645-1675`, `:1677-1710`), so the slot an outgoing
    // object frees is not seen by the pass that removes it.
    let mut templates = vec![template(1, 11, 20, MovementType::FaceUp, NEVER_HIDDEN)];
    templates.extend(crowd(15).into_iter().skip(1));
    templates.push(template(16, 20, 20, MovementType::LookAround, NEVER_HIDDEN));
    let mut rig = Rig::new(templates, (20, 20));
    rig.tick(1);
    assert_eq!(active_count(&rig), 15);
    assert!(is_unadmitted(&rig, 16));

    // Object 1 is 10 columns left of the player: out of the spawn window (`:1645-1675`).
    rig.player = (21, 20);
    rig.tick(1);
    assert_eq!(active_count(&rig), 14);
    assert!(is_unadmitted(&rig, 16));
    rig.tick(5);
    assert!(is_unadmitted(&rig, 16));

    rig.player = (22, 20);
    rig.tick(1);
    assert_eq!(active_count(&rig), 15);
    assert_eq!(object_slot(&rig, 16), Some(1));
    assert_eq!(sprite_slot(&rig, 16), Some(0));
    assert_eq!(phase(&rig, 16), 0);
    assert!(rig.rng.is_untouched());
}

#[test]
fn an_admitted_object_keeps_its_object_slot_while_a_later_one_takes_the_gap() {
    let mut templates = crowd(3);
    templates[1].flag = HIDDEN_FLAG;
    let mut rig = Rig::new(templates, (20, 20));
    let flag = assets::object_event_flags::resolve(HIDDEN_FLAG).unwrap();
    rig.event_data.flag_set(flag).unwrap();
    rig.tick(1);
    assert_eq!(object_slot(&rig, 1), Some(1));
    assert_eq!(object_slot(&rig, 2), None);
    assert_eq!(object_slot(&rig, 3), Some(2));

    rig.event_data.flag_clear(flag).unwrap();
    rig.player = (21, 20);
    rig.tick(1);
    assert_eq!(object_slot(&rig, 1), Some(1));
    assert_eq!(object_slot(&rig, 2), Some(3));
    assert_eq!(object_slot(&rig, 3), Some(2));
}
