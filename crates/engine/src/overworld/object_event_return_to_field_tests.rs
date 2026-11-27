use assets::MovementType;

use crate::overworld::direction::Direction;
use crate::overworld::object_event_harness::{template, Rig, ScriptedRng};

use super::Lifecycle;

/// `MovementType_LookAround_Step2` draws `Random() % 4` into `sMovementDelaysMedium`
/// (`event_object_movement.c:709,2860-2868`): 3 is the 128-frame delay, 0 the 32-frame one.
#[test]
fn a_recreated_sprite_starts_its_movement_type_over_and_draws_a_new_delay() {
    let mut rig = Rig::new(
        vec![template(1, 20, 20, MovementType::LookAround, "0")],
        (20, 20),
    );
    rig.rng = ScriptedRng::new([3, 0]);
    rig.tick(5);
    assert_eq!(rig.rng.draws(), 1);

    rig.collection.recreate_sprites_on_return_to_field();
    let entry = rig.collection.get(1).unwrap();
    assert_eq!(entry.type_func_id(), 0);
    assert!(matches!(
        entry.lifecycle,
        Lifecycle::Active { delay: 0, .. }
    ));

    rig.tick(1);
    assert_eq!(rig.rng.draws(), 2);
    assert!(matches!(
        rig.collection.get(1).unwrap().lifecycle,
        Lifecycle::Active { delay: 32, .. }
    ));
}

/// `ResetObjectEventFldEffData` clears the held movement while `currentCoords` and
/// `previousCoords` stay as the interrupted walk left them (`:1784-1809`).
#[test]
fn a_recreated_sprite_drops_the_walk_in_progress_and_keeps_the_objects_tiles() {
    let mut rig = Rig::new(
        vec![template(1, 20, 20, MovementType::FaceDown, "0")],
        (20, 20),
    );
    rig.tick(1);
    let state = rig.collection.get_mut(1).unwrap().state_mut();
    state.begin_normal_walk(Direction::East);

    rig.collection.recreate_sprites_on_return_to_field();
    let state = *rig.collection.get(1).unwrap().state();
    assert!(state.step().is_none());
    assert_eq!(state.position(), (21, 20));
    assert_eq!(state.previous_position(), (20, 20));
    assert_eq!(state.facing(), Direction::East);
}
