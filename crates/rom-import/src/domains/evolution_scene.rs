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
    let family = &roots.evolution_scene;
    super::write_plain(rom, family.images, family.palettes, family.tilemaps, writer)
}

#[cfg(test)]
mod tests;
