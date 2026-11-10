//! The starter chooser: `starter-chooser/*`.
//!
//! Three LZ77 4bpp sheets, their raw `.gbapal` palettes, and the two Birch
//! background tilemaps (LZ77, copied through as blobs). Every root is plain,
//! so each goes through the shared codecs.
//!
//! The chooser tiles are 4-bit in the ROM and in the pack: upstream's build
//! keeps the low nibble of the 8-bit `tiles.png`, and the checkout extractor
//! publishes that same nibble, so no depth split is needed.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every starter-chooser entry.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched. A profile with no chooser roots writes nothing.
///
/// # Errors
///
/// Any [`ImportError`] an image, palette or blob read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.starter_chooser;
    super::write_plain(rom, family.images, family.palettes, family.tilemaps, writer)
}

#[cfg(test)]
mod tests;
