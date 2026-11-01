use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const EXPECTED_ENTRY_COUNT: usize = 49;
const PALETTE_ENTRY_BYTES: usize = 2;
const EMBEDDED_PALETTE_FIRST_COLOR_BASE: u16 = 0x0100;
const STANDALONE_PALETTE_FIRST_COLOR_BASE: u16 = 0x0200;
const REGION_MAP_DIMENSIONS: (u32, u32) = (128, 120);

struct ExpectedSheet {
    name: &'static str,
    width: u32,
    height: u32,
    bit_depth: u8,
    palette_colors: u16,
}

const fn sheet(
    name: &'static str,
    width: u32,
    height: u32,
    bit_depth: u8,
    palette_colors: u16,
) -> ExpectedSheet {
    ExpectedSheet {
        name,
        width,
        height,
        bit_depth,
        palette_colors,
    }
}

const EXPECTED_SHEETS: [ExpectedSheet; 14] = [
    sheet("menu", 128, 128, 4, 16),
    sheet("interface", 32, 512, 4, 16),
    sheet("noball_unused", 8, 32, 4, 16),
    sheet("caught_ball", 8, 16, 4, 16),
    sheet("arrows", 8, 96, 4, 16),
    sheet("search_menu", 128, 64, 4, 16),
    sheet("area_glow", 32, 96, 4, 16),
    sheet("area_marker", 16, 16, 4, 16),
    sheet("area_unknown", 32, 96, 4, 16),
    sheet("region_map", 128, 120, 8, 256),
    sheet("region_map_affine", 128, 120, 8, 256),
    sheet("cry_meter", 80, 64, 4, 16),
    sheet("cry_meter_needle", 64, 64, 4, 16),
    sheet("cry_screen_bg", 8, 8, 4, 16),
];

const EXPECTED_PALETTES: [(&str, u16); 6] = [
    ("bg_hoenn", 96),
    ("bg_national", 96),
    ("search_results_bg", 96),
    ("search_menu", 64),
    ("region_map", 48),
    ("size_silhouette", 16),
];

const EXPECTED_RAWS: [(&str, usize); 15] = [
    ("start_menu_main", 768),
    ("start_menu_search_results", 768),
    ("list", 1280),
    ("list_underlay", 1280),
    ("info_screen", 1280),
    ("size_screen", 1280),
    ("screen_select_bar_main", 192),
    ("screen_select_bar_submenu", 192),
    ("search_menu_hoenn", 1280),
    ("search_menu_national", 1280),
    ("region_map", 2048),
    ("region_map_affine", 4096),
    ("cry_screen", 1280),
    ("cry_meter_map", 160),
    ("cry_screen_bg", 32),
];

fn leak(id: String) -> &'static str {
    Box::leak(id.into_boxed_str())
}

fn image_id(name: &str) -> &'static str {
    leak(format!("pokedex-graphics/image/{name}"))
}

fn image_palette_id(name: &str) -> &'static str {
    leak(format!("pokedex-graphics/image-palette/{name}"))
}

fn palette_id(name: &str) -> &'static str {
    leak(format!("pokedex-graphics/palette/{name}"))
}

fn raw_id(name: &str) -> &'static str {
    leak(format!("pokedex-graphics/raw/{name}"))
}

fn image(id: &'static str, (width, height): (u32, u32), pixel: u8) -> Entry {
    let bit_depth = if (width, height) == REGION_MAP_DIMENSIONS {
        INDEXED_8_BIT
    } else {
        TEXT_WINDOW_BIT_DEPTH
    };
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, bit_depth),
        payload: vec![pixel; (width * height) as usize],
    }
}

fn palette(id: &'static str, colors: u16, first_color: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * PALETTE_ENTRY_BYTES];
    payload[..PALETTE_ENTRY_BYTES].copy_from_slice(&first_color.to_le_bytes());
    Entry {
        id,
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

fn raw(id: &'static str, len: usize, fill: u8) -> Entry {
    Entry {
        id,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![fill; len],
    }
}

fn fixture_entries() -> Vec<Entry> {
    let mut entries = Vec::new();
    for (index, expected) in (1u8..).zip(EXPECTED_SHEETS) {
        entries.push(image(
            image_id(expected.name),
            (expected.width, expected.height),
            u8::try_from(expected.palette_colors - 1).unwrap(),
        ));
        entries.push(palette(
            image_palette_id(expected.name),
            expected.palette_colors,
            EMBEDDED_PALETTE_FIRST_COLOR_BASE + u16::from(index),
        ));
    }
    for (index, (name, colors)) in (1u16..).zip(EXPECTED_PALETTES) {
        entries.push(palette(
            palette_id(name),
            colors,
            STANDALONE_PALETTE_FIRST_COLOR_BASE + index,
        ));
    }
    for (index, (name, len)) in (1u8..).zip(EXPECTED_RAWS) {
        entries.push(raw(raw_id(name), len, index));
    }
    entries
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn replaced(id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id != id)
        .collect();
    entries.push(entry);
    entries
}

#[test]
fn contract_has_forty_nine_entries() {
    assert_eq!(fixture_entries().len(), EXPECTED_ENTRY_COUNT);
}

#[test]
fn pokedex_graphics_bundles_every_handle_group() {
    let (pack, path) = load("pokedex-graphics", fixture_entries());

    let graphics = pack.pokedex_graphics().unwrap();
    let sheets = [
        graphics.menu,
        graphics.interface,
        graphics.noball_unused,
        graphics.caught_ball,
        graphics.arrows,
        graphics.search_menu,
        graphics.area_glow,
        graphics.area_marker,
        graphics.area_unknown,
        graphics.region_map,
        graphics.region_map_affine,
        graphics.cry_meter,
        graphics.cry_meter_needle,
        graphics.cry_screen_bg,
    ];
    for (index, (sheet, expected)) in (1u16..).zip(sheets.into_iter().zip(EXPECTED_SHEETS)) {
        let name = expected.name;
        assert_eq!(
            (sheet.image.width, sheet.image.height),
            (expected.width, expected.height),
            "`{name}` dimensions"
        );
        assert_eq!(
            sheet.palette.color_count, expected.palette_colors,
            "`{name}` palette colours"
        );
        assert_eq!(
            sheet.palette.color(0),
            Some(EMBEDDED_PALETTE_FIRST_COLOR_BASE + index),
            "`{name}` is paired with its own palette"
        );
    }
    let palettes = [
        graphics.bg_hoenn_palette,
        graphics.bg_national_palette,
        graphics.search_results_palette,
        graphics.search_menu_palette,
        graphics.region_map_palette,
        graphics.size_silhouette_palette,
    ];
    for (index, (palette, (name, colors))) in
        (1u16..).zip(palettes.into_iter().zip(EXPECTED_PALETTES))
    {
        assert_eq!(palette.color_count, colors, "`{name}` palette colours");
        assert_eq!(
            palette.color(0),
            Some(STANDALONE_PALETTE_FIRST_COLOR_BASE + index),
            "`{name}` is the standalone palette of that name"
        );
    }
    let raw_entries = [
        graphics.start_menu_main_tilemap,
        graphics.start_menu_search_results_tilemap,
        graphics.list_tilemap,
        graphics.list_underlay_tilemap,
        graphics.info_screen_tilemap,
        graphics.size_screen_tilemap,
        graphics.screen_select_bar_main_tilemap,
        graphics.screen_select_bar_submenu_tilemap,
        graphics.search_menu_hoenn_tilemap,
        graphics.search_menu_national_tilemap,
        graphics.region_map_tilemap,
        graphics.region_map_affine_tilemap,
        graphics.cry_screen_tilemap,
        graphics.cry_meter_tilemap,
        graphics.cry_screen_bg_raw,
    ];
    for (index, (bytes, (name, len))) in (1u8..).zip(raw_entries.into_iter().zip(EXPECTED_RAWS)) {
        assert_eq!(
            bytes,
            vec![index; len].as_slice(),
            "`{name}` raw entry of {len} bytes"
        );
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_or_wrong_kind_entries_fail_without_substitution() {
    let missing = image_palette_id("arrows");
    let entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id != missing)
        .collect();
    let (pack, path) = load("pokedex-graphics-missing", entries);
    assert!(matches!(
        pack.pokedex_graphics().unwrap_err(),
        PackError::UnknownAsset(id) if id == missing
    ));
    let _ = std::fs::remove_file(path);

    let id = raw_id("list");
    let (pack, path) = load(
        "pokedex-graphics-wrong-kind",
        replaced(id, image(id, (2, 2), 0)),
    );
    assert!(matches!(
        pack.pokedex_graphics().unwrap_err(),
        PackError::WrongKind { id: bad, expected: "raw blob", .. } if bad == id
    ));
    let _ = std::fs::remove_file(path);
}

const EXPECTED_MENU_WIDTH: u32 = 128;
const EXPECTED_MENU_HEIGHT: u32 = 128;
const EXPECTED_BANK_COLORS: u16 = 16;
const EXPECTED_BG_HOENN_COLORS: u16 = 96;
const EXPECTED_REGION_MAP_AFFINE_BYTES: usize = 64 * 64;
const EXPECTED_CRY_SCREEN_BG_RAW_BYTES: usize = 32;
const EXPECTED_STANDALONE_PALETTE_BYTES: [(&str, usize); 6] = [
    ("bg_hoenn", 192),
    ("bg_national", 192),
    ("search_results_bg", 192),
    ("search_menu", 128),
    ("region_map", 96),
    ("size_silhouette", 32),
];
const MAX_4BPP_INDEX: u8 = 15;
const MAX_8BPP_INDEX: u8 = 255;

#[test]
fn every_sheet_keeps_its_declared_depth_and_byte_lengths() {
    let (pack, path) = load("pokedex-graphics-sheet-shapes", fixture_entries());
    let graphics = pack.pokedex_graphics().unwrap();
    let sheets = [
        ("menu", graphics.menu),
        ("interface", graphics.interface),
        ("noball_unused", graphics.noball_unused),
        ("caught_ball", graphics.caught_ball),
        ("arrows", graphics.arrows),
        ("search_menu", graphics.search_menu),
        ("area_glow", graphics.area_glow),
        ("area_marker", graphics.area_marker),
        ("area_unknown", graphics.area_unknown),
        ("region_map", graphics.region_map),
        ("region_map_affine", graphics.region_map_affine),
        ("cry_meter", graphics.cry_meter),
        ("cry_meter_needle", graphics.cry_meter_needle),
        ("cry_screen_bg", graphics.cry_screen_bg),
    ];
    for (handle_name, handle) in sheets {
        let expected = EXPECTED_SHEETS
            .iter()
            .find(|expected| expected.name == handle_name)
            .unwrap();
        assert_eq!(
            handle.image.bit_depth, expected.bit_depth,
            "`{handle_name}` bit depth"
        );
        assert_eq!(
            handle.image.pixels.len(),
            (expected.width * expected.height) as usize,
            "`{handle_name}` pixel bytes"
        );
        assert_eq!(
            handle.palette.raw.len(),
            usize::from(expected.palette_colors) * PALETTE_ENTRY_BYTES,
            "`{handle_name}` palette bytes"
        );
        let max_index = if expected.bit_depth == INDEXED_8_BIT {
            MAX_8BPP_INDEX
        } else {
            MAX_4BPP_INDEX
        };
        assert_eq!(
            handle.image.pixels.iter().copied().max(),
            Some(max_index),
            "`{handle_name}` accepts its highest palette index"
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn standalone_palettes_keep_their_payload_length() {
    let (pack, path) = load("pokedex-graphics-palette-bytes", fixture_entries());
    let graphics = pack.pokedex_graphics().unwrap();
    let palettes = [
        ("bg_hoenn", graphics.bg_hoenn_palette),
        ("bg_national", graphics.bg_national_palette),
        ("search_results_bg", graphics.search_results_palette),
        ("search_menu", graphics.search_menu_palette),
        ("region_map", graphics.region_map_palette),
        ("size_silhouette", graphics.size_silhouette_palette),
    ];
    for ((name, palette), (expected_name, expected_bytes)) in
        palettes.into_iter().zip(EXPECTED_STANDALONE_PALETTE_BYTES)
    {
        assert_eq!(name, expected_name);
        assert_eq!(palette.raw.len(), expected_bytes, "`{name}` palette bytes");
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn each_entry_group_reports_a_missing_or_wrong_kind_entry() {
    let ids = [
        ("image", image_id("menu")),
        ("embedded palette", image_palette_id("menu")),
        ("standalone palette", palette_id("bg_hoenn")),
        ("raw", raw_id("list")),
    ];
    for (n, (group, id)) in ids.into_iter().enumerate() {
        let missing: Vec<Entry> = fixture_entries()
            .into_iter()
            .filter(|e| e.id != id)
            .collect();
        let (pack, path) = load(&format!("pokedex-graphics-group-missing-{n}"), missing);
        assert!(
            matches!(
                pack.pokedex_graphics().unwrap_err(),
                PackError::UnknownAsset(bad) if bad == id
            ),
            "missing {group} entry `{id}`"
        );
        let _ = std::fs::remove_file(path);

        let other_kind = if group == "raw" {
            image(id, (2, 2), 0)
        } else {
            raw(id, 1, 0)
        };
        let (pack, path) = load(
            &format!("pokedex-graphics-group-wrong-kind-{n}"),
            replaced(id, other_kind),
        );
        assert!(
            matches!(
                pack.pokedex_graphics().unwrap_err(),
                PackError::WrongKind { id: bad, .. } if bad == id
            ),
            "wrong-kind {group} entry `{id}`"
        );
        let _ = std::fs::remove_file(path);
    }
}

type OffByOneCase = (&'static str, &'static str, Entry, Option<&'static str>);

fn off_by_one_cases() -> Vec<OffByOneCase> {
    let menu = image_id("menu");
    let area_glow = image_id("area_glow");
    let region_map = image_id("region_map");
    let mut last_pixel_out_of_bank = image(area_glow, (32, 96), MAX_4BPP_INDEX);
    *last_pixel_out_of_bank.payload.last_mut().unwrap() = MAX_4BPP_INDEX + 1;
    vec![
        (
            "menu one column narrower",
            menu,
            image(menu, (EXPECTED_MENU_WIDTH - 1, EXPECTED_MENU_HEIGHT), 0),
            None,
        ),
        (
            "menu one row taller",
            menu,
            image(menu, (EXPECTED_MENU_WIDTH, EXPECTED_MENU_HEIGHT + 1), 0),
            None,
        ),
        (
            "region map declared 4-bit",
            region_map,
            Entry {
                meta: image_meta(128, 120, TEXT_WINDOW_BIT_DEPTH),
                ..image(region_map, (128, 120), 0)
            },
            None,
        ),
        (
            "first invalid 4-bit index at the final pixel",
            area_glow,
            last_pixel_out_of_bank,
            None,
        ),
        (
            "embedded palette one colour short",
            image_palette_id("arrows"),
            palette(image_palette_id("arrows"), EXPECTED_BANK_COLORS - 1, 0),
            Some("declares 15 colours in 30 bytes: expected 16 colours"),
        ),
        (
            "embedded palette one colour long",
            image_palette_id("arrows"),
            palette(image_palette_id("arrows"), EXPECTED_BANK_COLORS + 1, 0),
            Some("declares 17 colours in 34 bytes: expected 16 colours"),
        ),
        (
            "standalone palette one colour short",
            palette_id("bg_hoenn"),
            palette(palette_id("bg_hoenn"), EXPECTED_BG_HOENN_COLORS - 1, 0),
            Some("declares 95 colours in 190 bytes: expected 96 colours"),
        ),
        (
            "standalone palette one colour long",
            palette_id("bg_hoenn"),
            palette(palette_id("bg_hoenn"), EXPECTED_BG_HOENN_COLORS + 1, 0),
            Some("declares 97 colours in 194 bytes: expected 96 colours"),
        ),
        (
            "affine tilemap one byte short",
            raw_id("region_map_affine"),
            raw(
                raw_id("region_map_affine"),
                EXPECTED_REGION_MAP_AFFINE_BYTES - 1,
                0,
            ),
            Some("is 4095 bytes: expected 4096"),
        ),
        (
            "affine tilemap one byte long",
            raw_id("region_map_affine"),
            raw(
                raw_id("region_map_affine"),
                EXPECTED_REGION_MAP_AFFINE_BYTES + 1,
                0,
            ),
            Some("is 4097 bytes: expected 4096"),
        ),
        (
            "opaque raw file one byte short",
            raw_id("cry_screen_bg"),
            raw(
                raw_id("cry_screen_bg"),
                EXPECTED_CRY_SCREEN_BG_RAW_BYTES - 1,
                0,
            ),
            Some("is 31 bytes: expected 32"),
        ),
        (
            "opaque raw file one byte long",
            raw_id("cry_screen_bg"),
            raw(
                raw_id("cry_screen_bg"),
                EXPECTED_CRY_SCREEN_BG_RAW_BYTES + 1,
                0,
            ),
            Some("is 33 bytes: expected 32"),
        ),
    ]
}

#[test]
fn off_by_one_lengths_depth_and_indices_are_rejected() {
    for (n, (label, id, entry, expected_detail)) in off_by_one_cases().into_iter().enumerate() {
        let (pack, path) = load(
            &format!("pokedex-graphics-off-by-one-{n}"),
            replaced(id, entry),
        );
        let error = pack.pokedex_graphics().unwrap_err();
        let _ = std::fs::remove_file(path);
        let PackError::MalformedPokedexGraphic { id: bad, detail } = error else {
            panic!("{label}: wrong error {error:?}");
        };
        assert_eq!(bad, id, "{label}");
        if let Some(expected_detail) = expected_detail {
            assert_eq!(detail, expected_detail, "{label}");
        }
    }
}

#[test]
fn malformed_entries_are_rejected_on_read() {
    let cases = [
        (
            "wrong image geometry",
            image_id("menu"),
            image(image_id("menu"), (64, 64), 0),
        ),
        (
            "wrong bit depth",
            image_id("arrows"),
            Entry {
                meta: image_meta(8, 96, INDEXED_8_BIT),
                ..image(image_id("arrows"), (8, 96), 0)
            },
        ),
        (
            "pixel index outside the 16-colour palette",
            image_id("area_glow"),
            image(image_id("area_glow"), (32, 96), 16),
        ),
        (
            "wrong embedded palette colour count",
            image_palette_id("cry_meter"),
            palette(image_palette_id("cry_meter"), 2, 0),
        ),
        (
            "wrong standalone palette colour count",
            palette_id("bg_hoenn"),
            palette(palette_id("bg_hoenn"), 16, 0),
        ),
        (
            "short tilemap",
            raw_id("list"),
            raw(raw_id("list"), 1278, 0),
        ),
        (
            "long tilemap",
            raw_id("region_map_affine"),
            raw(raw_id("region_map_affine"), 4098, 0),
        ),
        (
            "short opaque raw file",
            raw_id("cry_screen_bg"),
            raw(raw_id("cry_screen_bg"), 31, 0),
        ),
    ];
    for (n, (label, id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(
            &format!("pokedex-graphics-malformed-{n}"),
            replaced(id, entry),
        );
        assert!(
            matches!(
                pack.pokedex_graphics().unwrap_err(),
                PackError::MalformedPokedexGraphic { id: bad, .. } if bad == id
            ),
            "wrong error for malformed `{id}` ({label})"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn malformed_sheet_reports_the_shared_validation_order_and_wording() {
    let detail_of = |n: usize, id: &'static str, entry: Entry| {
        let (pack, path) = load(&format!("pokedex-graphics-detail-{n}"), replaced(id, entry));
        let error = pack.pokedex_graphics().unwrap_err();
        let _ = std::fs::remove_file(path);
        match error {
            PackError::MalformedPokedexGraphic { id: bad, detail } if bad == id => detail,
            other => panic!("wrong error for malformed `{id}`: {other:?}"),
        }
    };
    // Wrong geometry and wrong bit depth together: geometry is reported first.
    let both = Entry {
        meta: image_meta(64, 64, INDEXED_8_BIT),
        ..image(image_id("arrows"), (64, 64), 0)
    };
    let detail = detail_of(0, image_id("arrows"), both);
    assert!(detail.contains("is 64x64: expected 8x96"), "{detail}");
    // A pixel index past the 16-colour palette names the palette bound.
    let detail = detail_of(
        1,
        image_id("area_glow"),
        image(image_id("area_glow"), (32, 96), 16),
    );
    assert!(
        detail.contains("has pixel index 16: outside its palette's 16 colours"),
        "{detail}"
    );
}
