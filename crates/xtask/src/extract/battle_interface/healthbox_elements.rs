//! Extracts the health-box element table and the battle bar as raw 4bpp.
//!
//! Sixteen source sheets become fourteen records: the thirteen constituents of
//! `gHealthboxElementsGfxTable` (`graphics.c:358-370`) and the battle bar,
//! which `graphics_file_rules.mk:90-93` concatenates from `hpbar_anim_unused`,
//! `numbers1`, and `numbers2`. Each sheet is converted as `gbagfx` writes a
//! `.4bpp`: 8x8 tiles in row-major order, the left pixel in the low nibble.

use std::path::Path;

use super::{mismatch, push_raw_bytes, read_file, ExtractError, DIRECTORY, RAW_PREFIX};
use crate::extract::png::{self, IndexedImage};
use pack_format::PackWriter;

const TILE_SIDE: u32 = 8;
const NIBBLE_MASK: u8 = 0x0F;
const HIGH_NIBBLE_SHIFT: u32 = 4;

/// A source sheet and the size its upstream PNG has.
#[derive(Clone, Copy, Debug)]
struct Sheet {
    stem: &'static str,
    width: u32,
    height: u32,
}

const fn sheet(stem: &'static str, width: u32, height: u32) -> Sheet {
    Sheet {
        stem,
        width,
        height,
    }
}

/// The constituents of `gHealthboxElementsGfxTable`, in table order.
const ELEMENT_TABLE: [Sheet; 13] = [
    sheet("hpbar", 96, 8),
    sheet("expbar", 72, 8),
    sheet("status", 24, 40),
    sheet("misc", 88, 8),
    sheet("hpbar_anim", 144, 8),
    sheet("misc_frameend", 8, 8),
    sheet("ball_display", 32, 8),
    sheet("ball_caught_indicator", 8, 8),
    sheet("status2", 24, 40),
    sheet("status3", 24, 40),
    sheet("status4", 24, 40),
    sheet("healthbox_doubles_frameend", 8, 8),
    sheet("healthbox_doubles_frameend_bar", 8, 8),
];

/// The sources of `battle_bar.4bpp`, in the order the rule concatenates them.
const BATTLE_BAR_SOURCES: [Sheet; 3] = [
    sheet("hpbar_anim_unused", 144, 8),
    sheet("numbers1", 88, 8),
    sheet("numbers2", 96, 8),
];

/// The thirteen element pack ids in `gHealthboxElementsGfxTable` order.
pub(in crate::extract) fn table_pack_ids() -> Vec<String> {
    ELEMENT_TABLE
        .iter()
        .map(|element| format!("{RAW_PREFIX}/healthbox_element/{}", element.stem))
        .collect()
}

/// The battle bar's pack id.
pub(in crate::extract) fn battle_bar_pack_id() -> String {
    format!("{RAW_PREFIX}/battle_bar")
}

/// Every pack id [`extract_healthbox_elements`] emits: the thirteen table
/// elements in table order, then the battle bar.
pub(in crate::extract) fn pack_ids() -> Vec<String> {
    let mut ids = table_pack_ids();
    ids.push(battle_bar_pack_id());
    ids
}

/// Extracts the element table and the battle bar.
pub(in crate::extract) fn extract_healthbox_elements(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    for element in ELEMENT_TABLE {
        let tiles = convert_sheet(upstream, element, &mut read_file)?;
        push_raw_bytes(
            format!("{RAW_PREFIX}/healthbox_element/{}", element.stem),
            tiles,
            writer,
        );
    }
    let mut battle_bar = Vec::new();
    for source in BATTLE_BAR_SOURCES {
        battle_bar.extend(convert_sheet(upstream, source, &mut read_file)?);
    }
    push_raw_bytes(battle_bar_pack_id(), battle_bar, writer);
    Ok(())
}

/// The buffer decoded is the buffer converted: `read` is called once per sheet.
fn convert_sheet(
    upstream: &Path,
    sheet: Sheet,
    read: &mut dyn FnMut(&Path) -> Result<Vec<u8>, ExtractError>,
) -> Result<Vec<u8>, ExtractError> {
    let path = upstream.join(DIRECTORY).join(format!("{}.png", sheet.stem));
    let bytes = read(&path)?;
    let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if (image.width, image.height) != (sheet.width, sheet.height) {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {}x{}",
                image.width, image.height, sheet.width, sheet.height
            ),
        ));
    }
    Ok(tiles_4bpp(&image))
}

/// Row-major 8x8 tiles, four bytes per pixel row with the left pixel low.
///
/// An 8-bit sheet keeps only each index's low nibble, as `gbagfx` does:
/// `status.png` and its copies index past 15.
fn tiles_4bpp(image: &IndexedImage) -> Vec<u8> {
    let width = image.width as usize;
    let side = TILE_SIDE as usize;
    let mut out = Vec::with_capacity(image.pixels.len() / 2);
    for tile_y in 0..image.height as usize / side {
        for tile_x in 0..width / side {
            for row in 0..side {
                let start = (tile_y * side + row) * width + tile_x * side;
                for pair in image.pixels[start..start + side].chunks_exact(2) {
                    let (left, right) = (pair[0] & NIBBLE_MASK, pair[1] & NIBBLE_MASK);
                    out.push(left | (right << HIGH_NIBBLE_SHIFT));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use assets::pack::HealthboxElement;
    use assets::AssetPack;

    use super::super::super::png::tests::{grayscale_png_from_raw, indexed_png_from_raw_with_plte};
    use super::super::super::{extract_to, repo_root, upstream_present};
    use super::{
        convert_sheet, extract_healthbox_elements, pack_ids, tiles_4bpp, Sheet, BATTLE_BAR_SOURCES,
        ELEMENT_TABLE, TILE_SIDE,
    };
    use super::{png, DIRECTORY};
    use crate::extract::ExtractError;
    use pack_format::PackWriter;

    const TILE_BYTES: usize = 32;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-healthbox-elements-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(DIRECTORY)).unwrap();
        dir
    }

    /// An image whose pixel `(x, y)` is `pixel(x, y)`.
    fn image(width: u32, height: u32, pixel: impl Fn(u32, u32) -> u8) -> png::IndexedImage {
        png::IndexedImage {
            width,
            height,
            bit_depth: 4,
            pixels: (0..height)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|(x, y)| pixel(x, y))
                .collect(),
            palette: vec![],
        }
    }

    /// The 4bpp nibble of pixel `(x, y)`, found through tile and row arithmetic
    /// rather than the converter's loop order.
    fn nibble_at(bytes: &[u8], width: u32, x: u32, y: u32) -> u8 {
        let tiles_per_row = width / TILE_SIDE;
        let tile = (y / TILE_SIDE) * tiles_per_row + x / TILE_SIDE;
        let offset = tile * 32 + (y % TILE_SIDE) * 4 + (x % TILE_SIDE) / 2;
        let byte = bytes[offset as usize];
        if x.is_multiple_of(2) {
            byte & 0x0F
        } else {
            byte >> 4
        }
    }

    /// A PNG of `sheet` filled with palette index `fill`, in the colour type and
    /// bit depth the upstream file uses.
    fn sheet_png(sheet: Sheet, fill: u8) -> Vec<u8> {
        let row_bytes = |depth: u8| sheet.width as usize * usize::from(depth) / 8;
        let rows = |sample: u8, depth: u8| {
            let packed = if depth == 4 {
                sample << 4 | sample
            } else {
                sample
            };
            let mut raw = Vec::new();
            for _ in 0..sheet.height {
                raw.push(0);
                raw.extend(vec![packed; row_bytes(depth)]);
            }
            raw
        };
        if sheet.stem.starts_with("numbers") {
            // Grayscale samples run opposite to indices (`gbagfx` inverts them).
            grayscale_png_from_raw(4, sheet.width, sheet.height, &rows(15 - fill, 4))
        } else if ["status", "status2", "status3", "status4", "hpbar_anim"].contains(&sheet.stem) {
            indexed_png_from_raw_with_plte(8, sheet.width, sheet.height, &rows(fill, 8), &[7; 48])
        } else {
            indexed_png_from_raw_with_plte(4, sheet.width, sheet.height, &rows(fill, 4), &[7; 48])
        }
    }

    fn write_sheet(dir: &Path, sheet: Sheet, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = dir.join(DIRECTORY).join(format!("{}.png", sheet.stem));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// Table sheet `n` is filled with index `n + 1`; the battle bar sources
    /// with 10, 11, and 12.
    fn write_valid_checkout(dir: &Path) {
        for (sheet, fill) in ELEMENT_TABLE.into_iter().zip(1..) {
            write_sheet(dir, sheet, sheet_png(sheet, fill));
        }
        for (sheet, fill) in BATTLE_BAR_SOURCES.into_iter().zip(10..) {
            write_sheet(dir, sheet, sheet_png(sheet, fill));
        }
    }

    fn mismatch_path(err: ExtractError) -> PathBuf {
        match err {
            ExtractError::BattleInterfaceAssetMismatch { path, .. } => path,
            other => panic!("expected a battle-interface mismatch, got {other}"),
        }
    }

    #[test]
    fn tiles_are_row_major_with_the_left_pixel_in_the_low_nibble() {
        // A 16x16 sheet of four tiles, each filled with its row-major index.
        let sheet = image(16, 16, |x, y| u8::from(y >= 8) * 2 + u8::from(x >= 8));
        let bytes = tiles_4bpp(&sheet);
        assert_eq!(bytes.len(), 4 * TILE_BYTES);
        for (tile, chunk) in bytes.chunks(TILE_BYTES).enumerate() {
            let fill = u8::try_from(tile).unwrap() * 0x11;
            assert_eq!(chunk, [fill; TILE_BYTES].as_slice(), "tile {tile}");
        }

        // Pixels 0..8 of one row become 0x10, 0x32, 0x54, 0x76.
        let row = image(8, 8, |x, _| u8::try_from(x).unwrap());
        assert_eq!(tiles_4bpp(&row)[..4], [0x10, 0x32, 0x54, 0x76]);
    }

    #[test]
    fn conversion_agrees_with_per_pixel_addressing_on_an_asymmetric_sheet() {
        let sheet = image(24, 40, |x, y| u8::try_from((x * 3 + y * 7) % 16).unwrap());
        let bytes = tiles_4bpp(&sheet);
        assert_eq!(bytes.len(), 15 * TILE_BYTES);
        for y in 0..40 {
            for x in 0..24 {
                assert_eq!(
                    nibble_at(&bytes, 24, x, y),
                    sheet.pixels[(y * 24 + x) as usize],
                    "({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn every_sheet_converts_to_its_tile_count_in_32_byte_tiles() {
        let dir = scratch_dir("lengths");
        write_valid_checkout(&dir);
        let tile_counts = [12, 9, 15, 11, 18, 1, 4, 1, 15, 15, 15, 1, 1];
        for (sheet, tiles) in ELEMENT_TABLE.into_iter().zip(tile_counts) {
            let bytes = convert_sheet(&dir, sheet, &mut crate::extract::read_file).unwrap();
            assert_eq!(bytes.len(), tiles * TILE_BYTES, "{}", sheet.stem);
        }
        let bar_lengths: Vec<_> = BATTLE_BAR_SOURCES
            .into_iter()
            .map(|sheet| {
                convert_sheet(&dir, sheet, &mut crate::extract::read_file)
                    .unwrap()
                    .len()
            })
            .collect();
        assert_eq!(
            bar_lengths,
            [18 * TILE_BYTES, 11 * TILE_BYTES, 12 * TILE_BYTES]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_valid_checkout_yields_fourteen_records_the_typed_handles_read() {
        let dir = scratch_dir("valid");
        write_valid_checkout(&dir);
        let mut writer = PackWriter::new();
        extract_healthbox_elements(&dir, &mut writer).unwrap();
        assert_eq!(writer.len(), 14);
        let pack_path = dir.join("healthbox-elements.pack");
        std::fs::write(&pack_path, writer.finish().unwrap()).unwrap();

        let pack = AssetPack::load(&pack_path).unwrap();
        let elements = pack.healthbox_elements().unwrap();
        // Upstream order: sheet `n` carries index `n + 1`, so byte `(n + 1) * 0x11`.
        for (ordinal, (element, bytes)) in elements.sheets().enumerate() {
            let fill = u8::try_from(ordinal + 1).unwrap() * 0x11;
            assert_eq!(element, HealthboxElement::TABLE[ordinal]);
            assert!(bytes.iter().all(|&byte| byte == fill), "{element:?}");
        }
        // Source sheets with indices 10, 11, and 12 concatenate in rule order.
        let expected_bar = [
            vec![0xAA; 18 * TILE_BYTES],
            vec![0xBB; 11 * TILE_BYTES],
            vec![0xCC; 12 * TILE_BYTES],
        ]
        .concat();
        assert_eq!(pack.battle_bar().unwrap().tiles, expected_bar.as_slice());

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_fourteen_record_ids_are_the_table_stems_and_the_battle_bar() {
        let dir = scratch_dir("ids");
        write_valid_checkout(&dir);
        let mut writer = PackWriter::new();
        extract_healthbox_elements(&dir, &mut writer).unwrap();
        let pack = writer.finish().unwrap();
        let mut ids: Vec<String> = pack_format::parse_directory(&pack)
            .unwrap()
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        let mut expected: Vec<String> = ELEMENT_TABLE
            .iter()
            .map(|sheet| format!("battle-interface/raw/healthbox_element/{}", sheet.stem))
            .chain(["battle-interface/raw/battle_bar".to_owned()])
            .collect();
        let mut declared = pack_ids();
        ids.sort();
        expected.sort();
        declared.sort();
        assert_eq!(ids, expected);
        assert_eq!(declared, expected);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_source_in_a_complete_checkout_fails_closed() {
        for sheet in ELEMENT_TABLE.into_iter().chain(BATTLE_BAR_SOURCES) {
            let dir = scratch_dir("missing-one");
            write_valid_checkout(&dir);
            std::fs::remove_file(dir.join(DIRECTORY).join(format!("{}.png", sheet.stem))).unwrap();
            let mut writer = PackWriter::new();
            let err = extract_healthbox_elements(&dir, &mut writer).unwrap_err();
            assert!(
                matches!(&err, ExtractError::ReadFailed(path, _)
                    if path.ends_with(format!("{}.png", sheet.stem))),
                "{}: {err}",
                sheet.stem
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn a_sheet_that_is_not_the_upstream_shape_is_rejected() {
        let dir = scratch_dir("shape");
        let sheet = ELEMENT_TABLE[0];
        let path = write_sheet(&dir, sheet, b"not a png");
        let read = &mut crate::extract::read_file;
        let err = convert_sheet(&dir, sheet, read).unwrap_err();
        assert!(matches!(err, ExtractError::Png(error_path, _) if error_path == path));

        let wrong_size = Sheet { width: 88, ..sheet };
        write_sheet(&dir, sheet, sheet_png(wrong_size, 1));
        let err = convert_sheet(&dir, sheet, read).unwrap_err();
        assert_eq!(mismatch_path(err), path);

        write_sheet(&dir, sheet, sheet_png(sheet, 1));
        convert_sheet(&dir, sheet, read).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_wide_index_keeps_its_low_nibble_like_gbagfx() {
        let dir = scratch_dir("wide-index");
        let sheet = Sheet {
            stem: "status",
            width: 24,
            height: 40,
        };
        write_sheet(&dir, sheet, sheet_png(sheet, 0x1C));
        let bytes = convert_sheet(&dir, sheet, &mut crate::extract::read_file).unwrap();
        assert!(bytes.iter().all(|&byte| byte == 0xCC));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn grayscale_samples_are_inverted_like_gbagfx() {
        let dir = scratch_dir("grayscale");
        let sheet = BATTLE_BAR_SOURCES[1];
        write_sheet(&dir, sheet, sheet_png(sheet, 3));
        let bytes = convert_sheet(&dir, sheet, &mut crate::extract::read_file).unwrap();
        assert!(bytes.iter().all(|&byte| byte == 0x33));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_sheet_validated_is_the_sheet_converted() {
        let dir = scratch_dir("single-read");
        let sheet = ELEMENT_TABLE[5];
        let good = sheet_png(sheet, 2);
        let mut reads = 0;
        let mut read = |_: &Path| {
            reads += 1;
            // Any read after the first sees bytes that cannot decode.
            Ok(if reads == 1 { good.clone() } else { vec![0] })
        };
        let bytes = convert_sheet(&dir, sheet, &mut read).unwrap();
        assert_eq!(reads, 1);
        assert_eq!(bytes, [0x22; TILE_BYTES]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
    fn healthbox_elements_are_extracted() {
        assert!(upstream_present(), "run ./init.sh first");
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-test-healthbox-elements-{}.pack",
            std::process::id()
        ));
        extract_to(&path).expect("extraction should succeed against a real checkout");
        let pack = AssetPack::load(&path).unwrap();
        let elements = pack.healthbox_elements().unwrap();
        let bar = pack.battle_bar().unwrap();

        let upstream = repo_root().join("pokeemerald").join(DIRECTORY);
        let decode = |sheet: Sheet| {
            let bytes = std::fs::read(upstream.join(format!("{}.png", sheet.stem))).unwrap();
            png::decode(&bytes).unwrap()
        };
        let assert_pixels = |sheet: Sheet, tiles: &[u8]| {
            let source = decode(sheet);
            assert_eq!(tiles.len(), source.pixels.len() / 2, "{}", sheet.stem);
            for y in 0..source.height {
                for x in 0..source.width {
                    assert_eq!(
                        nibble_at(tiles, source.width, x, y),
                        source.pixels[(y * source.width + x) as usize] & 0x0F,
                        "{} ({x}, {y})",
                        sheet.stem
                    );
                }
            }
        };
        for (sheet, (_, tiles)) in ELEMENT_TABLE.into_iter().zip(elements.sheets()) {
            assert_pixels(sheet, tiles);
        }
        let mut rest = bar.tiles;
        for sheet in BATTLE_BAR_SOURCES {
            let (tiles, tail) = rest.split_at(decode(sheet).pixels.len() / 2);
            assert_pixels(sheet, tiles);
            rest = tail;
        }
        assert!(rest.is_empty());
        assert_eq!(bar.tiles.len(), 41 * TILE_BYTES);
        let _ = std::fs::remove_file(path);
    }
}
