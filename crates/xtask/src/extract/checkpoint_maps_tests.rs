//! Proves the extraction inventory covers everything the checkpoint maps
//! reference, by reading the maps, layouts, and object-event tables of a
//! real checkout instead of restating them.

use std::collections::BTreeSet;
use std::path::Path;

use super::scope::{
    sprite_palette_ids, sprite_sheet_ids, ObjectSheet, LAYOUTS, OBJECT_SHEETS, TILESETS,
};
use super::{read_text, repo_root, upstream_present};

/// `MAP_INSIDE_OF_TRUCK`, the Littleroot maps, `MAP_ROUTE101`,
/// `MAP_OLDALE_TOWN`, `MAP_ROUTE103`, and the accessible Oldale interiors, as
/// their `data/maps` directories.
pub(super) const CHECKPOINT_MAPS: [&str; 15] = [
    "InsideOfTruck",
    "LittlerootTown",
    "LittlerootTown_BrendansHouse_1F",
    "LittlerootTown_BrendansHouse_2F",
    "LittlerootTown_MaysHouse_1F",
    "LittlerootTown_MaysHouse_2F",
    "LittlerootTown_ProfessorBirchsLab",
    "Route101",
    "OldaleTown",
    "Route103",
    "OldaleTown_House1",
    "OldaleTown_House2",
    "OldaleTown_Mart",
    "OldaleTown_PokemonCenter_1F",
    "OldaleTown_PokemonCenter_2F",
];

/// Every `"key": "value"` string value for `key` in `text`, in order.
fn string_values<'a>(text: &'a str, key: &str) -> Vec<&'a str> {
    let needle = format!("\"{key}\": \"");
    text.lines()
        .filter_map(|line| line.trim().strip_prefix(needle.as_str()))
        .filter_map(|rest| rest.split('"').next())
        .collect()
}

/// The C text from `anchor` to the next `};`.
fn block<'a>(text: &'a str, anchor: &str) -> &'a str {
    let start = text
        .find(anchor)
        .unwrap_or_else(|| panic!("upstream no longer declares `{anchor}`"));
    let rest = &text[start..];
    &rest[..rest.find("};").expect("declaration ends")]
}

/// The identifier after `key` in `text` (`.images = sPicTable_X,` -> `sPicTable_X`).
fn field<'a>(text: &'a str, key: &str) -> &'a str {
    let rest = text
        .split(key)
        .nth(1)
        .unwrap_or_else(|| panic!("`{key}` missing"));
    rest.split([',', ')', ' ', '\n'])
        .next()
        .expect("identifier")
}

/// `CamelCase` as the `snake_case` directory upstream names it.
fn snake(camel: &str) -> String {
    let mut out = String::new();
    for (index, ch) in camel.chars().enumerate() {
        if ch.is_ascii_uppercase() && index > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

/// The berry a `BERRY_TREE_ROUTE_103_CHERI_1`-style tree id names, as a sheet stem.
fn berry_of(tree_id: &str) -> Option<String> {
    let rest = tree_id.strip_prefix("BERRY_TREE_ROUTE_")?;
    let (_, rest) = rest.split_once('_')?;
    let (berry, _) = rest.rsplit_once('_').unwrap_or((rest, ""));
    Some(berry.to_ascii_lowercase())
}

fn sheet_id(png_path: &str) -> String {
    let stem = png_path
        .strip_prefix("graphics/object_events/pics/")
        .and_then(|rest| rest.strip_suffix(".png"))
        .unwrap_or_else(|| panic!("unexpected object picture path `{png_path}`"));
    stem.strip_prefix("people/").map_or_else(
        || format!("sprite/{stem}"),
        |people| format!("sprite/{people}"),
    )
}

/// The sheet and palette pack ids one fixed `OBJ_EVENT_GFX_*` renders.
struct Graphic {
    sheets: BTreeSet<String>,
    palette: String,
}

fn graphic(upstream: &Path, gfx: &str) -> Graphic {
    let events = upstream.join("src/data/object_events");
    let read = |name: &str| read_text(&events.join(name)).expect("object-event table");
    let pointers = read("object_event_graphics_info_pointers.h");
    let info_name = pointers
        .lines()
        .find(|line| line.contains(&format!("[{gfx}]")))
        .unwrap_or_else(|| panic!("{gfx} has no graphics info"));
    let info_name = field(info_name, "&");
    let infos = read("object_event_graphics_info.h");
    let info = block(&infos, &format!("{info_name} = "));
    let tag = field(info, ".paletteTag = ");
    let images = field(info, ".images = ");

    let pics = read("object_event_graphics.h");
    let png_of = |symbol: &str| {
        let line = pics
            .lines()
            .find(|line| line.contains(&format!("{symbol}[]")))
            .unwrap_or_else(|| panic!("no declaration of {symbol}"));
        line.split('"').nth(1).expect("path").to_owned()
    };

    let mut sheets = BTreeSet::new();
    if images.starts_with("sPicTable_") {
        let tables = read("object_event_pic_tables.h");
        let table = block(&tables, &format!("{images}[] = "));
        for symbol in table
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|word| word.starts_with("gObjectEventPic_"))
        {
            sheets.insert(sheet_id(&png_of(symbol)));
        }
    }

    let movement = read_text(&upstream.join("src/event_object_movement.c")).expect("movement");
    let palette_line = block(&movement, "sObjectEventSpritePalettes[] = ")
        .lines()
        .find(|line| line.contains(tag))
        .unwrap_or_else(|| panic!("{tag} is not in sObjectEventSpritePalettes"));
    let palette_symbol = palette_line
        .trim()
        .trim_start_matches('{')
        .split(',')
        .next()
        .expect("symbol");
    let pal_path = png_of(palette_symbol);
    let name = pal_path
        .strip_prefix("graphics/object_events/palettes/")
        .and_then(|rest| rest.strip_suffix(".pal"))
        .expect("palette path");
    Graphic {
        sheets,
        palette: format!("sprite/palette/{name}"),
    }
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn the_inventory_covers_every_layout_tileset_and_object_graphic_the_checkpoint_maps_reference() {
    assert!(upstream_present(), "run ./init.sh first");
    let upstream = repo_root().join("pokeemerald");
    let layouts_json = read_text(&upstream.join("data/layouts/layouts.json")).expect("layouts");

    let layout_ids: BTreeSet<&str> = LAYOUTS.iter().map(|layout| layout.upstream_id).collect();
    let tileset_names: BTreeSet<&str> = TILESETS.iter().map(|tileset| tileset.name).collect();
    let sheets: BTreeSet<String> = sprite_sheet_ids().into_iter().collect();
    let palettes: BTreeSet<String> = sprite_palette_ids().into_iter().collect();

    let mut gfx_ids = BTreeSet::new();
    let mut berries = BTreeSet::new();
    let mut map_layouts = BTreeSet::new();
    for dir in CHECKPOINT_MAPS {
        let text = read_text(&upstream.join(format!("data/maps/{dir}/map.json"))).expect("map");
        map_layouts.extend(
            string_values(&text, "layout")
                .into_iter()
                .map(str::to_owned),
        );
        gfx_ids.extend(
            string_values(&text, "graphics_id")
                .into_iter()
                .map(str::to_owned),
        );
        berries.extend(
            string_values(&text, "trainer_sight_or_berry_tree_id")
                .into_iter()
                .filter_map(berry_of),
        );
    }

    let mut missing = Vec::new();
    let layout_lines: Vec<&str> = layouts_json.lines().collect();
    for layout in &map_layouts {
        if !layout_ids.contains(layout.as_str()) {
            missing.push(format!("layout {layout}"));
        }
        let at = layout_lines
            .iter()
            .position(|line| line.contains(&format!("\"id\": \"{layout}\"")))
            .unwrap_or_else(|| panic!("layouts.json has no {layout}"));
        for key in ["primary_tileset", "secondary_tileset"] {
            let value = layout_lines[at..]
                .iter()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix(&format!("\"{key}\": \"gTileset_"))
                        .and_then(|rest| rest.split('"').next())
                })
                .expect("tileset field");
            if !tileset_names.contains(snake(value).as_str()) {
                missing.push(format!("tileset gTileset_{value}"));
            }
        }
    }

    // Conditional graphics (`OBJ_EVENT_GFX_VAR_*`) resolve through script
    // variables, and a berry tree draws from per-berry sheets; neither names
    // a picture here.
    for gfx in gfx_ids
        .iter()
        .filter(|id| !id.contains("_VAR_") && *id != "OBJ_EVENT_GFX_BERRY_TREE")
    {
        let drawn = graphic(&upstream, gfx);
        for id in drawn.sheets {
            if !sheets.contains(&id) {
                missing.push(format!("{gfx} sheet {id}"));
            }
        }
        if !palettes.contains(&drawn.palette) {
            missing.push(format!("{gfx} palette {}", drawn.palette));
        }
    }
    let object_ids: BTreeSet<String> = OBJECT_SHEETS.map(ObjectSheet::pack_id).into();
    for berry in berries
        .iter()
        .map(String::as_str)
        .chain(["dirt_pile", "sprout"])
    {
        let id = format!("sprite/berry_trees/{berry}");
        if !object_ids.contains(&id) {
            missing.push(format!("berry tree sheet {id}"));
        }
    }
    assert!(missing.is_empty(), "inventory gaps: {missing:#?}");
}
