//! Static battler art: the `pokemon/<species>/*` entries of every species the profile roots.
//!
//! Images and palettes are plain LZ77 streams that go through the shared
//! codecs. What the profile adds is the four species tables
//! (`gMonStillFrontPicTable`, `gMonBackPicTable`, `gMonPaletteTable`,
//! `gMonShinyPaletteTable`): each species' row in each table must hold the
//! address the profile records for that species, and the tag its row
//! carries, so a profile that picked a stray copy of the art is
//! [`ImportError::StructMismatch`] rather than a bank that happens to have
//! the right pixels today.
//!
//! A row's `size` is always the 64x64 picture the game loads, but a stream
//! may hold more: Poochyena's still front is a 64x256 sheet, so its root
//! declares the whole sheet and the importer reads every byte of it.

use pack_format::PackWriter;

use super::{check_pointer, palette};
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{BattlerRoot, BattlerRoots, Roots};

/// Every table row is `{data, size, tag}` or `{data, tag, pad}`: eight bytes.
const RECORD_BYTES: usize = 8;
/// How many rows each table holds: `SPECIES_UNOWN_QMARK + 1`.
const TABLE_ROWS: usize = 440;
/// `MON_PIC_SIZE`, the `size` of every picture row.
const PIC_SIZE: u16 = 0x800;
/// `SPECIES_SHINY_TAG`, added to a species' tag in the shiny palette table.
const SHINY_TAG: u16 = 500;
/// Offset of a picture row's `size` field.
const PIC_SIZE_FIELD: usize = 4;
/// Offset of a picture row's `tag` field.
const PIC_TAG_FIELD: usize = 6;
/// Offset of a palette row's `tag` field.
const PALETTE_TAG_FIELD: usize = 4;

/// Write every battler image and palette.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched rather than holding part of the domain. A profile with
/// no battler roots writes nothing.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if a species table row disagrees with the
/// profile; [`ImportError::IndexOutOfRange`] if a species is past the end of
/// a table; any [`ImportError`] an image or palette read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let family = &roots.battlers;
    let mut entries = Vec::with_capacity(family.species.len() * 4);
    for battler in family.species {
        corroborate(&reader, family, battler)?;
        entries.push(super::image(&reader, &battler.front)?);
        entries.push(super::image(&reader, &battler.back)?);
        entries.push(palette(&reader, &battler.normal)?);
        entries.push(palette(&reader, &battler.shiny)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

/// Check one species' row in each of the four tables.
fn corroborate(
    reader: &RomReader<'_>,
    family: &BattlerRoots,
    battler: &BattlerRoot,
) -> Result<(), ImportError> {
    let species = battler.species;
    picture_row(
        reader,
        ("gMonStillFrontPicTable", family.front_table),
        species,
        battler.front.addr,
        battler.front.id,
    )?;
    picture_row(
        reader,
        ("gMonBackPicTable", family.back_table),
        species,
        battler.back.addr,
        battler.back.id,
    )?;
    palette_row(
        reader,
        ("gMonPaletteTable", family.palette_table),
        species,
        battler.normal.addr,
        battler.normal.id,
        species,
    )?;
    palette_row(
        reader,
        ("gMonShinyPaletteTable", family.shiny_palette_table),
        species,
        battler.shiny.addr,
        battler.shiny.id,
        species.saturating_add(SHINY_TAG),
    )
}

/// One `struct CompressedSpriteSheet` row: `data`, `size`, `tag`.
fn picture_row(
    reader: &RomReader<'_>,
    (table, base): (&'static str, GbaPtr),
    species: u16,
    expected: GbaPtr,
    root: &'static str,
) -> Result<(), ImportError> {
    let row = row_offset(reader, table, base, species)?;
    check_pointer(reader, row, 0, expected, root, "CompressedSpriteSheet.data")?;
    if reader.u16_le(row + PIC_SIZE_FIELD)? != PIC_SIZE {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpriteSheet.size",
        });
    }
    if reader.u16_le(row + PIC_TAG_FIELD)? != species {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpriteSheet.tag",
        });
    }
    Ok(())
}

/// One `struct CompressedSpritePalette` row: `data`, `tag`.
fn palette_row(
    reader: &RomReader<'_>,
    (table, base): (&'static str, GbaPtr),
    species: u16,
    expected: GbaPtr,
    root: &'static str,
    tag: u16,
) -> Result<(), ImportError> {
    let row = row_offset(reader, table, base, species)?;
    check_pointer(
        reader,
        row,
        0,
        expected,
        root,
        "CompressedSpritePalette.data",
    )?;
    if reader.u16_le(row + PALETTE_TAG_FIELD)? != tag {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpritePalette.tag",
        });
    }
    Ok(())
}

/// The ROM offset of `species`' row, bounds-checked against the table.
fn row_offset(
    reader: &RomReader<'_>,
    table: &'static str,
    base: GbaPtr,
    species: u16,
) -> Result<usize, ImportError> {
    reader.table_entry(table, base, usize::from(species), TABLE_ROWS, RECORD_BYTES)?;
    Ok(base.offset() + usize::from(species) * RECORD_BYTES)
}

#[cfg(test)]
mod tests;
