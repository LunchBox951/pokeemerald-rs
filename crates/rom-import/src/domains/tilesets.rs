//! The tileset domain: tile sheets, palette blocks, metatile tables, and
//! the animation frames `src/tileset_anims.c` cycles through.
//!
//! Everything a tileset owns hangs off one `struct Tileset`
//! (`pokeemerald/include/global.fieldmap.h`). The profile already records
//! where each piece lives, so this reader does not walk those pointers to
//! *find* anything; it reads the struct to check that the ROM agrees with
//! the profile, and refuses if it does not.
//!
//! Two shapes recur and are worth stating once. Tile sheets are 4bpp and
//! LZ77-compressed, and the ROM often stores fewer tiles than the raster
//! needs, because upstream's `-num_tiles` cut drops all-zero trailing tiles;
//! [`pack_format::image_entry_from_tiles`] zero-fills the rest. Animation
//! frames are the opposite: raw 4bpp arrays, scattered and out of frame
//! order, each at its own address.

use pack_format::PackWriter;

use super::{blob, check_pointer, image, palette};
use crate::error::ImportError;
use crate::reader::RomReader;
use crate::rom::Rom;
use crate::roots::{Roots, TilesetRoot};

/// Offset of `isCompressed` in `struct Tileset`.
const FIELD_IS_COMPRESSED: usize = 0x00;
/// Offset of `isSecondary`.
const FIELD_IS_SECONDARY: usize = 0x01;
/// Offset of the `tiles` pointer.
const FIELD_TILES: usize = 0x04;
/// Offset of the `palettes` pointer.
const FIELD_PALETTES: usize = 0x08;
/// Offset of the `metatiles` pointer.
const FIELD_METATILES: usize = 0x0C;
/// Offset of the `metatileAttributes` pointer.
const FIELD_METATILE_ATTRIBUTES: usize = 0x10;
/// Offset of the `callback` pointer.
const FIELD_CALLBACK: usize = 0x14;

/// Read every tileset the profile records and push its pack entries.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if a `struct Tileset` disagrees with the
/// profile; [`ImportError::Truncated`] if any root runs past the end of the
/// ROM; [`ImportError::Lz77`] if a tile sheet does not decode to its
/// recorded size; [`ImportError::EntryShape`] if the bytes will not shape
/// into the entry the profile describes.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    for tileset in roots.tilesets {
        corroborate(&reader, tileset)?;

        writer.push(image(&reader, &tileset.tiles)?);
        for bank in tileset.palettes {
            writer.push(palette(&reader, bank)?);
        }
        writer.push(blob(&reader, &tileset.metatiles)?);
        writer.push(blob(&reader, &tileset.metatile_attributes)?);
        for anim in tileset.anims {
            for frame in anim.frames {
                writer.push(image(&reader, frame)?);
            }
        }
    }
    Ok(())
}

/// Check the ROM's `struct Tileset` against what the profile claims.
///
/// The fields are checked in `global.fieldmap.h`'s declaration order, which
/// is also the order they are read in.
fn corroborate(reader: &RomReader<'_>, tileset: &TilesetRoot) -> Result<(), ImportError> {
    let base = tileset.struct_addr.offset();
    let name = tileset.name;

    flag(
        reader,
        base,
        FIELD_IS_COMPRESSED,
        tileset.is_compressed,
        name,
        "Tileset.isCompressed",
    )?;
    flag(
        reader,
        base,
        FIELD_IS_SECONDARY,
        tileset.is_secondary,
        name,
        "Tileset.isSecondary",
    )?;
    check_pointer(
        reader,
        base,
        FIELD_TILES,
        tileset.tiles.addr,
        name,
        "Tileset.tiles",
    )?;
    // The struct points at the palette block, which is bank 0's address.
    let bank0 = tileset
        .palettes
        .first()
        .ok_or(ImportError::StructMismatch {
            root: name,
            field: "Tileset.palettes",
        })?
        .addr;
    check_pointer(
        reader,
        base,
        FIELD_PALETTES,
        bank0,
        name,
        "Tileset.palettes",
    )?;
    check_pointer(
        reader,
        base,
        FIELD_METATILES,
        tileset.metatiles.addr,
        name,
        "Tileset.metatiles",
    )?;
    check_pointer(
        reader,
        base,
        FIELD_METATILE_ATTRIBUTES,
        tileset.metatile_attributes.addr,
        name,
        "Tileset.metatileAttributes",
    )?;

    // The callback addresses code with the Thumb bit set, so it is a plain
    // word rather than a cartridge pointer, and `0` for a tileset that
    // animates nothing.
    let callback = reader.u32_le(base.saturating_add(FIELD_CALLBACK))?;
    if callback != tileset.callback {
        return Err(ImportError::StructMismatch {
            root: name,
            field: "Tileset.callback",
        });
    }
    Ok(())
}

/// Check one `bool8` struct field.
///
/// A `bool8` the compiler wrote holds 0 or 1. Anything else means these
/// bytes are not the struct the profile thinks they are, so the range check
/// runs before the comparison and reports the same way.
fn flag(
    reader: &RomReader<'_>,
    base: usize,
    offset: usize,
    expected: bool,
    root: &'static str,
    field: &'static str,
) -> Result<(), ImportError> {
    // A saturated offset is past any ROM, so the read below fails instead of
    // wrapping into a wrong one.
    let value = reader.u8(base.saturating_add(offset))?;
    if value > 1 || (value == 1) != expected {
        return Err(ImportError::StructMismatch { root, field });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
