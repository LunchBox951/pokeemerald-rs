use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use assets::{AssetPack, DoorSize, DoorSound};
use pack_format::PackWriter;

use super::super::png::tests::{grayscale_png_from_raw, indexed_png_from_raw_with_plte};
use super::super::{png, repo_root, upstream_present};
use super::{
    extract_door_animation, pack_ids, records, sheet_width, DIRECTORY, DOORS, SHEETS, TABLE_ID,
};
use crate::extract::ExtractError;

/// The one sheet upstream saves as a 4-bit grayscale PNG.
const GRAYSCALE_STEM: &str = "unused_battle_frontier";

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-door-animation-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(DIRECTORY)).unwrap();
    dir
}

fn source_path(upstream: &Path, stem: &str) -> PathBuf {
    upstream.join(DIRECTORY).join(format!("{stem}.png"))
}

/// A synthetic PNG of the shape `stem` has upstream, its indices running past
/// 15 where the source is 8-bit, so that narrowing matters.
fn sheet_png(stem: &str, height: u32, depth: u8) -> Vec<u8> {
    let width = sheet_width(stem);
    let modulus = if depth == 8 { 64 } else { 16 };
    let value = |x: u32, y: u32| u8::try_from((x + 3 * y + 5) % modulus).unwrap();
    let mut raw = Vec::new();
    for y in 0..height {
        raw.push(0);
        let row: Vec<u8> = (0..width).map(|x| value(x, y)).collect();
        if depth == 4 {
            raw.extend(row.chunks(2).map(|pair| (pair[0] << 4) | pair[1]));
        } else {
            raw.extend(row);
        }
    }
    if stem == GRAYSCALE_STEM {
        return grayscale_png_from_raw(depth, width, height, &raw);
    }
    let colors = if depth == 8 { 64 } else { 16 };
    indexed_png_from_raw_with_plte(depth, width, height, &raw, &vec![8; colors * 3])
}

fn write_valid_checkout(upstream: &Path) {
    for sheet in &SHEETS {
        std::fs::write(
            source_path(upstream, sheet.stem),
            sheet_png(sheet.stem, sheet.height, sheet.source_depth),
        )
        .unwrap();
    }
}

fn extracted(upstream: &Path) -> Vec<u8> {
    let mut writer = PackWriter::new();
    extract_door_animation(upstream, &mut writer).unwrap();
    writer.finish().unwrap()
}

fn load(upstream: &Path) -> AssetPack {
    load_into(upstream, upstream)
}

/// The pack extracted from `upstream`, written under `dir`.
fn load_into(dir: &Path, upstream: &Path) -> AssetPack {
    let path = dir.join("door.pack");
    std::fs::write(&path, extracted(upstream)).unwrap();
    AssetPack::load(&path).unwrap()
}

#[test]
fn the_inventory_is_the_fifty_one_sheets_and_the_table() {
    let ids = pack_ids();
    assert_eq!(ids.len(), 52);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 52);
    assert_eq!(ids.last().map(String::as_str), Some(TABLE_ID));
    for stem in [
        "littleroot",
        "birchs_lab",
        "battle_tower_multi_corridor",
        "battle_tent",
        "unused_battle_frontier",
    ] {
        assert!(
            ids.contains(&format!("door-animation/image/{stem}")),
            "{stem}"
        );
    }
    // No table row names the two unused halves, so no metatile reaches them.
    for stem in ["unused_top", "unused_bottom"] {
        assert!(
            !ids.contains(&format!("door-animation/image/{stem}")),
            "{stem}"
        );
    }
}

#[test]
fn the_manifest_is_consistent() {
    assert_eq!(DOORS.len(), 53);
    let metatiles: BTreeSet<_> = DOORS.iter().map(|door| door.metatile).collect();
    assert_eq!(metatiles.len(), DOORS.len());
    let sheets: BTreeSet<_> = SHEETS.iter().map(|sheet| sheet.stem).collect();
    assert_eq!(sheets.len(), SHEETS.len());
    let used: BTreeSet<_> = DOORS.iter().map(|door| door.sheet).collect();
    assert_eq!(sheets, used, "every sheet is used and every use is a sheet");
    for sheet in &SHEETS {
        let sizes: BTreeSet<_> = DOORS
            .iter()
            .filter(|door| door.sheet == sheet.stem)
            .map(|door| door.size.sheet_width())
            .collect();
        assert_eq!(sizes.len(), 1, "{}", sheet.stem);
    }
    // The three Battle Tent doors share one sheet.
    assert_eq!(DOORS.iter().filter(|d| d.sheet == "battle_tent").count(), 3);
}

#[test]
fn the_checkpoint_doors_carry_upstreams_values() {
    let records = records();
    let find = |metatile| records.iter().find(|r| r.metatile == metatile).unwrap();
    let littleroot = find(0x248);
    assert_eq!(
        (littleroot.sheet.as_str(), littleroot.sound, littleroot.size),
        ("littleroot", DoorSound::Normal, DoorSize::OneTile)
    );
    assert_eq!(littleroot.palette_slots, [10, 10, 6, 6, 6, 6, 6, 6]);
    let lab = find(0x249);
    assert_eq!(lab.sheet, "birchs_lab");
    assert_eq!(lab.palette_slots, [8; 8]);
    let large = find(0x2AD);
    assert_eq!(
        (large.sheet.as_str(), large.sound, large.size),
        (
            "battle_tower_multi_corridor",
            DoorSound::Sliding,
            DoorSize::Large
        )
    );
    assert_eq!(large.palette_slots, [7; 8]);
    assert_eq!(find(0x21B).sound, DoorSound::Arena);
}

#[test]
fn a_complete_checkout_yields_every_sheet_narrowed_and_a_table_the_pack_reads() {
    let dir = scratch_dir("valid");
    write_valid_checkout(&dir);
    let pack = load(&dir);
    assert_eq!(pack.entries().count(), 52);

    let littleroot = pack.door_animation(0x248).unwrap().unwrap();
    assert_eq!((littleroot.sheet.width, littleroot.sheet.height), (16, 96));
    assert_eq!(littleroot.sheet.bit_depth, 4);
    // The source is 8-bit with indices up to 63; only the low nibble is kept.
    let value = |x: u32, y: u32| (x + 3 * y + 5) % 64;
    for (x, y) in [(0, 0), (9, 20), (15, 95)] {
        let at = (y * 16 + x) as usize;
        assert_eq!(u32::from(littleroot.sheet.pixels[at]), value(x, y) & 0x0F);
    }
    assert!(littleroot.sheet.pixels.iter().any(|&p| p != 0));

    let large = pack.door_animation(0x2AD).unwrap().unwrap();
    assert_eq!((large.sheet.width, large.sheet.height), (32, 96));
    assert_eq!(large.size, DoorSize::Large);
    let tent = pack.door_animation(0x393).unwrap().unwrap();
    assert_eq!((tent.sheet.width, tent.sheet.height), (16, 128));
    assert!(pack.door_animation(0x100).unwrap().is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn extraction_is_deterministic() {
    let dir = scratch_dir("determinism");
    write_valid_checkout(&dir);
    assert_eq!(extracted(&dir), extracted(&dir));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_source_fails_naming_it_and_queues_nothing() {
    for sheet in &SHEETS {
        let dir = scratch_dir("missing");
        write_valid_checkout(&dir);
        std::fs::remove_file(source_path(&dir, sheet.stem)).unwrap();
        let mut writer = PackWriter::new();
        let err = extract_door_animation(&dir, &mut writer).unwrap_err();
        let expected = format!("{}.png", sheet.stem);
        assert!(
            matches!(err, ExtractError::ReadFailed(path, _) if path.ends_with(&expected)),
            "{}",
            sheet.stem
        );
        assert_eq!(writer.len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn a_sheet_of_the_wrong_size_or_depth_is_rejected_before_anything_is_queued() {
    let wrong_depth = if SHEETS[0].source_depth == 4 { 8 } else { 4 };
    let cases = [
        ("general", sheet_png("general", 88, 4)),
        ("general", sheet_png("general", 96, wrong_depth)),
        ("battle_tower_multi_corridor", sheet_png("general", 96, 4)),
    ];
    for (stem, bytes) in cases {
        let dir = scratch_dir("shape");
        write_valid_checkout(&dir);
        std::fs::write(source_path(&dir, stem), bytes).unwrap();
        let mut writer = PackWriter::new();
        let err = extract_door_animation(&dir, &mut writer).unwrap_err();
        assert!(
            matches!(err, ExtractError::DoorAnimationAssetMismatch { path, .. } if path.ends_with(format!("{stem}.png"))),
            "{stem}"
        );
        assert_eq!(writer.len(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn a_file_the_manifest_does_not_name_changes_nothing() {
    let dir = scratch_dir("extra");
    write_valid_checkout(&dir);
    let before = extracted(&dir);
    std::fs::write(source_path(&dir, "unused_top"), b"not a png").unwrap();
    std::fs::write(dir.join(DIRECTORY).join("notes.txt"), b"x").unwrap();
    assert_eq!(extracted(&dir), before);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn the_real_checkout_yields_every_door_sheet_pixel_for_pixel() {
    assert!(upstream_present(), "run ./init.sh first");
    let upstream = repo_root().join("pokeemerald");
    let dir = scratch_dir("real");
    let pack = load_into(&dir, &upstream);
    for sheet in &SHEETS {
        let source = std::fs::read(source_path(&upstream, sheet.stem)).unwrap();
        let decoded = png::decode(&source).unwrap();
        let published = pack
            .image(&format!("door-animation/image/{}", sheet.stem))
            .unwrap();
        assert_eq!(published.bit_depth, 4, "{}", sheet.stem);
        assert_eq!(
            (published.width, published.height),
            (decoded.width, decoded.height)
        );
        let narrowed: Vec<u8> = decoded.pixels.iter().map(|p| p & 0x0F).collect();
        assert_eq!(published.pixels, narrowed.as_slice(), "{}", sheet.stem);
    }
    // Littleroot's source is 8-bit with indices above 15.
    let littleroot =
        png::decode(&std::fs::read(source_path(&upstream, "littleroot")).unwrap()).unwrap();
    assert!(littleroot.pixels.iter().any(|&p| p > 15));
    for record in records() {
        let door = pack.door_animation(record.metatile).unwrap().unwrap();
        assert_eq!(door.palette_slots, record.palette_slots);
        assert_eq!(door.size, record.size);
        assert_eq!(door.sound, record.sound);
    }
}
