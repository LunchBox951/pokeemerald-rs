use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

/// (name, width, height, embedded palette colours)
const SHEETS: [(&str, u32, u32, u16); 14] = [
    ("menu", 128, 128, 16),
    ("interface", 32, 512, 16),
    ("noball_unused", 8, 32, 16),
    ("caught_ball", 8, 16, 16),
    ("arrows", 8, 96, 16),
    ("search_menu", 128, 64, 16),
    ("area_glow", 32, 96, 16),
    ("area_marker", 16, 16, 16),
    ("area_unknown", 32, 96, 16),
    ("region_map", 128, 120, 256),
    ("region_map_affine", 128, 120, 256),
    ("cry_meter", 80, 64, 16),
    ("cry_meter_needle", 64, 64, 16),
    ("cry_screen_bg", 8, 8, 16),
];

const PALETTES: [(&str, u16); 6] = [
    ("bg_hoenn", 96),
    ("bg_national", 96),
    ("search_results_bg", 96),
    ("search_menu", 64),
    ("region_map", 48),
    ("size_silhouette", 16),
];

const RAWS: [(&str, usize); 15] = [
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
    let depth = if width == 128 && height == 120 {
        INDEXED_8_BIT
    } else {
        TEXT_WINDOW_BIT_DEPTH
    };
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, depth),
        payload: vec![pixel; (width * height) as usize],
    }
}

fn palette(id: &'static str, colors: u16, first: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * 2];
    payload[..2].copy_from_slice(&first.to_le_bytes());
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
    for (n, (name, width, height, colors)) in (1u8..).zip(SHEETS) {
        entries.push(image(
            image_id(name),
            (width, height),
            u8::try_from(colors - 1).unwrap(),
        ));
        entries.push(palette(
            image_palette_id(name),
            colors,
            0x0100 + u16::from(n),
        ));
    }
    for (n, (name, colors)) in (1u16..).zip(PALETTES) {
        entries.push(palette(palette_id(name), colors, 0x0200 + n));
    }
    for (n, (name, len)) in (1u8..).zip(RAWS) {
        entries.push(raw(raw_id(name), len, n));
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
    assert_eq!(fixture_entries().len(), 49);
}

#[test]
fn pokedex_graphics_bundles_every_handle_group() {
    let (pack, path) = load("pokedex-graphics", fixture_entries());

    let g = pack.pokedex_graphics().unwrap();
    let sheets = [
        g.menu,
        g.interface,
        g.noball_unused,
        g.caught_ball,
        g.arrows,
        g.search_menu,
        g.area_glow,
        g.area_marker,
        g.area_unknown,
        g.region_map,
        g.region_map_affine,
        g.cry_meter,
        g.cry_meter_needle,
        g.cry_screen_bg,
    ];
    for (n, (sheet, (_, width, height, colors))) in (1u16..).zip(sheets.into_iter().zip(SHEETS)) {
        assert_eq!((sheet.image.width, sheet.image.height), (width, height));
        assert_eq!(sheet.palette.color_count, colors);
        assert_eq!(sheet.palette.color(0), Some(0x0100 + n));
    }
    let palettes = [
        g.bg_hoenn_palette,
        g.bg_national_palette,
        g.search_results_palette,
        g.search_menu_palette,
        g.region_map_palette,
        g.size_silhouette_palette,
    ];
    for (n, (palette, (_, colors))) in (1u16..).zip(palettes.into_iter().zip(PALETTES)) {
        assert_eq!(palette.color_count, colors);
        assert_eq!(palette.color(0), Some(0x0200 + n));
    }
    let tilemaps = [
        g.start_menu_main_tilemap,
        g.start_menu_search_results_tilemap,
        g.list_tilemap,
        g.list_underlay_tilemap,
        g.info_screen_tilemap,
        g.size_screen_tilemap,
        g.screen_select_bar_main_tilemap,
        g.screen_select_bar_submenu_tilemap,
        g.search_menu_hoenn_tilemap,
        g.search_menu_national_tilemap,
        g.region_map_tilemap,
        g.region_map_affine_tilemap,
        g.cry_screen_tilemap,
        g.cry_meter_tilemap,
        g.cry_screen_bg_raw,
    ];
    for (n, (bytes, (_, len))) in (1u8..).zip(tilemaps.into_iter().zip(RAWS)) {
        assert_eq!(bytes, vec![n; len].as_slice());
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

#[test]
fn malformed_entries_are_rejected_on_read() {
    let cases = [
        // Wrong image geometry.
        (image_id("menu"), image(image_id("menu"), (64, 64), 0)),
        // Wrong bit depth.
        (
            image_id("arrows"),
            Entry {
                meta: image_meta(8, 96, INDEXED_8_BIT),
                ..image(image_id("arrows"), (8, 96), 0)
            },
        ),
        // Pixel index outside the 16-colour palette.
        (
            image_id("area_glow"),
            image(image_id("area_glow"), (32, 96), 16),
        ),
        // Wrong embedded palette colour count.
        (
            image_palette_id("cry_meter"),
            palette(image_palette_id("cry_meter"), 2, 0),
        ),
        // Wrong standalone palette colour count.
        (
            palette_id("bg_hoenn"),
            palette(palette_id("bg_hoenn"), 16, 0),
        ),
        // Short tilemap.
        (raw_id("list"), raw(raw_id("list"), 1278, 0)),
        // Long tilemap.
        (
            raw_id("region_map_affine"),
            raw(raw_id("region_map_affine"), 4098, 0),
        ),
        // Extensionless `cry_screen_bg` is a 32-byte raw member, not a palette.
        (raw_id("cry_screen_bg"), raw(raw_id("cry_screen_bg"), 31, 0)),
    ];
    for (n, (id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(
            &format!("pokedex-graphics-malformed-{n}"),
            replaced(id, entry),
        );
        assert!(
            matches!(
                pack.pokedex_graphics().unwrap_err(),
                PackError::MalformedPokedexGraphic { id: bad, .. } if bad == id
            ),
            "wrong error for malformed `{id}`"
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
