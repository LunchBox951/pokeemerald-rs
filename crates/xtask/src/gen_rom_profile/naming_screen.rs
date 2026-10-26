//! Locating the naming screen's sheets, palettes, and tilemaps.
//!
//! The expectation is the checkout's fixed naming-screen inventory, which
//! extraction also reads ([`crate::extract::scope::naming_screen_ids`]):
//! fifteen sheets, seven palettes, and four tilemaps. Every root is found from
//! the pack payload alone.
//!
//! How each is stored is read from upstream's C (`src/graphics.c:1394-1419`,
//! `src/naming_screen.c:185-187`):
//!
//! - `menu.png` is a `.4bpp.lz` stream; the other fourteen sheets are raw
//!   `.4bpp`, two of them (`pc_icon_*`) `static` in `naming_screen.c`.
//! - The three cursor sheets are built with `-num_tiles 5`, so the ROM stores
//!   the first five tiles. The pack publishes those as an 8x40 strip, which
//!   the located tile count of 5 reproduces without a special case.
//! - Six palettes are the rows of `gNamingScreenMenu_Pal[6][16]`, in the order
//!   [`MENU_ROWS`] lists; `sKeyboard_Pal` is a separate `static`.
//! - The four tilemaps are `.lz` streams.
//!
//! Nothing here picks among repeated bytes. A sheet or the keyboard palette
//! must turn up at exactly one address ([`locate_unique_images`]); the six
//! menu banks are settled by the array they form, not one by one, so a bank
//! copied elsewhere (or two equal rows) cannot move an ordinal row.

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_unique_images, ImageQuery};
use super::locate::exactly_one;
use super::lz77_blobs::locate_lz77_blobs;
use super::palettes::locate_unique;
use super::plan::{
    BlobPlan, NamingScreenPlan, PalettePlan, ReportLine, Resolution, SymbolExpectation,
};
use super::tilesets::len32;
use super::Context;
use crate::extract::scope::{
    naming_screen_ids, NAMING_SCREEN_SHEETS, NAMING_SCREEN_TILEMAPS, NAMING_SCREEN_TILEMAP_BYTES,
};

const IMAGE_PREFIX: &str = "naming-screen/image/";
const PALETTE_PREFIX: &str = "naming-screen/palette/";
const TILEMAP_PREFIX: &str = "naming-screen/raw/";

/// The only sheet stored as an LZ77 stream.
const COMPRESSED_IMAGE: &str = "naming-screen/image/menu";
/// The palette `static` in `naming_screen.c:187`, outside the menu array.
const KEYBOARD_PALETTE: &str = "naming-screen/palette/keyboard";
/// Colours in each bank, and the array's row stride in bytes.
const BANK_COLORS: u16 = 16;
const BANK_BYTES: u32 = 32;
/// `gNamingScreenMenu_Pal`'s rows in array order (`graphics.c:1394-1402`):
/// stems under `naming-screen/palette/`.
const MENU_ROWS: [&str; 6] = [
    "menu",
    "page_swap_upper",
    "page_swap_lower",
    "page_swap_others",
    "buttons",
    "cursor",
];
const MENU_TABLE_SYMBOL: &str = "gNamingScreenMenu_Pal";

/// The fixed ids under `prefix`, in id order.
fn ids_under(prefix: &str) -> Vec<String> {
    let mut ids: Vec<String> = naming_screen_ids()
        .into_iter()
        .filter(|id| id.starts_with(prefix))
        .collect();
    ids.sort();
    ids
}

/// Locate every naming-screen root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for a surplus
/// `naming-screen/` pack entry, [`GenRomProfileError::WrongPackEntryKind`]
/// or [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`]
/// for a root the ROM does not hold exactly once.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<NamingScreenPlan, GenRomProfileError> {
    super::inventory::require_naming_screen(ctx.pack)?;
    validate_shapes(ctx)?;
    let queries: Vec<ImageQuery> = ids_under(IMAGE_PREFIX)
        .into_iter()
        .map(|id| {
            if id == COMPRESSED_IMAGE {
                ImageQuery::lz77(id)
            } else {
                ImageQuery::raw(id)
            }
        })
        .collect();
    Ok(NamingScreenPlan {
        images: locate_unique_images(ctx, &queries, report)?,
        palettes: locate_palettes(ctx, report)?,
        tilemaps: locate_tilemaps(ctx, report)?,
    })
}

/// Refuse a pack entry that is not the shape the profile table will declare,
/// before any report line is written.
///
/// A pack built from another extractor could hold a valid entry that is not
/// the cut upstream stores (a 16x24 cursor, a short palette, a 640-byte
/// tilemap); the located bytes would then describe a different asset.
fn validate_shapes(ctx: &Context<'_>) -> Result<(), GenRomProfileError> {
    for sheet in NAMING_SCREEN_SHEETS {
        let id = format!("{IMAGE_PREFIX}{}", sheet.stem);
        // A cursor sheet is published as the strip of its kept tiles.
        let expected = match sheet.kept_tiles {
            Some(kept) => (8, kept * 8),
            None => (sheet.width, sheet.height),
        };
        let (_, width, height, bit_depth) = ctx.pack.get(&id)?.image_raster(&id)?;
        if (width, height, bit_depth) != (expected.0, expected.1, 4) {
            return Err(GenRomProfileError::EntryShape {
                id,
                reason: format!(
                    "is {width}x{height} at {bit_depth} bits: the naming screen stores \
                     {}x{} at 4 bits",
                    expected.0, expected.1
                ),
            });
        }
    }
    for id in ids_under(PALETTE_PREFIX) {
        let (payload, colors) = ctx.pack.get(&id)?.palette_payload(&id)?;
        if colors != BANK_COLORS || len32(payload) != BANK_BYTES {
            return Err(GenRomProfileError::EntryShape {
                id,
                reason: format!(
                    "holds {colors} colours: every naming-screen palette is a {BANK_COLORS}-colour bank"
                ),
            });
        }
    }
    for id in ids_under(TILEMAP_PREFIX) {
        let len = ctx.pack.raw_payload(&id)?.len();
        if len != NAMING_SCREEN_TILEMAP_BYTES {
            return Err(GenRomProfileError::EntryShape {
                id,
                reason: format!(
                    "is {len} bytes: a naming-screen tilemap is {NAMING_SCREEN_TILEMAP_BYTES}"
                ),
            });
        }
    }
    Ok(())
}

/// Locate the six menu rows as one array, then the keyboard palette.
fn locate_palettes(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<PalettePlan>, GenRomProfileError> {
    let mut plans = locate_menu_table(ctx, report)?;
    plans.extend(locate_unique(
        ctx,
        &[KEYBOARD_PALETTE.to_owned()],
        // `sKeyboard_Pal` is `static` (`naming_screen.c:187`).
        &|_| SymbolExpectation::Local("sKeyboard_Pal".to_owned()),
        report,
    )?);
    plans.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(plans)
}

/// The six `gNamingScreenMenu_Pal` rows, found as one 192-byte signature.
///
/// Each row's bytes may repeat elsewhere (a bank is only 32 bytes), so no row
/// is searched alone: the array is the unit upstream declares, and row `n` is
/// `base + n * 32`.
fn locate_menu_table(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<PalettePlan>, GenRomProfileError> {
    let mut table = Vec::with_capacity(MENU_ROWS.len() * BANK_BYTES as usize);
    let mut rows = Vec::with_capacity(MENU_ROWS.len());
    for stem in MENU_ROWS {
        let id = format!("{PALETTE_PREFIX}{stem}");
        table.extend_from_slice(ctx.pack.get(&id)?.palette_payload(&id)?.0);
        rows.push(id);
    }
    let hits = ctx.raw.find_all(std::slice::from_ref(&table));
    let base = exactly_one(&rows[0], &hits[0])?;

    let mut plans = Vec::with_capacity(rows.len());
    for (row, id) in (0u32..).zip(&rows) {
        let addr = base + row * BANK_BYTES;
        let mut line = ReportLine::unique(id, addr, BANK_BYTES)
            .with(Resolution::StructDerived)
            .note(format!(
                "row {row} of {MENU_TABLE_SYMBOL}[6][16], found as the whole 6-row array"
            ));
        line = if row == 0 {
            line.symbol(MENU_TABLE_SYMBOL)
        } else {
            line.interior()
        };
        report.push(line);
        plans.push(PalettePlan {
            id: id.clone(),
            addr,
            color_count: BANK_COLORS,
            encoding: Encoding::Raw,
        });
    }
    Ok(plans)
}

/// Locate each `naming-screen/raw/*` tilemap as the LZ77 stream it is.
fn locate_tilemaps(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<BlobPlan>, GenRomProfileError> {
    debug_assert_eq!(
        ids_under(TILEMAP_PREFIX).len(),
        NAMING_SCREEN_TILEMAPS.len()
    );
    locate_lz77_blobs(ctx, &ids_under(TILEMAP_PREFIX), &tilemap_symbol, report)
}

/// `gNamingScreen<Name>_Tilemap` (`graphics.c:1416-1419`), all global.
fn tilemap_symbol(id: &str) -> SymbolExpectation {
    let stem = id.strip_prefix(TILEMAP_PREFIX).unwrap_or(id);
    SymbolExpectation::Exact(format!(
        "gNamingScreen{}_Tilemap",
        super::locate::camel_case(stem)
    ))
}

#[cfg(test)]
#[path = "naming_screen_tests/mod.rs"]
mod tests;
