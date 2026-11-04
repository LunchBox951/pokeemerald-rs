use assets::MovementType;

use crate::overworld::direction::Direction;
use crate::overworld::object_event_harness::{template, Rig};

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
    rig.tick(1);
    assert!(rig.collection.get(1).unwrap().is_active());
    rig.tick(1);
    assert_eq!(phase(&rig, 1), 2);
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
