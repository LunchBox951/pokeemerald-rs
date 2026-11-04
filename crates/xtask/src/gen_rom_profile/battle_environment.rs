//! Locating the checkpoint battle environments' roots (`src/battle_bg.c`).
//!
//! The expectation is the checkout's fixed inventory, which extraction also
//! reads ([`crate::extract::scope::battle_environment_ids`]): four tile sheets,
//! two palettes, and four tilemaps. The ROM stores all ten as LZ77 streams,
//! the palettes included (`.gbapal.lz`).
//!
//! The walk:
//!
//! 1. The sheets (by their decompressed tiles, [`super::images`]) and the
//!    palettes (by their decompressed colours) must each turn up at exactly
//!    one address; a repeat is refused rather than paired. The tilemaps are
//!    different: the environments share byte-identical ones, so each gets its
//!    full set of candidate streams.
//! 2. `sBattleEnvironmentTable` is found through the pointers to the grass
//!    tile sheet: its row is the first, so a pointer to the sheet is a
//!    candidate base. Exactly one base must hold the five grass pointers in
//!    row 0 and the five plain pointers in row 9, a tilemap pointer being
//!    any candidate stream. That pins the table and corroborates that plain
//!    aliases the building art.
//! 3. Each tilemap's address is the pointer its table row holds, which
//!    settles which of the identical streams belongs to the environment.
//!
//! Plain's row reuses the building tiles, tilemap, entry tiles and entry
//! tilemap, so the plan carries the building members under the plain record.

use std::collections::BTreeSet;

use super::error::GenRomProfileError;
use super::images::{locate_unique_images, ImageQuery};
use super::inventory;
use super::locate::{only_one_matching, to_addr, u32_at_addr};
use super::palettes::locate_unique_as;
use super::plan::{
    BattleEnvironmentPlan, BattleEnvironmentRecordPlan, BlobPlan, ImagePlan, PalettePlan,
    ReportLine, Resolution, SymbolExpectation,
};
use super::tilesets::len32;
use super::Context;
use rom_import::Encoding;

/// `struct BattleBackground`: five pointers, in bytes.
const RECORD_BYTES: u32 = 20;
/// `BATTLE_ENVIRONMENT_PLAIN + 1`, the rows the table holds.
const TABLE_ROWS: u32 = 10;
/// Pixels along one side of a tile.
const TILE_SIDE: u32 = 8;
/// `BATTLE_ENVIRONMENT_PLAIN`; `BATTLE_ENVIRONMENT_GRASS` is row 0.
const PLAIN_ROW: u32 = 9;

const GRASS_TILES: &str = "battle-environment/image/tall_grass/tiles";
const GRASS_ENTRY_TILES: &str = "battle-environment/image/tall_grass/anim_tiles";
const BUILDING_TILES: &str = "battle-environment/image/building/tiles";
const BUILDING_ENTRY_TILES: &str = "battle-environment/image/building/anim_tiles";
const GRASS_PALETTE: &str = "battle-environment/palette/tall_grass";
const PLAIN_PALETTE: &str = "battle-environment/palette/plain";
const GRASS_TILEMAP: &str = "battle-environment/raw/tall_grass/map";
const GRASS_ENTRY_TILEMAP: &str = "battle-environment/raw/tall_grass/anim_map";
const BUILDING_TILEMAP: &str = "battle-environment/raw/building/map";
const BUILDING_ENTRY_TILEMAP: &str = "battle-environment/raw/building/anim_map";

/// The global symbol upstream gives a palette or tilemap id.
fn global_symbol(id: &str) -> SymbolExpectation {
    let name = match id {
        GRASS_PALETTE => "gBattleEnvironmentPalette_TallGrass",
        PLAIN_PALETTE => "gBattleEnvironmentPalette_Plain",
        GRASS_TILEMAP => "gBattleEnvironmentTilemap_TallGrass",
        GRASS_ENTRY_TILEMAP => "gBattleEnvironmentAnimTilemap_TallGrass",
        BUILDING_TILEMAP => "gBattleEnvironmentTilemap_Building",
        BUILDING_ENTRY_TILEMAP => "gBattleEnvironmentAnimTilemap_Building",
        _ => return SymbolExpectation::Unnamed,
    };
    SymbolExpectation::Exact(name.to_owned())
}

/// A tilemap id and every address its decompressed bytes turn up at.
struct TilemapCandidates {
    id: &'static str,
    addrs: Vec<u32>,
    len: u32,
}

/// One environment's located sheets and palette, awaiting its tilemaps.
struct Art {
    tiles: ImagePlan,
    entry_tiles: ImagePlan,
    palette: PalettePlan,
}

/// Locate every battle-environment root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for a surplus
/// `battle-environment/` pack entry or a table that cannot be pinned to
/// exactly one base, [`GenRomProfileError::WrongPackEntryKind`] or
/// [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`] for
/// a sheet or palette the ROM does not hold exactly once.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<BattleEnvironmentPlan, GenRomProfileError> {
    inventory::require_battle_environment(ctx.pack)?;

    let queries = [
        GRASS_TILES,
        GRASS_ENTRY_TILES,
        BUILDING_TILES,
        BUILDING_ENTRY_TILES,
    ]
    .map(ImageQuery::lz77);
    let images = locate_unique_images(ctx, &queries, report)?
        .into_iter()
        .map(default_packing)
        .collect::<Result<Vec<_>, _>>()?;
    let palettes = locate_unique_as(
        ctx,
        &[GRASS_PALETTE.to_owned(), PLAIN_PALETTE.to_owned()],
        Encoding::Lz77,
        &global_symbol,
        report,
    )?;
    let [grass_tiles, grass_entry_tiles, building_tiles, building_entry_tiles] = images
        .try_into()
        .unwrap_or_else(|_| unreachable!("one plan comes back per query"));
    let [grass_palette, plain_palette] = palettes
        .try_into()
        .unwrap_or_else(|_| unreachable!("one plan comes back per id"));
    let grass = Art {
        tiles: grass_tiles,
        entry_tiles: grass_entry_tiles,
        palette: grass_palette,
    };
    let plain = Art {
        tiles: building_tiles,
        entry_tiles: building_entry_tiles,
        palette: plain_palette,
    };

    let [grass_map, grass_entry_map, plain_map, plain_entry_map] = tilemap_candidates(ctx)?;
    let table = find_table(
        ctx,
        &grass,
        [&grass_map, &grass_entry_map],
        &plain,
        [&plain_map, &plain_entry_map],
    )?;
    report.push(
        ReportLine::unique("sBattleEnvironmentTable", table, TABLE_ROWS * RECORD_BYTES)
            .with(Resolution::StructDerived)
            .local_symbol("sBattleEnvironmentTable")
            .note("anchored by the pointers from its grass and plain rows"),
    );

    let tall_grass = record(ctx, table, 0, grass, [&grass_map, &grass_entry_map], report);
    let plain = record(
        ctx,
        table,
        PLAIN_ROW,
        plain,
        [&plain_map, &plain_entry_map],
        report,
    );
    Ok(BattleEnvironmentPlan {
        table,
        tall_grass,
        plain,
    })
}

/// Every address each of the four tilemaps decompresses from.
///
/// The environments share byte-identical tilemaps (seven streams hold the
/// grass map), so only the table's rows can say which one belongs here.
fn tilemap_candidates(ctx: &Context<'_>) -> Result<[TilemapCandidates; 4], GenRomProfileError> {
    let ids = [
        GRASS_TILEMAP,
        GRASS_ENTRY_TILEMAP,
        BUILDING_TILEMAP,
        BUILDING_ENTRY_TILEMAP,
    ];
    let mut needles = Vec::with_capacity(ids.len());
    for id in ids {
        needles.push(ctx.pack.raw_payload(id)?.to_vec());
    }
    let hits = ctx.lz77.find_all(&needles);
    let mut found = Vec::with_capacity(ids.len());
    for ((id, needle), offsets) in ids.into_iter().zip(&needles).zip(hits) {
        if offsets.is_empty() {
            return Err(GenRomProfileError::NotFound { id: id.to_owned() });
        }
        found.push(TilemapCandidates {
            id,
            addrs: offsets.into_iter().map(to_addr).collect(),
            len: len32(needle),
        });
    }
    found
        .try_into()
        .map_err(|_| unreachable!("one candidate set per id"))
}

/// Restate a sheet's packing as the 1x1 default upstream builds it with.
///
/// The image locator reports the largest metatile shape that reproduces the
/// bytes, and a metatile spanning the whole sheet walks the tiles row-major,
/// exactly as 1x1 does. The family's sheets are not metatiled
/// (`graphics_file_rules.mk` has no rule for them), so that one equivalent
/// shape is restated as upstream's own; any other shape matching is not this
/// art and is refused.
fn default_packing(mut sheet: ImagePlan) -> Result<ImagePlan, GenRomProfileError> {
    let whole_sheet = (sheet.width / TILE_SIDE, sheet.height / TILE_SIDE);
    match (sheet.metatile_width, sheet.metatile_height) {
        (1, 1) => {}
        shape if shape == whole_sheet => {
            sheet.metatile_width = 1;
            sheet.metatile_height = 1;
        }
        (width, height) => {
            return Err(GenRomProfileError::StructMismatch {
                id: sheet.id,
                reason: format!(
                    "matched with a {width}x{height}-tile metatile; upstream packs 1x1"
                ),
            });
        }
    }
    Ok(sheet)
}

/// The tilemap addresses `row` holds when it points at `art` and at a stream
/// of each candidate set, as `(tilemap, entry tilemap)`.
fn row_tilemaps(
    ctx: &Context<'_>,
    row: u32,
    art: &Art,
    [map, entry_map]: [&TilemapCandidates; 2],
) -> Option<(u32, u32)> {
    let word = |member: u32| u32_at_addr(ctx.rom, row + member * 4);
    let tilemap = word(1).filter(|addr| map.addrs.contains(addr))?;
    let entry_tilemap = word(3).filter(|addr| entry_map.addrs.contains(addr))?;
    (word(0) == Some(art.tiles.addr)
        && word(2) == Some(art.entry_tiles.addr)
        && word(4) == Some(art.palette.addr))
    .then_some((tilemap, entry_tilemap))
}

/// Find the one table whose grass row and plain row hold the located members.
fn find_table(
    ctx: &Context<'_>,
    grass: &Art,
    grass_maps: [&TilemapCandidates; 2],
    plain: &Art,
    plain_maps: [&TilemapCandidates; 2],
) -> Result<u32, GenRomProfileError> {
    let bases: BTreeSet<u32> = ctx
        .pointers
        .refs_to(grass.tiles.addr)
        .iter()
        .copied()
        .collect();
    only_one_matching(
        "sBattleEnvironmentTable",
        "its grass and plain rows holding the located members",
        bases,
        |&base| {
            row_tilemaps(ctx, base, grass, grass_maps).is_some()
                && row_tilemaps(ctx, base + PLAIN_ROW * RECORD_BYTES, plain, plain_maps).is_some()
        },
    )
}

/// One environment's plan, taking its tilemaps from the table row that names them.
fn record(
    ctx: &Context<'_>,
    table: u32,
    row: u32,
    art: Art,
    maps: [&TilemapCandidates; 2],
    report: &mut Vec<ReportLine>,
) -> BattleEnvironmentRecordPlan {
    let (tilemap, entry_tilemap) = row_tilemaps(ctx, table + row * RECORD_BYTES, &art, maps)
        .unwrap_or_else(|| unreachable!("`find_table` checked this row"));
    let blob = |candidates: &TilemapCandidates, addr: u32, report: &mut Vec<ReportLine>| {
        let mut line = ReportLine::unique(candidates.id, addr, candidates.len)
            .with_symbol(global_symbol(candidates.id));
        if candidates.addrs.len() > 1 {
            line = line.with(Resolution::StructDerived).note(format!(
                "{} identical streams in the ROM; the table row names this one",
                candidates.addrs.len()
            ));
        }
        report.push(line);
        BlobPlan {
            id: candidates.id.to_owned(),
            addr,
            encoding: Encoding::Lz77,
            len: candidates.len,
        }
    };
    BattleEnvironmentRecordPlan {
        tiles: art.tiles,
        tilemap: blob(maps[0], tilemap, report),
        entry_tiles: art.entry_tiles,
        entry_tilemap: blob(maps[1], entry_tilemap, report),
        palette: art.palette,
    }
}

#[cfg(test)]
#[path = "battle_environment_tests.rs"]
mod tests;
