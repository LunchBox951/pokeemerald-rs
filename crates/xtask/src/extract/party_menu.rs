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
//! | `pokeball.png` sheet, palette | `gPartyMenuPokeball_Gfx` (`:1115`), `gPartyMenuPokeball_Pal` (`:1117`), `src/graphics.c` | LZ77, whole 32 tiles |
//! | `pokeball_small.png` sheet | `gPartyMenuPokeballSmall_Gfx`, `src/graphics.c:1116` | LZ77, whole 24 tiles |
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
//! `pokeball_small.png` is marked `//unused` upstream, but
//! `sSpriteSheet_MenuPokeballSmall` (`src/data/party_menu.h:981-984`) loads it
//! (`src/party_menu.c:4180`) for the Confirm and Cancel ball sprites drawn when
//! `chooseHalf` is set (`src/party_menu.c:1108-1110`). Its sprite template uses
//! the Poké Ball palette tag (`src/data/party_menu.h:990`), so the pack holds
//! no palette for it: the extractor requires its `PLTE` to equal
//! `pokeball.png`'s and publishes the image alone.
//!
//! Ids are `party-menu/{image,palette,raw}/<stem>`.

use std::path::Path;

use super::jasc_pal;
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

/// Where a sheet's palette comes from.
#[derive(Clone, Copy)]
enum PaletteSource {
    /// The sheet publishes the palette its PNG embeds, of this many colours.
    Own { colors: usize },
    /// The sheet draws with the palette of `SHEETS[with]`, listed earlier: its
    /// PNG must embed the same colours and the pack holds no palette for it.
    Shared { with: usize },
}

/// A sheet and the palette its PNG embeds.
struct Sheet {
    stem: &'static str,
    /// The PNG's width, height and bit depth.
    source: (u32, u32, u8),
    /// The tiles `-num_tiles` keeps, in tile order; `None` keeps the whole sheet.
    kept_tiles: Option<u32>,
    palette: PaletteSource,
}

const SHEETS: [Sheet; 4] = [
    // `gPartyMenuBg_Gfx`: 64 tiles built with `-num_tiles 62`; eleven banks.
    Sheet {
        stem: "bg",
        source: (64, 64, 8),
        kept_tiles: Some(62),
        palette: PaletteSource::Own {
            colors: 11 * BANK_COLORS,
        },
    },
    // `gPartyMenuPokeball_Gfx`: 32 tiles.
    Sheet {
        stem: "pokeball",
        source: (32, 64, BANK_BIT_DEPTH),
        kept_tiles: None,
        palette: PaletteSource::Own {
            colors: BANK_COLORS,
        },
    },
    // `sHeldItemGfx`: 2 tiles.
    Sheet {
        stem: "hold_icons",
        source: (8, 16, BANK_BIT_DEPTH),
        kept_tiles: None,
        palette: PaletteSource::Own {
            colors: BANK_COLORS,
        },
    },
    // `gPartyMenuPokeballSmall_Gfx`: 24 tiles, drawn with `gPartyMenuPokeball_Pal`.
    Sheet {
        stem: "pokeball_small",
        source: (16, 96, BANK_BIT_DEPTH),
        kept_tiles: None,
        palette: PaletteSource::Shared {
            with: POKEBALL_SHEET,
        },
    },
];

/// `pokeball` in [`SHEETS`].
const POKEBALL_SHEET: usize = 1;

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
        .filter(|sheet| matches!(sheet.palette, PaletteSource::Own { .. }))
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
    let mut decoded: Vec<Vec<jasc_pal::Rgb888>> = Vec::with_capacity(SHEETS.len());
    for sheet in &SHEETS {
        let shared = match sheet.palette {
            PaletteSource::Own { .. } => &[],
            PaletteSource::Shared { with } => decoded[with].as_slice(),
        };
        let colors = extract_sheet(&dir, sheet, shared, writer)?;
        decoded.push(colors);
    }
    for (stem, len) in MAPS {
        extract_map(&dir, stem, len, &mut read_file, writer)?;
    }
    debug_assert_eq!(writer.len() - queued_before, pack_ids().len());
    Ok(())
}

/// Queues `sheet`'s image, and its palette unless it shares one, and returns
/// the colours its PNG embeds. `shared` is the palette a
/// [`PaletteSource::Shared`] sheet must match; other sheets ignore it.
fn extract_sheet(
    dir: &Path,
    sheet: &Sheet,
    shared: &[jasc_pal::Rgb888],
    writer: &mut PackWriter,
) -> Result<Vec<jasc_pal::Rgb888>, ExtractError> {
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
    match sheet.palette {
        PaletteSource::Own { colors: expected } => {
            if colors.len() != expected {
                return Err(mismatch(
                    &path,
                    format!("has {} embedded colours: expected {expected}", colors.len()),
                ));
            }
            writer.push(build_palette_entry(
                &path,
                &colors,
                format!("party-menu/palette/{}", sheet.stem),
            )?);
        }
        PaletteSource::Shared { with } => {
            if colors != shared {
                return Err(mismatch(
                    &path,
                    format!(
                        "embeds a palette that differs from `{}.png`'s, which it draws with",
                        SHEETS[with].stem
                    ),
                ));
            }
        }
    }
    writer.push(build_image_entry(
        &path,
        format!("party-menu/image/{}", sheet.stem),
        image,
    )?);
    Ok(colors)
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
