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
    let reader = rom.reader();
    let family = &roots.bag;
    let mut entries =
        Vec::with_capacity(family.images.len() + family.palettes.len() + family.raws.len());
    for root in family.images {
        entries.push(super::image(&reader, root)?);
    }
    for root in family.palettes {
        entries.push(super::palette(&reader, root)?);
    }
    for root in family.raws {
        entries.push(super::blob(&reader, root)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
