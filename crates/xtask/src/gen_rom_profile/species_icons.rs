//! Locating the party-mon icon sheets, palette banks and source tables.
//!
//! The expectation is the checkout's fixed `species-icon/` inventory, which
//! extraction also emits ([`crate::extract::scope::species_icon_ids`]): 417
//! raw 4bpp sheets, three palette banks, and the sheet, palette-index and
//! palette-record tables. How the ROM stores them is read from upstream's C
//! (`src/pokemon_icon.c`, `src/graphics.c`):
//!
//! - Every sheet is an `INCGFX_U8` `.4bpp` array, stored raw. A sheet's bytes
//!   can repeat in the ROM (equal art under two symbols, or a stray copy), so
//!   bytes never choose a sheet.
//! - `gMonIconTable` holds one 4-byte pointer per species slot. The generator
//!   anchors the table on the pointers that reach the first slot's sheet
//!   candidates and accepts the one base whose every row names a candidate of
//!   its slot's sheet, with shared slots agreeing and distinct sheets at
//!   distinct addresses. The sheet addresses are then the table's pointers.
//! - `gMonIconDeoxysSpeed` is reached as `0x400` past the Deoxys sheet
//!   (`GetMonIconTiles`), so its address is derived and its bytes checked.
//!   The unused wide sheet has no pointer to it; it is taken only when its
//!   bytes occur once.
//! - `gMonIconPalettes` is three contiguous banks, and `gMonIconPaletteTable`
//!   six 8-byte `{data, tag}` rows over them (the last three past the banks),
//!   so the banks and the table are accepted only as a pair that agrees row
//!   by row.
//! - `gMonIconPaletteIndices` has no table pointing at it; a copy of its
//!   bytes that repeats is settled by the one a pointer references, and is
//!   otherwise an error.
//!
//! The planner loads no ELF symbols, so a map label never decides a copy.

use std::collections::BTreeSet;

use assets::species_icons::{
    decode_palette_table, decode_species_sheets, encode_palette_table, palette_pack_id,
    SpeciesIconPaletteRecord, SpeciesIconSheetId, BANK_COLORS, PALETTE_BANKS, PALETTE_INDICES_ID,
    PALETTE_RECORDS, PALETTE_TABLE_ID, SHEETS, SPECIES_SHEETS_ID, SPECIES_SLOTS, SPEED_SHEET,
    SPEED_WIDE_SHEET, TABLE_SHEETS,
};
use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::inventory;
use super::locate::{only_one_matching, to_addr, u16_at_addr, u32_at_addr};
use super::pack_source::image_tiles;
use super::plan::{BlobPlan, ImagePlan, PalettePlan, ReportLine, Resolution, SpeciesIconPlan};
use super::Context;

/// Bytes of one `gMonIconTable` pointer.
const SHEET_POINTER_BYTES: u32 = 4;
/// Bytes of one `gMonIconPaletteTable` row.
const PALETTE_ROW_BYTES: u32 = 8;
/// Where a palette row's tag sits.
const PALETTE_TAG_FIELD: u32 = 4;
/// Bytes of one 16-colour bank.
const BANK_BYTES: u32 = BANK_COLORS as u32 * 2;
/// `GetMonIconTiles` offsets the Deoxys sheet by this to reach the speed form.
const DEOXYS_SPEED_OFFSET: u32 = 0x400;
/// Species slots as a ROM length.
const SPECIES_SLOTS_U32: u32 = 440;
const _: () = assert!(SPECIES_SLOTS == 440);
/// `SPECIES_DEOXYS`.
const SPECIES_DEOXYS: usize = 410;

fn mismatch(id: &str, reason: impl Into<String>) -> GenRomProfileError {
    GenRomProfileError::StructMismatch {
        id: id.to_owned(),
        reason: reason.into(),
    }
}

/// The two per-species tables as the pack holds them.
struct Tables {
    species_sheets: Vec<u16>,
    palette_indices: Vec<u8>,
}

/// Locate every species-icon root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for a surplus `species-icon/`
/// pack entry or a table the candidates do not narrow to exactly one,
/// [`GenRomProfileError::WrongPackEntryKind`] or
/// [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] for a root the ROM does not hold.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<SpeciesIconPlan, GenRomProfileError> {
    inventory::require_species_icons(ctx.pack)?;
    let tables = read_tables(ctx)?;
    validate_shapes(ctx)?;

    let candidates = sheet_candidates(ctx)?;
    let sheet_table = locate_sheet_table(ctx, &tables, &candidates)?;
    let mut lines = Vec::new();
    let sheets = locate_sheets(ctx, &tables, &candidates, sheet_table, &mut lines)?;
    let (palette_table, palettes) = locate_palettes(ctx, &mut lines)?;
    let palette_indices = locate_palette_indices(ctx, &tables, &mut lines)?;

    report.push(
        ReportLine::unique(
            "gMonIconTable",
            sheet_table,
            SPECIES_SLOTS_U32 * SHEET_POINTER_BYTES,
        )
        .with(Resolution::StructDerived)
        .symbol("gMonIconTable")
        .note("anchored by the pointers from its species rows"),
    );
    report.push(
        ReportLine::unique(
            "gMonIconPaletteTable",
            palette_table,
            u32::try_from(PALETTE_RECORDS).expect("six") * PALETTE_ROW_BYTES,
        )
        .with(Resolution::StructDerived)
        .symbol("gMonIconPaletteTable")
        .note("paired with the banks its rows point at, by pointer and tag"),
    );
    report.append(&mut lines);
    Ok(SpeciesIconPlan {
        sheet_table,
        palette_table,
        palette_indices,
        sheets,
        palettes,
    })
}

fn read_tables(ctx: &Context<'_>) -> Result<Tables, GenRomProfileError> {
    let shape = |id: &str, reason: String| GenRomProfileError::EntryShape {
        id: id.to_owned(),
        reason,
    };
    let species_sheets = decode_species_sheets(ctx.pack.raw_payload(SPECIES_SHEETS_ID)?)
        .map_err(|error| shape(SPECIES_SHEETS_ID, error.to_string()))?;
    let palette_indices = ctx.pack.raw_payload(PALETTE_INDICES_ID)?.to_vec();
    assets::species_icons::check_palette_indices(&palette_indices)
        .map_err(|error| shape(PALETTE_INDICES_ID, error.to_string()))?;
    let records = ctx.pack.raw_payload(PALETTE_TABLE_ID)?;
    decode_palette_table(records).map_err(|error| shape(PALETTE_TABLE_ID, error.to_string()))?;
    if records != encode_palette_table() {
        return Err(shape(
            PALETTE_TABLE_ID,
            "is not the table upstream declares".to_owned(),
        ));
    }
    Ok(Tables {
        species_sheets,
        palette_indices,
    })
}

/// Refuse a pack entry that is not the shape the profile table will declare,
/// before any report line is written.
fn validate_shapes(ctx: &Context<'_>) -> Result<(), GenRomProfileError> {
    for number in 0..SHEETS {
        let sheet = sheet_id(number);
        let id = sheet.pack_id();
        let (_, width, height, depth) = ctx.pack.get(&id)?.image_raster(&id)?;
        let (expected_width, expected_height) = sheet.size();
        if (width, height, depth) != (expected_width, expected_height, 4) {
            return Err(GenRomProfileError::EntryShape {
                id,
                reason: format!(
                    "is {width}x{height} at {depth} bits: an icon sheet is \
                     {expected_width}x{expected_height} at 4 bits"
                ),
            });
        }
    }
    for bank in 0..PALETTE_BANKS {
        let id = palette_pack_id(bank);
        let (payload, colors) = ctx.pack.get(&id)?.palette_payload(&id)?;
        if colors != BANK_COLORS || payload.len() != usize::from(BANK_COLORS) * 2 {
            return Err(GenRomProfileError::EntryShape {
                id,
                reason: format!("holds {colors} colours: an icon bank holds {BANK_COLORS}"),
            });
        }
    }
    Ok(())
}

fn sheet_id(number: u16) -> SpeciesIconSheetId {
    SpeciesIconSheetId::new(number).expect("a sheet number below the declared sheets")
}

/// Every address each sheet's tiles turn up at, in sheet-number order,
/// uncapped so that no copy is hidden.
fn sheet_candidates(ctx: &Context<'_>) -> Result<Vec<Vec<u32>>, GenRomProfileError> {
    let ids: Vec<String> = (0..SHEETS).map(|n| sheet_id(n).pack_id()).collect();
    let needles = ids
        .iter()
        .map(|id| image_tiles(ctx.pack, id, 4, (1, 1)))
        .collect::<Result<Vec<_>, _>>()?;
    ids.iter()
        .zip(ctx.raw.find_all_uncapped(&needles))
        .map(|(id, offsets)| {
            if offsets.is_empty() {
                return Err(GenRomProfileError::NotFound { id: id.clone() });
            }
            Ok(offsets.into_iter().map(to_addr).collect())
        })
        .collect()
}

/// The sheet address each species slot of the table at `base` names, if every
/// row names a candidate of its slot's sheet, slots that share a sheet agree,
/// and distinct sheets sit at distinct addresses. Indexed by sheet number.
fn table_assignment(
    ctx: &Context<'_>,
    tables: &Tables,
    candidates: &[Vec<u32>],
    base: u32,
) -> Option<Vec<u32>> {
    let mut assigned: Vec<Option<u32>> = vec![None; usize::from(TABLE_SHEETS)];
    for (slot, &sheet) in tables.species_sheets.iter().enumerate() {
        let row = base.checked_add(u32::try_from(slot).ok()? * SHEET_POINTER_BYTES)?;
        let pointer = u32_at_addr(ctx.rom, row)?;
        let sheet = usize::from(sheet);
        if !candidates[sheet].contains(&pointer) {
            return None;
        }
        match assigned[sheet] {
            Some(previous) if previous != pointer => return None,
            _ => assigned[sheet] = Some(pointer),
        }
    }
    let addresses: Vec<u32> = assigned.into_iter().collect::<Option<_>>()?;
    let distinct: BTreeSet<u32> = addresses.iter().copied().collect();
    (distinct.len() == addresses.len()).then_some(addresses)
}

/// The one base of `gMonIconTable` whose rows name the sheets.
fn locate_sheet_table(
    ctx: &Context<'_>,
    tables: &Tables,
    candidates: &[Vec<u32>],
) -> Result<u32, GenRomProfileError> {
    let anchor_sheet = usize::from(tables.species_sheets[0]);
    let bases: BTreeSet<u32> = candidates[anchor_sheet]
        .iter()
        .flat_map(|&sheet| ctx.pointers.refs_to(sheet).iter().copied())
        .collect();
    only_one_matching(
        "gMonIconTable",
        "every species row naming a candidate of its sheet, consistently",
        bases,
        |&base| table_assignment(ctx, tables, candidates, base).is_some(),
    )
}

/// The image plan of every sheet, addressed by the table's pointers.
fn locate_sheets(
    ctx: &Context<'_>,
    tables: &Tables,
    candidates: &[Vec<u32>],
    sheet_table: u32,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<ImagePlan>, GenRomProfileError> {
    let mut addresses = table_assignment(ctx, tables, candidates, sheet_table)
        .ok_or_else(|| mismatch("gMonIconTable", "no longer agrees with its sheets"))?;

    let deoxys = addresses[usize::from(tables.species_sheets[SPECIES_DEOXYS])];
    let speed = deoxys + DEOXYS_SPEED_OFFSET;
    let speed_id = sheet_id(SPEED_SHEET).pack_id();
    if !candidates[usize::from(SPEED_SHEET)].contains(&speed) {
        return Err(mismatch(
            &speed_id,
            "the bytes are not 0x400 past the Deoxys sheet, where GetMonIconTiles reaches them",
        ));
    }
    addresses.push(speed);
    let wide_id = sheet_id(SPEED_WIDE_SHEET).pack_id();
    let wide = exactly_one_address(&wide_id, &candidates[usize::from(SPEED_WIDE_SHEET)])?;
    addresses.push(wide);

    let mut plans = Vec::with_capacity(usize::from(SHEETS));
    for (number, &addr) in (0..SHEETS).zip(&addresses) {
        let id = sheet_id(number);
        let (width, height) = id.size();
        let tile_count = width / 8 * (height / 8);
        let copies = candidates[usize::from(number)].len();
        let mut line = ReportLine::unique(id.pack_id(), addr, tile_count * 32);
        line = match number {
            SPEED_SHEET => line
                .with(Resolution::StructDerived)
                .symbol("gMonIcon_DeoxysSpeed")
                .note("0x400 past the Deoxys sheet, as GetMonIconTiles reaches it"),
            SPEED_WIDE_SHEET => line.symbol("gMonIcon_DeoxysSpeedWide"),
            _ => {
                let line = line.with(Resolution::PointerWalk);
                if copies > 1 {
                    line.note(format!(
                        "named by gMonIconTable; the bytes repeat at {copies} addresses"
                    ))
                } else {
                    line
                }
            }
        };
        report.push(line);
        plans.push(ImagePlan {
            id: id.pack_id(),
            addr,
            encoding: Encoding::Raw,
            rom_bit_depth: 4,
            pack_bit_depth: 4,
            width,
            height,
            metatile_width: 1,
            metatile_height: 1,
            tile_count,
        });
    }
    Ok(plans)
}

/// The only address `addrs` holds. Never chooses among several.
fn exactly_one_address(id: &str, addrs: &[u32]) -> Result<u32, GenRomProfileError> {
    match addrs {
        [only] => Ok(*only),
        [] => Err(GenRomProfileError::NotFound { id: id.to_owned() }),
        many => Err(GenRomProfileError::Ambiguous {
            id: id.to_owned(),
            addrs: many.to_vec(),
        }),
    }
}

/// The palette banks and `gMonIconPaletteTable`, accepted as the one pair
/// whose rows point at the banks by number and carry upstream's tags.
fn locate_palettes(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<(u32, Vec<PalettePlan>), GenRomProfileError> {
    let mut block = Vec::new();
    for bank in 0..PALETTE_BANKS {
        let id = palette_pack_id(bank);
        block.extend_from_slice(ctx.pack.get(&id)?.palette_payload(&id)?.0);
    }
    let bases: Vec<u32> = ctx
        .raw
        .find_all_uncapped(&[block])
        .remove(0)
        .into_iter()
        .map(to_addr)
        .collect();
    if bases.is_empty() {
        return Err(GenRomProfileError::NotFound {
            id: "species-icon/palette".to_owned(),
        });
    }
    let pairs = bases.iter().flat_map(|&base| {
        ctx.pointers
            .refs_to(base)
            .iter()
            .map(move |&table| (base, table))
    });
    let (bank_base, table) = only_one_matching(
        "gMonIconPaletteTable",
        "a six-row table over the banks, by pointer and tag",
        pairs,
        |&(base, table)| palette_rows_hold(ctx, base, table),
    )?;

    let mut plans = Vec::new();
    for bank in 0..PALETTE_BANKS {
        let id = palette_pack_id(bank);
        let addr = bank_base + u32::from(bank) * BANK_BYTES;
        let line = ReportLine::unique(id.clone(), addr, BANK_BYTES).with(Resolution::StructDerived);
        report.push(
            if bank == 0 {
                line.symbol("gMonIconPalettes")
            } else {
                line.interior()
            }
            .note(if bases.len() > 1 {
                format!(
                    "the banks repeat at {} addresses; the palette table names this one",
                    bases.len()
                )
            } else {
                "named by the palette table".to_owned()
            }),
        );
        plans.push(PalettePlan {
            id,
            addr,
            color_count: BANK_COLORS,
            encoding: Encoding::Raw,
        });
    }
    Ok((table, plans))
}

/// Whether the six rows at `table` point at bank `n` of the array at `base`
/// and carry tag `PALETTE_TAG_BASE + n`.
fn palette_rows_hold(ctx: &Context<'_>, base: u32, table: u32) -> bool {
    (0..PALETTE_RECORDS).all(|row| {
        let Ok(index) = u8::try_from(row) else {
            return false;
        };
        let row_addr = table + u32::from(index) * PALETTE_ROW_BYTES;
        u32_at_addr(ctx.rom, row_addr) == Some(base + u32::from(index) * BANK_BYTES)
            && u16_at_addr(ctx.rom, row_addr + PALETTE_TAG_FIELD)
                == Some(SpeciesIconPaletteRecord::declared(index).tag)
    })
}

/// `gMonIconPaletteIndices`: the one copy of its bytes, or of those a
/// pointer references.
fn locate_palette_indices(
    ctx: &Context<'_>,
    tables: &Tables,
    report: &mut Vec<ReportLine>,
) -> Result<BlobPlan, GenRomProfileError> {
    let copies: Vec<u32> = ctx
        .raw
        .find_all_uncapped(std::slice::from_ref(&tables.palette_indices))
        .remove(0)
        .into_iter()
        .map(to_addr)
        .collect();
    let (addr, resolution, note) = match copies.as_slice() {
        [] => {
            return Err(GenRomProfileError::NotFound {
                id: PALETTE_INDICES_ID.to_owned(),
            })
        }
        [only] => (*only, Resolution::UniqueSignature, None),
        many => {
            let referenced = only_one_matching(
                PALETTE_INDICES_ID,
                "a copy some pointer references",
                many.iter().copied(),
                |&copy| !ctx.pointers.refs_to(copy).is_empty(),
            )?;
            (
                referenced,
                Resolution::StructDerived,
                Some(format!(
                    "the bytes repeat at {} addresses; this is the only one a pointer references",
                    many.len()
                )),
            )
        }
    };
    let mut line = ReportLine::unique(PALETTE_INDICES_ID, addr, SPECIES_SLOTS_U32)
        .with(resolution)
        .symbol("gMonIconPaletteIndices");
    if let Some(note) = note {
        line = line.note(note);
    }
    report.push(line);
    Ok(BlobPlan {
        id: PALETTE_INDICES_ID.to_owned(),
        addr,
        encoding: Encoding::Raw,
        len: SPECIES_SLOTS_U32,
    })
}

#[cfg(test)]
#[path = "species_icons_tests.rs"]
mod tests;
