//! The evolution scene's background and sparkle sprite: `evolution-scene/*`.
//!
//! Two 4bpp images and two screen tilemaps, all LZ77 streams, and three raw
//! `.gbapal` palettes. Every root is plain, so each goes through the shared
//! codecs.
//!
//! The background is a 4-bit grayscale PNG upstream; `gbagfx` packs each
//! inverted sample as the tile index, and the checkout extractor decodes it
//! the same way, so the ROM's tiles and the pack agree without a depth split.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every evolution-scene entry.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched. A profile with no evolution-scene roots writes nothing.
///
/// # Errors
///
/// Any [`ImportError`] an image, palette or blob read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let family = &roots.evolution_scene;
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
