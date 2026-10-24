//! Locating the battle textbox and menu text palettes.
//!
//! The expectation is the checkout's fixed battle-interface inventory, which
//! extraction also reads ([`crate::extract::scope::battle_interface_ids`]):
//! the textbox sheet, its three palettes, and its tilemap. Every root is found
//! from the pack payload alone.
//!
//! How each is stored is read from upstream's C (`src/graphics.c:4-6,965-966`):
//! the sheet, the textbox and window-text palettes, and the tilemap are LZ77
//! streams, while the PP text palette is a raw `.gbapal` run. The textbox
//! palette is one 32-colour root: `graphics_file_rules.mk` concatenates
//! `textbox_0.pal` and `textbox_1.pal` into the single `gBattleTextboxPalette`.
//! The sheet is 4-bit in the ROM and in the pack, so the located depth needs
//! no split. All five symbols are global, so a linker map names each root.

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::locate::exactly_one;
use super::palettes::{locate_unique, locate_unique_as};
use super::plan::{BattleInterfacePlan, BlobPlan, PalettePlan, ReportLine, SymbolExpectation};
use super::tilesets::len32;
use super::Context;

const IMAGE_PREFIX: &str = "battle-interface/image/";
const PALETTE_PREFIX: &str = "battle-interface/palette/";
const TILEMAP_PREFIX: &str = "battle-interface/raw/";

/// The only palette linked uncompressed.
const RAW_PALETTE: &str = "battle-interface/palette/pp_text";
const TILEMAP_SYMBOL: &str = "gBattleTextboxTilemap";

/// The fixed ids under `prefix`, in id order.
fn ids_under(prefix: &str) -> Vec<String> {
    let mut ids: Vec<String> = crate::extract::scope::battle_interface_ids()
        .into_iter()
        .filter(|id| id.starts_with(prefix))
        .collect();
    ids.sort();
    ids
}

/// The global symbol upstream declares each palette under.
fn palette_symbol(id: &str) -> SymbolExpectation {
    match id {
        "battle-interface/palette/textbox" => {
            SymbolExpectation::Exact("gBattleTextboxPalette".to_owned())
        }
        "battle-interface/palette/window_text" => {
            SymbolExpectation::Exact("gBattleWindowTextPalette".to_owned())
        }
        RAW_PALETTE => SymbolExpectation::Exact("gPPTextPalette".to_owned()),
        _ => SymbolExpectation::Unnamed,
    }
}

/// Locate every battle-interface root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for a surplus
/// `battle-interface/` pack entry, [`GenRomProfileError::WrongPackEntryKind`]
/// or [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`]
/// for a root the ROM does not hold as expected.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<BattleInterfacePlan, GenRomProfileError> {
    super::inventory::require_battle_interface(ctx.pack)?;
    let queries: Vec<ImageQuery> = ids_under(IMAGE_PREFIX)
        .into_iter()
        .map(ImageQuery::lz77)
        .collect();
    Ok(BattleInterfacePlan {
        images: locate_images(ctx, &queries, report)?,
        palettes: locate_palettes(ctx, report)?,
        tilemaps: locate_tilemaps(ctx, report)?,
    })
}

/// Locate each palette under the encoding its symbol is linked with.
fn locate_palettes(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<PalettePlan>, GenRomProfileError> {
    let (raw, compressed): (Vec<String>, Vec<String>) = ids_under(PALETTE_PREFIX)
        .into_iter()
        .partition(|id| id == RAW_PALETTE);
    let mut plans = locate_unique(ctx, &raw, &palette_symbol, report)?;
    plans.extend(locate_unique_as(
        ctx,
        &compressed,
        Encoding::Lz77,
        &palette_symbol,
        report,
    )?);
    plans.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(plans)
}

/// Locate each `battle-interface/raw/*` tilemap as the LZ77 stream it is.
fn locate_tilemaps(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<BlobPlan>, GenRomProfileError> {
    let ids = ids_under(TILEMAP_PREFIX);
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        needles.push(ctx.pack.raw_payload(id)?.to_vec());
    }
    let hits = ctx.lz77.find_all(&needles);

    let mut plans = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let addr = exactly_one(id, &hits[index])?;
        let len = len32(&needles[index]);
        report.push(ReportLine::unique(id, addr, len).symbol(TILEMAP_SYMBOL));
        plans.push(BlobPlan {
            id: id.clone(),
            addr,
            encoding: Encoding::Lz77,
            len,
        });
    }
    Ok(plans)
}

#[cfg(test)]
#[path = "battle_interface_tests.rs"]
mod tests;
