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
    PALETTE_KIND_TAG,
};
use super::{OverworldScene, PaletteStage};

const PLAYER_TILE: (i32, i32) = (2, 2);
const TWIN: &str = "OBJ_EVENT_GFX_TWIN";
const ZIGZAGOON: &str = "OBJ_EVENT_GFX_ZIGZAGOON_1";
/// The opaque palette index the ZIGZAGOON fixture sheet is filled with.
const ZIGZAGOON_PIXEL_INDEX: u8 = 3;
/// Green, distinct from the player's blue and the ground's red.
const ZIGZAGOON_BGR555: u16 = 0x03E0;

/// A 4x4 scene (every cell at elevation 3) with one TWIN NPC authored at
/// `(1, 3)`, one tile row below the player.
fn scene() -> OverworldScene {
    scene_with(TWIN, (1, 3))
}

/// A 4x4 scene (every cell at elevation 3) with one NPC of `graphics_id`
/// authored at `at`. TWIN is a 16x32 sheet; ZIGZAGOON a 32x32 one, drawn in
/// its own opaque green palette.
fn scene_with(graphics_id: &'static str, at: (i16, i16)) -> OverworldScene {
    let mut entries = synthetic_overworld_pack_entries_for("general", 4, 4);
    if graphics_id == ZIGZAGOON {
        entries.push(Entry {
            id: "sprite/pokemon/enemy_zigzagoon".into(),
            kind_tag: 0,
            meta: image_meta(288, 32, 8),
            payload: vec![ZIGZAGOON_PIXEL_INDEX; 288 * 32],
        });
        let mut palette = vec![0u8; 16 * 2];
        let at = usize::from(ZIGZAGOON_PIXEL_INDEX) * 2;
        palette[at..at + 2].copy_from_slice(&ZIGZAGOON_BGR555.to_le_bytes());
        entries.push(Entry {
            id: "sprite/palette/enemy_zigzagoon".into(),
            kind_tag: PALETTE_KIND_TAG,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: palette,
        });
    } else {
        entries.push(Entry {
            id: "sprite/twin".into(),
            kind_tag: 0,
            meta: image_meta(144, 32, 8),
            payload: vec![3u8; 144 * 32],
        });
    }
    let object_events: &'static [ObjectEvent] = Box::leak(Box::new([ObjectEvent {
        local_id: 1,
        graphics_id,
        x: at.0,
        y: at.1,
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
        .live_entries(objects, &player(), &EventData::new(), &scene.grass, (0, 0))
}

/// Whether the NPC's OBJ precedes the player's in `drawn`.
fn npc_leads(scene: &OverworldScene, drawn: &[SheetSprite]) -> bool {
    npc_leads_as(scene, drawn, TWIN)
}

/// Whether the `graphics_id` NPC's OBJ precedes the player's in `drawn`.
fn npc_leads_as(scene: &OverworldScene, drawn: &[SheetSprite], graphics_id: &str) -> bool {
    assert_eq!(drawn.len(), 2, "the player and the NPC are drawn");
    is_npc(scene, drawn[0], graphics_id)
}

fn is_npc(scene: &OverworldScene, sprite: SheetSprite, graphics_id: &str) -> bool {
    let binding = scene.sprites.bindings()[graphics_id];
    let npc_tiles = binding.base_tile()..binding.base_tile() + binding.packed_tiles();
    npc_tiles.contains(&sprite.tile_index())
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

/// The composed colours at a pixel only the player covers, one only the NPC
/// covers, and one inside their overlap, after the OAM order `drawn`.
fn overlap_colours(
    scene: &OverworldScene,
    objects: &ObjectEventCollection,
    drawn: &[SheetSprite],
) -> [rendering::Rgb888; 3] {
    assert_eq!(drawn.len(), 2, "the player and the NPC are drawn");
    let (npc, avatar) = if is_npc(scene, drawn[0], ZIGZAGOON) {
        (drawn[0].oam(), drawn[1].oam())
    } else {
        (drawn[1].oam(), drawn[0].oam())
    };
    let (px, py) = (i32::from(avatar.x()), i32::from(avatar.y()));
    let (nx, ny) = (i32::from(npc.x()), i32::from(npc.y()));
    // The 32x32 NPC centred on the next tile east covers the player's right
    // eight columns.
    assert_eq!(nx, px + 8, "the NPC overlaps the player's right half");
    let frame = scene.compose_live(
        &player(),
        &EventData::new(),
        0,
        objects,
        PaletteStage::default(),
    );
    let at = |x: i32, y: i32| {
        frame
            .pixel(usize::try_from(x).unwrap(), usize::try_from(y).unwrap())
            .expect("on screen")
    };
    [
        at(px + 2, py + 2),
        at(nx + 28, ny + 28),
        at(px + 12, py.max(ny) + 4),
    ]
}

/// The issue's footprint: an opaque 32x32 NPC one tile east of the player's
/// column overlaps the 16x32 player across eight columns. One row south it sorts
/// ahead and its pixels cover the overlap; stepping north onto the player's row
/// makes an exact tie, and upstream's `SortSprites` (`sprite.c:372-440`) leaves
/// it ahead, so its pixels still cover the overlap. A stateless player-first
/// sort would flip both the OAM order and the visible overlap.
#[test]
fn an_exact_tie_keeps_the_overlapping_32x32_npcs_pixels_in_front() {
    let scene = scene_with(ZIGZAGOON, (3, 3));
    let mut objects = ObjectEventCollection::from_templates(scene.map_events().object_events);
    let _ = objects.admit_on_map_load(PLAYER_TILE, &EventData::new());
    scene.update_object_elevations(&mut objects);

    let drawn = order(&scene, &objects);
    assert!(npc_leads_as(&scene, &drawn, ZIGZAGOON), "NPC is lower");
    let [player_only, npc_only, overlap] = overlap_colours(&scene, &objects, &drawn);
    assert_ne!(
        player_only, npc_only,
        "the fixture colours are distinguishable"
    );
    assert_eq!(overlap, npc_only, "the lower NPC covers the overlap");

    objects
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::North);
    scene.update_object_elevations(&mut objects);
    assert_eq!(objects.get(1).unwrap().state().position(), (3, 2));
    let drawn = order(&scene, &objects);
    assert!(
        npc_leads_as(&scene, &drawn, ZIGZAGOON),
        "a tie on the player's row keeps last frame's OAM order"
    );
    let [tied_player_only, tied_npc_only, tied_overlap] = overlap_colours(&scene, &objects, &drawn);
    assert_eq!((tied_player_only, tied_npc_only), (player_only, npc_only));
    assert_eq!(
        tied_overlap, npc_only,
        "a tie on the player's row keeps the NPC's pixels in front"
    );
}
