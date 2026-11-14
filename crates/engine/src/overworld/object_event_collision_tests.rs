use assets::MovementType;

use super::*;
use crate::overworld::collision::{ELEVATION_MULTI_LEVEL, ELEVATION_TRANSITION};
use crate::overworld::metatile_behavior::{MB_IMPASSABLE_NORTH, MB_IMPASSABLE_SOUTH};
use crate::overworld::object_event::{
    blocking_object_event_at, visible_object_event_at, ObjectEventCollection, ObjectEventState,
    NORMAL_STEP_FRAMES,
};
use crate::overworld::object_event_harness::{template, Rig, Terrain, FLOOR_ELEVATION};

use Direction::{East, North, South, West};

const NEVER_HIDDEN: &str = "0";
const HIDDEN_FLAG: &str = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM";

fn mover_at(x: i16, y: i16) -> ObjectEventState {
    ObjectEventState::from_template(&template(1, x, y, MovementType::FaceDown, NEVER_HIDDEN))
}

fn occupant(current: TilePos, previous: TilePos) -> Occupant {
    Occupant {
        current,
        previous,
        elevation: FLOOR_ELEVATION,
    }
}

fn step(terrain: &Terrain, mover: &ObjectEventState, direction: Direction) -> Option<NpcCollision> {
    collision_in_direction(&terrain.runtime(), mover, None, direction, [])
}

#[test]
fn open_ground_refuses_nothing() {
    let terrain = Terrain::flat();
    for direction in [North, South, West, East] {
        assert_eq!(step(&terrain, &mover_at(20, 20), direction), None);
    }
}

#[test]
fn the_range_is_inclusive_and_anchored_to_the_initial_tile() {
    let terrain = Terrain::flat();
    let range = MovementRange {
        initial: (20, 20),
        x: 2,
        y: 1,
    };
    let refused = |from: (i16, i16), direction| {
        collision_in_direction(
            &terrain.runtime(),
            &mover_at(from.0, from.1),
            Some(range),
            direction,
            [],
        )
    };

    assert_eq!(refused((21, 20), East), None, "the far edge is reachable");
    assert_eq!(refused((22, 20), East), Some(NpcCollision::OutsideRange));
    assert_eq!(refused((18, 20), West), Some(NpcCollision::OutsideRange));
    assert_eq!(refused((19, 20), West), None);
    assert_eq!(refused((20, 21), South), Some(NpcCollision::OutsideRange));
    assert_eq!(refused((20, 20), South), None);
    assert_eq!(refused((20, 19), North), Some(NpcCollision::OutsideRange));
    assert_eq!(
        refused((22, 21), North),
        None,
        "the range follows the initial tile, not the tile stepped from"
    );
}

#[test]
fn a_zero_range_leaves_its_axis_unbounded() {
    let terrain = Terrain::flat();
    let vertical_only = MovementRange {
        initial: (20, 20),
        x: 0,
        y: 1,
    };
    let from = |x, y, direction| {
        collision_in_direction(
            &terrain.runtime(),
            &mover_at(x, y),
            Some(vertical_only),
            direction,
            [],
        )
    };

    assert_eq!(from(40, 20, East), None, "x is unbounded");
    assert_eq!(from(40, 21, South), Some(NpcCollision::OutsideRange));
}

#[test]
fn range_distance_never_overflows_at_extreme_coordinates() {
    let range = |initial, x, y| MovementRange { initial, x, y };

    assert!(!range((i32::MIN, 0), 15, 0).contains((20, 0)));
    assert!(!range((i32::MAX, 0), 15, 0).contains((-20, 0)));
    assert!(!range((0, i32::MIN), 0, 15).contains((0, i32::MAX)));
    assert!(!range((0, i32::MAX), 0, 15).contains((0, i32::MIN)));
    assert!(range((i32::MIN, 0), 1, 0).contains((i32::MIN + 1, 0)));
    assert!(!range((i32::MIN, 0), 1, 0).contains((i32::MIN + 2, 0)));
    assert!(range((i32::MAX, 0), u8::MAX, 0).contains((i32::MAX - 255, 0)));
    assert!(!range((i32::MAX, 0), u8::MAX, 0).contains((i32::MAX - 256, 0)));
    assert!(range((i32::MIN, i32::MAX), 0, 0).contains((i32::MAX, i32::MIN)));
}

#[test]
fn a_range_anchored_at_an_extreme_tile_refuses_a_distant_step() {
    let terrain = Terrain::flat();
    let anchored = |initial| MovementRange {
        initial,
        x: 2,
        y: 2,
    };
    for initial in [(i32::MIN, 20), (20, i32::MIN), (i32::MAX, 20), (20, i32::MAX)] {
        assert_eq!(
            collision_in_direction(
                &terrain.runtime(),
                &mover_at(20, 20),
                Some(anchored(initial)),
                East,
                [],
            ),
            Some(NpcCollision::OutsideRange),
            "{initial:?}"
        );
    }
}

#[test]
fn a_solid_cell_or_a_tile_off_the_map_is_impassable() {
    let mut terrain = Terrain::flat();
    terrain.set_wall((21, 20));

    assert_eq!(
        step(&terrain, &mover_at(20, 20), East),
        Some(NpcCollision::Impassable)
    );
    assert_eq!(
        step(&terrain, &mover_at(0, 5), West),
        Some(NpcCollision::Impassable)
    );
    let last = i16::try_from(crate::overworld::object_event_harness::TERRAIN_SIDE - 1).unwrap();
    assert_eq!(
        step(&terrain, &mover_at(last, 5), East),
        Some(NpcCollision::Impassable)
    );
    assert_eq!(
        step(&terrain, &mover_at(5, last), South),
        Some(NpcCollision::Impassable)
    );
}

#[test]
fn a_closed_directional_edge_blocks_both_exit_and_entry() {
    let mut terrain = Terrain::flat();
    terrain.set_behavior((20, 20), MB_IMPASSABLE_SOUTH);
    terrain.set_behavior((30, 31), MB_IMPASSABLE_NORTH);

    assert_eq!(
        step(&terrain, &mover_at(20, 20), South),
        Some(NpcCollision::Impassable),
        "the standing tile closes its south edge"
    );
    assert_eq!(
        step(&terrain, &mover_at(20, 20), North),
        None,
        "the north edge is open"
    );
    assert_eq!(
        step(&terrain, &mover_at(30, 30), South),
        Some(NpcCollision::Impassable),
        "the target tile closes its north edge"
    );
    assert_eq!(step(&terrain, &mover_at(31, 31), West), None);
}

#[test]
fn a_cell_of_another_elevation_is_a_mismatch_unless_it_is_a_wildcard() {
    let mut terrain = Terrain::flat();
    terrain.set_elevation((21, 20), FLOOR_ELEVATION + 1);
    terrain.set_elevation((19, 20), ELEVATION_TRANSITION);
    terrain.set_elevation((20, 19), ELEVATION_MULTI_LEVEL);

    let mover = mover_at(20, 20);
    assert_eq!(
        step(&terrain, &mover, East),
        Some(NpcCollision::ElevationMismatch)
    );
    assert_eq!(step(&terrain, &mover, West), None);
    assert_eq!(step(&terrain, &mover, North), None);
}

#[test]
fn the_refusals_take_upstreams_precedence() {
    let mut terrain = Terrain::flat();
    terrain.set_wall((21, 20));
    terrain.set_elevation((21, 20), FLOOR_ELEVATION + 1);
    terrain.set_elevation((20, 21), FLOOR_ELEVATION + 1);
    let mover = mover_at(20, 20);
    let blocker = [occupant((21, 20), (21, 20)), occupant((20, 21), (20, 21))];

    assert_eq!(
        collision_in_direction(
            &terrain.runtime(),
            &mover,
            Some(MovementRange {
                initial: (10, 20),
                x: 1,
                y: 0,
            }),
            East,
            blocker,
        ),
        Some(NpcCollision::OutsideRange),
        "range outranks a wall"
    );
    assert_eq!(
        collision_in_direction(&terrain.runtime(), &mover, None, East, blocker),
        Some(NpcCollision::Impassable),
        "a wall outranks an elevation mismatch and an object"
    );
    assert_eq!(
        collision_in_direction(&terrain.runtime(), &mover, None, South, blocker),
        Some(NpcCollision::ElevationMismatch),
        "an elevation mismatch outranks an object"
    );
}

#[test]
fn an_occupant_blocks_the_tile_it_stands_on_and_the_tile_it_is_leaving() {
    let terrain = Terrain::flat();
    let mover = mover_at(20, 20);
    let blocked = |occupant| {
        collision_in_direction(&terrain.runtime(), &mover, None, East, [occupant])
            == Some(NpcCollision::ObjectEvent)
    };

    assert!(blocked(occupant((21, 20), (21, 20))), "standing on it");
    assert!(blocked(occupant((21, 20), (22, 20))), "arrived on it");
    assert!(blocked(occupant((22, 20), (21, 20))), "still leaving it");
    assert!(!blocked(occupant((22, 20), (22, 20))), "settled elsewhere");
    assert!(!blocked(occupant((21, 21), (20, 21))), "nowhere near");
}

#[test]
fn an_occupant_blocks_by_object_elevation_not_by_cell_elevation() {
    let terrain = Terrain::flat();
    let mover = mover_at(20, 20);
    let at = |elevation| {
        collision_in_direction(
            &terrain.runtime(),
            &mover,
            None,
            East,
            [Occupant {
                elevation,
                ..occupant((21, 20), (21, 20))
            }],
        )
    };

    assert_eq!(at(FLOOR_ELEVATION), Some(NpcCollision::ObjectEvent));
    assert_eq!(at(ELEVATION_TRANSITION), Some(NpcCollision::ObjectEvent));
    assert_eq!(at(FLOOR_ELEVATION + 1), None);
    assert_eq!(
        at(ELEVATION_MULTI_LEVEL),
        None,
        "multi-level is a wildcard for cells, not for objects"
    );
}

/// Close enough to admit objects around (20, 20), clear of the tiles the tests step across.
const PLAYER_HOME: TilePos = (18, 20);

fn admitted_rig(templates: Vec<assets::ObjectEvent>) -> Rig {
    let mut rig = Rig::new(templates, PLAYER_HOME);
    rig.tick(1);
    rig
}

fn rig_collision(rig: &Rig, direction: Direction) -> Option<NpcCollision> {
    let id = rig.collection.get(1).unwrap().id();
    rig.collection
        .collision_in_direction(id, direction, None, &rig.context())
}

#[test]
fn an_entry_is_blocked_by_other_admitted_entries_at_either_of_their_tiles() {
    let mut rig = admitted_rig(vec![
        template(1, 20, 20, MovementType::FaceDown, NEVER_HIDDEN),
        template(2, 22, 20, MovementType::FaceDown, NEVER_HIDDEN),
    ]);
    assert_eq!(rig_collision(&rig, East), None, "21,20 is empty");

    rig.collection
        .get_mut(2)
        .unwrap()
        .state_mut()
        .begin_normal_walk(West);
    assert_eq!(
        rig_collision(&rig, East),
        Some(NpcCollision::ObjectEvent),
        "entry 2 has arrived on 21,20"
    );

    rig.collection
        .get_mut(2)
        .unwrap()
        .state_mut()
        .begin_normal_walk(West);
    assert_eq!(
        rig_collision(&rig, East),
        Some(NpcCollision::ObjectEvent),
        "entry 2 is leaving 21,20"
    );

    rig.collection
        .get_mut(2)
        .unwrap()
        .state_mut()
        .settle_previous_position();
    assert_eq!(rig_collision(&rig, East), None, "and has settled on 20,20");
}

#[test]
fn an_entry_never_blocks_itself_and_an_unadmitted_entry_blocks_nobody() {
    let mut rig = Rig::new(
        vec![
            template(1, 20, 20, MovementType::FaceDown, NEVER_HIDDEN),
            template(2, 21, 20, MovementType::FaceDown, NEVER_HIDDEN),
        ],
        (10, 10),
    );
    assert_eq!(
        rig_collision(&rig, East),
        None,
        "before admission, entry 2 holds nothing"
    );

    rig.player = PLAYER_HOME;
    rig.tick(1);
    assert!(rig.collection.get(2).unwrap().is_active());
    assert_eq!(rig_collision(&rig, East), Some(NpcCollision::ObjectEvent));

    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .begin_normal_walk(North);
    assert_eq!(
        rig_collision(&rig, South),
        None,
        "its own previous tile is no obstacle"
    );
}

#[test]
fn the_player_blocks_the_tile_it_stands_on_and_the_tile_it_is_leaving() {
    let mut rig = admitted_rig(vec![template(
        1,
        20,
        20,
        MovementType::FaceDown,
        NEVER_HIDDEN,
    )]);

    rig.player = (21, 20);
    assert_eq!(rig_collision(&rig, East), Some(NpcCollision::ObjectEvent));

    rig.player = (22, 20);
    rig.player_previous = Some((21, 20));
    assert_eq!(rig_collision(&rig, East), Some(NpcCollision::ObjectEvent));

    rig.player_previous = None;
    assert_eq!(rig_collision(&rig, East), None);

    rig.player = (21, 20);
    rig.player_elevation = FLOOR_ELEVATION + 1;
    assert_eq!(
        rig_collision(&rig, East),
        None,
        "a player on another level does not block"
    );
}

#[test]
fn an_unknown_id_is_refused_nothing() {
    let rig = admitted_rig(vec![template(
        1,
        20,
        20,
        MovementType::FaceDown,
        NEVER_HIDDEN,
    )]);
    let unknown = rig.collection.get(1).unwrap().id() + 1;

    assert_eq!(
        rig.collection
            .collision_in_direction(unknown, East, None, &rig.context()),
        None
    );
}

#[test]
fn a_carried_mover_is_not_confused_with_an_entered_map_entry_of_its_local_id() {
    const DEPARTED: assets::MapId = assets::MapId("MAP_DEPARTED");
    const ENTERED: assets::MapId = assets::MapId("MAP_ENTERED");
    let mut rig = admitted_rig(vec![template(
        1,
        20,
        20,
        MovementType::FaceDown,
        NEVER_HIDDEN,
    )]);
    rig.collection = rig.collection.carry_across_connection(
        DEPARTED,
        ENTERED,
        crate::overworld::object_event_harness::leaked(vec![template(
            1,
            30,
            30,
            MovementType::FaceDown,
            NEVER_HIDDEN,
        )]),
        (0, 0),
    );
    rig.terrain.set_wall((31, 30));
    let carried = rig.collection.find(Some(DEPARTED), 1).unwrap();
    assert_eq!(carried.state().position(), (20, 20));
    let entered = rig.collection.find(None, 1).unwrap();
    assert_eq!(entered.state().position(), (30, 30));

    assert_eq!(
        rig.collection
            .collision_in_direction(carried.id(), East, None, &rig.context()),
        None,
        "the carried entry steps from (20, 20) onto open ground"
    );
    assert_eq!(
        rig.collection
            .collision_in_direction(entered.id(), East, None, &rig.context()),
        Some(NpcCollision::Impassable),
        "the entered map's entry faces the wall at (31, 30)"
    );
}

fn local_ids<'a>(
    entries: impl Iterator<Item = &'a crate::overworld::object_event::LiveObjectEvent>,
) -> Vec<u8> {
    entries.map(|entry| entry.template().local_id).collect()
}

#[test]
fn identification_follows_the_current_tile_while_collision_holds_both() {
    let mut rig = admitted_rig(vec![template(
        1,
        20,
        20,
        MovementType::FaceDown,
        NEVER_HIDDEN,
    )]);
    let identified =
        |rig: &Rig, x, y| local_ids(rig.collection.object_events_at(x, y, FLOOR_ELEVATION));
    let holding =
        |rig: &Rig, x, y| local_ids(rig.collection.collision_objects_at(x, y, FLOOR_ELEVATION));

    rig.collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .begin_normal_walk(East);
    for frame in 1..NORMAL_STEP_FRAMES {
        assert_eq!(identified(&rig, 21, 20), [1], "frame {frame}");
        assert!(identified(&rig, 20, 20).is_empty(), "frame {frame}");
        assert_eq!(holding(&rig, 21, 20), [1], "frame {frame}");
        assert_eq!(holding(&rig, 20, 20), [1], "frame {frame}");
        rig.collection
            .get_mut(1)
            .unwrap()
            .state_mut()
            .advance_step();
    }
    assert_eq!(identified(&rig, 21, 20), [1]);
    assert_eq!(holding(&rig, 21, 20), [1]);
    assert!(
        holding(&rig, 20, 20).is_empty(),
        "the sixteenth frame releases the source"
    );
}

#[test]
fn a_runtime_blocks_on_the_tile_a_walker_is_leaving_but_identifies_only_the_tile_it_is_on() {
    let mut collection =
        ObjectEventCollection::from_templates(crate::overworld::object_event_harness::leaked(
            vec![template(1, 20, 20, MovementType::FaceDown, NEVER_HIDDEN)],
        ));
    collection.admit_all_for_test();
    collection
        .get_mut(1)
        .unwrap()
        .state_mut()
        .begin_normal_walk(East);
    let terrain = Terrain::flat();
    let runtime = terrain.runtime().with_object_events(&collection);
    let data = crate::event_data::EventData::new();

    let local_id = |found: Option<&assets::ObjectEvent>| found.map(|event| event.local_id);
    assert_eq!(
        local_id(visible_object_event_at(
            &runtime,
            20,
            20,
            FLOOR_ELEVATION,
            &data
        )),
        None,
        "interaction no longer finds it on the tile it left"
    );
    assert_eq!(
        local_id(blocking_object_event_at(
            &runtime,
            20,
            20,
            FLOOR_ELEVATION,
            &data
        )),
        Some(1),
        "the tile it left still blocks"
    );
    assert_eq!(
        local_id(visible_object_event_at(
            &runtime,
            21,
            20,
            FLOOR_ELEVATION,
            &data
        )),
        Some(1)
    );
    assert_eq!(
        local_id(blocking_object_event_at(
            &runtime,
            21,
            20,
            FLOOR_ELEVATION,
            &data
        )),
        Some(1)
    );
}

#[test]
fn a_hidden_object_blocks_nothing_and_an_unbound_runtime_scans_authored_tiles() {
    let hidden = template(1, 20, 20, MovementType::FaceDown, HIDDEN_FLAG);
    let shown = template(2, 21, 20, MovementType::FaceDown, NEVER_HIDDEN);
    let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: crate::overworld::object_event_harness::leaked(vec![hidden, shown]),
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));
    let terrain = Terrain::flat();
    let base = terrain.runtime();
    let runtime = crate::overworld::MapRuntime::new(
        base.id(),
        base.header(),
        events,
        Box::leak(Box::new(assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 4,
            height: 4,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }))
        .grid(Box::leak(vec![0u8; 32].into_boxed_slice()))
        .unwrap(),
        assets::MetatileAttributeTable::new(&[]),
        assets::MetatileAttributeTable::new(&[]),
    );
    let mut data = crate::event_data::EventData::new();
    data.flag_set(assets::object_event_flags::resolve(HIDDEN_FLAG).unwrap())
        .unwrap();

    let local_id = |found: Option<&assets::ObjectEvent>| found.map(|event| event.local_id);
    assert_eq!(
        local_id(blocking_object_event_at(
            &runtime,
            20,
            20,
            FLOOR_ELEVATION,
            &data
        )),
        None
    );
    assert_eq!(
        local_id(blocking_object_event_at(
            &runtime,
            21,
            20,
            FLOOR_ELEVATION,
            &data
        )),
        Some(2)
    );
}

#[test]
fn adjudicator_an_unadmitted_object_blocks_no_player_step() {
    let flag = assets::object_event_flags::resolve(HIDDEN_FLAG).unwrap();
    let mut rig = Rig::new(
        vec![template(1, 19, 20, MovementType::FaceDown, HIDDEN_FLAG)],
        PLAYER_HOME,
    );
    rig.event_data.flag_set(flag).unwrap();
    rig.tick(1);
    rig.event_data.flag_clear(flag).unwrap();
    rig.tick(1);
    assert!(
        !rig.collection.get(1).unwrap().is_active(),
        "the camera has not moved, so the unhidden object is still unadmitted"
    );
    let runtime = rig.terrain.runtime().with_object_events(&rig.collection);
    assert_eq!(
        blocking_object_event_at(&runtime, 19, 20, FLOOR_ELEVATION, &rig.event_data)
            .map(|event| event.local_id),
        None,
        "upstream has no active object on (19, 20) to collide with"
    );

    let mut player = crate::overworld::PlayerState::new(PLAYER_HOME, FLOOR_ELEVATION, East);
    let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };
    assert_eq!(
        player.step(Some(East), &runtime, &no_connections, &rig.event_data),
        crate::overworld::StepOutcome::Advanced {
            from: PLAYER_HOME,
            to: (19, 20)
        },
        "the unadmitted object does not block the player's landing"
    );
}
