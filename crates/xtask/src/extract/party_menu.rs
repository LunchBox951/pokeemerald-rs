//! Extracts the party menu's graphics (upstream `graphics/party_menu/`).
//!
//! The manifest is fixed here, so a file it does not name cannot change the pack
//! and a missing one fails extraction. Conversion follows each upstream
//! declaration, with the checkout holding the uncompressed sources of what the
//! ROM build compresses (no decompression happens here):
//!
//! | Records | Declaration | Built form |
//! |---|---|---|
//! | `bg.png` sheet, palette | `gPartyMenuBg_Gfx`/`_Pal`, `src/graphics.c:1111-1112` | LZ77; sheet cut with `-num_tiles 62` |
//! | `bg.bin` | `gPartyMenuBg_Tilemap`, `src/graphics.c:1113` | LZ77, 32x32 `u16` entries |
//! | `pokeball.png` sheet, palette | `gPartyMenuPokeball_Gfx`/`_Pal`, `src/graphics.c:1114-1115` | LZ77, whole 32 tiles |
//! | `slot_*.bin` | `sSlotTilemap_*`, `INCBIN_U8`, `src/data/party_menu.h:565-569` | raw `u8` tile indices |
//! | `confirm_button.bin`, `cancel_button.bin` | `INCBIN_U32`, `src/data/party_menu.h:109-110` | raw 7x2 `u16` entries |
//! | `hold_icons.png` sheet, palette | `sHeldItemGfx`/`sHeldItemPalette`, `src/data/party_menu.h:801-802` | raw `.4bpp`/`.gbapal` |
//!
//! `bg.png` is an 8-bit image: `gbagfx` keeps the low nibble of each index for
//! its 4bpp sheet, the pack publishes the first 62 tiles as an 8-pixel-wide
//! strip in tile order (as the naming screen's cursor sheets are), and all 176
//! colours of its embedded palette are kept, since `LoadPalette` loads eleven
//! banks (`src/party_menu.c:737-750`). The sheet and palette of the Poké Ball
//! and held-item icons come from the PNG's own `PLTE`.
//!
//! `pokeball_small.png` (`gPartyMenuPokeballSmall_Gfx`, `src/graphics.c:1116`)
//! is outside this manifest. Upstream marks it `//unused`, but
//! `sSpriteSheet_MenuPokeballSmall` (`src/data/party_menu.h:981-984`) loads it
//! (`src/party_menu.c:4180`) for the Confirm and Cancel ball sprites drawn when
//! `chooseHalf` is set (`src/party_menu.c:1108-1110`), so a later slice must add
//! it. This extractor never opens it.
//!
//! Ids are `party-menu/{image,palette,raw}/<stem>`.

use std::path::Path;

use super::naming_screen::keep_leading_tiles;
use super::{build_image_entry, build_palette_entry, png, push_raw_bytes};
use super::{read_file, ExtractError};
use pack_format::PackWriter;

const PARTY_MENU_DIRECTORY: &str = "graphics/party_menu";

/// Colours in one 4bpp bank.
const BANK_COLORS: usize = 16;
/// Keeps the low nibble `gbagfx` keeps when it narrows an 8-bit index.
const BANK_INDEX_MASK: u8 = 0x0F;
const BANK_BIT_DEPTH: u8 = 4;

/// A sheet and the palette its PNG embeds.
struct Sheet {
    stem: &'static str,
    /// The PNG's width, height and bit depth.
    source: (u32, u32, u8),
    /// The tiles `-num_tiles` keeps, in tile order; `None` keeps the whole sheet.
    kept_tiles: Option<u32>,
    palette_colors: usize,
}

const SHEETS: [Sheet; 3] = [
    // `gPartyMenuBg_Gfx`: 64 tiles built with `-num_tiles 62`; eleven banks.
    Sheet {
        stem: "bg",
        source: (64, 64, 8),
        kept_tiles: Some(62),
        palette_colors: 11 * BANK_COLORS,
    },
    // `gPartyMenuPokeball_Gfx`: 32 tiles.
    Sheet {
        stem: "pokeball",
        source: (32, 64, BANK_BIT_DEPTH),
        kept_tiles: None,
        palette_colors: BANK_COLORS,
    },
    // `sHeldItemGfx`: 2 tiles.
    Sheet {
        stem: "hold_icons",
        source: (8, 16, BANK_BIT_DEPTH),
        kept_tiles: None,
        palette_colors: BANK_COLORS,
    },
];

/// Raw tilemaps as `(file stem, byte length)`: the 32x32 background screen,
/// the five `u8` slot grids (10x7, 10x7, 18x3, 18x3, 18x3), and the two 7x2
/// `u16` button maps.
const MAPS: [(&str, usize); 8] = [
    ("bg", 32 * 32 * 2),
    ("slot_main", 10 * 7),
    ("slot_main_no_hp", 10 * 7),
    ("slot_wide", 18 * 3),
    ("slot_wide_no_hp", 18 * 3),
    ("slot_wide_empty", 18 * 3),
    ("confirm_button", 7 * 2 * 2),
    ("cancel_button", 7 * 2 * 2),
];

/// Every id the party menu publishes, in extraction order.
pub(crate) fn pack_ids() -> Vec<String> {
    let images = SHEETS
        .iter()
        .map(|sheet| format!("party-menu/image/{}", sheet.stem));
    let palettes = SHEETS
        .iter()
        .map(|sheet| format!("party-menu/palette/{}", sheet.stem));
    let maps = MAPS
        .iter()
        .map(|(stem, _)| format!("party-menu/raw/{stem}"));
    images.chain(palettes).chain(maps).collect()
}

/// Extracts the party menu's sheets, palettes, and tilemaps.
pub(super) fn extract_party_menu(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let queued_before = writer.len();
    let dir = upstream.join(PARTY_MENU_DIRECTORY);
    for sheet in &SHEETS {
        extract_sheet(&dir, sheet, writer)?;
    }
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
    if let Some(kept_tiles) = sheet.kept_tiles {
        keep_leading_tiles(&mut image, kept_tiles).map_err(|detail| mismatch(&path, detail))?;
    }
    let colors = png::decode_palette(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
    if colors.len() != sheet.palette_colors {
        return Err(mismatch(
            &path,
            format!(
                "has {} embedded colours: expected {}",
                colors.len(),
                sheet.palette_colors
            ),
        ));
    }
    writer.push(build_palette_entry(
        &path,
        &colors,
        format!("party-menu/palette/{}", sheet.stem),
    )?);
    writer.push(build_image_entry(
        &path,
        format!("party-menu/image/{}", sheet.stem),
        image,
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
    push_raw_bytes(format!("party-menu/raw/{stem}"), payload, writer);
    Ok(())
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::PartyMenuAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests;
