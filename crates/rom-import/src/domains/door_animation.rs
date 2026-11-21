//! The door animations: `door-animation/image/*` and `door-animation/table`.
//!
//! The sheets are raw `.4bpp` tiles that go through the shared image codec.
//! What this reader adds is `sDoorAnimGraphicsTable` itself: each 12-byte row
//! is read for its metatile, sound, size class, and the two pointers it
//! holds. The metatile and both pointers must be the ones the profile records
//! for that row, so a profile that picked a stray copy of a sheet is
//! [`ImportError::StructMismatch`]. The sound, the size class, and the eight
//! palette banks the pack publishes are the ROM's own bytes, and the pack's
//! table is written by [`assets::DoorAnimationTable`], the schema its reader
//! decodes.

use assets::{DoorAnimationRecord, DoorAnimationTable, DoorSize, DoorSound, DOOR_PALETTE_SLOTS};
use pack_format::{raw_entry, PackWriter};

use super::{check_pointer, image};
use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::{DoorAnimationRoots, DoorAnimationRow, Roots};

const TABLE_ID: &str = "door-animation/table";
const IMAGE_PREFIX: &str = "door-animation/image/";
/// A `DoorGraphics` row: `{u16 metatileNum, u8 sound, u8 size, tiles,
/// palettes}`.
const ROW_BYTES: usize = 12;
const SOUND_FIELD: usize = 2;
const SIZE_FIELD: usize = 3;
const TILES_FIELD: usize = 4;
const PALETTES_FIELD: usize = 8;
/// Banks a tileset carries; a palette-slot byte at or past this is no bank.
const BANK_COUNT: u8 = 16;

/// Write every door sheet and the door table.
///
/// Entries are read before any is queued, so a failing root leaves the writer
/// untouched. A profile with no door rows writes nothing.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if a table row, its terminator, or a bank
/// disagrees with the profile or the schema; any [`ImportError`] an image
/// read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.door_animation;
    if family.rows.is_empty() {
        return Ok(());
    }
    let reader = rom.reader();
    let mut records = Vec::with_capacity(family.rows.len());
    for (index, row) in family.rows.iter().enumerate() {
        records.push(read_row(&reader, family, index, row)?);
    }
    corroborate_terminator(&reader, family)?;
    let table = DoorAnimationTable::new(records).map_err(|_| ImportError::StructMismatch {
        root: TABLE_ID,
        field: "DoorGraphics.metatileNum",
    })?;
    let mut entries = Vec::with_capacity(family.images.len() + 1);
    for root in family.images {
        entries.push(image(&reader, root)?);
    }
    entries.push(raw_entry(TABLE_ID.to_owned(), table.encode()));
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

/// Read row `index`, corroborating it against the profile.
fn read_row(
    reader: &crate::reader::RomReader<'_>,
    family: &DoorAnimationRoots,
    index: usize,
    row: &DoorAnimationRow,
) -> Result<DoorAnimationRecord, ImportError> {
    let root = row.sheet;
    // The terminator row follows the last, so the table holds one more.
    reader.table_entry(
        "sDoorAnimGraphicsTable",
        family.table,
        index,
        family.rows.len() + 1,
        ROW_BYTES,
    )?;
    let base = family.table.offset() + index * ROW_BYTES;
    let mismatch = |field| ImportError::StructMismatch { root, field };
    if reader.u16_le(base)? != row.metatile {
        return Err(mismatch("DoorGraphics.metatileNum"));
    }
    let sound = DoorSound::from_tag(reader.u8(base + SOUND_FIELD)?)
        .ok_or_else(|| mismatch("DoorGraphics.sound"))?;
    let size = DoorSize::from_tag(reader.u8(base + SIZE_FIELD)?)
        .ok_or_else(|| mismatch("DoorGraphics.size"))?;
    let sheet = family
        .images
        .iter()
        .find(|image| image.id.strip_prefix(IMAGE_PREFIX) == Some(row.sheet))
        .ok_or_else(|| mismatch("DoorGraphics.tiles"))?;
    if sheet.width != size.sheet_width() {
        return Err(mismatch("DoorGraphics.size"));
    }
    check_pointer(
        reader,
        base,
        TILES_FIELD,
        sheet.addr,
        root,
        "DoorGraphics.tiles",
    )?;
    check_pointer(
        reader,
        base,
        PALETTES_FIELD,
        row.palette_slots,
        root,
        "DoorGraphics.palettes",
    )?;
    let banks = reader.slice(row.palette_slots, DOOR_PALETTE_SLOTS)?;
    if banks.iter().any(|&bank| bank >= BANK_COUNT) {
        return Err(mismatch("DoorGraphics.palettes"));
    }
    let mut palette_slots = [0; DOOR_PALETTE_SLOTS];
    palette_slots.copy_from_slice(banks);
    Ok(DoorAnimationRecord {
        metatile: row.metatile,
        sound,
        size,
        palette_slots,
        sheet: row.sheet.to_owned(),
    })
}

/// The row after the last must be all zero: the table's `{}` terminator.
fn corroborate_terminator(
    reader: &crate::reader::RomReader<'_>,
    family: &DoorAnimationRoots,
) -> Result<(), ImportError> {
    let row = reader.table_entry(
        "sDoorAnimGraphicsTable",
        family.table,
        family.rows.len(),
        family.rows.len() + 1,
        ROW_BYTES,
    )?;
    if row.iter().any(|&byte| byte != 0) {
        return Err(ImportError::StructMismatch {
            root: TABLE_ID,
            field: "DoorGraphics terminator",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
