//! Locating the non-default standard PC wallpapers (`src/data/wallpapers.h`).
//!
//! The expectation is the checkout's fixed inventory, which extraction also
//! reads ([`crate::extract::scope::PC_WALLPAPERS`]): per wallpaper a tile
//! sheet, a tilemap and the frame and background palettes. The ROM stores the
//! sheet and tilemap as LZ77 streams and the two palettes as one raw 64-byte
//! `u16[2][16]` array.
//!
//! The walk:
//!
//! 1. Sheets are found by their decompressed tiles ([`super::images`]),
//!    tilemaps by their decompressed bytes, and each palette pair by its
//!    frame and background colours together; any can match more than one copy.
//! 2. `sWallpapers` holds `{tiles, tilemap, palettes}` rows indexed by
//!    `WALLPAPER_*`. Biased back by the first wallpaper's own row, the
//!    pointers to its art name candidate table bases, and exactly one base
//!    must hold, at every wallpaper's row, pointers to that wallpaper's art.
//! 3. Each address is the pointer its row holds, which settles art the ROM
//!    stores more than once. The background palette is the 32 bytes after the
//!    frame palette, never a separate match.

use std::collections::BTreeSet;

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_image_copies, ImageQuery};
use super::inventory;
use super::locate::{camel_case, only_one_matching, to_addr, u32_at_addr};
use super::plan::{
    BlobPlan, PalettePlan, PcWallpaperPlan, PcWallpaperRecordPlan, ReportLine, Resolution,
    SymbolExpectation,
};
use super::tilesets::len32;
use super::Context;
use crate::extract::scope::PC_WALLPAPERS;

/// One `struct Wallpaper` row: three pointers.
const RECORD_BYTES: u32 = 12;
/// `WALLPAPER_COUNT - 1`, the rows `sWallpapers` holds.
const TABLE_ROWS: u32 = 16;
const TILEMAP_FIELD: u32 = 4;
const PALETTES_FIELD: u32 = 8;
/// One 16-colour palette, in bytes.
const BANK_BYTES: u32 = 32;
const BANK_COLORS: u16 = 16;

/// Where one wallpaper's art may sit, before the table decides.
struct Candidates {
    row: u32,
    tiles: Vec<u32>,
    tilemaps: Vec<u32>,
    palettes: Vec<u32>,
}

/// Locate every non-default wallpaper's sheet, tilemap, palettes and `sWallpapers`.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an absent pack entry,
/// [`GenRomProfileError::StructMismatch`] for a surplus `pc-wallpaper/` pack
/// entry or a table that cannot be pinned to exactly one base;
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`] for
/// art the ROM does not hold as expected.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<PcWallpaperPlan, GenRomProfileError> {
    inventory::require_pc_wallpaper(ctx.pack)?;

    let queries: Vec<ImageQuery> = PC_WALLPAPERS
        .iter()
        .map(|source| ImageQuery::lz77(source.wallpaper.tiles_id()))
        .collect();
    let sheet_copies = locate_image_copies(ctx, &queries)?;
    let tilemaps = tilemap_candidates(ctx)?;
    let palettes = palette_candidates(ctx)?;

    let candidates: Vec<Candidates> = PC_WALLPAPERS
        .iter()
        .enumerate()
        .map(|(index, source)| Candidates {
            row: u32::from(source.wallpaper.index()),
            tiles: sheet_copies[index].addrs(),
            tilemaps: tilemaps[index].clone(),
            palettes: palettes[index].clone(),
        })
        .collect();
    let table = find_table(ctx, &candidates)?;
    report.push(
        ReportLine::unique("sWallpapers", table, TABLE_ROWS * RECORD_BYTES)
            .with(Resolution::StructDerived)
            .local_symbol("sWallpapers")
            .note("anchored by the pointers from its wallpaper rows"),
    );

    let mut wallpapers = Vec::with_capacity(PC_WALLPAPERS.len());
    for (index, source) in PC_WALLPAPERS.iter().enumerate() {
        let id = source.wallpaper;
        let wanted = &candidates[index];
        let row = table + wanted.row * RECORD_BYTES;
        let missing = |id: String| GenRomProfileError::NotFound { id };
        let tiles_addr = u32_at_addr(ctx.rom, row).ok_or_else(|| missing(id.tiles_id()))?;
        let tilemap_addr =
            u32_at_addr(ctx.rom, row + TILEMAP_FIELD).ok_or_else(|| missing(id.tilemap_id()))?;
        let palettes_addr = u32_at_addr(ctx.rom, row + PALETTES_FIELD)
            .ok_or_else(|| missing(id.frame_palette_id()))?;

        let upstream = upstream_name(id.stem());
        let first_line = report.len();
        let tiles = sheet_copies[index].plan_at(ctx, tiles_addr, report)?;
        report[first_line].symbol = SymbolExpectation::Local(format!("sWallpaperTiles_{upstream}"));

        let tilemap_id = id.tilemap_id();
        let tilemap = tilemap_plan(
            ctx,
            &tilemap_id,
            tilemap_addr,
            &wanted.tilemaps,
            &upstream,
            report,
        )?;
        let (frame_palette, background_palette) = palette_plans(
            ctx,
            &[id.frame_palette_id(), id.background_palette_id()],
            palettes_addr,
            &wanted.palettes,
            &upstream,
            report,
        )?;
        wallpapers.push(PcWallpaperRecordPlan {
            wallpaper: id.index(),
            tiles,
            tilemap,
            frame_palette,
            background_palette,
        });
    }
    Ok(PcWallpaperPlan { table, wallpapers })
}

/// Upstream's spelling of a wallpaper in its `sWallpaper*_<Name>` symbols.
fn upstream_name(stem: &str) -> String {
    if stem == "polkadot" {
        return "PolkaDot".to_owned();
    }
    camel_case(stem)
}

/// Every address each tilemap's decompressed bytes turn up at, in
/// wallpaper order.
fn tilemap_candidates(ctx: &Context<'_>) -> Result<Vec<Vec<u32>>, GenRomProfileError> {
    let mut needles = Vec::with_capacity(PC_WALLPAPERS.len());
    for source in &PC_WALLPAPERS {
        needles.push(
            ctx.pack
                .raw_payload(&source.wallpaper.tilemap_id())?
                .to_vec(),
        );
    }
    let hits = ctx.lz77.find_all_uncapped(&needles);
    found_everywhere(
        PC_WALLPAPERS.iter().map(|s| s.wallpaper.tilemap_id()),
        &hits,
    )
}

/// Every address each frame palette followed by its background palette turns
/// up at, in wallpaper order.
fn palette_candidates(ctx: &Context<'_>) -> Result<Vec<Vec<u32>>, GenRomProfileError> {
    let mut needles = Vec::with_capacity(PC_WALLPAPERS.len());
    for source in &PC_WALLPAPERS {
        let id = source.wallpaper;
        let frame_id = id.frame_palette_id();
        let background_id = id.background_palette_id();
        let mut needle = Vec::with_capacity(2 * BANK_BYTES as usize);
        for bank_id in [&frame_id, &background_id] {
            let (payload, colors) = ctx.pack.get(bank_id)?.palette_payload(bank_id)?;
            if colors != BANK_COLORS {
                return Err(GenRomProfileError::StructMismatch {
                    id: bank_id.clone(),
                    reason: format!("has {colors} colours: expected {BANK_COLORS}"),
                });
            }
            needle.extend_from_slice(payload);
        }
        needles.push(needle);
    }
    let hits = ctx.raw.find_all_uncapped(&needles);
    found_everywhere(
        PC_WALLPAPERS.iter().map(|s| s.wallpaper.frame_palette_id()),
        &hits,
    )
}

fn found_everywhere(
    ids: impl Iterator<Item = String>,
    hits: &[Vec<u32>],
) -> Result<Vec<Vec<u32>>, GenRomProfileError> {
    ids.zip(hits)
        .map(|(id, offsets)| {
            if offsets.is_empty() {
                return Err(GenRomProfileError::NotFound { id });
            }
            Ok(offsets.iter().copied().map(to_addr).collect())
        })
        .collect()
}

/// Find the one `sWallpapers` whose every wallpaper row points at that
/// wallpaper's art.
fn find_table(ctx: &Context<'_>, wallpapers: &[Candidates]) -> Result<u32, GenRomProfileError> {
    let name = "sWallpapers";
    let Some(first) = wallpapers.first() else {
        return Err(GenRomProfileError::StructMismatch {
            id: name.to_owned(),
            reason: "no wallpaper to anchor the table".to_owned(),
        });
    };
    let first_bias = first.row * RECORD_BYTES;
    let bases: BTreeSet<u32> = first
        .tiles
        .iter()
        .flat_map(|&art| ctx.pointers.refs_to(art).iter().copied())
        .filter_map(|at| at.checked_sub(first_bias))
        .collect();
    only_one_matching(
        name,
        "every wallpaper row pointing at its tiles, tilemap and palettes",
        bases,
        |&base| {
            wallpapers.iter().all(|paper| {
                let row = base + paper.row * RECORD_BYTES;
                let names = |field: u32, art: &[u32]| {
                    u32_at_addr(ctx.rom, row + field).is_some_and(|ptr| art.contains(&ptr))
                };
                names(0, &paper.tiles)
                    && names(TILEMAP_FIELD, &paper.tilemaps)
                    && names(PALETTES_FIELD, &paper.palettes)
            })
        },
    )
}

/// The tilemap plan for the stream a row points at.
fn tilemap_plan(
    ctx: &Context<'_>,
    id: &str,
    addr: u32,
    candidates: &[u32],
    upstream: &str,
    report: &mut Vec<ReportLine>,
) -> Result<BlobPlan, GenRomProfileError> {
    if !candidates.contains(&addr) {
        return Err(GenRomProfileError::NotFound { id: id.to_owned() });
    }
    let len = len32(ctx.pack.raw_payload(id)?);
    let mut line =
        ReportLine::unique(id, addr, len).local_symbol(format!("sWallpaperTilemap_{upstream}"));
    if candidates.len() > 1 {
        line = line.with(Resolution::StructDerived).note(format!(
            "{} identical streams in the ROM; the wallpaper table names this one",
            candidates.len()
        ));
    }
    report.push(line);
    Ok(BlobPlan {
        id: id.to_owned(),
        addr,
        encoding: Encoding::Lz77,
        len,
    })
}

/// The frame and background palette plans for the pair a row points at.
fn palette_plans(
    ctx: &Context<'_>,
    ids: &[String; 2],
    addr: u32,
    candidates: &[u32],
    upstream: &str,
    report: &mut Vec<ReportLine>,
) -> Result<(PalettePlan, PalettePlan), GenRomProfileError> {
    if !candidates.contains(&addr) {
        return Err(GenRomProfileError::NotFound { id: ids[0].clone() });
    }
    let repeats = (candidates.len() > 1).then(|| {
        format!(
            "{} identical palette pairs in the ROM; the wallpaper table names this one",
            candidates.len()
        )
    });
    let mut plans = Vec::with_capacity(2);
    for (slot, id) in ids.iter().enumerate() {
        let slot_addr = addr + BANK_BYTES * u32::try_from(slot).expect("two palettes");
        let asset = ctx.pack.get(id)?;
        let mut line = ReportLine::unique(id, slot_addr, len32(&asset.payload));
        line = if slot == 0 {
            line.local_symbol(format!("sWallpaperPalettes_{upstream}"))
        } else {
            line.interior()
        };
        if let Some(note) = &repeats {
            line = line.with(Resolution::StructDerived).note(note.clone());
        }
        report.push(line);
        plans.push(PalettePlan {
            id: id.clone(),
            addr: slot_addr,
            encoding: Encoding::Raw,
            color_count: asset.palette_colors(id)?,
        });
    }
    let background = plans.pop().expect("two palettes");
    let frame = plans.pop().expect("two palettes");
    Ok((frame, background))
}

#[cfg(test)]
#[path = "pc_wallpaper_tests.rs"]
mod tests;
