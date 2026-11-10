//! The ordinary bag: `bag/*`.
//!
//! The fixed bag images, palettes and tilemap (including the list-scroll
//! arrow sheet with `red.pal`, the swap-line sheet with its own palette, and
//! the menu-info icon sheet with its three palettes), plus the deduplicated
//! item-icon images and palettes the checkout extractor emits. Every root is
//! a plain image, palette or blob, so each goes through the shared codecs.
//! The `gItemIconTable` row-to-icon mapping is not read here: it is typed
//! checkout data, not a pack entry.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every bag root.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched rather than holding part of the domain. A profile with
/// no bag roots writes nothing.
///
/// # Errors
///
/// Any [`ImportError`] an image, palette or blob read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.bag;
    super::write_plain(rom, family.images, family.palettes, family.tilemaps, writer)
}

#[cfg(test)]
mod tests;
