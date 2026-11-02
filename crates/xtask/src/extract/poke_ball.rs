//! The Poké Ball's presentation art: the closed-ball sheet with its palette
//! and the opened-ball sheet (`gBallGfx_Poke`, `gBallPal_Poke`, and
//! `gOpenPokeballGfx`, `src/data/graphics/pokeballs.h`).
//!
//! Upstream links no palette of its own for `open.png`; the opened ball is
//! drawn with the Poké Ball's palette, so the pack carries only that one. A
//! 4-bit index can never leave a 16-colour palette, so the bit depth and the
//! colour count are the whole index check.

use std::path::{Path, PathBuf};

use pack_format::{PackEntry, PackWriter};

use super::{build_image_entry, build_palette_entry, png, read_file, ExtractError};

pub(crate) const SHEET_ID: &str = "poke-ball/image/poke";
pub(crate) const PALETTE_ID: &str = "poke-ball/palette/poke";
pub(crate) const OPEN_SHEET_ID: &str = "poke-ball/image/open";

const SHEET_PNG: &str = "graphics/balls/poke.png";
const OPEN_SHEET_PNG: &str = "graphics/balls/open.png";

const SHEET_SIZE: (u32, u32) = (16, 48);
const OPEN_SHEET_SIZE: (u32, u32) = (16, 16);
const BIT_DEPTH: u8 = 4;
const PALETTE_COLORS: usize = 16;

/// Every pack id [`extract_poke_ball`] emits. The profile generator reads
/// this as its fixed inventory so extraction and generation cannot drift apart.
pub(crate) fn pack_ids() -> Vec<String> {
    [SHEET_ID, PALETTE_ID, OPEN_SHEET_ID]
        .map(str::to_owned)
        .to_vec()
}

/// Extracts the Poké Ball's sheet, its embedded palette, and the opened sheet.
///
/// All three entries are validated before any is queued, so a failure leaves
/// the writer untouched.
pub(super) fn extract_poke_ball(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let sheet_path = upstream.join(SHEET_PNG);
    let sheet_bytes = read_file(&sheet_path)?;
    let sheet = decode_sheet(&sheet_path, &sheet_bytes, SHEET_SIZE)?;
    let colors =
        png::decode_palette(&sheet_bytes).map_err(|e| ExtractError::Png(sheet_path.clone(), e))?;
    if colors.len() != PALETTE_COLORS {
        return Err(mismatch(
            &sheet_path,
            format!("has {} colours: expected {PALETTE_COLORS}", colors.len()),
        ));
    }

    let open_path = upstream.join(OPEN_SHEET_PNG);
    let open = decode_sheet(&open_path, &read_file(&open_path)?, OPEN_SHEET_SIZE)?;

    let entries: [PackEntry; 3] = [
        build_image_entry(&sheet_path, SHEET_ID.to_owned(), sheet)?,
        build_palette_entry(&sheet_path, &colors, PALETTE_ID.to_owned())?,
        build_image_entry(&open_path, OPEN_SHEET_ID.to_owned(), open)?,
    ];
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

fn decode_sheet(
    path: &Path,
    bytes: &[u8],
    (width, height): (u32, u32),
) -> Result<png::IndexedImage, ExtractError> {
    let image = png::decode(bytes).map_err(|e| ExtractError::Png(path.to_path_buf(), e))?;
    if (image.width, image.height) != (width, height) {
        return Err(mismatch(
            path,
            format!(
                "is {}x{}: expected {width}x{height}",
                image.width, image.height
            ),
        ));
    }
    if image.bit_depth != BIT_DEPTH {
        return Err(mismatch(
            path,
            format!("is {}-bit: expected {BIT_DEPTH}-bit", image.bit_depth),
        ));
    }
    Ok(image)
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::PokeBallAssetMismatch {
        path: PathBuf::from(path),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use assets::AssetPack;
    use pack_format::PackWriter;

    use super::super::png::tests::indexed_png_from_raw_with_plte;
    use super::super::{extract_to, upstream_present};
    use super::{
        extract_poke_ball, pack_ids, BIT_DEPTH, OPEN_SHEET_PNG, OPEN_SHEET_SIZE, PALETTE_COLORS,
        SHEET_PNG, SHEET_SIZE,
    };
    use crate::extract::ExtractError;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-poke-ball-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 4-bit PNG whose every pixel is `pixel` and whose palette holds
    /// `colors` colours, each channel `shade`.
    fn png_bytes(size: (u32, u32), pixel: u8, colors: usize, shade: u8) -> Vec<u8> {
        let row_bytes = (size.0 as usize).div_ceil(2);
        let mut raw = Vec::new();
        for _ in 0..size.1 {
            raw.push(0);
            raw.resize(raw.len() + row_bytes, pixel << 4 | pixel);
        }
        indexed_png_from_raw_with_plte(BIT_DEPTH, size.0, size.1, &raw, &vec![shade; colors * 3])
    }

    fn write(dir: &std::path::Path, relative: &str, bytes: &[u8]) {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn write_valid_checkout(dir: &std::path::Path) {
        write(dir, SHEET_PNG, &png_bytes(SHEET_SIZE, 3, PALETTE_COLORS, 8));
        write(
            dir,
            OPEN_SHEET_PNG,
            &png_bytes(OPEN_SHEET_SIZE, 9, PALETTE_COLORS, 8),
        );
    }

    #[test]
    fn the_manifest_names_the_sheet_palette_and_open_sheet() {
        assert_eq!(
            pack_ids(),
            [
                "poke-ball/image/poke",
                "poke-ball/palette/poke",
                "poke-ball/image/open"
            ]
        );
    }

    #[test]
    fn a_valid_checkout_yields_a_typed_ball_with_the_sheets_pixels_and_colours() {
        let dir = scratch_dir("valid");
        write_valid_checkout(&dir);
        let mut writer = PackWriter::new();
        extract_poke_ball(&dir, &mut writer).unwrap();
        let path = dir.join("pack.bin");
        std::fs::write(&path, writer.finish().unwrap()).unwrap();

        let pack = AssetPack::load(&path).unwrap();
        let ball = pack.poke_ball().unwrap();
        assert_eq!((ball.sheet.image.width, ball.sheet.image.height), (16, 48));
        assert!(ball.sheet.image.pixels.iter().all(|&pixel| pixel == 3));
        assert_eq!(ball.sheet.palette.color_count, 16);
        // Channel 8 becomes 8 >> 3 = 1 in each BGR555 field.
        assert_eq!(ball.sheet.palette.color(0), Some(0b00001_00001_00001));
        assert!(ball.open_sheet.pixels.iter().all(|&pixel| pixel == 9));

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_source_fails_without_queueing_anything() {
        for missing in [SHEET_PNG, OPEN_SHEET_PNG] {
            let dir = scratch_dir("missing");
            write_valid_checkout(&dir);
            std::fs::remove_file(dir.join(missing)).unwrap();
            let mut writer = PackWriter::new();

            let err = extract_poke_ball(&dir, &mut writer).unwrap_err();
            assert!(
                matches!(err, ExtractError::ReadFailed(path, _) if path.ends_with(missing)),
                "{missing}"
            );
            assert_eq!(writer.len(), 0);

            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn a_source_of_the_wrong_shape_fails_without_queueing_anything() {
        let cases: [(&str, Vec<u8>); 4] = [
            (SHEET_PNG, png_bytes(OPEN_SHEET_SIZE, 3, PALETTE_COLORS, 8)),
            (OPEN_SHEET_PNG, png_bytes(SHEET_SIZE, 9, PALETTE_COLORS, 8)),
            (SHEET_PNG, png_bytes(SHEET_SIZE, 3, 15, 8)),
            (SHEET_PNG, png_bytes(SHEET_SIZE, 3, 8, 8)),
        ];
        for (index, (target, bytes)) in cases.into_iter().enumerate() {
            let dir = scratch_dir("shape");
            write_valid_checkout(&dir);
            write(&dir, target, &bytes);
            let mut writer = PackWriter::new();

            let err = extract_poke_ball(&dir, &mut writer).unwrap_err();
            assert!(
                matches!(&err, ExtractError::PokeBallAssetMismatch { path, .. } if path.ends_with(target)),
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
            "pokeemerald-rs-poke-ball-real-{}.bin",
            std::process::id()
        ));
        extract_to(&path).unwrap();
        let pack = AssetPack::load(&path).unwrap();
        let ball = pack.poke_ball().unwrap();
        assert_eq!((ball.sheet.image.width, ball.sheet.image.height), (16, 48));
        assert_eq!(ball.sheet.image.pixels.len(), 768);
        assert_eq!(ball.sheet.palette.color_count, 16);
        assert_eq!((ball.open_sheet.width, ball.open_sheet.height), (16, 16));
        std::fs::remove_file(path).unwrap();
    }
}
