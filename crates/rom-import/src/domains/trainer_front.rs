//! The Birch-speech trainer front pictures: `trainer-pic/*`.
//!
//! Each picture is an LZ77 4bpp image and an LZ77 palette that go through
//! the shared codecs. What the profile adds is the two trainer tables
//! (`gTrainerFrontPicTable`, `gTrainerFrontPicPaletteTable`): a picture's row
//! in each table must hold the address the profile records for that picture
//! and the tag its row carries, so a profile that picked a stray copy of the
//! art is [`ImportError::StructMismatch`] rather than a picture that happens
//! to have the right pixels today.

use pack_format::PackWriter;

use super::{check_pointer, palette};
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{Roots, TrainerFrontRoot, TrainerFrontRoots};

/// Every table row is `{data, size, tag}` or `{data, tag, pad}`: eight bytes.
const RECORD_BYTES: usize = 8;
/// How many rows each table holds: `TRAINER_PIC_RS_MAY + 1`.
const TABLE_ROWS: usize = 93;
/// `TRAINER_PIC_SIZE`, the `size` of every picture row: 64x64 pixels at 4bpp.
const PICTURE_SIZE: u16 = 0x800;
/// Offset of a picture row's `size` field.
const PICTURE_SIZE_FIELD: usize = 4;
/// Offset of a picture row's `tag` field.
const PICTURE_TAG_FIELD: usize = 6;
/// Offset of a palette row's `tag` field.
const PALETTE_TAG_FIELD: usize = 4;

/// Write every trainer front image and palette.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched rather than holding part of the domain. A profile with
/// no trainer front roots writes nothing.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if a table row disagrees with the
/// profile; [`ImportError::IndexOutOfRange`] if a picture is past the end of
/// a table; any [`ImportError`] an image or palette read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let family = &roots.trainer_front;
    let mut entries = Vec::with_capacity(family.trainers.len() * 2);
    for trainer in family.trainers {
        corroborate(&reader, family, trainer)?;
        entries.push(super::image(&reader, &trainer.front)?);
        entries.push(palette(&reader, &trainer.palette)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

/// Check one picture's row in each of the two tables.
fn corroborate(
    reader: &RomReader<'_>,
    family: &TrainerFrontRoots,
    trainer: &TrainerFrontRoot,
) -> Result<(), ImportError> {
    picture_row(reader, family.front_table, trainer)?;
    palette_row(reader, family.palette_table, trainer)
}

/// One `struct CompressedSpriteSheet` row: `data`, `size`, `tag`.
fn picture_row(
    reader: &RomReader<'_>,
    table: GbaPtr,
    trainer: &TrainerFrontRoot,
) -> Result<(), ImportError> {
    let root = trainer.front.id;
    let row = row_offset(reader, "gTrainerFrontPicTable", table, trainer.trainer_pic)?;
    check_pointer(
        reader,
        row,
        0,
        trainer.front.addr,
        root,
        "CompressedSpriteSheet.data",
    )?;
    if reader.u16_le(row + PICTURE_SIZE_FIELD)? != PICTURE_SIZE {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpriteSheet.size",
        });
    }
    if reader.u16_le(row + PICTURE_TAG_FIELD)? != trainer.trainer_pic {
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
    table: GbaPtr,
    trainer: &TrainerFrontRoot,
) -> Result<(), ImportError> {
    let root = trainer.palette.id;
    let row = row_offset(
        reader,
        "gTrainerFrontPicPaletteTable",
        table,
        trainer.trainer_pic,
    )?;
    check_pointer(
        reader,
        row,
        0,
        trainer.palette.addr,
        root,
        "CompressedSpritePalette.data",
    )?;
    if reader.u16_le(row + PALETTE_TAG_FIELD)? != trainer.trainer_pic {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpritePalette.tag",
        });
    }
    Ok(())
}

/// The ROM offset of `trainer_pic`'s row, bounds-checked against the table.
fn row_offset(
    reader: &RomReader<'_>,
    table: &'static str,
    base: GbaPtr,
    trainer_pic: u16,
) -> Result<usize, ImportError> {
    let index = usize::from(trainer_pic);
    reader.table_entry(table, base, index, TABLE_ROWS, RECORD_BYTES)?;
    Ok(base.offset() + index * RECORD_BYTES)
}

#[cfg(test)]
mod tests;
