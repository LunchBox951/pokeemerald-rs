//! A live NPC's retained elevation (`ObjectEventUpdateElevation`,
//! `event_object_movement.c:7759-7771`): stored on the object as it settles on
//! each cell, then read by `compose_live` for the sprite's OAM priority.

use assets::{MapEvents, MapId, MovementType, ObjectEvent, TrainerType};
use engine::event_data::EventData;
use engine::overworld::object_event::ObjectEventCollection;
use engine::overworld::{Direction, PlayerState};
use rendering::Rgb888;

use super::test_support::{
    image_meta, synthetic_overworld_pack_entries_for,
    synthetic_scene_result_with_connections_and_events, write_synthetic_pack, Entry,
};
use super::{OverworldScene, PaletteStage};

const TRANSITION: u8 = 0;
const MULTI_LEVEL: u8 = 15;
const PLAYER_TILE: (i32, i32) = (2, 2);

/// A 4x4 scene whose listed cells carry the given elevation (the rest are 3),
/// with one TWIN NPC authored at `(npc_x, 1)` at `template` elevation.
fn scene(cells: &[((u16, u16), u8)], npc_x: i16, template: u8) -> OverworldScene {
    let mut entries = synthetic_overworld_pack_entries_for("general", 4, 4);
    entries.push(Entry {
        id: "sprite/twin".into(),
        kind_tag: 0,
        meta: image_meta(144, 32, 8),
        payload: vec![3u8; 144 * 32],
    });
    let grid = entries
        .iter_mut()
        .find(|e| e.id == "layout/map_test/map")
        .expect("the general fixture fabricates its own layout entry");
    for &((x, y), elevation) in cells {
        let cell = assets::MetatileCell {
            metatile_id: 0,
            collision: 0,
            elevation,
        }
        .pack();
        let idx = (usize::from(y) * 4 + usize::from(x)) * 2;
        grid.payload[idx..idx + 2].copy_from_slice(&cell.to_le_bytes());
    }
    let object_events: &'static [ObjectEvent] = Box::leak(Box::new([ObjectEvent {
        local_id: 1,
        graphics_id: "OBJ_EVENT_GFX_TWIN",
        x: npc_x,
        y: 1,
        elevation: template,
        movement_type: MovementType::FaceDown,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script: None,
        flag: "0",
    }]));
    let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
        id: MapId("MAP_ROUTE122"),
        shared_events_map: None,
        object_events,
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));
    synthetic_scene_result_with_connections_and_events(
        write_synthetic_pack(entries),
        "gTileset_General",
        4,
        4,
        &[],
        events,
    )
    .expect("synthetic pack decodes")
}

fn seeded(scene: &OverworldScene) -> ObjectEventCollection {
    let mut objects = ObjectEventCollection::from_templates(scene.map_events().object_events);
    scene.update_object_elevations(&mut objects);
    objects
}

fn player() -> PlayerState {
    PlayerState::new(PLAYER_TILE, 3, Direction::South)
}

/// The NPC's `(currentElevation, previousElevation)`.
fn pair(objects: &ObjectEventCollection) -> (u8, u8) {
    let state = objects.get(1).unwrap().state();
    (state.elevation(), state.previous_elevation())
}

/// Walks the NPC one tile east and settles it, as a frame of movement does.
fn walk_east(scene: &OverworldScene, objects: &mut ObjectEventCollection) {
    objects
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::East);
    scene.update_object_elevations(objects);
}

fn npc_priority(scene: &OverworldScene, objects: &ObjectEventCollection) -> u8 {
    let entries =
        scene.live_entries_for_test(objects, &player(), &EventData::new(), "OBJ_EVENT_GFX_TWIN");
    assert_eq!(entries.len(), 1, "the NPC is drawn");
    entries[0].priority()
}

fn frame(scene: &OverworldScene, objects: &ObjectEventCollection) -> Vec<Rgb888> {
    scene
        .compose_live(
            &player(),
            &EventData::new(),
            0,
            objects,
            PaletteStage::default(),
        )
        .pixels()
        .to_vec()
}

/// An object authored at 3 that spawns on a 4 cell draws at priority 1 (4 maps
/// to priority 1, 3 to 2), and keeps it when it then walks onto a transition
/// cell. The NPC ends one tile north of the player, where equal priority would
/// draw it behind the player, so the composed frame matches a reference NPC
/// standing on an ordinary elevation-4 cell only at priority 1.
#[test]
fn walking_onto_a_transition_keeps_the_retained_elevation_through_compose_live() {
    let scene = scene(&[((1, 1), 4), ((2, 1), TRANSITION)], 1, 3);
    let mut objects = seeded(&scene);
    assert_eq!(pair(&objects), (4, 4), "the spawn takes its cell's");
    assert_eq!(npc_priority(&scene, &objects), 1);

    walk_east(&scene, &mut objects);
    assert_eq!(pair(&objects), (TRANSITION, 4));
    assert_eq!(
        npc_priority(&scene, &objects),
        1,
        "upstream retains previousElevation 4 on a transition cell"
    );

    let reference_scene = scene_reference(&[((2, 1), 4)], 2, 4);
    let reference = seeded(&reference_scene);
    assert_eq!(pair(&reference), (4, 4));
    let composed = frame(&scene, &objects);
    assert_eq!(composed, frame(&reference_scene, &reference));
    // The control: the player's priority-2 sprite would win the overlap were
    // the NPC drawn at priority 2.
    let behind = scene_reference(&[((2, 1), 3)], 2, 3);
    assert_ne!(composed, frame(&behind, &seeded(&behind)));
}

fn scene_reference(cells: &[((u16, u16), u8)], npc_x: i16, template: u8) -> OverworldScene {
    scene(cells, npc_x, template)
}

/// `ObjectEventUpdateElevation` returns early when the previous cell is
/// multi-level. 5 has priority 2 against 4's 1, so adopting it would show.
#[test]
fn leaving_a_multi_level_cell_keeps_the_elevation_through_compose_live() {
    let scene = scene(&[((1, 1), MULTI_LEVEL), ((2, 1), 5)], 1, 4);
    let mut objects = seeded(&scene);
    assert_eq!(
        pair(&objects),
        (4, 4),
        "a multi-level spawn cell is skipped"
    );

    walk_east(&scene, &mut objects);
    assert_eq!(pair(&objects), (4, 4));
    assert_eq!(npc_priority(&scene, &objects), 1);

    let reference_scene = scene_reference(&[((2, 1), 4)], 2, 4);
    assert_eq!(
        frame(&scene, &objects),
        frame(&reference_scene, &seeded(&reference_scene))
    );
}

/// The elevation a multi-level departure leaves is the destination's own
/// pair when it is 6, as the issue's second case states.
#[test]
fn leaving_a_multi_level_cell_onto_six_keeps_four() {
    let scene = scene(&[((1, 1), MULTI_LEVEL), ((2, 1), 6)], 1, 4);
    let mut objects = seeded(&scene);
    walk_east(&scene, &mut objects);
    assert_eq!(pair(&objects), (4, 4));
}

#[test]
fn entering_a_multi_level_cell_leaves_both_elevations() {
    let scene = scene(&[((1, 1), 4), ((2, 1), MULTI_LEVEL)], 1, 3);
    let mut objects = seeded(&scene);
    assert_eq!(pair(&objects), (4, 4));
    walk_east(&scene, &mut objects);
    assert_eq!(pair(&objects), (4, 4));
}

#[test]
fn an_ordinary_step_takes_the_new_cells_elevation() {
    let scene = scene(&[((1, 1), 4), ((2, 1), 6)], 1, 3);
    let mut objects = seeded(&scene);
    walk_east(&scene, &mut objects);
    assert_eq!(pair(&objects), (6, 6));
}

#[test]
fn an_object_spawned_on_a_lower_cell_takes_that_priority() {
    let scene = scene(&[((1, 1), 3)], 1, 4);
    let objects = seeded(&scene);
    assert_eq!(pair(&objects), (3, 3));
    assert_eq!(npc_priority(&scene, &objects), 2);
}

#[test]
fn a_transition_spawn_keeps_the_authored_elevation() {
    let scene = scene(&[((1, 1), TRANSITION)], 1, 4);
    let objects = seeded(&scene);
    assert_eq!(pair(&objects), (TRANSITION, 4));
    assert_eq!(npc_priority(&scene, &objects), 1);
}

#[test]
fn composing_again_does_not_change_the_state_or_the_frame() {
    let scene = scene(&[((1, 1), 4), ((2, 1), TRANSITION)], 1, 3);
    let mut objects = seeded(&scene);
    walk_east(&scene, &mut objects);
    let first = frame(&scene, &objects);
    assert_eq!(frame(&scene, &objects), first);
    assert_eq!(pair(&objects), (TRANSITION, 4));
}

/// `UpdateMovementNormal` ends a walked tile with `ShiftStillObjectEventCoords`
/// (`event_object_movement.c:5116-5125`) and then the finish-step elevation
/// update reads `(dest, dest)` (`:8130`): the NPC that began its step off a
/// multi-level cell at 4 adopts the ordinary destination's 5 once the walk is
/// done, and so draws at priority 2 instead of 1.
#[test]
fn finishing_a_walk_off_a_multi_level_cell_draws_at_the_destinations_priority() {
    let scene = scene(&[((1, 1), MULTI_LEVEL), ((2, 1), 5)], 1, 4);
    let mut objects = seeded(&scene);
    walk_east(&scene, &mut objects);
    assert_eq!(pair(&objects), (4, 4), "the step's first frame is skipped");
    assert_eq!(npc_priority(&scene, &objects), 1);

    objects
        .get_mut(1)
        .unwrap()
        .state_mut()
        .settle_previous_position();
    scene.update_object_elevations(&mut objects);
    assert_eq!(pair(&objects), (5, 5));
    assert_eq!(npc_priority(&scene, &objects), 2);

    let reference_scene = scene_reference(&[((2, 1), 5)], 2, 5);
    assert_eq!(
        frame(&scene, &objects),
        frame(&reference_scene, &seeded(&reference_scene))
    );
}
