//! The naming screen: `naming-screen/image/*`, `naming-screen/palette/*`,
//! `naming-screen/raw/*`.
//!
//! Only the menu sheet and the four tilemaps are LZ77 streams; the other
//! fourteen sheets and the seven palettes are raw. The three cursor sheets
//! are stored as their first five tiles (`-num_tiles 5`), which the profile
//! declares as the 8x40 strip the pack publishes, so every root goes through
//! the shared image, palette and blob codecs.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every naming-screen entry.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched. A profile with no naming-screen roots writes nothing.
///
/// # Errors
///
/// Any [`ImportError`] an image, palette or blob read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let family = &roots.naming_screen;
    let mut entries =
        Vec::with_capacity(family.images.len() + family.palettes.len() + family.tilemaps.len());
    for root in family.images {
        entries.push(super::image(&reader, root)?);
    }
    for root in family.palettes {
        entries.push(super::palette(&reader, root)?);
    }
    for root in family.tilemaps {
        entries.push(super::blob(&reader, root)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
