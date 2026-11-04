//! Locating the battle-effect sheets and palettes (`src/battle_anim.c`,
//! `src/data/battle_anim.h`).
//!
//! The expectation is the pairs the animation catalog loads
//! ([`expectation::records`]), as the checkout's pack holds them, and the size
//! and tag the checkout's registration rows give each ([`registration`]). The
//! ROM stores every sheet and palette as an LZ77 stream.
//!
//! The walk:
//!
//! 1. Each sheet's decompressed tiles and each palette's decompressed colours
//!    are searched for, and every address that matched is kept per record.
//!    Tags may share art (three tags point at one glowy orb), so no record is
//!    settled by its bytes alone.
//! 2. `gBattleAnimPicTable` and `gBattleAnimPaletteTable` are found through the
//!    pointers to that art: the first record's pointer, biased back by its
//!    row, names the candidate table bases, and exactly one base must hold
//!    *every* record's row with a pointer to one of its candidates and the
//!    size and tag the checkout registers.
//! 3. Each record's address is the pointer its table row holds.

mod expectation;
#[cfg(test)]
pub(super) mod fixture;
mod registration;

use std::collections::BTreeMap;

use assets::pack::BattleEffectRecord;
use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::trim_zero_tiles;
use super::locate::{only_one_matching, to_addr, u16_at_addr, u32_at_addr};
use super::pack_source::image_tiles;
use super::plan::{
    BattleEffectPlan, BattleEffectRecordPlan, ImagePlan, PalettePlan, ReportLine, Resolution,
};
use super::search::MATCH_LIMIT;
use super::tilesets::len32;
use super::Context;
use registration::{Registration, SPRITES_START};

pub(super) use expectation::records as expected_records;

const SHEET_TABLE: &str = "gBattleAnimPicTable";
const PALETTE_TABLE: &str = "gBattleAnimPaletteTable";
/// One table row, in bytes.
const RECORD_BYTES: u32 = 8;
/// `ANIM_SPRITES_COUNT`, the rows each table holds.
const TABLE_ROWS: u32 = assets::battle_anim::EFFECT_SPRITE_COUNT as u32;
const SHEET_SIZE_FIELD: u32 = 4;
const SHEET_TAG_FIELD: u32 = 6;
const PALETTE_TAG_FIELD: u32 = 4;
const BYTES_PER_TILE: usize = 32;

/// What one record's table rows must hold besides the pointer.
#[derive(Clone, Copy)]
enum Row {
    /// `{data, size, tag}`.
    Sheet { size: u16 },
    /// `{data, tag}`.
    Palette,
}

/// A record's row in one table: the art its pointer may name, and the shape.
struct Expected<'a> {
    sprite: u16,
    candidates: &'a [u32],
    shape: Row,
}

impl Expected<'_> {
    /// The table row's address for a table at `base`.
    fn row(&self, base: u32) -> Option<u32> {
        base.checked_add(u32::from(self.sprite).checked_mul(RECORD_BYTES)?)
    }

    fn holds(&self, ctx: &Context<'_>, base: u32) -> bool {
        let Some(row) = self.row(base) else {
            return false;
        };
        let tag = SPRITES_START + self.sprite;
        let named = u32_at_addr(ctx.rom, row).is_some_and(|ptr| self.candidates.contains(&ptr));
        named
            && match self.shape {
                Row::Sheet { size } => {
                    u16_at_addr(ctx.rom, row + SHEET_SIZE_FIELD) == Some(size)
                        && u16_at_addr(ctx.rom, row + SHEET_TAG_FIELD) == Some(tag)
                }
                Row::Palette => u16_at_addr(ctx.rom, row + PALETTE_TAG_FIELD) == Some(tag),
            }
    }
}

/// Locate the battle-effect sheets, palettes, and the two tables.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`],
/// [`GenRomProfileError::WrongPackEntryKind`] or
/// [`GenRomProfileError::EntryShape`] for a pack entry the catalog requires
/// that is absent or malformed, [`GenRomProfileError::StructMismatch`] for a
/// surplus `battle-effect/` entry or a table that cannot be pinned to exactly
/// one base, [`GenRomProfileError::UpstreamSource`] for a checkout that does
/// not register a selected sprite, and [`GenRomProfileError::NotFound`] for art
/// the ROM does not hold.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<BattleEffectPlan, GenRomProfileError> {
    let records = expectation::records(ctx.pack)?;
    let sprites: Vec<u16> = records.iter().map(|record| record.sprite().get()).collect();
    let registrations = registration::load(&ctx.upstream, &sprites)?;

    let sheets = sheet_candidates(ctx, &records)?;
    let palettes = palette_candidates(ctx, &records)?;
    for (record, (sheet, palette)) in records.iter().zip(sheets.iter().zip(&palettes)) {
        for (id, found) in [
            (record.image_id(), sheet.addrs.is_empty()),
            (record.palette_id(), palette.is_empty()),
        ] {
            if found {
                return Err(GenRomProfileError::NotFound { id });
            }
        }
    }

    let sheet_rows: Vec<Expected<'_>> = records
        .iter()
        .zip(&sheets)
        .map(|(record, sheet)| Expected {
            sprite: record.sprite().get(),
            candidates: &sheet.addrs,
            shape: Row::Sheet {
                size: registrations[&record.sprite().get()].sheet_bytes,
            },
        })
        .collect();
    let palette_rows: Vec<Expected<'_>> = records
        .iter()
        .zip(&palettes)
        .map(|(record, addrs)| Expected {
            sprite: record.sprite().get(),
            candidates: addrs,
            shape: Row::Palette,
        })
        .collect();
    let sheet_table = find_table(ctx, SHEET_TABLE, &sheet_rows, report)?;
    let palette_table = find_table(ctx, PALETTE_TABLE, &palette_rows, report)?;

    let mut planned = Vec::with_capacity(records.len());
    for ((record, sheet), palette) in records.iter().zip(&sheets).zip(&palettes) {
        let registration = &registrations[&record.sprite().get()];
        planned.push(record_plan(
            ctx,
            record,
            registration,
            (sheet_table, palette_table),
            (sheet, palette),
            report,
        )?);
    }
    Ok(BattleEffectPlan {
        sheet_table,
        palette_table,
        records: planned,
    })
}

/// A sheet's matches: each address and the tiles the ROM stores there.
struct SheetMatches {
    addrs: Vec<u32>,
    tile_counts: BTreeMap<u32, u32>,
}

/// Every address each record's sheet tiles turn up at, row-major, as 4bpp.
///
/// The ROM may hold a prefix of the sheet when upstream trimmed trailing
/// all-zero tiles, so every length from the last non-zero tile to the whole
/// sheet is searched, and the longest match at an address is its stored length.
fn sheet_candidates(
    ctx: &Context<'_>,
    records: &[BattleEffectRecord],
) -> Result<Vec<SheetMatches>, GenRomProfileError> {
    let mut needles = Vec::new();
    let mut owners = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let tiles = image_tiles(ctx.pack, &record.image_id(), 4, (1, 1))?;
        let kept = trim_zero_tiles(&tiles, BYTES_PER_TILE).max(BYTES_PER_TILE);
        for len in (kept..=tiles.len()).step_by(BYTES_PER_TILE) {
            needles.push(tiles[..len].to_vec());
            owners.push((index, len / BYTES_PER_TILE));
        }
    }
    let hits = ctx
        .lz77
        .find_all_up_to(&needles, records.len().max(MATCH_LIMIT));
    let mut matches: Vec<SheetMatches> = records
        .iter()
        .map(|_| SheetMatches {
            addrs: Vec::new(),
            tile_counts: BTreeMap::new(),
        })
        .collect();
    for ((index, tiles), offsets) in owners.into_iter().zip(hits) {
        let tiles = u32::try_from(tiles).expect("a tile count fits in u32");
        for offset in offsets {
            let slot = matches[index]
                .tile_counts
                .entry(to_addr(offset))
                .or_insert(0);
            *slot = (*slot).max(tiles);
        }
    }
    for found in &mut matches {
        found.addrs = found.tile_counts.keys().copied().collect();
    }
    Ok(matches)
}

/// Every address each record's palette colours turn up at.
fn palette_candidates(
    ctx: &Context<'_>,
    records: &[BattleEffectRecord],
) -> Result<Vec<Vec<u32>>, GenRomProfileError> {
    let mut needles = Vec::with_capacity(records.len());
    for record in records {
        let id = record.palette_id();
        needles.push(ctx.pack.get(&id)?.palette_payload(&id)?.0.to_vec());
    }
    let hits = ctx
        .lz77
        .find_all_up_to(&needles, records.len().max(MATCH_LIMIT));
    Ok(hits
        .into_iter()
        .map(|offsets| offsets.into_iter().map(to_addr).collect())
        .collect())
}

/// Find the one table that holds every `rows` entry.
fn find_table(
    ctx: &Context<'_>,
    name: &'static str,
    rows: &[Expected<'_>],
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let anchor = rows.first().ok_or_else(|| GenRomProfileError::NotFound {
        id: name.to_owned(),
    })?;
    let mut bases: Vec<u32> = anchor
        .candidates
        .iter()
        .flat_map(|&addr| ctx.pointers.refs_to(addr).iter().copied())
        .filter_map(|pointer| pointer.checked_sub(u32::from(anchor.sprite) * RECORD_BYTES))
        .collect();
    bases.sort_unstable();
    bases.dedup();
    let base = only_one_matching(
        name,
        "every selected row pointing at its art with its size and tag",
        bases,
        |&base| rows.iter().all(|row| row.holds(ctx, base)),
    )?;
    report.push(
        ReportLine::unique(name, base, TABLE_ROWS * RECORD_BYTES)
            .with(Resolution::StructDerived)
            .symbol(name)
            .note("anchored by the pointer from its first selected row"),
    );
    Ok(base)
}

/// The plan of one record, its art taken from its table rows.
fn record_plan(
    ctx: &Context<'_>,
    record: &BattleEffectRecord,
    registration: &Registration,
    (sheet_table, palette_table): (u32, u32),
    (sheet, palette): (&SheetMatches, &[u32]),
    report: &mut Vec<ReportLine>,
) -> Result<BattleEffectRecordPlan, GenRomProfileError> {
    let sprite = record.sprite().get();
    let row = |table: u32| table + u32::from(sprite) * RECORD_BYTES;

    let image_id = record.image_id();
    let sheet_addr = u32_at_addr(ctx.rom, row(sheet_table)).expect("the table row was matched");
    let tile_count = sheet.tile_counts[&sheet_addr];
    let (width, height, depth) = ctx.pack.get(&image_id)?.image_shape(&image_id)?;
    report.push(art_line(
        &image_id,
        sheet_addr,
        tile_count * 32,
        sheet.addrs.len(),
        &registration.sheet_symbol,
    ));

    let palette_id = record.palette_id();
    let palette_addr = u32_at_addr(ctx.rom, row(palette_table)).expect("the table row was matched");
    let color_count = ctx.pack.get(&palette_id)?.palette_colors(&palette_id)?;
    report.push(art_line(
        &palette_id,
        palette_addr,
        len32(&ctx.pack.get(&palette_id)?.payload),
        palette.len(),
        &registration.palette_symbol,
    ));

    Ok(BattleEffectRecordPlan {
        sprite,
        sheet_bytes: registration.sheet_bytes,
        sheet: ImagePlan {
            id: image_id,
            addr: sheet_addr,
            encoding: Encoding::Lz77,
            rom_bit_depth: 4,
            pack_bit_depth: depth,
            width,
            height,
            metatile_width: 1,
            metatile_height: 1,
            tile_count,
        },
        palette: PalettePlan {
            id: palette_id,
            addr: palette_addr,
            color_count,
            encoding: Encoding::Lz77,
        },
    })
}

/// A report line for art whose table row named the address among `candidates`.
fn art_line(id: &str, addr: u32, len: u32, candidates: usize, symbol: &str) -> ReportLine {
    let line = ReportLine::unique(id, addr, len).symbol(symbol);
    if candidates > 1 {
        line.with(Resolution::StructDerived).note(format!(
            "{candidates} identical streams in the ROM; the table names this one"
        ))
    } else {
        line
    }
}

#[cfg(test)]
mod tests;
