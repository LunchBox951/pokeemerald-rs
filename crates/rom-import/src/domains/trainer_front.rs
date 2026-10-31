//! The Birch-speech trainer front pictures: `trainer-pic/*`.
//!
//! Each picture is an LZ77 4bpp image and an LZ77 palette that go through
//! the shared codecs. What the profile adds is the two trainer tables
//! (`gTrainerFrontPicTable`, `gTrainerFrontPicPaletteTable`): a picture's row
//! in each table must hold the address the profile records for that picture
//! and the tag its row carries, so a profile that picked a stray copy of the
//! art is [`ImportError::StructMismatch`] rather than a picture that happens
//! to have the right pixels today.

use pack_format::{PackEntry, PackWriter};

use super::{check_pointer, palette};
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{ImageRoot, Roots, TrainerFrontRoot, TrainerFrontRoots};

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
    let entries = read_entries(&rom.reader(), &roots.trainer_front)?;
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

/// Read and corroborate every picture of one front family: an image and a
/// palette per trainer, in that order.
///
/// # Errors
///
/// As [`write`].
pub(super) fn read_entries(
    reader: &RomReader<'_>,
    family: &TrainerFrontRoots,
) -> Result<Vec<PackEntry>, ImportError> {
    let mut entries = Vec::with_capacity(family.trainers.len() * 2);
    for trainer in family.trainers {
        corroborate(reader, family, trainer)?;
        entries.push(super::image(reader, &trainer.front)?);
        entries.push(palette(reader, &trainer.palette)?);
    }
    Ok(entries)
}

/// Check one picture's row in each of the two tables.
fn corroborate(
    reader: &RomReader<'_>,
    family: &TrainerFrontRoots,
    trainer: &TrainerFrontRoot,
) -> Result<(), ImportError> {
    let pictures = PicTable {
        name: "gTrainerFrontPicTable",
        base: family.front_table,
        rows: TABLE_ROWS,
    };
    let palettes = PicTable {
        name: "gTrainerFrontPicPaletteTable",
        base: family.palette_table,
        rows: TABLE_ROWS,
    };
    picture_row(
        reader,
        pictures,
        trainer.trainer_pic,
        &trainer.front,
        PICTURE_SIZE,
    )?;
    palette_row(
        reader,
        palettes,
        trainer.trainer_pic,
        trainer.palette.id,
        trainer.palette.addr,
    )
}

/// One of the trainer picture tables: where it is and how many rows it holds.
#[derive(Clone, Copy)]
pub(super) struct PicTable {
    /// The upstream symbol, for the bounds fault.
    pub(super) name: &'static str,
    pub(super) base: GbaPtr,
    pub(super) rows: usize,
}

/// One `struct CompressedSpriteSheet` row: `data`, `size`, `tag`. Row `index`
/// must name `picture`'s address, carry `size`, and be tagged with `index`.
pub(super) fn picture_row(
    reader: &RomReader<'_>,
    table: PicTable,
    index: u16,
    picture: &ImageRoot,
    size: u16,
) -> Result<(), ImportError> {
    let root = picture.id;
    let row = row_offset(reader, table, index)?;
    check_pointer(
        reader,
        row,
        0,
        picture.addr,
        root,
        "CompressedSpriteSheet.data",
    )?;
    if reader.u16_le(row + PICTURE_SIZE_FIELD)? != size {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpriteSheet.size",
        });
    }
    if reader.u16_le(row + PICTURE_TAG_FIELD)? != index {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpriteSheet.tag",
        });
    }
    Ok(())
}

/// One `struct CompressedSpritePalette` row: `data`, `tag`. Row `index` must
/// name the palette at `addr` and be tagged with `index`; `root` is the
/// palette root the failure is reported against.
pub(super) fn palette_row(
    reader: &RomReader<'_>,
    table: PicTable,
    index: u16,
    root: &'static str,
    addr: GbaPtr,
) -> Result<(), ImportError> {
    let row = row_offset(reader, table, index)?;
    check_pointer(reader, row, 0, addr, root, "CompressedSpritePalette.data")?;
    if reader.u16_le(row + PALETTE_TAG_FIELD)? != index {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpritePalette.tag",
        });
    }
    Ok(())
}

/// The ROM offset of row `index`, bounds-checked against the table.
fn row_offset(reader: &RomReader<'_>, table: PicTable, index: u16) -> Result<usize, ImportError> {
    let index = usize::from(index);
    reader.table_entry(table.name, table.base, index, table.rows, RECORD_BYTES)?;
    Ok(table.base.offset() + index * RECORD_BYTES)
}

#[cfg(test)]
mod tests;
