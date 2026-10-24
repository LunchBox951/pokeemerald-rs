//! The checkpoint maps' object graphics loaded from the checkout pack.

use super::*;
use crate::overworld::{load_room_from_source, OverworldScene};
use crate::pack_source::PackSource;
use assets::AssetPack;
use engine::overworld::{object_event_is_visible, Direction};

/// The checkpoint maps whose scenes carry object events, interiors and
/// exteriors.
const CHECKPOINT_MAPS: [&str; 15] = [
    "MAP_INSIDE_OF_TRUCK",
    "MAP_LITTLEROOT_TOWN",
    "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
    "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
    "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
    "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
    "MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
    "MAP_OLDALE_TOWN",
    "MAP_OLDALE_TOWN_HOUSE1",
    "MAP_OLDALE_TOWN_HOUSE2",
    "MAP_OLDALE_TOWN_MART",
    "MAP_OLDALE_TOWN_POKEMON_CENTER_1F",
    "MAP_OLDALE_TOWN_POKEMON_CENTER_2F",
    "MAP_ROUTE101",
    "MAP_ROUTE103",
];

fn load(map: &'static str, player: PlayerCharacter, data: &EventData) -> OverworldScene {
    load_room_from_source(&PackSource::Repo, assets::MapId(map), player, data)
        .unwrap_or_else(|err| panic!("{map} must load from the checkout pack: {err}"))
}

fn fresh_save() -> EventData {
    crate::new_game::init_save_blocks_for_new_game()
        .0
        .event_data
}

/// Every checkpoint scene loads for either protagonist on a fresh save, every
/// visible object event whose graphic is not an unresolved `OBJ_EVENT_GFX_VAR_*`
/// slot has a binding, and standing on each such event draws its OBJs with the
/// binding's shape, palette bank and elevation priority.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn every_checkpoint_scene_binds_and_draws_each_visible_object() {
    let data = fresh_save();
    for player_character in [PlayerCharacter::Brendan, PlayerCharacter::May] {
        for map in CHECKPOINT_MAPS {
            let scene = load(map, player_character, &data);
            let events = scene.map_events().object_events;
            for event in events {
                if !object_event_is_visible(event, &data) {
                    continue;
                }
                if event.graphics_id.starts_with("OBJ_EVENT_GFX_VAR_") {
                    continue;
                }
                let key = binding_key(event);
                let binding = *scene
                    .sprites
                    .bindings()
                    .get(key)
                    .unwrap_or_else(|| panic!("{map}: {} has no binding", event.graphics_id));
                let player = PlayerState::new(
                    (i32::from(event.x), i32::from(event.y)),
                    event.elevation,
                    Direction::South,
                );
                let tiles = binding.base_tile()..binding.base_tile() + binding.packed_tiles();
                let drawn: Vec<_> = scene
                    .sprites
                    .entries(&player, &data)
                    .into_iter()
                    .filter(|entry| {
                        tiles.contains(&entry.tile_index())
                            && entry.palette_bank() == binding.palette_bank()
                    })
                    .collect();
                let pieces = if binding.width == 48 { 12 } else { 1 };
                assert!(
                    drawn.len() >= pieces,
                    "{map}: {} draws {} OBJs, expected at least {pieces}",
                    event.graphics_id,
                    drawn.len()
                );
                if pieces == 1 {
                    assert!(drawn.iter().any(|entry| entry.dimensions()
                        == (usize::from(binding.width), usize::from(binding.height))
                        && entry.enabled()));
                }
                // The whole scene composes with the object in view.
                let _ = scene.compose(&player, &data, 0);
            }
        }
    }
}

/// The two Littleroot trucks are 48x48 sheets drawn as twelve OBJs each. With
/// only the first visible, the frame changes exactly inside its 48x48
/// rectangle, so the extracted tiles, palette and OBJ layout reach the
/// compositor.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_littleroot_truck_composes_as_twelve_objs_over_its_rectangle() {
    let hide = |names: &[&str]| {
        let mut data = EventData::new();
        for name in names {
            data.flag_set(assets::object_event_flags::resolve(name).unwrap())
                .unwrap();
        }
        data
    };
    let first = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_TRUCK";
    let second = "FLAG_HIDE_LITTLEROOT_TOWN_MAYS_HOUSE_TRUCK";
    let one_truck = hide(&[second]);
    let no_truck = hide(&[first, second]);
    let scene = load("MAP_LITTLEROOT_TOWN", PlayerCharacter::Brendan, &one_truck);
    let player = PlayerState::new((2, 10), 3, Direction::South);

    let truck_bank = 8;
    let truck: Vec<_> = scene
        .sprites
        .entries(&player, &one_truck)
        .into_iter()
        .filter(|entry| entry.palette_bank() == truck_bank)
        .collect();
    assert_eq!(truck.len(), 12);
    let ys: Vec<usize> = truck.iter().map(|e| usize::from(e.y())).collect();
    assert_eq!(ys, [40, 40, 48, 48, 56, 56, 64, 64, 72, 72, 80, 80]);
    assert!(truck.iter().all(|e| e.priority() == 2));

    let with = scene.compose(&player, &one_truck, 0);
    let without = scene.compose(&player, &no_truck, 0);
    let mut changed = 0;
    for y in 0..rendering::Framebuffer::HEIGHT {
        for x in 0..rendering::Framebuffer::WIDTH {
            let differs = with.pixel(x, y) != without.pixel(x, y);
            let inside = (96..144).contains(&x) && (40..88).contains(&y);
            assert!(
                !differs || inside,
                "pixel ({x}, {y}) changed outside the truck"
            );
            changed += usize::from(differs);
        }
    }
    assert!(
        changed > 100,
        "the truck's opaque texels reach the frame ({changed})"
    );
}

/// May's Pichu doll is authored at elevation 4 on a cell of elevation 3, so it
/// draws at priority 2 (`ObjectEventUpdateElevation`,
/// `event_object_movement.c:7759-7771`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_pichu_doll_settles_on_its_cells_elevation() {
    let data = EventData::new();
    let scene = load(
        "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
        PlayerCharacter::Brendan,
        &data,
    );
    let doll = scene
        .map_events()
        .object_events
        .iter()
        .find(|event| event.graphics_id == "OBJ_EVENT_GFX_PICHU_DOLL")
        .unwrap();
    assert_eq!((doll.x, doll.y, doll.elevation), (3, 4, 4));
    let binding = scene.sprites.bindings()["OBJ_EVENT_GFX_PICHU_DOLL"];
    let player = PlayerState::new((3, 4), 3, Direction::South);
    let entry = scene
        .sprites
        .entries(&player, &data)
        .into_iter()
        .find(|entry| entry.tile_index() == binding.base_tile())
        .unwrap();
    assert_eq!(entry.priority(), 2);
    assert_eq!(entry.dimensions(), (16, 16));
}

/// Route 103's three berry trees are drawn at their fresh-save berries stage,
/// Cheri twice and Leppa once, each with its own binding.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn route_103s_fresh_berry_trees_bind_per_tree() {
    let data = fresh_save();
    let scene = load("MAP_ROUTE103", PlayerCharacter::Brendan, &data);
    let bindings = scene.sprites.bindings();
    let cheri = bindings["BERRY_TREE_ROUTE_103_CHERI_1"];
    let leppa = bindings["BERRY_TREE_ROUTE_103_LEPPA"];
    assert_eq!((cheri.palette_bank(), leppa.palette_bank()), (3, 2));
    assert_eq!((cheri.width, cheri.height), (16, 32));
    assert_ne!(cheri.base_tile(), leppa.base_tile());
}

/// Every sheet row's declared geometry matches the extracted sheet, and
/// packing all its cells yields `cells` raster-order blocks of
/// `width/8 * height/8` tiles.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn every_object_sheet_row_matches_its_extracted_sheet() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let graphics = [
        "OBJ_EVENT_GFX_NINJA_BOY",
        "OBJ_EVENT_GFX_NURSE",
        "OBJ_EVENT_GFX_MYSTERY_GIFT_MAN",
        "OBJ_EVENT_GFX_ITEM_BALL",
        "OBJ_EVENT_GFX_BIRCHS_BAG",
        "OBJ_EVENT_GFX_CUTTABLE_TREE",
        "OBJ_EVENT_GFX_MOVING_BOX",
        "OBJ_EVENT_GFX_PICHU_DOLL",
        "OBJ_EVENT_GFX_SWABLU_DOLL",
        "OBJ_EVENT_GFX_TRUCK",
        "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX",
        "OBJ_EVENT_GFX_ZIGZAGOON_1",
    ];
    let mut sheets: Vec<_> = graphics
        .iter()
        .map(|id| sheets::object_sheet(id).unwrap())
        .collect();
    sheets.extend(
        ["BERRY_TREE_ROUTE_103_CHERI_1", "BERRY_TREE_ROUTE_103_LEPPA"]
            .map(|tree| sheets::initial_berry_sheet(tree).unwrap()),
    );
    for sheet in sheets {
        let all_cells = (1u16 << sheet.cells) - 1;
        let bytes = sheet.pack(&pack, all_cells).unwrap();
        let frame_tiles = usize::from(sheet.width / 8) * usize::from(sheet.height / 8);
        assert_eq!(
            bytes.len(),
            usize::from(sheet.cells) * frame_tiles * TILE_BYTES,
            "{}",
            sheet.sprite_path
        );
        assert!(
            bytes.iter().any(|&b| b != 0),
            "{} is not blank",
            sheet.sprite_path
        );
    }
}
