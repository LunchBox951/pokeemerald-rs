use super::*;
use crate::event_data::EventData;
use crate::overworld::direction::Direction;
use crate::overworld::object_event::LiveObjectEvent;
use crate::overworld::object_event_harness::{leaked, template, Rig, ScriptedRng};
use assets::{MapId, MovementType};

const NEVER_HIDDEN: &str = "0";
const HIDDEN_FLAG: &str = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM";
const PLAYER: TilePos = (20, 20);

fn no_carried(_: MapId, _: u8) -> Option<&'static assets::ObjectEvent> {
    None
}

fn rig() -> Rig {
    let mut rig = Rig::new(
        vec![
            template(1, 20, 18, MovementType::LookAround, NEVER_HIDDEN),
            template(2, 22, 20, MovementType::FaceDown, HIDDEN_FLAG),
            template(3, 18, 20, MovementType::FaceUp, NEVER_HIDDEN),
        ],
        PLAYER,
    );
    rig.rng = ScriptedRng::new(std::iter::repeat_n(0, 256));
    rig
}

fn hide_flag_set() -> EventData {
    let mut data = EventData::new();
    data.flag_set(assets::object_event_flags::resolve(HIDDEN_FLAG).unwrap())
        .unwrap();
    data
}

/// A fresh collection over `rig`'s templates, as a continue's loaded scene seeds it.
fn reloaded(rig: &Rig) -> ObjectEventCollection {
    ObjectEventCollection::from_templates(leaked(
        rig.collection
            .iter()
            .map(|entry| *entry.template())
            .collect(),
    ))
}

#[test]
fn only_active_entries_are_saved_with_their_live_state() {
    let mut rig = rig();
    rig.event_data = hide_flag_set();
    rig.tick(1);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::West);
    rig.collection
        .get_mut(3)
        .unwrap()
        .state_mut()
        .walk(Direction::East);

    let saved = rig.collection.saved_objects();

    assert_eq!(
        saved
            .iter()
            .map(|object| object.local_id)
            .collect::<Vec<_>>(),
        [1, 3],
        "the entry its hide flag kept out has no object"
    );
    let first = saved[0].state;
    assert_eq!(first.facing, Direction::West);
    assert_eq!(first.position, (20, 18));
    assert_eq!(first.initial, (20, 18));
    assert_eq!(first.movement_type, MovementType::LookAround);
    let walked = saved[1].state;
    assert_eq!(walked.position, (19, 20));
    assert_eq!(walked.previous_position, (18, 20));
    assert_eq!(
        walked.initial,
        (18, 20),
        "the spawn tile, not the walked-to one"
    );
    assert_eq!((walked.elevation, walked.previous_elevation), (3, 3));
}

#[test]
fn saved_objects_follow_sprite_slot_order() {
    let mut rig = Rig::new(
        vec![
            template(1, 20, 18, MovementType::FaceUp, HIDDEN_FLAG),
            template(2, 22, 20, MovementType::FaceUp, NEVER_HIDDEN),
        ],
        PLAYER,
    );
    rig.event_data = hide_flag_set();
    rig.tick(1);
    // The first template is admitted after the second, so it holds the higher slot.
    rig.event_data = EventData::new();
    rig.player = (20, 21);
    rig.tick(1);
    let ids: Vec<u8> = rig
        .collection
        .saved_objects()
        .iter()
        .map(|o| o.local_id)
        .collect();
    assert_eq!(ids, [2, 1]);
}

#[test]
fn restoring_makes_the_saved_objects_active_with_their_saved_state() {
    let mut rig = rig();
    rig.event_data = hide_flag_set();
    rig.tick(1);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::West);
    rig.collection
        .get_mut(3)
        .unwrap()
        .state_mut()
        .walk(Direction::East);
    let saved = rig.collection.saved_objects();

    let mut resumed = reloaded(&rig);
    assert_eq!(resumed.restore_saved(&saved, PLAYER, no_carried), 2);

    assert_eq!(
        resumed.saved_objects(),
        saved,
        "a second save writes the same objects"
    );
    let look_around = resumed.get(1).unwrap();
    assert!(look_around.is_active());
    assert_eq!(look_around.state().facing(), Direction::West);
    let walked = resumed.get(3).unwrap();
    assert_eq!(walked.state().position(), (19, 20));
    assert_eq!(walked.state().previous_position(), (18, 20));
    assert_eq!(
        walked.state().template_position(),
        (18, 20),
        "a respawn still reads the template"
    );
    assert!(!resumed.get(2).unwrap().is_active());
    assert_eq!(
        resumed.object_events_at(20, 18, 3).count(),
        1,
        "queryable before any frame"
    );
}

#[test]
fn a_restored_object_ignores_a_hide_flag_set_since_but_no_map_load_admission_follows() {
    let mut rig = rig();
    rig.tick(1);
    assert!(rig.collection.get(2).unwrap().is_active());
    let saved = rig.collection.saved_objects();
    assert_eq!(saved.len(), 3);

    let mut resumed = reloaded(&rig);
    resumed.restore_saved(&saved[..2], PLAYER, no_carried);
    let mut resumed = Rig {
        collection: resumed,
        event_data: hide_flag_set(),
        ..rig
    };
    resumed.tick(1);

    assert!(
        resumed.collection.get(2).unwrap().is_active(),
        "the flag gates a spawn, not an object the save held"
    );
    assert!(
        !resumed.collection.get(3).unwrap().is_active(),
        "an object the save did not hold is not freshly admitted by the continue"
    );
}

#[test]
fn an_unsaved_entry_is_admitted_once_the_player_moves() {
    let mut rig = rig();
    rig.tick(1);
    let saved = rig.collection.saved_objects();
    let mut resumed = reloaded(&rig);
    resumed.restore_saved(&saved[..1], PLAYER, no_carried);

    let mut moved = Rig {
        collection: resumed,
        ..rig
    };
    moved.player = (20, 21);
    moved.tick(1);

    assert!(moved.collection.get(3).unwrap().is_active());
}

#[test]
fn an_unknown_or_repeated_id_is_skipped() {
    let rig = rig();
    let mut active = rig;
    active.tick(1);
    let mut saved = active.collection.saved_objects();
    let mut stray = saved[0];
    stray.local_id = 99;
    saved.push(stray);
    saved.push(saved[0]);

    let mut resumed = reloaded(&active);
    assert_eq!(resumed.restore_saved(&saved, PLAYER, no_carried), 3);
}

const DEPARTED: MapId = MapId("MAP_ROUTE101");

/// `rig`'s active entries after a crossing from `DEPARTED` into Oldale.
fn crossed(rig: &Rig) -> ObjectEventCollection {
    rig.collection.carry_across_connection(
        DEPARTED,
        MapId("MAP_OLDALE_TOWN"),
        leaked(vec![template(9, 5, 5, MovementType::FaceUp, NEVER_HIDDEN)]),
        (0, 20),
    )
}

#[test]
fn an_active_entry_carried_from_another_map_is_saved() {
    let mut rig = rig();
    rig.tick(1);
    let carried = crossed(&rig);
    assert!(carried.find(Some(DEPARTED), 1).unwrap().is_active());
    let saved = carried.saved_objects();
    let youngster = saved.iter().find(|o| o.local_id == 1).expect("carried");
    assert_eq!(youngster.origin, Some(DEPARTED));
    assert_eq!(
        youngster.state.position,
        (20, 38),
        "in the entered map's coordinates"
    );
    assert!(
        saved.iter().all(|o| o.local_id != 9),
        "the entered map's unadmitted template has no object"
    );
}

#[test]
fn a_carried_entry_is_restored_beside_the_maps_own_with_its_origin() {
    let mut rig = rig();
    rig.tick(1);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::West);
    let carried = crossed(&rig);
    let saved = carried.saved_objects();
    assert_eq!(saved.len(), 3);

    let entered = leaked(vec![template(9, 5, 5, MovementType::FaceUp, NEVER_HIDDEN)]);
    let mut resumed = ObjectEventCollection::from_templates(entered);
    let origin_templates: Vec<_> = rig
        .collection
        .iter()
        .map(LiveObjectEvent::template)
        .collect();
    let restored = resumed.restore_saved(&saved, PLAYER, |map, id| {
        assert_eq!(map, DEPARTED);
        origin_templates.iter().copied().find(|t| t.local_id == id)
    });

    assert_eq!(restored, 3);
    assert_eq!(
        resumed.saved_objects(),
        saved,
        "a second save writes the same objects"
    );
    let youngster = resumed.find(Some(DEPARTED), 1).unwrap();
    assert!(youngster.is_active());
    assert_eq!(youngster.state().facing(), Direction::West);
    assert_eq!(youngster.state().position(), (20, 38));
    assert!(
        !resumed.get(9).unwrap().is_active(),
        "the entered map's own template is not admitted by the restore"
    );
    assert_eq!(
        resumed
            .iter()
            .map(LiveObjectEvent::carried_from)
            .collect::<Vec<_>>(),
        [None, Some(DEPARTED), Some(DEPARTED), Some(DEPARTED)],
        "the map's own entries first, then the carried ones, as a crossing leaves them"
    );
    let ids: std::collections::HashSet<u32> = resumed.iter().map(LiveObjectEvent::id).collect();
    assert_eq!(ids.len(), 4, "every entry has its own identity");
}

#[test]
fn a_carried_object_the_origin_cannot_name_is_skipped_and_counted_out() {
    let mut rig = rig();
    rig.tick(1);
    let saved = crossed(&rig).saved_objects();
    let mut resumed = ObjectEventCollection::from_templates(leaked(vec![]));
    assert_eq!(resumed.restore_saved(&saved, PLAYER, no_carried), 0);
    assert!(resumed.saved_objects().is_empty());
}

#[test]
fn a_restored_look_around_object_keeps_its_facing_through_its_next_frames() {
    let mut rig = rig();
    rig.tick(1);
    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .face(Direction::West);
    let saved = rig.collection.saved_objects();

    let mut resumed = Rig {
        collection: reloaded(&rig),
        ..rig
    };
    resumed.collection.restore_saved(&saved, PLAYER, no_carried);
    // Frame 1 of a restarted callback chooses a delay, as a fresh admission's does.
    resumed.rng = ScriptedRng::new([0]);
    resumed.tick(1);

    assert_eq!(
        resumed.collection.get(1).unwrap().state().facing(),
        Direction::West
    );
}
