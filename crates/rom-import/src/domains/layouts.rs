//! The map-layout domain: each layout's metatile grid and its border block.
//!
//! A layout is one `struct MapLayout`
//! (`pokeemerald/include/global.fieldmap.h`) and the two grids it points at.
//! Both are opaque `u16` cells the pack copies through, so the only work
//! here is corroborating the struct and reading the bytes.
//!
//! The grid's length is what upstream's own `map.bin` holds, which for a few
//! layouts is longer than `width * height * 2` (`littleroot_town_house3`'s
//! 13x13 grid is 340 bytes, two past the 338 its size implies). The profile
//! records that length, so the padding is read rather than reconstructed,
//! and `width`/`height` bound the grid rather than defining it.

use pack_format::PackWriter;

use super::{blob, check_pointer, len_usize};
use crate::error::ImportError;
use crate::reader::RomReader;
use crate::rom::Rom;
use crate::roots::{MapLayoutRoot, Roots};

/// Offset of `width` in `struct MapLayout`.
const FIELD_WIDTH: usize = 0x00;
/// Offset of `height`.
const FIELD_HEIGHT: usize = 0x04;
/// Offset of the `border` pointer.
const FIELD_BORDER: usize = 0x08;
/// Offset of the `map` pointer.
const FIELD_MAP: usize = 0x0C;
/// Offset of the `primaryTileset` pointer.
const FIELD_PRIMARY_TILESET: usize = 0x10;
/// Offset of the `secondaryTileset` pointer.
const FIELD_SECONDARY_TILESET: usize = 0x14;

/// Read every map layout the profile records and push its pack entries.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if a `struct MapLayout` disagrees with
/// the profile; [`ImportError::Truncated`] if a grid runs past the end of
/// the ROM.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    for layout in roots.layouts {
        corroborate(&reader, layout)?;
        writer.push(blob(&reader, &layout.map)?);
        writer.push(blob(&reader, &layout.border)?);
    }
    Ok(())
}

/// Check the ROM's `struct MapLayout` against what the profile claims.
///
/// Fields are checked in `global.fieldmap.h`'s declaration order.
fn corroborate(reader: &RomReader<'_>, layout: &MapLayoutRoot) -> Result<(), ImportError> {
    let base = layout.struct_addr.offset();
    let name = layout.name;

    dimension(
        reader,
        base,
        FIELD_WIDTH,
        layout.width,
        name,
        "MapLayout.width",
    )?;
    dimension(
        reader,
        base,
        FIELD_HEIGHT,
        layout.height,
        name,
        "MapLayout.height",
    )?;
    check_pointer(
        reader,
        base,
        FIELD_BORDER,
        layout.border.addr,
        name,
        "MapLayout.border",
    )?;
    check_pointer(
        reader,
        base,
        FIELD_MAP,
        layout.map.addr,
        name,
        "MapLayout.map",
    )?;
    check_pointer(
        reader,
        base,
        FIELD_PRIMARY_TILESET,
        layout.primary_tileset,
        name,
        "MapLayout.primaryTileset",
    )?;
    check_pointer(
        reader,
        base,
        FIELD_SECONDARY_TILESET,
        layout.secondary_tileset,
        name,
        "MapLayout.secondaryTileset",
    )?;

    // The declared size bounds the grid rather than defining it: upstream
    // pads a few `map.bin` files past `width * height * 2`, never trims one.
    let cells = len_usize(layout.width)
        .checked_mul(len_usize(layout.height))
        .and_then(|cells| cells.checked_mul(2));
    if cells.is_none_or(|needed| needed > len_usize(layout.map.len)) {
        return Err(ImportError::StructMismatch {
            root: name,
            field: "MapLayout.map",
        });
    }
    Ok(())
}

/// Check one `s32` dimension field against the profile.
fn dimension(
    reader: &RomReader<'_>,
    base: usize,
    offset: usize,
    expected: u32,
    root: &'static str,
    field: &'static str,
) -> Result<(), ImportError> {
    // A saturated offset is past any ROM, so the read below fails instead of
    // wrapping into a wrong one.
    let value = reader.u32_le(base.saturating_add(offset))?;
    if value != expected {
        return Err(ImportError::StructMismatch { root, field });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
