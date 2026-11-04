use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use assets::{AssetPack, BattleEnvironment};
use pack_format::PackWriter;

use super::super::png::tests::indexed_png_from_raw_with_plte;
use super::super::{extract_to, upstream_present};
use super::{
    extract_battle_environment, pack_ids, ENTRY_TILEMAP_BYTES, IMAGES, MAIN_TILEMAP_BYTES,
    PALETTES, TILEMAPS,
};
use crate::extract::ExtractError;

const MANIFEST_ENTRY_COUNT: usize = 10;

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-battle-environment-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn root(dir: &Path) -> PathBuf {
    dir.join("graphics/battle_environment")
}

fn palette_text(colors: usize, first: u8) -> String {
    let mut text = format!("JASC-PAL\r\n0100\r\n{colors}\r\n");
    for i in 0..colors {
        let channel = if i == 0 { first } else { 0 };
        let _ = write!(text, "{channel} 0 0\r\n");
    }
    text
}

/// A 4-bit indexed PNG of `pixel` whose `PLTE` deliberately disagrees with the
/// external `.pal` files.
fn sheet_png(width: u32, height: u32, depth: u8, pixel: u8) -> Vec<u8> {
    let row_bytes = (width as usize * usize::from(depth)).div_ceil(8);
    let mut raw = Vec::new();
    for _ in 0..height {
        raw.push(0);
        raw.resize(raw.len() + row_bytes, pixel << 4 | pixel);
    }
    let plte: Vec<u8> = (0..16u8).flat_map(|i| [248, i, i]).collect();
    indexed_png_from_raw_with_plte(depth, width, height, &raw, &plte)
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A distinct marker per source, so a swapped constituent fails.
fn marker(index: usize) -> u8 {
    u8::try_from(index + 1).unwrap()
}

fn write_valid_checkout(dir: &Path) {
    let root = root(dir);
    for (index, image) in IMAGES.iter().enumerate() {
        write(
            &root.join(image.png),
            &sheet_png(128, 128, 4, marker(index)),
        );
    }
    for (index, palette) in PALETTES.iter().enumerate() {
        write(
            &root.join(palette.pal),
            palette_text(48, marker(index) * 8).as_bytes(),
        );
    }
    for (index, tilemap) in TILEMAPS.iter().enumerate() {
        write(
            &root.join(tilemap.bin),
            &vec![marker(index + 6); tilemap.bytes],
        );
    }
    // Neighbouring environments outside the closure must not matter.
    write(&root.join("building/palette.pal"), b"not a palette");
}

#[test]
fn the_manifest_names_exactly_the_ten_constituents() {
    assert_eq!(
        pack_ids(),
        [
            "battle-environment/image/tall_grass/tiles",
            "battle-environment/image/tall_grass/anim_tiles",
            "battle-environment/image/building/tiles",
            "battle-environment/image/building/anim_tiles",
            "battle-environment/palette/tall_grass",
            "battle-environment/palette/plain",
            "battle-environment/raw/tall_grass/map",
            "battle-environment/raw/tall_grass/anim_map",
            "battle-environment/raw/building/map",
            "battle-environment/raw/building/anim_map",
        ]
    );
}

#[test]
fn a_valid_checkout_yields_both_typed_records() {
    let dir = scratch_dir("valid");
    write_valid_checkout(&dir);
    let mut writer = PackWriter::new();
    extract_battle_environment(&dir, &mut writer).unwrap();
    assert_eq!(writer.len(), MANIFEST_ENTRY_COUNT);
    let path = dir.join("pack.bin");
    std::fs::write(&path, writer.finish().unwrap()).unwrap();

    let pack = AssetPack::load(&path).unwrap();
    let grass = pack
        .battle_environment(BattleEnvironment::TallGrass)
        .unwrap();
    assert!(grass.tiles.pixels.iter().all(|&p| p == 1));
    assert!(grass.entry_tiles.pixels.iter().all(|&p| p == 2));
    // The external `.pal` wins over the PNG's red `PLTE`: red 8 >> 3 = 1.
    assert_eq!(grass.palette.color_count, 48);
    assert_eq!(grass.palette.color(0), Some(0b00000_00000_00001));
    assert_eq!(grass.tilemap.len(), MAIN_TILEMAP_BYTES);
    assert!(grass.tilemap.iter().all(|&b| b == 7));
    assert_eq!(grass.entry_tilemap.len(), ENTRY_TILEMAP_BYTES);
    assert!(grass.entry_tilemap.iter().all(|&b| b == 8));

    let plain = pack.battle_environment(BattleEnvironment::Plain).unwrap();
    assert!(plain.tiles.pixels.iter().all(|&p| p == 3));
    assert!(plain.entry_tiles.pixels.iter().all(|&p| p == 4));
    assert_eq!(plain.palette.color(0), Some(0b00000_00000_00010));
    assert!(plain.tilemap.iter().all(|&b| b == 9));
    assert!(plain.entry_tilemap.iter().all(|&b| b == 10));

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_source_fails_without_queueing_anything() {
    let sources: Vec<&str> = IMAGES
        .iter()
        .map(|s| s.png)
        .chain(PALETTES.iter().map(|s| s.pal))
        .chain(TILEMAPS.iter().map(|s| s.bin))
        .collect();
    for missing in sources {
        let dir = scratch_dir("missing");
        write_valid_checkout(&dir);
        std::fs::remove_file(root(&dir).join(missing)).unwrap();

        let mut writer = PackWriter::new();

        let err = extract_battle_environment(&dir, &mut writer).unwrap_err();
        assert!(
            matches!(err, ExtractError::ReadFailed(path, _) if path.ends_with(missing)),
            "{missing}"
        );
        assert_eq!(writer.len(), 0, "{missing}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn a_source_of_the_wrong_shape_is_refused_and_leaves_the_writer_unchanged() {
    let cases: [(&str, Vec<u8>); 8] = [
        (IMAGES[0].png, sheet_png(128, 64, 4, 1)),
        (IMAGES[1].png, sheet_png(64, 128, 4, 1)),
        (IMAGES[2].png, sheet_png(128, 128, 8, 1)),
        // The last constituent read before the tilemaps fails after others pass.
        (IMAGES[3].png, sheet_png(8, 8, 4, 1)),
        (PALETTES[0].pal, palette_text(16, 8).into_bytes()),
        (PALETTES[1].pal, palette_text(49, 8).into_bytes()),
        (TILEMAPS[0].bin, vec![0; ENTRY_TILEMAP_BYTES]),
        (TILEMAPS[3].bin, vec![0; MAIN_TILEMAP_BYTES]),
    ];
    for (index, (target, bytes)) in cases.into_iter().enumerate() {
        let dir = scratch_dir("shape");
        write_valid_checkout(&dir);
        write(&root(&dir).join(target), &bytes);
        let mut writer = PackWriter::new();

        let err = extract_battle_environment(&dir, &mut writer).unwrap_err();
        assert!(
            matches!(&err, ExtractError::BattleEnvironmentAssetMismatch { path, .. } if path.ends_with(target)),
            "case {index}: {err}"
        );
        assert_eq!(writer.len(), 0, "case {index}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn the_real_checkout_extracts_to_the_documented_shapes() {
    assert!(upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-battle-environment-real-{}.bin",
        std::process::id()
    ));
    extract_to(&path).unwrap();
    let pack = AssetPack::load(&path).unwrap();
    for environment in [BattleEnvironment::TallGrass, BattleEnvironment::Plain] {
        let record = pack.battle_environment(environment).unwrap();
        assert_eq!((record.tiles.width, record.tiles.height), (128, 128));
        assert_eq!(record.entry_tiles.pixels.len(), 128 * 128);
        assert_eq!(record.palette.color_count, 48);
        assert_eq!(record.tilemap.len(), MAIN_TILEMAP_BYTES);
        assert_eq!(record.entry_tilemap.len(), ENTRY_TILEMAP_BYTES);
        // Both main maps open with the same blank entry; the second screenblock
        // (entry 1024) must survive too.
        assert_eq!(record.tilemap[..2], [0x62, 0x20]);
        assert_eq!(record.tilemap[2048..2050], [0x62, 0x20]);
    }
    let grass = pack
        .battle_environment(BattleEnvironment::TallGrass)
        .unwrap();
    let plain = pack.battle_environment(BattleEnvironment::Plain).unwrap();
    assert_eq!(grass.palette.color(1), Some(0x6bfb));
    assert_eq!(plain.palette.color(1), Some(0x73fd));
    assert_eq!(grass.entry_tilemap[..2], [0x00, 0x00]);
    assert_eq!(plain.entry_tilemap[..2], [0x00, 0x40]);
    std::fs::remove_file(path).unwrap();
}
