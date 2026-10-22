//! Birch's speech scene and the wall clock: `opening-scene/*`.
//!
//! Five 4bpp sheets (the Birch sprite raw, the rest LZ77), eight palettes
//! (Lotad's normal palette LZ77, the rest raw `.gbapal` runs), and three
//! LZ77 screen tilemaps copied through as blobs. Every root is plain, so each
//! goes through the shared codecs.
//!
//! The clock hand is a 4-bit grayscale PNG upstream; `gbagfx` packs each
//! inverted sample as the palette index, and the checkout extractor decodes
//! it the same way, so the ROM's tiles and the pack agree without a depth split.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every opening-scene entry.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched. A profile with no opening-scene roots writes nothing.
///
/// # Errors
///
/// Any [`ImportError`] an image, palette or blob read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let family = &roots.opening_scene;
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
