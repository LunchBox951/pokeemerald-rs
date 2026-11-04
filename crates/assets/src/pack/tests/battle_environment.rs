use super::super::{AssetPack, BattleEnvironment, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const GRASS_TILES: &str = "battle-environment/image/tall_grass/tiles";
const GRASS_MAP: &str = "battle-environment/raw/tall_grass/map";
const GRASS_ENTRY_TILES: &str = "battle-environment/image/tall_grass/anim_tiles";
const GRASS_ENTRY_MAP: &str = "battle-environment/raw/tall_grass/anim_map";
const GRASS_PALETTE: &str = "battle-environment/palette/tall_grass";
const BUILDING_TILES: &str = "battle-environment/image/building/tiles";
const BUILDING_MAP: &str = "battle-environment/raw/building/map";
const BUILDING_ENTRY_TILES: &str = "battle-environment/image/building/anim_tiles";
const BUILDING_ENTRY_MAP: &str = "battle-environment/raw/building/anim_map";
const PLAIN_PALETTE: &str = "battle-environment/palette/plain";

const ALL_IDS: [&str; 10] = [
    GRASS_TILES,
    GRASS_MAP,
    GRASS_ENTRY_TILES,
    GRASS_ENTRY_MAP,
    GRASS_PALETTE,
    BUILDING_TILES,
    BUILDING_MAP,
    BUILDING_ENTRY_TILES,
    BUILDING_ENTRY_MAP,
    PLAIN_PALETTE,
];
const MAIN_MAP_BYTES: usize = 4096;
const ENTRY_MAP_BYTES: usize = 2048;

/// A distinct marker per constituent, so a swapped or shared one fails.
fn marker(id: &str) -> u8 {
    1 + u8::try_from(ALL_IDS.iter().position(|&known| known == id).unwrap()).unwrap()
}

fn image(id: &'static str, size: (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(size.0, size.1, depth),
        payload: vec![pixel; (size.0 * size.1) as usize],
    }
}

fn palette(id: &'static str, colors: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * 2];
    payload[..2].copy_from_slice(&u16::from(marker(id)).to_le_bytes());
    Entry {
        id,
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

fn tilemap(id: &'static str, len: usize) -> Entry {
    Entry {
        id,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![marker(id); len],
    }
}

fn entry(id: &'static str) -> Entry {
    match id {
        GRASS_TILES | GRASS_ENTRY_TILES | BUILDING_TILES | BUILDING_ENTRY_TILES => {
            image(id, (128, 128), TEXT_WINDOW_BIT_DEPTH, marker(id))
        }
        GRASS_MAP | BUILDING_MAP => tilemap(id, MAIN_MAP_BYTES),
        GRASS_ENTRY_MAP | BUILDING_ENTRY_MAP => tilemap(id, ENTRY_MAP_BYTES),
        _ => palette(id, 48),
    }
}

fn entries() -> Vec<Entry> {
    ALL_IDS.into_iter().map(entry).collect()
}

fn replaced(id: &str, replacement: Entry) -> Vec<Entry> {
    let mut entries = without(id);
    entries.push(replacement);
    entries
}

fn without(id: &str) -> Vec<Entry> {
    entries().into_iter().filter(|e| e.id != id).collect()
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

#[test]
fn tall_grass_resolves_its_five_members_and_dimensions() {
    let (pack, path) = load("battle-environment-grass", entries());
    let record = pack
        .battle_environment(BattleEnvironment::TallGrass)
        .unwrap();

    for (sheet, id) in [
        (record.tiles, GRASS_TILES),
        (record.entry_tiles, GRASS_ENTRY_TILES),
    ] {
        assert_eq!((sheet.width, sheet.height, sheet.bit_depth), (128, 128, 4));
        assert_eq!(sheet.pixels.len(), 128 * 128);
        assert!(sheet.pixels.iter().all(|&p| p == marker(id)), "{id}");
    }
    assert_eq!(record.tilemap.len(), MAIN_MAP_BYTES);
    assert!(record.tilemap.iter().all(|&b| b == marker(GRASS_MAP)));
    assert_eq!(record.entry_tilemap.len(), ENTRY_MAP_BYTES);
    assert!(record
        .entry_tilemap
        .iter()
        .all(|&b| b == marker(GRASS_ENTRY_MAP)));
    assert_eq!(record.palette.color_count, 48);
    assert_eq!(
        record.palette.color(0),
        Some(u16::from(marker(GRASS_PALETTE)))
    );
    assert_eq!(record.tilemap_dimensions(), (64, 32));
    assert_eq!(record.entry_tilemap_dimensions(), (32, 32));
    let _ = std::fs::remove_file(path);
}

#[test]
fn plain_uses_the_building_art_with_its_own_palette() {
    let (pack, path) = load("battle-environment-plain", entries());
    let record = pack.battle_environment(BattleEnvironment::Plain).unwrap();

    assert!(record
        .tiles
        .pixels
        .iter()
        .all(|&p| p == marker(BUILDING_TILES)));
    assert!(record
        .entry_tiles
        .pixels
        .iter()
        .all(|&p| p == marker(BUILDING_ENTRY_TILES)));
    assert!(record.tilemap.iter().all(|&b| b == marker(BUILDING_MAP)));
    assert!(record
        .entry_tilemap
        .iter()
        .all(|&b| b == marker(BUILDING_ENTRY_MAP)));
    assert_eq!(
        record.palette.color(0),
        Some(u16::from(marker(PLAIN_PALETTE)))
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_pack_with_only_tall_grass_does_not_lend_it_to_plain() {
    let only_grass = entries()
        .into_iter()
        .filter(|e| e.id.contains("tall_grass"))
        .collect();
    let (pack, path) = load("battle-environment-only-grass", only_grass);

    assert!(pack
        .battle_environment(BattleEnvironment::TallGrass)
        .is_ok());
    assert!(matches!(
        pack.battle_environment(BattleEnvironment::Plain).unwrap_err(),
        PackError::UnknownAsset(missing) if missing == BUILDING_TILES
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn each_missing_constituent_is_named() {
    for id in ALL_IDS {
        let (pack, path) = load("battle-environment-missing", without(id));
        let environment = if id.contains("tall_grass") {
            BattleEnvironment::TallGrass
        } else {
            BattleEnvironment::Plain
        };
        assert!(
            matches!(
                pack.battle_environment(environment).unwrap_err(),
                PackError::UnknownAsset(missing) if missing == id
            ),
            "{id}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_wrong_kind_constituent_is_named() {
    for id in ALL_IDS {
        // A raw entry stands in everywhere except the maps, which get an image.
        let wrong = if id.contains("/raw/") {
            image(id_static(id), (8, 8), TEXT_WINDOW_BIT_DEPTH, 1)
        } else {
            tilemap(id_static(id), 4)
        };
        let (pack, path) = load("battle-environment-wrong-kind", replaced(id, wrong));
        let environment = if id.contains("tall_grass") {
            BattleEnvironment::TallGrass
        } else {
            BattleEnvironment::Plain
        };
        assert!(
            matches!(
                pack.battle_environment(environment).unwrap_err(),
                PackError::WrongKind { id: wrong, .. } if wrong == id
            ),
            "{id}"
        );
        let _ = std::fs::remove_file(path);
    }
}

fn id_static(id: &str) -> &'static str {
    ALL_IDS.into_iter().find(|&known| known == id).unwrap()
}

#[test]
fn malformed_constituents_are_rejected_on_read() {
    let sheet = |id, size, depth, pixel| (id, image(id, size, depth, pixel));
    let cases = [
        sheet(GRASS_TILES, (128, 64), TEXT_WINDOW_BIT_DEPTH, 1),
        sheet(GRASS_TILES, (64, 128), TEXT_WINDOW_BIT_DEPTH, 1),
        sheet(GRASS_TILES, (128, 128), INDEXED_8_BIT, 1),
        // Index 16 is inside the 48-colour palette but outside 4 bits.
        sheet(GRASS_ENTRY_TILES, (128, 128), TEXT_WINDOW_BIT_DEPTH, 16),
        sheet(BUILDING_TILES, (128, 128), INDEXED_8_BIT, 1),
        sheet(BUILDING_ENTRY_TILES, (64, 64), TEXT_WINDOW_BIT_DEPTH, 1),
        (GRASS_PALETTE, palette(GRASS_PALETTE, 16)),
        (GRASS_PALETTE, palette(GRASS_PALETTE, 47)),
        (PLAIN_PALETTE, palette(PLAIN_PALETTE, 49)),
        (GRASS_MAP, tilemap(GRASS_MAP, ENTRY_MAP_BYTES)),
        (GRASS_MAP, tilemap(GRASS_MAP, MAIN_MAP_BYTES - 1)),
        (BUILDING_MAP, tilemap(BUILDING_MAP, ENTRY_MAP_BYTES)),
        (GRASS_ENTRY_MAP, tilemap(GRASS_ENTRY_MAP, MAIN_MAP_BYTES)),
        (
            GRASS_ENTRY_MAP,
            tilemap(GRASS_ENTRY_MAP, ENTRY_MAP_BYTES - 1),
        ),
        (
            BUILDING_ENTRY_MAP,
            tilemap(BUILDING_ENTRY_MAP, MAIN_MAP_BYTES),
        ),
    ];
    for (index, (id, bad)) in cases.into_iter().enumerate() {
        let (pack, path) = load(
            &format!("battle-environment-bad-{index}"),
            replaced(id, bad),
        );
        let environment = if id.contains("tall_grass") {
            BattleEnvironment::TallGrass
        } else {
            BattleEnvironment::Plain
        };
        assert!(
            matches!(
                pack.battle_environment(environment).unwrap_err(),
                PackError::MalformedBattleEnvironmentAsset { id: named, .. } if named == id
            ),
            "case {index}: {id}"
        );
        let _ = std::fs::remove_file(path);
    }
}
