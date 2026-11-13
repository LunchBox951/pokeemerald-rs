//! Extracts the summary screen's graphics (upstream `graphics/summary_screen/`).
//!
//! The manifest is fixed here, so a file it does not name cannot change the pack
//! and a missing one fails extraction. Conversion follows each upstream
//! declaration, with the checkout holding the uncompressed sources of what the
//! ROM build compresses (no decompression happens here):
//!
//! | Records | Declaration | Built form |
//! |---|---|---|
//! | `tiles.png` sheet, palette | `gSummaryScreen_Gfx`/`_Pal`, `src/graphics.c:1128-1129` | LZ77, whole 240 tiles |
//! | `page_*.bin` | `gSummaryPage_*_Tilemap`, `src/graphics.c:1130-1134` | LZ77, 32x32 `u16` entries |
//! | `move_select.png` sheet, palette | `gSummaryMoveSelect_Gfx`/`_Pal`, `src/graphics.c:1125-1126` | LZ77, whole 32 tiles |
//! | `effect_*.bin` | `gSummaryScreen_MoveEffect_*_Tilemap`, `INCBIN_U16`, `src/graphics.c:1253-1255` | raw `u16` entries |
//! | `status_tilemap.bin` | `sStatusTilemap`, `INCBIN_U16`, `src/pokemon_summary_screen.c:369` | raw 10x2 `u16` entries |
//! | `markings.pal` | `sMarkings_Pal`, `src/pokemon_summary_screen.c:1097` | raw `.gbapal` |
//! | `a_button.png`, `b_button.png` | `sButtons_Gfx`, `src/pokemon_summary_screen.c:725-728` | raw `.4bpp`, four tiles each |
//!
//! `tiles.png` is an 8-bit image: `gbagfx` keeps the low nibble of each index for
//! its 4bpp sheet, so the pack narrows the same way, publishes the whole raster
//! (no `-num_tiles` cut), and keeps all 128 colours of its embedded palette,
//! since the screen loads eight banks (`src/pokemon_summary_screen.c:1354`). The
//! move-select palette comes from its PNG's own `PLTE`. The button declarations
//! build only `.4bpp`, so the button sheets publish no palette.
//!
//! The battle and contest move-effect tilemaps store 80 entries and the cancel
//! tilemap 64, though the sliding window that draws them reads fewer
//! (`src/pokemon_summary_screen.c:388-405`); each is published in full.
//!
//! Ids are `summary-screen-graphics/{image,palette,raw}/<stem>`.

use std::path::Path;

use super::{build_image_entry, build_palette_entry, jasc_pal, png, push_raw_bytes};
use super::{read_file, read_text, ExtractError};
use pack_format::PackWriter;

const SUMMARY_SCREEN_DIRECTORY: &str = "graphics/summary_screen";

/// Colours in one 4bpp bank.
const BANK_COLORS: usize = 16;
/// Keeps the low nibble `gbagfx` keeps when it narrows an 8-bit index.
const BANK_INDEX_MASK: u8 = 0x0F;
const BANK_BIT_DEPTH: u8 = 4;
/// Bytes of one `u16` tile entry.
const ENTRY_BYTES: usize = 2;

/// A sheet and the palette its PNG embeds, when upstream builds one.
struct Sheet {
    stem: &'static str,
    /// The PNG's width, height and bit depth; the published image is the same
    /// size at 4 bits.
    source: (u32, u32, u8),
    /// The embedded colours the palette keeps; `None` publishes no palette.
    palette_colors: Option<usize>,
}

const SHEETS: [Sheet; 4] = [
    // `gSummaryScreen_Gfx`: 240 tiles; eight banks.
    Sheet {
        stem: "tiles",
        source: (128, 120, 8),
        palette_colors: Some(8 * BANK_COLORS),
    },
    // `gSummaryMoveSelect_Gfx`: 32 tiles.
    Sheet {
        stem: "move_select",
        source: (16, 128, BANK_BIT_DEPTH),
        palette_colors: Some(BANK_COLORS),
    },
    // `sButtons_Gfx[0]` and `[1]`: four tiles each, `.4bpp` only.
    Sheet {
        stem: "a_button",
        source: (16, 16, BANK_BIT_DEPTH),
        palette_colors: None,
    },
    Sheet {
        stem: "b_button",
        source: (16, 16, BANK_BIT_DEPTH),
        palette_colors: None,
    },
];

/// The JASC palette `sMarkings_Pal` builds.
const MARKINGS_STEM: &str = "markings";

/// Raw tilemaps as `(file stem, byte length)`: the five 32x32 page screens, the
/// three move-effect maps, and the 10x2 status map.
const MAPS: [(&str, usize); 9] = [
    ("page_info", 32 * 32 * ENTRY_BYTES),
    ("page_info_egg", 32 * 32 * ENTRY_BYTES),
    ("page_skills", 32 * 32 * ENTRY_BYTES),
    ("page_battle_moves", 32 * 32 * ENTRY_BYTES),
    ("page_contest_moves", 32 * 32 * ENTRY_BYTES),
    ("effect_battle", 80 * ENTRY_BYTES),
    ("effect_contest", 80 * ENTRY_BYTES),
    ("effect_cancel", 64 * ENTRY_BYTES),
    ("status_tilemap", 10 * 2 * ENTRY_BYTES),
];

/// Every id the summary screen's graphics publish, in extraction order.
pub(crate) fn pack_ids() -> Vec<String> {
    let images = SHEETS
        .iter()
        .map(|sheet| format!("summary-screen-graphics/image/{}", sheet.stem));
    let palettes = SHEETS
        .iter()
        .filter(|sheet| sheet.palette_colors.is_some())
        .map(|sheet| format!("summary-screen-graphics/palette/{}", sheet.stem))
        .chain([format!("summary-screen-graphics/palette/{MARKINGS_STEM}")]);
    let maps = MAPS
        .iter()
        .map(|(stem, _)| format!("summary-screen-graphics/raw/{stem}"));
    images.chain(palettes).chain(maps).collect()
}

/// Extracts the summary screen's sheets, palettes, and tilemaps.
pub(super) fn extract_summary_screen_graphics(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let queued_before = writer.len();
    let dir = upstream.join(SUMMARY_SCREEN_DIRECTORY);
    for sheet in &SHEETS {
        extract_sheet(&dir, sheet, writer)?;
    }
    extract_markings(&dir, writer)?;
    for (stem, len) in MAPS {
        extract_map(&dir, stem, len, &mut read_file, writer)?;
    }
    debug_assert_eq!(writer.len() - queued_before, pack_ids().len());
    Ok(())
}

fn extract_sheet(dir: &Path, sheet: &Sheet, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let path = dir.join(format!("{}.png", sheet.stem));
    let bytes = read_file(&path)?;
    let mut image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
    let (width, height, bit_depth) = sheet.source;
    if (image.width, image.height) != (width, height) {
        return Err(mismatch(
            &path,
            format!(
                "is {}x{}: expected {width}x{height}",
                image.width, image.height
            ),
        ));
    }
    if image.bit_depth != bit_depth {
        return Err(mismatch(
            &path,
            format!("is {}-bit: expected {bit_depth}-bit", image.bit_depth),
        ));
    }
    if bit_depth != BANK_BIT_DEPTH {
        for pixel in &mut image.pixels {
            *pixel &= BANK_INDEX_MASK;
        }
        image.bit_depth = BANK_BIT_DEPTH;
    }
    if let Some(expected) = sheet.palette_colors {
        let colors = png::decode_palette(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
        if colors.len() != expected {
            return Err(mismatch(
                &path,
                format!("has {} embedded colours: expected {expected}", colors.len()),
            ));
        }
        writer.push(build_palette_entry(
            &path,
            &colors,
            format!("summary-screen-graphics/palette/{}", sheet.stem),
        )?);
    }
    writer.push(build_image_entry(
        &path,
        format!("summary-screen-graphics/image/{}", sheet.stem),
        image,
    )?);
    Ok(())
}

fn extract_markings(dir: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let path = dir.join(format!("{MARKINGS_STEM}.pal"));
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
        format!("summary-screen-graphics/palette/{MARKINGS_STEM}"),
    )?);
    Ok(())
}

/// The buffer validated is the buffer queued: a second read of the path
/// could publish bytes that changed after the length check.
fn extract_map(
    dir: &Path,
    stem: &str,
    expected_len: usize,
    read: &mut dyn FnMut(&Path) -> Result<Vec<u8>, ExtractError>,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let path = dir.join(format!("{stem}.bin"));
    let payload = read(&path)?;
    if payload.len() != expected_len {
        return Err(mismatch(
            &path,
            format!("is {} bytes: expected {expected_len}", payload.len()),
        ));
    }
    push_raw_bytes(
        format!("summary-screen-graphics/raw/{stem}"),
        payload,
        writer,
    );
    Ok(())
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::SummaryScreenGraphicsAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests;
