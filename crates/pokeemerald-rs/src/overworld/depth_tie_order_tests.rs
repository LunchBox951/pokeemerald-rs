//! Exact depth ties keep the previous frame's sprite order, as upstream's
//! `SortSprites` does (`sprite.c:372-440`): an insertion sort over the
//! persistent `sSpriteOrder` that swaps only on a strictly lower priority, or
//! an equal priority and a larger signed `oam.y`.

use assets::{MapEvents, MapId, MovementType, ObjectEvent, TrainerType};
use engine::event_data::EventData;
use engine::overworld::object_event::ObjectEventCollection;
use engine::overworld::{Direction, PlayerState};

use super::obj_vram::SheetSprite;
use super::test_support::{
    image_meta, synthetic_overworld_pack_entries_for,
    synthetic_scene_result_with_connections_and_events, write_synthetic_pack, Entry,
};
use super::OverworldScene;

const PLAYER_TILE: (i32, i32) = (2, 2);

/// A 4x4 scene (every cell at elevation 3) with one TWIN NPC authored at
/// `(1, 3)`, one tile row below the player.
fn scene() -> OverworldScene {
    let mut entries = synthetic_overworld_pack_entries_for("general", 4, 4);
    entries.push(Entry {
        id: "sprite/twin".into(),
        kind_tag: 0,
        meta: image_meta(144, 32, 8),
        payload: vec![3u8; 144 * 32],
    });
    let object_events: &'static [ObjectEvent] = Box::leak(Box::new([ObjectEvent {
        local_id: 1,
        graphics_id: "OBJ_EVENT_GFX_TWIN",
        x: 1,
        y: 3,
        elevation: 3,
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

fn player() -> PlayerState {
    PlayerState::new(PLAYER_TILE, 3, Direction::South)
}

/// The full draw order, as `compose_live` hands it to the renderer.
fn order(scene: &OverworldScene, objects: &ObjectEventCollection) -> Vec<SheetSprite> {
    scene
        .sprites
        .live_entries(objects, &player(), &EventData::new(), &scene.grass)
}

/// Whether the NPC's OBJ precedes the player's in `drawn`.
fn npc_leads(scene: &OverworldScene, drawn: &[SheetSprite]) -> bool {
    let binding = scene.sprites.bindings()["OBJ_EVENT_GFX_TWIN"];
    let npc_tiles = binding.base_tile()..binding.base_tile() + binding.packed_tiles();
    assert_eq!(drawn.len(), 2, "the player and the NPC are drawn");
    npc_tiles.contains(&drawn[0].tile_index())
}

/// The NPC one row below the player sorts ahead (lower on screen); stepping it
/// north onto the player's row makes an exact tie in priority, subpriority and
/// `oam.y`, which upstream resolves by leaving the NPC where it was. A
/// stateless sort would hand the tie to the player, who is listed first.
#[test]
fn an_exact_tie_keeps_the_npc_ahead_of_the_player_it_led_the_frame_before() {
    let scene = scene();
    let mut objects = ObjectEventCollection::from_templates(scene.map_events().object_events);
    let _ = objects.admit_on_map_load(PLAYER_TILE, &EventData::new());
    scene.update_object_elevations(&mut objects);

    assert!(npc_leads(&scene, &order(&scene, &objects)), "NPC is lower");

    objects
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::North);
    scene.update_object_elevations(&mut objects);
    assert_eq!(objects.get(1).unwrap().state().position(), (1, 2));
    assert!(
        npc_leads(&scene, &order(&scene, &objects)),
        "a tie on the player's row keeps last frame's order"
    );
}

/// With no earlier frame the tie is the player's: the first sort runs over the
/// slot order, and the player's sprite is created first.
#[test]
fn a_first_frame_tie_goes_to_the_player() {
    let scene = scene();
    let mut objects = ObjectEventCollection::from_templates(scene.map_events().object_events);
    let _ = objects.admit_on_map_load(PLAYER_TILE, &EventData::new());
    objects
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::North);
    scene.update_object_elevations(&mut objects);
    assert!(!npc_leads(&scene, &order(&scene, &objects)));
}

/// `ResetSpriteData` (a return to the field) resets `sSpriteOrder` to the
/// identity (`sprite.c:848-858`), so the same tie goes to the player again.
#[test]
fn resetting_the_field_effects_forgets_the_order_history() {
    let mut scene = scene();
    let mut objects = ObjectEventCollection::from_templates(scene.map_events().object_events);
    let _ = objects.admit_on_map_load(PLAYER_TILE, &EventData::new());
    scene.update_object_elevations(&mut objects);
    assert!(npc_leads(&scene, &order(&scene, &objects)));
    objects
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::North);
    scene.update_object_elevations(&mut objects);

    scene.reset_field_effects();
    assert!(!npc_leads(&scene, &order(&scene, &objects)));
}

/// A seamless connection crossing keeps every sprite and never calls
/// `ResetSpriteData` (`overworld.c:784-825`), so the entered scene inherits
/// the departed scene's order: the NPC that led keeps the tie.
#[test]
fn a_connection_crossing_carries_the_order_history_into_the_entered_scene() {
    let departed = scene();
    let mut entered = scene();
    let mut objects = ObjectEventCollection::from_templates(departed.map_events().object_events);
    let _ = objects.admit_on_map_load(PLAYER_TILE, &EventData::new());
    departed.update_object_elevations(&mut objects);
    assert!(npc_leads(&departed, &order(&departed, &objects)));

    entered.carry_sprite_order_from(&departed);
    objects
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::North);
    entered.update_object_elevations(&mut objects);
    assert!(npc_leads(&entered, &order(&entered, &objects)));
}
