use super::super::{AssetPack, PackError, PcWallpaperId, PcWallpaperTile};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};
use pack_format::EntryKind;

/// Literal expectations, written apart from the production manifest.
const STYLES: [(u8, PcWallpaperId, &str); 12] = [
    (4, PcWallpaperId::Crag, "crag"),
    (5, PcWallpaperId::Volcano, "volcano"),
    (6, PcWallpaperId::Snow, "snow"),
    (7, PcWallpaperId::Cave, "cave"),
    (8, PcWallpaperId::Beach, "beach"),
    (9, PcWallpaperId::Seafloor, "seafloor"),
    (10, PcWallpaperId::River, "river"),
    (11, PcWallpaperId::Sky, "sky"),
    (12, PcWallpaperId::Polkadot, "polkadot"),
    (13, PcWallpaperId::Pokecenter, "pokecenter"),
    (14, PcWallpaperId::Machine, "machine"),
    (15, PcWallpaperId::Plain, "plain"),
];

/// Bytes a save file may hold that no record backs.
const UNSUPPORTED: [u8; 6] = [0, 1, 2, 3, 16, 255];

/// Three-tile sheet: tile 0 transparent, tile 1 all `tag`, tile 2 all 15.
const TILE_COUNT: usize = 3;
/// Blank, frame, flipped background, flipped frame, doubly flipped background.
const LEADING_WORDS: [u16; 5] = [0x0000, 0x1001, 0x2402, 0x1801, 0x2c02];
const MAP_ENTRIES: usize = 360;

fn leak(id: String) -> &'static str {
    Box::leak(id.into_boxed_str())
}

fn tile_sheet(id: String, tiles: usize, tag: u8) -> Entry {
    let mut payload = vec![0u8; 64];
    payload.extend(std::iter::repeat_n(tag, 64));
    payload.extend(std::iter::repeat_n(15, 64 * tiles.saturating_sub(2)));
    payload.truncate(64 * tiles);
    sheet(id, 8, u32::try_from(8 * tiles).unwrap(), 4, payload)
}

fn sheet(id: String, width: u32, height: u32, depth: u8, payload: Vec<u8>) -> Entry {
    Entry {
        id: leak(id),
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, depth),
        payload,
    }
}

fn palette(id: String, colors: u16, first: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * 2];
    if let Some(head) = payload.get_mut(..2) {
        head.copy_from_slice(&first.to_le_bytes());
    }
    Entry {
        id: leak(id),
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

fn raw(id: String, payload: Vec<u8>) -> Entry {
    Entry {
        id: leak(id),
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload,
    }
}

fn map_bytes(words: &[u16]) -> Vec<u8> {
    let mut all = words.to_vec();
    all.resize(MAP_ENTRIES, 0x1001);
    all.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn id_of(stem: &str, part: &str) -> String {
    format!("pc-wallpaper/{stem}/{part}")
}

/// One style's four records; `k` seeds the markers.
fn style(stem: &str, k: u8) -> Vec<Entry> {
    vec![
        tile_sheet(id_of(stem, "tiles"), TILE_COUNT, k + 1),
        raw(id_of(stem, "tilemap"), map_bytes(&LEADING_WORDS)),
        palette(id_of(stem, "palette/frame"), 16, 0x0100 + u16::from(k)),
        palette(id_of(stem, "palette/background"), 16, 0x0200 + u16::from(k)),
    ]
}

fn full() -> Vec<Entry> {
    STYLES
        .iter()
        .enumerate()
        .flat_map(|(k, (_, _, stem))| style(stem, u8::try_from(k).unwrap()))
        .collect()
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

/// `full()` with the record `id` swapped for `entry` (or dropped when `None`).
fn replaced(id: &str, entry: Option<Entry>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = full().into_iter().filter(|e| e.id != id).collect();
    entries.extend(entry);
    entries
}

fn crag_with(entry: Entry) -> Vec<Entry> {
    replaced(entry.id, Some(entry))
}

/// Run one Crag fixture and clean its file up.
fn crag_result(name: &str, entries: Vec<Entry>) -> PackError {
    let (pack, path) = load(name, entries);
    let error = pack.pc_wallpaper(4).unwrap_err();
    let _ = std::fs::remove_file(path);
    error
}

fn assert_malformed(error: &PackError, expected_id: &str) {
    assert!(
        matches!(error, PackError::MalformedPcWallpaperAsset { id, .. } if id == expected_id),
        "{error:?} should be a malformed {expected_id}"
    );
}

fn tilemap_words(words: &[u16]) -> Entry {
    raw(id_of("crag", "tilemap"), map_bytes(words))
}

#[test]
fn manifest_matches_upstream_order_and_ids() {
    assert_eq!(PcWallpaperId::ALL.len(), STYLES.len());
    let mut seen = std::collections::HashSet::new();
    for ((byte, expected, stem), actual) in STYLES.iter().zip(PcWallpaperId::ALL) {
        assert_eq!(actual, *expected);
        assert_eq!(actual.index(), *byte);
        assert_eq!(actual.stem(), *stem);
        assert_eq!(PcWallpaperId::try_from(*byte).unwrap(), *expected);
        for (id, part) in [
            (actual.tiles_id(), "tiles"),
            (actual.tilemap_id(), "tilemap"),
            (actual.frame_palette_id(), "palette/frame"),
            (actual.background_palette_id(), "palette/background"),
        ] {
            assert_eq!(id, format!("pc-wallpaper/{stem}/{part}"));
            assert!(seen.insert(id));
        }
    }
    assert_eq!(seen.len(), 48);
}

#[test]
fn all_twelve_wallpapers_decode_their_own_payloads() {
    let (pack, path) = load("pc-wallpaper-all", full());
    for (k, (byte, id, _)) in STYLES.iter().enumerate() {
        let k = u8::try_from(k).unwrap();
        let handle = pack.pc_wallpaper(*byte).unwrap();
        assert_eq!(handle.id, *id);
        let tiles = handle.tiles;
        assert_eq!((tiles.width, tiles.height, tiles.bit_depth), (8, 24, 4));
        assert_eq!(tiles.pixels.len(), 192);
        assert_eq!(tiles.pixels[0], 0);
        assert_eq!(tiles.pixels[64], k + 1);
        assert_eq!(tiles.pixels[128], 15);
        assert_eq!(handle.tilemap, map_bytes(&LEADING_WORDS));
        let [frame, background] = handle.palettes;
        assert_eq!(frame.color_count, 16);
        assert_eq!(background.color_count, 16);
        assert_eq!(frame.color(0), Some(0x0100 + u16::from(k)));
        assert_eq!(background.color(0), Some(0x0200 + u16::from(k)));
        assert_eq!(handle.entries().count(), MAP_ENTRIES);
        let leading: Vec<PcWallpaperTile> = handle.entries().take(5).collect();
        let expected = [
            (0, false, false, 0),
            (1, false, false, 1),
            (2, true, false, 2),
            (1, false, true, 1),
            (2, true, true, 2),
        ];
        for (tile, (tile_index, h_flip, v_flip, palette_bank)) in leading.iter().zip(expected) {
            assert_eq!(
                *tile,
                PcWallpaperTile {
                    tile_index,
                    h_flip,
                    v_flip,
                    palette_bank
                }
            );
        }
        assert!(leading[0].is_blank());
        assert!(leading[1..].iter().all(|tile| !tile.is_blank()));
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn bytes_outside_the_manifest_fail_before_any_lookup() {
    let (full_pack, full_path) = load("pc-wallpaper-unsupported-full", full());
    let (empty_pack, empty_path) = load("pc-wallpaper-unsupported-empty", vec![]);
    for byte in UNSUPPORTED {
        for pack in [&full_pack, &empty_pack] {
            assert_eq!(
                pack.pc_wallpaper(byte).unwrap_err(),
                PackError::UnsupportedPcWallpaper(byte)
            );
        }
        assert_eq!(
            PcWallpaperId::try_from(byte).unwrap_err(),
            PackError::UnsupportedPcWallpaper(byte)
        );
    }
    let _ = std::fs::remove_file(full_path);
    let _ = std::fs::remove_file(empty_path);
}

#[test]
fn a_pack_without_wallpaper_records_reports_the_missing_entry() {
    let (pack, path) = load("pc-wallpaper-none", vec![]);
    for (byte, _, stem) in STYLES {
        assert_eq!(
            pack.pc_wallpaper(byte).unwrap_err(),
            PackError::UnknownAsset(format!("pc-wallpaper/{stem}/tiles"))
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn each_missing_record_is_named() {
    for (index, part) in ["tiles", "tilemap", "palette/frame", "palette/background"]
        .into_iter()
        .enumerate()
    {
        let id = id_of("crag", part);
        let (pack, path) = load(
            &format!("pc-wallpaper-missing-{index}"),
            replaced(&id, None),
        );
        assert_eq!(
            pack.pc_wallpaper(4).unwrap_err(),
            PackError::UnknownAsset(id)
        );
        assert!(pack.pc_wallpaper(5).is_ok(), "other styles still resolve");
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn wrong_kind_records_name_expected_and_actual() {
    let cases = [
        (raw(id_of("crag", "tiles"), vec![0; 4]), "image", "raw blob"),
        (
            sheet(id_of("crag", "tilemap"), 8, 8, 4, vec![0; 64]),
            "raw blob",
            "image",
        ),
        (
            raw(id_of("crag", "palette/frame"), vec![0; 32]),
            "palette",
            "raw blob",
        ),
        (
            raw(id_of("crag", "palette/background"), vec![0; 32]),
            "palette",
            "raw blob",
        ),
    ];
    for (index, (entry, want, actual)) in cases.into_iter().enumerate() {
        let entry_id = entry.id;
        let error = crag_result(&format!("pc-wallpaper-kind-{index}"), crag_with(entry));
        assert!(
            matches!(
                &error,
                PackError::WrongKind { id, expected, actual: found }
                    if id == entry_id && *expected == want && *found == actual
            ),
            "case {index}: {error:?}"
        );
    }
}

#[test]
fn tile_strip_dimensions_are_bounded() {
    for (index, (width, height)) in [(16, 24), (0, 24), (8, 0), (8, 7), (8, 25), (8, 2056)]
        .into_iter()
        .enumerate()
    {
        let payload = vec![0; usize::try_from(width * height).unwrap()];
        let entry = sheet(id_of("crag", "tiles"), width, height, 4, payload);
        let error = crag_result(&format!("pc-wallpaper-dims-{index}"), crag_with(entry));
        assert_malformed(&error, "pc-wallpaper/crag/tiles");
    }
}

#[test]
fn one_tile_and_the_largest_sheet_decode() {
    let one = tile_sheet(id_of("crag", "tiles"), 1, 1);
    let mut entries = crag_with(one);
    entries.retain(|e| e.id != "pc-wallpaper/crag/tilemap");
    entries.push(raw(
        id_of("crag", "tilemap"),
        map_bytes(&[0x1000; MAP_ENTRIES]),
    ));
    let (pack, path) = load("pc-wallpaper-one-tile", entries);
    assert_eq!(pack.pc_wallpaper(4).unwrap().tiles.height, 8);
    let _ = std::fs::remove_file(path);

    let largest = tile_sheet(id_of("crag", "tiles"), 256, 1);
    let mut entries = crag_with(largest);
    entries.retain(|e| e.id != "pc-wallpaper/crag/tilemap");
    entries.push(tilemap_words(&[0x0000, 0x20ff]));
    let (pack, path) = load("pc-wallpaper-256-tiles", entries);
    let handle = pack.pc_wallpaper(4).unwrap();
    assert_eq!(handle.tiles.height, 2048);
    assert_eq!(handle.entries().nth(1).unwrap().tile_index, 255);
    let _ = std::fs::remove_file(path);
}

#[test]
fn tiles_require_four_bit_bank_local_pixels() {
    let cases = [
        sheet(id_of("crag", "tiles"), 8, 24, 2, vec![3; 192]),
        sheet(id_of("crag", "tiles"), 8, 24, INDEXED_8_BIT, vec![3; 192]),
        sheet(id_of("crag", "tiles"), 8, 24, TEXT_WINDOW_BIT_DEPTH, {
            let mut pixels = vec![0; 192];
            pixels[100] = 16;
            pixels
        }),
        sheet(id_of("crag", "tiles"), 8, 24, 4, vec![255; 192]),
    ];
    for (index, entry) in cases.into_iter().enumerate() {
        let error = crag_result(&format!("pc-wallpaper-pixels-{index}"), crag_with(entry));
        assert_malformed(&error, "pc-wallpaper/crag/tiles");
    }
}

#[test]
fn a_short_tile_payload_is_rejected_by_the_accessor() {
    let (mut pack, path) = load("pc-wallpaper-short-tiles", full());
    let entry = pack
        .entries
        .iter_mut()
        .find(|e| e.id == "pc-wallpaper/crag/tiles")
        .unwrap();
    entry.length -= 1;
    assert_malformed(
        &pack.pc_wallpaper(4).unwrap_err(),
        "pc-wallpaper/crag/tiles",
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_wrong_size_tilemap_is_rejected() {
    for len in [0usize, 2, 718, 719, 721, 722, 1440] {
        let entry = raw(id_of("crag", "tilemap"), vec![0; len]);
        let error = crag_result(&format!("pc-wallpaper-map-len-{len}"), crag_with(entry));
        assert_malformed(&error, "pc-wallpaper/crag/tilemap");
    }
}

#[test]
fn each_palette_must_be_one_complete_bank() {
    for part in ["palette/frame", "palette/background"] {
        let id = id_of("crag", part);
        for colors in [0u16, 15, 17, 32] {
            let error = crag_result(
                &format!("pc-wallpaper-{colors}-colours"),
                crag_with(palette(id.clone(), colors, 0)),
            );
            assert_malformed(&error, &id);
        }
        let (mut pack, path) = load("pc-wallpaper-short-palette", full());
        pack.entries.iter_mut().find(|e| e.id == id).unwrap().length -= 2;
        assert_malformed(&pack.pc_wallpaper(4).unwrap_err(), &id);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_pack_entry_kind_swap_in_memory_is_rejected_cleanly() {
    let (mut pack, path) = load("pc-wallpaper-kind-swap", full());
    let entry = pack
        .entries
        .iter_mut()
        .find(|e| e.id == "pc-wallpaper/crag/palette/frame")
        .unwrap();
    entry.kind = EntryKind::Palette { color_count: 17 };
    assert_malformed(
        &pack.pc_wallpaper(4).unwrap_err(),
        "pc-wallpaper/crag/palette/frame",
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn tile_references_are_checked_after_masking_the_flags() {
    // Three tiles: 0x2c02 is tile 2 under both flips.
    let (pack, path) = load("pc-wallpaper-tile-ok", crag_with(tilemap_words(&[0x2c02])));
    assert!(pack.pc_wallpaper(4).is_ok());
    let _ = std::fs::remove_file(path);
    for (index, word) in [0x1003u16, 0x2c03, 0x23ff, 0x1100].into_iter().enumerate() {
        let error = crag_result(
            &format!("pc-wallpaper-tile-{index}"),
            crag_with(tilemap_words(&[0x0000, word])),
        );
        assert_malformed(&error, "pc-wallpaper/crag/tilemap");
        assert!(error.to_string().contains("cell 1"), "{error}");
    }

    let mut entries = crag_with(tile_sheet(id_of("crag", "tiles"), 256, 1));
    entries.retain(|e| e.id != "pc-wallpaper/crag/tilemap");
    entries.push(tilemap_words(&[0x0000, 0x2100]));
    let error = crag_result("pc-wallpaper-tile-256", entries);
    assert_malformed(&error, "pc-wallpaper/crag/tilemap");
}

#[test]
fn palette_banks_are_one_based_with_only_a_blank_zero_exception() {
    for bank in (3u16..16).chain([0]) {
        let word = (bank << 12) | 1;
        let error = crag_result(
            &format!("pc-wallpaper-bank-{bank}"),
            crag_with(tilemap_words(&[0x0000, word])),
        );
        assert_malformed(&error, "pc-wallpaper/crag/tilemap");
        assert!(error.to_string().contains("palette bank"), "{error}");
    }
    // Bank 0 is no blank when a flip or a tile is stored beside it.
    for (index, word) in [0x0400u16, 0x0800, 0x0c00].into_iter().enumerate() {
        let error = crag_result(
            &format!("pc-wallpaper-bank0-flip-{index}"),
            crag_with(tilemap_words(&[word])),
        );
        assert_malformed(&error, "pc-wallpaper/crag/tilemap");
    }
}

#[test]
fn blank_cells_require_a_transparent_tile_zero() {
    let mut pixels = vec![0u8; 192];
    pixels[5] = 1;
    let opaque = sheet(id_of("crag", "tiles"), 8, 24, 4, pixels.clone());
    let error = crag_result("pc-wallpaper-opaque-zero", crag_with(opaque));
    assert_malformed(&error, "pc-wallpaper/crag/tilemap");

    // With no blank cell, tile 0 may be opaque.
    let mut entries = crag_with(sheet(id_of("crag", "tiles"), 8, 24, 4, pixels));
    entries.retain(|e| e.id != "pc-wallpaper/crag/tilemap");
    entries.push(raw(
        id_of("crag", "tilemap"),
        map_bytes(&[0x1001; MAP_ENTRIES]),
    ));
    let (pack, path) = load("pc-wallpaper-no-blank-ok", entries);
    assert!(pack.pc_wallpaper(4).is_ok());
    let _ = std::fs::remove_file(path);
}

#[test]
fn wallpaper_errors_render_and_carry_no_source() {
    use std::error::Error;
    let unsupported = PackError::UnsupportedPcWallpaper(255);
    assert_eq!(
        unsupported.to_string(),
        "asset pack: unsupported PC wallpaper id 255: expected 4..=15"
    );
    let malformed = PackError::MalformedPcWallpaperAsset {
        id: "pc-wallpaper/crag/tiles".to_owned(),
        detail: "is odd".to_owned(),
    };
    assert_eq!(
        malformed.to_string(),
        "asset pack: PC wallpaper entry `pc-wallpaper/crag/tiles` is odd"
    );
    assert!(unsupported.source().is_none());
    assert!(malformed.source().is_none());
}
