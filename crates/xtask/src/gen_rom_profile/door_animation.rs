//! Locating the door animations' table and sheets (`src/field_door.c`).
//!
//! The expectation is the checkout's fixed inventory, which extraction also
//! reads ([`crate::extract::scope::door_animation_ids`]): one sheet per
//! referenced `sDoorAnimTiles_*` and the door table the pack carries. Every
//! sheet is a raw `.4bpp` `INCBIN`, `static`, in plain tile order but for the
//! Battle Tower multi-corridor sheet, which upstream builds with
//! `-mwidth 2 -mheight 4` (`src/field_door.c:120`).
//!
//! The pack's table must be exactly the checkout's, so that every required
//! sheet is named by a row and no row is missing; that is checked before any
//! search.
//!
//! The walk:
//!
//! 1. Each sheet is searched in its own packing, uncapped, so the sheets that
//!    share bytes (several doors are drawn from near-identical art) keep
//!    every copy.
//! 2. `sDoorAnimGraphicsTable` is found through the pointers to the first
//!    row's sheet: biased back by the row's `tiles` field they name candidate
//!    table bases, and exactly one base must hold the whole table: every
//!    row's metatile, sound and size, a pointer to a copy of its sheet, a
//!    pointer to its eight palette banks, and the all-zero terminator behind
//!    the last row.
//! 3. Each sheet's address, and each row's palette-bank array, is the pointer
//!    that row holds, which settles art and arrays the ROM stores more than
//!    once.
//!
//! The pack carries no door that no row names: `unused_top`, `unused_bottom`
//! and `sDoorAnimPalettes_Unused` have no ROM structure to find them by, so
//! they are outside the inventory.

use std::collections::BTreeMap;

use assets::{DoorAnimationRecord, DoorAnimationTable};
use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::inventory;
use super::locate::{only_one_matching, to_addr, u16_at_addr, u32_at_addr};
use super::pack_source::image_tiles;
use super::plan::{DoorAnimationPlan, DoorRowPlan, ImagePlan, ReportLine, Resolution};
use super::Context;
use crate::extract::door_animation::{records, TABLE_ID};

const TILE_BYTES: u32 = 32;
/// A `DoorGraphics` row, in bytes.
const ROW_BYTES: u32 = 12;
const SOUND_FIELD: u32 = 2;
const SIZE_FIELD: u32 = 3;
const TILES_FIELD: u32 = 4;
const PALETTES_FIELD: u32 = 8;
/// The palette banks one row names.
const BANKS_LEN: usize = assets::DOOR_PALETTE_SLOTS;
/// The sheet built with `-mwidth 2 -mheight 4`.
const BLOCK_PACKED_SHEET: &str = "battle_tower_multi_corridor";

/// The packing `stem`'s sheet is stored in, in tiles.
fn packing(stem: &str) -> (u32, u32) {
    if stem == BLOCK_PACKED_SHEET {
        (2, 4)
    } else {
        (1, 1)
    }
}

fn image_id(stem: &str) -> String {
    format!("door-animation/image/{stem}")
}

/// What one sheet needs to be located and planned.
struct Sheet {
    stem: String,
    width: u32,
    height: u32,
    /// Every address the sheet's tiles turn up at.
    copies: Vec<u32>,
}

/// Locate the door table and every sheet its rows name.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an absent pack entry,
/// [`GenRomProfileError::StructMismatch`] for a surplus `door-animation/` pack
/// entry, a table the pack does not parse, or a table the ROM does not hold in
/// exactly one place; [`GenRomProfileError::WrongPackEntryKind`] or
/// [`GenRomProfileError::EntryShape`] for a malformed entry;
/// [`GenRomProfileError::NotFound`] for a sheet the ROM does not hold.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<DoorAnimationPlan, GenRomProfileError> {
    inventory::require_door_animation(ctx.pack)?;
    let table = DoorAnimationTable::decode(ctx.pack.raw_payload(TABLE_ID)?).map_err(|error| {
        GenRomProfileError::StructMismatch {
            id: TABLE_ID.to_owned(),
            reason: format!("is not a door table: {error}"),
        }
    })?;
    // The profile must describe every door and every sheet the inventory
    // requires: a table that names fewer would leave required sheets out of
    // the profile, and the ROM's one-row suffix would still match it.
    if table.records() != records().as_slice() {
        return Err(GenRomProfileError::StructMismatch {
            id: TABLE_ID.to_owned(),
            reason: "does not hold exactly the rows of the checkout's door table".to_owned(),
        });
    }
    let sheets = find_sheets(ctx, table.records())?;
    let base = find_table(ctx, table.records(), &sheets)?;

    let mut rows = Vec::with_capacity(table.records().len());
    let mut sheet_addrs: BTreeMap<&str, u32> = BTreeMap::new();
    for (index, record) in table.records().iter().enumerate() {
        let row = row_addr(base, index);
        let tiles = pointer(ctx, row + TILES_FIELD, &record.sheet)?;
        // Rows sharing a sheet share its address.
        if *sheet_addrs.entry(&record.sheet).or_insert(tiles) != tiles {
            return Err(GenRomProfileError::StructMismatch {
                id: image_id(&record.sheet),
                reason: "is pointed at from two addresses by rows that share it".to_owned(),
            });
        }
        rows.push(DoorRowPlan {
            metatile: record.metatile,
            sheet: record.sheet.clone(),
            palette_slots: pointer(ctx, row + PALETTES_FIELD, &record.sheet)?,
        });
    }
    report.push(
        ReportLine::unique(
            TABLE_ID,
            base,
            row_addr(base, table.records().len() + 1) - base,
        )
        .with(Resolution::StructDerived)
        .local_symbol("sDoorAnimGraphicsTable")
        .note("anchored by the pointer from its first row's sheet"),
    );

    let mut images = Vec::with_capacity(sheets.len());
    for sheet in &sheets {
        let addr = sheet_addrs[sheet.stem.as_str()];
        images.push(image_plan(sheet, addr));
        report.push(image_line(sheet, addr));
    }
    Ok(DoorAnimationPlan {
        table: base,
        rows,
        images,
    })
}

/// The bus address of row `index` of the table at `base`.
fn row_addr(base: u32, index: usize) -> u32 {
    base + u32::try_from(index).expect("a table index fits u32") * ROW_BYTES
}

/// The pointer at `at`, or [`GenRomProfileError::NotFound`] for the sheet
/// `stem` when the row does not read.
fn pointer(ctx: &Context<'_>, at: u32, stem: &str) -> Result<u32, GenRomProfileError> {
    u32_at_addr(ctx.rom, at).ok_or_else(|| GenRomProfileError::NotFound { id: image_id(stem) })
}

/// Every referenced sheet, in table order of first use, with all its copies.
fn find_sheets(
    ctx: &Context<'_>,
    records: &[DoorAnimationRecord],
) -> Result<Vec<Sheet>, GenRomProfileError> {
    let mut sheets: Vec<Sheet> = Vec::new();
    let mut needles = Vec::new();
    for record in records {
        if sheets.iter().any(|sheet| sheet.stem == record.sheet) {
            continue;
        }
        let id = image_id(&record.sheet);
        let (width, height) = shape(ctx, &id, record)?;
        needles.push(image_tiles(ctx.pack, &id, 4, packing(&record.sheet))?);
        sheets.push(Sheet {
            stem: record.sheet.clone(),
            width,
            height,
            copies: Vec::new(),
        });
    }
    for (sheet, offsets) in sheets.iter_mut().zip(ctx.raw.find_all_uncapped(&needles)) {
        sheet.copies = offsets.into_iter().map(to_addr).collect();
        if sheet.copies.is_empty() {
            return Err(GenRomProfileError::NotFound {
                id: image_id(&sheet.stem),
            });
        }
    }
    Ok(sheets)
}

/// The pack sheet's size, refused unless it is the 4-bit raster of whole tiles
/// the record's size class gives it.
fn shape(
    ctx: &Context<'_>,
    id: &str,
    record: &DoorAnimationRecord,
) -> Result<(u32, u32), GenRomProfileError> {
    let (width, height, depth) = ctx.pack.get(id)?.image_shape(id)?;
    if width != record.size.sheet_width() || height == 0 || height % 8 != 0 || depth != 4 {
        return Err(GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: format!(
                "is {width}x{height} at {depth} bits: a door sheet is {} pixels wide, a whole \
                 number of tile rows tall, at 4 bits",
                record.size.sheet_width()
            ),
        });
    }
    Ok((width, height))
}

/// The one table base whose every row agrees with the pack's table.
fn find_table(
    ctx: &Context<'_>,
    records: &[DoorAnimationRecord],
    sheets: &[Sheet],
) -> Result<u32, GenRomProfileError> {
    let Some(first) = records.first() else {
        return Err(GenRomProfileError::StructMismatch {
            id: TABLE_ID.to_owned(),
            reason: "holds no door".to_owned(),
        });
    };
    let anchor = sheets
        .iter()
        .find(|sheet| sheet.stem == first.sheet)
        .expect("find_sheets covers every record");
    let mut bases: Vec<u32> = anchor
        .copies
        .iter()
        .flat_map(|&copy| ctx.pointers.refs_to(copy).iter().copied())
        .filter_map(|field| field.checked_sub(TILES_FIELD))
        .collect();
    bases.sort_unstable();
    bases.dedup();
    only_one_matching(
        TABLE_ID,
        "every row of the pack's door table and its all-zero terminator",
        bases,
        |&base| table_matches(ctx, base, records, sheets),
    )
}

/// Whether the ROM holds the pack's table at `base`.
fn table_matches(
    ctx: &Context<'_>,
    base: u32,
    records: &[DoorAnimationRecord],
    sheets: &[Sheet],
) -> bool {
    let rows_agree = records.iter().enumerate().all(|(index, record)| {
        let row = row_addr(base, index);
        let sheet = sheets.iter().find(|sheet| sheet.stem == record.sheet);
        let sound_size = super::locate::u8_at_addr(ctx.rom, row + SOUND_FIELD)
            .zip(super::locate::u8_at_addr(ctx.rom, row + SIZE_FIELD));
        u16_at_addr(ctx.rom, row) == Some(record.metatile)
            && sound_size == Some((record.sound.tag(), record.size.tag()))
            && u32_at_addr(ctx.rom, row + TILES_FIELD)
                .zip(sheet)
                .is_some_and(|(tiles, sheet)| sheet.copies.contains(&tiles))
            && u32_at_addr(ctx.rom, row + PALETTES_FIELD)
                .and_then(|banks| super::locate::slice_at_addr(ctx.rom, banks, BANKS_LEN))
                .is_some_and(|banks| banks == record.palette_slots)
    });
    let terminator = row_addr(base, records.len());
    rows_agree
        && super::locate::slice_at_addr(ctx.rom, terminator, ROW_BYTES as usize)
            .is_some_and(|row| row.iter().all(|&byte| byte == 0))
}

fn image_plan(sheet: &Sheet, addr: u32) -> ImagePlan {
    let (metatile_width, metatile_height) = packing(&sheet.stem);
    ImagePlan {
        id: image_id(&sheet.stem),
        addr,
        encoding: Encoding::Raw,
        rom_bit_depth: 4,
        pack_bit_depth: 4,
        width: sheet.width,
        height: sheet.height,
        metatile_width,
        metatile_height,
        tile_count: sheet.width / 8 * (sheet.height / 8),
    }
}

fn image_line(sheet: &Sheet, addr: u32) -> ReportLine {
    let id = image_id(&sheet.stem);
    let line = ReportLine::unique(&id, addr, sheet.width / 8 * (sheet.height / 8) * TILE_BYTES)
        .local_symbol(format!(
            "sDoorAnimTiles_{}",
            super::locate::camel_case(&sheet.stem)
        ));
    if sheet.copies.len() > 1 {
        line.with(Resolution::StructDerived).note(format!(
            "the art repeats at {} addresses holding identical bytes; the table row names this one",
            sheet.copies.len()
        ))
    } else {
        line
    }
}

#[cfg(test)]
#[path = "door_animation_tests.rs"]
mod tests;
