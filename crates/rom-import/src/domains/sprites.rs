//! Object-event sprites: `sprite/*` sheets and `sprite/palette/*` banks.
//!
//! The sheets are the plainest art in the ROM: raw 4bpp tiles in `gbagfx`'s
//! metatile order, one per `gObjectEventPic_*`, with nothing pointing at
//! them that the profile needs. They read straight through [`super::image`].
//!
//! The palettes have a struct behind them. `sObjectEventSpritePalettes`
//! (`pokeemerald/src/event_object_movement.c`) is the `{data, tag}` table
//! the game resolves a palette tag through, and it is the reason the
//! profile records the addresses it does: several banks are stored two or
//! three times, byte for byte, and only the copy the table names is the
//! one the game loads. This reader walks the table to its terminator and
//! requires every profile palette to be named in it, so a profile that
//! picked a stray copy is [`ImportError::StructMismatch`] rather than a
//! bank that happens to have the right colours today.
//!
//! The terminator is an all-zero record. Upstream's source spells it `{}`
//! and notes that `FindObjectEventPaletteIndexByTag` looks for
//! `OBJ_EVENT_PAL_TAG_NONE` instead, a bug the retail build ships with
//! (`BUGFIX` writes `{NULL, OBJ_EVENT_PAL_TAG_NONE}`). The walk stops at a
//! `NULL` `data` pointer, which both spellings share.

use pack_format::PackWriter;

use super::{image, palette};
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{Roots, SpriteRoots};

/// One `struct SpritePalette`: a pointer, a `u16` tag, two bytes of
/// padding.
const RECORD_BYTES: usize = 8;
/// How many records the walk will read before deciding the terminator is
/// missing. Upstream declares 37; the bound only stops a wrong address
/// from walking the whole ROM.
const MAX_RECORDS: usize = 128;
/// The table's name in an error.
const TABLE: &str = "sObjectEventSpritePalettes";

/// Write every sprite sheet and palette.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if the palette table does not name a
/// profile palette, or has no terminator within [`MAX_RECORDS`];
/// [`ImportError::Truncated`] if any root runs past the end of the ROM;
/// [`ImportError::EntryShape`] if a sheet's tiles do not fit its raster.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let sprites = &roots.sprites;
    corroborate(&reader, sprites)?;

    for root in sprites.sheets {
        writer.push(image(&reader, root)?);
    }
    for root in sprites.palettes {
        writer.push(palette(&reader, root)?);
    }
    Ok(())
}

/// Check that `sObjectEventSpritePalettes` names every profile palette.
///
/// Skipped for a profile with no palettes: there is nothing to corroborate
/// and no table to read.
fn corroborate(reader: &RomReader<'_>, sprites: &SpriteRoots) -> Result<(), ImportError> {
    if sprites.palettes.is_empty() {
        return Ok(());
    }
    let named = palette_table(reader, sprites.palette_table)?;
    for root in sprites.palettes {
        if !named.contains(&root.addr) {
            return Err(ImportError::StructMismatch {
                root: root.id,
                field: "SpritePalette.data",
            });
        }
    }
    Ok(())
}

/// Every `data` pointer in the table, up to its terminator.
fn palette_table(reader: &RomReader<'_>, table: GbaPtr) -> Result<Vec<GbaPtr>, ImportError> {
    let mut named = Vec::new();
    for index in 0..MAX_RECORDS {
        let record = reader.table_entry(TABLE, table, index, MAX_RECORDS, RECORD_BYTES)?;
        let data = u32::from_le_bytes([record[0], record[1], record[2], record[3]]);
        if data == 0 {
            return Ok(named);
        }
        // Only a live record's pointer has to parse as one.
        let base = table.offset().saturating_add(index * RECORD_BYTES);
        named.push(reader.ptr(base)?);
    }
    Err(ImportError::StructMismatch {
        root: TABLE,
        field: "SpritePalette.data",
    })
}

#[cfg(test)]
mod tests;
