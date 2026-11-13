//! The summary screen's graphics: `summary-screen-graphics/image/*`,
//! `summary-screen-graphics/palette/*`, `summary-screen-graphics/raw/*`.
//!
//! The tile and move-select sheets and palettes and the five page screens are
//! LZ77 streams (`src/graphics.c:1125-1134`); the three move-effect maps, the
//! status map, the markings palette, and the two button sheets are raw
//! (`src/graphics.c:1253-1255`, `src/pokemon_summary_screen.c`). The tile
//! sheet is stored whole (240 tiles), with eight palette banks, so every root
//! goes through the shared image, palette and blob codecs.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every summary-screen graphics entry.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched. A profile with no summary-screen graphics roots writes
/// nothing.
///
/// # Errors
///
/// Any [`ImportError`] an image, palette or blob read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let family = &roots.summary_screen_graphics;
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
