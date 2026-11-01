//! Locating the Poké Ball's roots (`src/pokeball.c`).
//!
//! The expectation is the checkout's fixed inventory, which extraction also
//! reads ([`crate::extract::scope::poke_ball_ids`]): the closed-ball sheet, its
//! palette, and the opened-ball sheet. The ROM stores all three as LZ77
//! streams.
//!
//! The walk:
//!
//! 1. The sheets are found by their decompressed tiles ([`super::images`]) and
//!    the palette by its decompressed colours, which can match more than one
//!    stream.
//! 2. `gBallSpriteSheets` and `gBallSpritePalettes` are found through the
//!    pointers to that art: biased back by `BALL_POKE`'s row, which is the
//!    first, they name candidate table bases, and exactly one base must hold
//!    the size and tag upstream writes.
//! 3. The palette's address is the pointer its table row holds, which settles
//!    a bank the ROM stores more than once.
//!
//! The opened-ball sheet has no table row; its tiles alone locate it.

use std::collections::BTreeSet;

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::inventory;
use super::locate::{only_one_matching, to_addr, u16_at_addr, u32_at_addr};
use super::plan::{PalettePlan, PokeBallPlan, ReportLine, Resolution};
use super::tilesets::len32;
use super::Context;
use crate::extract::poke_ball::{OPEN_SHEET_ID, PALETTE_ID, SHEET_ID};

/// One table row, in bytes.
const RECORD_BYTES: u32 = 8;
/// `POKEBALL_COUNT`, the rows each table holds.
const TABLE_ROWS: u32 = 12;
/// The `size` upstream writes for every ball sheet: 12 4bpp tiles.
const SHEET_BYTES: u16 = 384;
/// `GFX_TAG_POKE_BALL`.
const POKE_BALL_TAG: u16 = 55000;
/// Offset of a sheet row's `size` field.
const SHEET_SIZE_FIELD: u32 = 4;
/// Offset of a sheet row's `tag` field.
const SHEET_TAG_FIELD: u32 = 6;
/// Offset of a palette row's `tag` field.
const PALETTE_TAG_FIELD: u32 = 4;

/// What a table's `BALL_POKE` row must carry besides the pointer.
#[derive(Clone, Copy)]
enum Row {
    /// `{data, size: 384, tag: GFX_TAG_POKE_BALL}`.
    Sheet,
    /// `{data, tag: GFX_TAG_POKE_BALL}`.
    Palette,
}

impl Row {
    fn holds(self, ctx: &Context<'_>, row: u32) -> bool {
        match self {
            Self::Sheet => {
                u16_at_addr(ctx.rom, row + SHEET_SIZE_FIELD) == Some(SHEET_BYTES)
                    && u16_at_addr(ctx.rom, row + SHEET_TAG_FIELD) == Some(POKE_BALL_TAG)
            }
            Self::Palette => u16_at_addr(ctx.rom, row + PALETTE_TAG_FIELD) == Some(POKE_BALL_TAG),
        }
    }
}

/// Locate the Poké Ball's sheets, palette, and the two tables.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an absent pack entry,
/// [`GenRomProfileError::StructMismatch`] for a surplus `poke-ball/` pack
/// entry or a table that cannot be pinned to exactly one base;
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`]
/// for art the ROM does not hold as expected.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<PokeBallPlan, GenRomProfileError> {
    inventory::require_poke_ball(ctx.pack)?;

    let mut sheets = locate_images(
        ctx,
        &[ImageQuery::lz77(SHEET_ID), ImageQuery::lz77(OPEN_SHEET_ID)],
        report,
    )?
    .into_iter();
    let (Some(sheet), Some(open_sheet)) = (sheets.next(), sheets.next()) else {
        unreachable!("one plan comes back per query");
    };
    let palette_candidates = palette_candidates(ctx)?;
    let sheet_table = find_table(ctx, "gBallSpriteSheets", &[sheet.addr], Row::Sheet, report)?;
    let palette_table = find_table(
        ctx,
        "gBallSpritePalettes",
        &palette_candidates,
        Row::Palette,
        report,
    )?;
    let palette = palette_plan(ctx, palette_table, &palette_candidates, report)?;

    Ok(PokeBallPlan {
        sheet_table,
        palette_table,
        sheet,
        palette,
        open_sheet,
    })
}

/// Every address the palette's decompressed colours turn up at.
fn palette_candidates(ctx: &Context<'_>) -> Result<Vec<u32>, GenRomProfileError> {
    let needle = ctx
        .pack
        .get(PALETTE_ID)?
        .palette_payload(PALETTE_ID)?
        .0
        .to_vec();
    let hits = ctx.lz77.find_all(&[needle]);
    if hits[0].is_empty() {
        return Err(GenRomProfileError::NotFound {
            id: PALETTE_ID.to_owned(),
        });
    }
    Ok(hits[0].iter().copied().map(to_addr).collect())
}

/// The palette plan for the stream the palette table's first row points at.
fn palette_plan(
    ctx: &Context<'_>,
    palette_table: u32,
    candidates: &[u32],
    report: &mut Vec<ReportLine>,
) -> Result<PalettePlan, GenRomProfileError> {
    let addr = u32_at_addr(ctx.rom, palette_table)
        .filter(|addr| candidates.contains(addr))
        .ok_or_else(|| GenRomProfileError::NotFound {
            id: PALETTE_ID.to_owned(),
        })?;
    let asset = ctx.pack.get(PALETTE_ID)?;
    let mut line =
        ReportLine::unique(PALETTE_ID, addr, len32(&asset.payload)).symbol("gBallPal_Poke");
    if candidates.len() > 1 {
        line = line.with(Resolution::StructDerived).note(format!(
            "{} identical streams in the ROM; the table names this one",
            candidates.len()
        ));
    }
    report.push(line);
    Ok(PalettePlan {
        id: PALETTE_ID.to_owned(),
        addr,
        encoding: Encoding::Lz77,
        color_count: asset.palette_colors(PALETTE_ID)?,
    })
}

/// Find the one table whose first row points at one of `art` with `shape`.
fn find_table(
    ctx: &Context<'_>,
    name: &'static str,
    art: &[u32],
    shape: Row,
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let bases: BTreeSet<u32> = art
        .iter()
        .flat_map(|&addr| ctx.pointers.refs_to(addr).iter().copied())
        .collect();
    let base = only_one_matching(
        name,
        "its BALL_POKE row pointing at the art with its size and tag",
        bases,
        |&base| shape.holds(ctx, base),
    )?;
    report.push(
        ReportLine::unique(name, base, TABLE_ROWS * RECORD_BYTES)
            .with(Resolution::StructDerived)
            .symbol(name)
            .note("anchored by the pointer from its BALL_POKE row"),
    );
    Ok(base)
}

#[cfg(test)]
#[path = "poke_ball_tests.rs"]
mod tests;
