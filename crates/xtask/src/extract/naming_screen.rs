//! Extracts the naming screen's visual assets (upstream
//! `graphics/naming_screen/`).
//!
//! The inventory is the fixed [`scope`](super::scope) manifest, so a file the
//! manifest does not name (`rival.pal`, upstream's unused leftover) cannot
//! change the pack and a missing one fails extraction. Each entry is the
//! decoded form of what upstream's build converts: `menu.png` and the four
//! tilemaps are LZ77-compressed only in the built ROM, and the checkout holds
//! their uncompressed source, so no decompression happens here. The three
//! cursor sheets are built with `-num_tiles 5`; each is published as an 8-pixel
//! wide strip of its first five tiles in tile order, which
//! `pack_format::tiles_from_image` turns back into the built 160 bytes.

use std::path::Path;

use super::scope::{
    naming_screen_ids, NamingScreenSheet, NAMING_SCREEN_DIRECTORY, NAMING_SCREEN_PALETTES,
    NAMING_SCREEN_SHEETS, NAMING_SCREEN_TILEMAPS, NAMING_SCREEN_TILEMAP_BYTES,
};
use super::{build_image_entry, build_palette_entry, jasc_pal, png, push_raw_bytes};
use super::{read_file, read_text, ExtractError};
use pack_format::PackWriter;

const TILE_SIDE: u32 = 8;
/// Every naming-screen sheet is 4bpp.
const SHEET_BIT_DEPTH: u8 = 4;
/// Colours in one 4bpp bank, the size of every source palette.
const BANK_COLORS: usize = 16;

/// Extracts the naming screen's sheets, palettes, and tilemaps.
pub(super) fn extract_naming_screen(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let queued_before = writer.len();
    let dir = upstream.join(NAMING_SCREEN_DIRECTORY);
    for sheet in NAMING_SCREEN_SHEETS {
        extract_sheet(&dir, sheet, writer)?;
    }
    for stem in NAMING_SCREEN_PALETTES {
        extract_palette(&dir, stem, writer)?;
    }
    for stem in NAMING_SCREEN_TILEMAPS {
        extract_tilemap(&dir, stem, &mut read_file, writer)?;
    }
    debug_assert_eq!(writer.len() - queued_before, naming_screen_ids().len());
    Ok(())
}

fn extract_sheet(
    dir: &Path,
    sheet: NamingScreenSheet,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = dir.join(format!("{}.png", sheet.stem));
    let bytes = read_file(&path)?;
    let mut image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if image.bit_depth != SHEET_BIT_DEPTH {
        return Err(mismatch(
            &path,
            format!("is {}-bit: expected {SHEET_BIT_DEPTH}-bit", image.bit_depth),
        ));
    }
    if (image.width, image.height) != (sheet.width, sheet.height) {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {}x{}",
                image.width, image.height, sheet.width, sheet.height
            ),
        ));
    }
    if let Some(kept_tiles) = sheet.kept_tiles {
        keep_leading_tiles(&mut image, kept_tiles).map_err(|detail| mismatch(&path, detail))?;
    }
    writer.push(build_image_entry(
        &path,
        format!("naming-screen/image/{}", sheet.stem),
        image,
    )?);
    Ok(())
}

/// Replaces `image` with a one-tile-wide strip of its first `kept_tiles`
/// tiles in tile order.
///
/// Upstream's `-Wnum_tiles` build keeps the whole sheet when a dropped tile
/// holds any colour, so a sheet that does that is not the cut this models and
/// is refused.
fn keep_leading_tiles(image: &mut png::IndexedImage, kept_tiles: u32) -> Result<(), String> {
    let (width, tiles_per_row) = (image.width, image.width / TILE_SIDE);
    let tile_count = tiles_per_row * (image.height / TILE_SIDE);
    // The pixels of each tile in `tiles`, row by row, in tile order.
    let tile_pixels = |tiles: std::ops::Range<u32>| -> Vec<u8> {
        tiles
            .flat_map(|tile| {
                let (column, tile_row) = (tile % tiles_per_row, tile / tiles_per_row);
                (0..TILE_SIDE).map(move |row| {
                    ((tile_row * TILE_SIDE + row) * width + column * TILE_SIDE) as usize
                })
            })
            .flat_map(|start| &image.pixels[start..start + TILE_SIDE as usize])
            .copied()
            .collect()
    };
    if tile_pixels(kept_tiles..tile_count)
        .iter()
        .any(|&index| index != 0)
    {
        return Err(format!(
            "has colour past its first {kept_tiles} tiles: upstream's cut would keep them"
        ));
    }
    let strip = tile_pixels(0..kept_tiles);
    image.width = TILE_SIDE;
    image.height = kept_tiles * TILE_SIDE;
    image.pixels = strip;
    Ok(())
}

fn extract_palette(dir: &Path, stem: &str, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let path = dir.join(format!("{stem}.pal"));
    let text = read_text(&path)?;
    let colors = jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(path.clone(), e))?;
    if colors.len() != BANK_COLORS {
        return Err(mismatch(
            &path,
            format!("has {} colours: expected {BANK_COLORS}", colors.len()),
        ));
    }
    writer.push(build_palette_entry(
        &path,
        &colors,
        format!("naming-screen/palette/{stem}"),
    )?);
    Ok(())
}

/// The buffer validated is the buffer queued: a second read of the path
/// could publish bytes that changed after the length check.
fn extract_tilemap(
    dir: &Path,
    stem: &str,
    read: &mut dyn FnMut(&Path) -> Result<Vec<u8>, ExtractError>,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = dir.join(format!("{stem}.bin"));
    let payload = read(&path)?;
    if payload.len() != NAMING_SCREEN_TILEMAP_BYTES {
        return Err(mismatch(
            &path,
            format!(
                "is {} bytes: expected {NAMING_SCREEN_TILEMAP_BYTES}",
                payload.len()
            ),
        ));
    }
    push_raw_bytes(format!("naming-screen/raw/{stem}"), payload, writer);
    Ok(())
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::NamingScreenAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests;
