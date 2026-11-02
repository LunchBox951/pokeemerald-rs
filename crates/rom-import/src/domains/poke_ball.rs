//! The Poké Ball: `poke-ball/*`.
//!
//! The sheet and its palette are LZ77 streams that go through the shared
//! codecs, and so is the opened-ball sheet. What the profile adds is the
//! `BALL_POKE` row of `gBallSpriteSheets` and `gBallSpritePalettes`: each must
//! hold the address the profile records, and the size and tag upstream writes,
//! so a profile that picked a stray copy of the art is
//! [`ImportError::StructMismatch`]. The opened-ball sheet has no row of its
//! own; it is drawn with the Poké Ball's palette.

use pack_format::PackWriter;

use super::{check_pointer, image, palette};
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{PokeBallRoot, Roots};

/// Both tables hold `{data, size, tag}` or `{data, tag, pad}` rows: eight bytes.
const RECORD_BYTES: usize = 8;
/// `POKEBALL_COUNT`, the rows each table holds.
const TABLE_ROWS: usize = 12;
/// `BALL_POKE`, the first row of both tables.
const BALL_POKE_ROW: usize = 0;
/// The `size` upstream writes for every ball sheet: 12 4bpp tiles.
const SHEET_BYTES: u16 = 384;
/// `GFX_TAG_POKE_BALL`.
const POKE_BALL_TAG: u16 = 55000;
/// Offset of a sheet row's `size` field.
const SHEET_SIZE_FIELD: usize = 4;
/// Offset of a sheet row's `tag` field.
const SHEET_TAG_FIELD: usize = 6;
/// Offset of a palette row's `tag` field.
const PALETTE_TAG_FIELD: usize = 4;

/// Write the Poké Ball's sheet, palette, and opened-ball sheet.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched. A profile with no Poké Ball record writes nothing.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if the `BALL_POKE` row of either table
/// disagrees with the profile; any [`ImportError`] an image or palette read
/// raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.poke_ball;
    let Some(ball) = &family.poke_ball else {
        return Ok(());
    };
    let reader = rom.reader();
    corroborate(&reader, family.sheet_table, family.palette_table, ball)?;
    let entries = [
        image(&reader, &ball.sheet)?,
        palette(&reader, &ball.palette)?,
        image(&reader, &ball.open_sheet)?,
    ];
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

/// Check the `BALL_POKE` row of both tables against the recorded art.
fn corroborate(
    reader: &RomReader<'_>,
    sheet_table: GbaPtr,
    palette_table: GbaPtr,
    ball: &PokeBallRoot,
) -> Result<(), ImportError> {
    let sheet_row = row_offset(reader, "gBallSpriteSheets", sheet_table)?;
    let root = ball.sheet.id;
    check_pointer(
        reader,
        sheet_row,
        0,
        ball.sheet.addr,
        root,
        "CompressedSpriteSheet.data",
    )?;
    if reader.u16_le(sheet_row + SHEET_SIZE_FIELD)? != SHEET_BYTES {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpriteSheet.size",
        });
    }
    if reader.u16_le(sheet_row + SHEET_TAG_FIELD)? != POKE_BALL_TAG {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpriteSheet.tag",
        });
    }

    let palette_row = row_offset(reader, "gBallSpritePalettes", palette_table)?;
    let root = ball.palette.id;
    check_pointer(
        reader,
        palette_row,
        0,
        ball.palette.addr,
        root,
        "CompressedSpritePalette.data",
    )?;
    if reader.u16_le(palette_row + PALETTE_TAG_FIELD)? != POKE_BALL_TAG {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpritePalette.tag",
        });
    }
    Ok(())
}

/// The ROM offset of the `BALL_POKE` row, bounds-checked against the table.
fn row_offset(
    reader: &RomReader<'_>,
    table: &'static str,
    base: GbaPtr,
) -> Result<usize, ImportError> {
    reader.table_entry(table, base, BALL_POKE_ROW, TABLE_ROWS, RECORD_BYTES)?;
    Ok(base.offset() + BALL_POKE_ROW * RECORD_BYTES)
}

#[cfg(test)]
mod tests;
