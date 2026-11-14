//! The party menu: `party-menu/image/*`, `party-menu/palette/*`,
//! `party-menu/raw/*`.
//!
//! The background sheet, palette and screen and the Poké Ball sheet and
//! palette are LZ77 streams (`src/graphics.c:1111-1117`); the held-item sheet
//! and palette, the five `u8` slot maps and the two button maps are raw
//! (`src/data/party_menu.h`). The background sheet is stored as its first 62
//! tiles (`-num_tiles 62`), which the profile declares as the 8x496 strip the
//! pack publishes, so every root goes through the shared image, palette and
//! blob codecs.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every party-menu entry.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched. A profile with no party-menu roots writes nothing.
///
/// # Errors
///
/// Any [`ImportError`] an image, palette or blob read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let family = &roots.party_menu;
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
