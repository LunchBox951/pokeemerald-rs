//! Extracts the twelve non-default standard PC wallpapers (upstream
//! `sWallpapers` entries 4 to 15, `src/data/wallpapers.h`).
//!
//! Each style's `tiles.4bpp` is not a checkout file: `graphics_file_rules.mk`
//! builds `frame.png` and `bg.png` into 4bpp tiles, each cut to a per-style
//! `-num_tiles` count, and concatenates the frame's tiles before the
//! background's. The pack carries that sheet as one 8-pixel-wide strip, so the
//! tilemap's indices select into it unchanged. The tilemap is the checkout's
//! uncompressed `tilemap.bin` (only the built ROM LZ77-compresses it), and the
//! two palettes are the `PLTE` chunks of `frame.png` and `bg.png`, which is
//! what the `.gbapal` rule converts. Ids come from the typed accessor's
//! [`assets::PcWallpaperId`], so extraction and lookup cannot drift apart.

use std::path::Path;

use assets::PcWallpaperTile;
use pack_format::{PackEntry, PackWriter};

use super::naming_screen::keep_leading_tiles;
use super::scope::{
    pc_wallpaper_ids, PcWallpaperHalf, PcWallpaperSource, PC_WALLPAPERS, PC_WALLPAPER_DIRECTORY,
    PC_WALLPAPER_PALETTE_COLORS, PC_WALLPAPER_TILEMAP_BYTES,
};
use super::{build_image_entry, build_palette_entry, jasc_pal, png, read_file, ExtractError};

/// Every wallpaper PNG is 4bpp.
const SHEET_BIT_DEPTH: u8 = 4;
const TILE_PIXELS: usize = 64;
/// Stored banks of the frame and background palettes.
const PALETTE_BANKS: [u8; 2] = [1, 2];

/// Extracts every non-default wallpaper's sheet, tilemap and palettes.
///
/// Every style is validated before any entry is queued, so a failure leaves
/// the writer untouched.
pub(super) fn extract_pc_wallpapers(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let dir = upstream.join(PC_WALLPAPER_DIRECTORY);
    let mut entries = Vec::new();
    for source in PC_WALLPAPERS {
        entries.extend(extract_style(&dir, &source)?);
    }
    debug_assert_eq!(entries.len(), pc_wallpaper_ids().len());
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

/// One wallpaper's four entries: the sheet, the tilemap, then the frame and
/// background palettes.
fn extract_style(dir: &Path, source: &PcWallpaperSource) -> Result<[PackEntry; 4], ExtractError> {
    let id = source.wallpaper;
    let style_dir = dir.join(id.stem());
    let frame_path = style_dir.join("frame.png");
    let background_path = style_dir.join("bg.png");
    let tilemap_path = style_dir.join("tilemap.bin");

    let frame = read_half(&frame_path, source.frame)?;
    let background = read_half(&background_path, source.background)?;
    let tilemap = read_file(&tilemap_path)?;
    if tilemap.len() != PC_WALLPAPER_TILEMAP_BYTES {
        return Err(mismatch(
            &tilemap_path,
            format!(
                "is {} bytes: expected {PC_WALLPAPER_TILEMAP_BYTES}",
                tilemap.len()
            ),
        ));
    }

    let mut sheet = frame.image;
    sheet.height += background.image.height;
    sheet.pixels.extend(background.image.pixels);
    check_tilemap_words(&tilemap_path, &tilemap, &sheet.pixels)?;

    Ok([
        build_image_entry(&frame_path, id.tiles_id(), sheet)?,
        pack_format::raw_entry(id.tilemap_id(), tilemap),
        build_palette_entry(&frame_path, &frame.colors, id.frame_palette_id())?,
        build_palette_entry(
            &background_path,
            &background.colors,
            id.background_palette_id(),
        )?,
    ])
}

/// One decoded half: its kept tiles as a one-tile-wide strip, and its palette.
struct Half {
    image: png::IndexedImage,
    colors: Vec<jasc_pal::Rgb888>,
}

fn read_half(path: &Path, shape: PcWallpaperHalf) -> Result<Half, ExtractError> {
    let bytes = read_file(path)?;
    let mut image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.to_path_buf(), e))?;
    if image.bit_depth != SHEET_BIT_DEPTH {
        return Err(mismatch(
            path,
            format!("is {}-bit: expected {SHEET_BIT_DEPTH}-bit", image.bit_depth),
        ));
    }
    if (image.width, image.height) != (shape.width, shape.height) {
        return Err(mismatch(
            path,
            format!(
                "is {}x{}: expected {}x{}",
                image.width, image.height, shape.width, shape.height
            ),
        ));
    }
    keep_leading_tiles(&mut image, shape.kept_tiles).map_err(|detail| mismatch(path, detail))?;
    let colors =
        png::decode_palette(&bytes).map_err(|e| ExtractError::Png(path.to_path_buf(), e))?;
    if colors.len() != PC_WALLPAPER_PALETTE_COLORS {
        return Err(mismatch(
            path,
            format!(
                "has {} colours: expected {PC_WALLPAPER_PALETTE_COLORS}",
                colors.len()
            ),
        ));
    }
    Ok(Half { image, colors })
}

/// Refuses the words `AssetPack::pc_wallpaper` would reject, so extraction
/// never publishes an entry the reader refuses.
fn check_tilemap_words(path: &Path, tilemap: &[u8], sheet: &[u8]) -> Result<(), ExtractError> {
    let tile_count = sheet.len() / TILE_PIXELS;
    let mut blank_seen = false;
    for (cell, pair) in tilemap.chunks_exact(2).enumerate() {
        let tile = PcWallpaperTile::decode(u16::from_le_bytes([pair[0], pair[1]]));
        if usize::from(tile.tile_index) >= tile_count {
            return Err(mismatch(
                path,
                format!(
                    "cell {cell} names tile {}: the sheet holds {tile_count}",
                    tile.tile_index
                ),
            ));
        }
        if tile.is_blank() {
            blank_seen = true;
        } else if !PALETTE_BANKS.contains(&tile.palette_bank) {
            return Err(mismatch(
                path,
                format!(
                    "cell {cell} selects palette bank {}: expected {PALETTE_BANKS:?}",
                    tile.palette_bank
                ),
            ));
        }
    }
    if blank_seen && sheet[..TILE_PIXELS].iter().any(|&pixel| pixel != 0) {
        return Err(mismatch(
            path,
            "has blank cells over a tile 0 that is not transparent".to_owned(),
        ));
    }
    Ok(())
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::PcWallpaperAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests;
