//! Locating the battle textbox, menu text palettes, health-box elements,
//! battle bar, singles health-box frames, enemy shadow, and ball palettes.
//!
//! The expectation is the checkout's fixed battle-interface inventory, which
//! extraction also reads ([`crate::extract::scope::battle_interface_ids`],
//! [`crate::extract::scope::healthbox_element_ids`], and
//! [`crate::extract::scope::healthbox_frame_ids`]): the textbox sheet, its
//! three palettes, its tilemap, the thirteen health-box elements, the battle
//! bar, the two singles frames, the enemy shadow, and the two ball palettes.
//! Every root is found from the pack payload alone.
//!
//! How each is stored is read from upstream's C (`src/graphics.c:4-6,965-966`):
//! the sheet, the textbox and window-text palettes, and the tilemap are LZ77
//! streams, while the PP text palette is a raw `.gbapal` run. The textbox
//! palette is one 32-colour root: `graphics_file_rules.mk` concatenates
//! `textbox_0.pal` and `textbox_1.pal` into the single `gBattleTextboxPalette`.
//! The sheet is 4-bit in the ROM and in the pack, so the located depth needs
//! no split. All five symbols are global, so a linker map names each root.
//!
//! The singles frames, the enemy shadow, and the two ball palettes follow the
//! same rule. `gHealthboxSinglesPlayerGfx`, `gHealthboxSinglesOpponentGfx`
//! (`graphics.c:628-629`, cut as 8x8- and 8x4-tile metatiles) and
//! `gEnemyMonShadow_Gfx` (`graphics.c:1037`) are `.4bpp.lz` streams, found by
//! the generic image search, which recovers each frame's own metatile shape
//! because a wrong walk permutes the tiles. `gBattleInterface_BallStatusBarPal`
//! and `gBattleInterface_BallDisplayPal` (`graphics.c:354,356`) are
//! uncompressed `.gbapal` runs of the 16 colours embedded in their PNGs, so
//! each is a unique raw 16-colour hit, like the PP text palette. All five are
//! global symbols.

use rom_import::{Encoding, HEALTHBOX_BATTLE_BAR, HEALTHBOX_ELEMENTS, HEALTHBOX_TILE_BYTES};

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::locate::exactly_one;
use super::palettes::{locate_unique, locate_unique_as};
use super::plan::{BattleInterfacePlan, BlobPlan, PalettePlan, ReportLine, SymbolExpectation};
use super::tilesets::len32;
use super::Context;
use crate::extract::scope::{healthbox_battle_bar_id, healthbox_frame_ids, healthbox_table_ids};

const IMAGE_PREFIX: &str = "battle-interface/image/";
const PALETTE_PREFIX: &str = "battle-interface/palette/";
const TILEMAP_PREFIX: &str = "battle-interface/raw/";

/// The palettes linked uncompressed: the PP text palette (`gPPTextPalette`)
/// and the two ball palettes (`src/graphics.c:354,356`).
const RAW_PALETTES: [&str; 3] = [
    "battle-interface/palette/pp_text",
    "battle-interface/palette/ball_status_bar",
    "battle-interface/palette/ball_display",
];
const TILEMAP_SYMBOL: &str = "gBattleTextboxTilemap";
const ELEMENT_TABLE_SYMBOL: &str = "gHealthboxElementsGfxTable";
const BATTLE_BAR_SYMBOL: &str = "gBattleInterfaceGfx_BattleBar";

/// The fixed textbox and singles-frame ids under `prefix`, in id order.
fn ids_under(prefix: &str) -> Vec<String> {
    let mut ids: Vec<String> = crate::extract::scope::battle_interface_ids()
        .into_iter()
        .chain(healthbox_frame_ids())
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
        "battle-interface/palette/pp_text" => SymbolExpectation::Exact("gPPTextPalette".to_owned()),
        "battle-interface/palette/ball_status_bar" => {
            SymbolExpectation::Exact("gBattleInterface_BallStatusBarPal".to_owned())
        }
        "battle-interface/palette/ball_display" => {
            SymbolExpectation::Exact("gBattleInterface_BallDisplayPal".to_owned())
        }
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
        healthbox_elements: locate_element_table(ctx, report)?,
        battle_bar: locate_battle_bar(ctx, report)?,
    })
}

/// Locate each palette under the encoding its symbol is linked with.
fn locate_palettes(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<PalettePlan>, GenRomProfileError> {
    let (raw, compressed): (Vec<String>, Vec<String>) = ids_under(PALETTE_PREFIX)
        .into_iter()
        .partition(|id| RAW_PALETTES.contains(&id.as_str()));
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

/// Refuse a record that is not the table row or bar the importer will check:
/// the id the importer expects at this position, and exactly `tiles` 4bpp tiles.
/// A payload with the right concatenation but a shifted boundary would otherwise
/// produce roots every import rejects.
fn check_tiles(
    id: &str,
    expected_id: &str,
    payload: &[u8],
    tiles: u32,
) -> Result<(), GenRomProfileError> {
    if id != expected_id {
        return Err(GenRomProfileError::StructMismatch {
            id: id.to_owned(),
            reason: format!("is not the importer's `{expected_id}` at this position"),
        });
    }
    // Compared as `usize`: a pack payload may exceed `u32`, which `len32`
    // would panic on.
    let expected = tiles * HEALTHBOX_TILE_BYTES;
    if usize::try_from(expected).ok() != Some(payload.len()) {
        return Err(GenRomProfileError::StructMismatch {
            id: id.to_owned(),
            reason: format!(
                "holds {} bytes: the table row is {tiles} tiles, {expected} bytes",
                payload.len()
            ),
        });
    }
    Ok(())
}

/// Locate the thirteen element records as consecutive slices of the one raw
/// run `gHealthboxElementsGfxTable` is.
fn locate_element_table(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<BlobPlan>, GenRomProfileError> {
    let ids = healthbox_table_ids();
    let mut payloads = Vec::with_capacity(ids.len());
    for (id, (expected_id, tiles)) in ids.iter().zip(HEALTHBOX_ELEMENTS) {
        let payload = ctx.pack.raw_payload(id)?;
        check_tiles(id, expected_id, payload, tiles)?;
        payloads.push(payload.to_vec());
    }
    let run: Vec<u8> = payloads.concat();
    let first = &ids[0];
    let hits = ctx.raw.find_all(&[run]);
    let base = exactly_one(first, &hits[0])?;

    let mut plans = Vec::with_capacity(ids.len());
    let mut addr = base;
    for (id, payload) in ids.iter().zip(&payloads) {
        let len = len32(payload);
        let line = ReportLine::unique(id, addr, len);
        report.push(if addr == base {
            line.symbol(ELEMENT_TABLE_SYMBOL)
                .note("first row of the thirteen-record table, found as their concatenation")
        } else {
            line.interior()
        });
        plans.push(BlobPlan {
            id: id.clone(),
            addr,
            encoding: Encoding::Raw,
            len,
        });
        addr += len;
    }
    Ok(plans)
}

/// Locate the battle bar as the LZ77 stream of its concatenated sheets.
fn locate_battle_bar(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<BlobPlan, GenRomProfileError> {
    let id = healthbox_battle_bar_id();
    let payload = ctx.pack.raw_payload(&id)?;
    let (expected_id, tiles) = HEALTHBOX_BATTLE_BAR;
    check_tiles(&id, expected_id, payload, tiles)?;
    let len = len32(payload);
    let payload = payload.to_vec();
    let hits = ctx.lz77.find_all(&[payload]);
    let addr = exactly_one(&id, &hits[0])?;
    report.push(ReportLine::unique(&id, addr, len).symbol(BATTLE_BAR_SYMBOL));
    Ok(BlobPlan {
        id,
        addr,
        encoding: Encoding::Lz77,
        len,
    })
}

#[cfg(test)]
#[path = "battle_interface_frame_tests.rs"]
mod frame_tests;
#[cfg(test)]
#[path = "battle_interface_healthbox_tests.rs"]
mod healthbox_tests;
#[cfg(test)]
#[path = "battle_interface_inventory_tests.rs"]
mod inventory_tests;
#[cfg(test)]
#[path = "battle_interface_tests.rs"]
mod tests;
