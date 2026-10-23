//! The battle textbox and menu text palettes: `battle-interface/*`.
//!
//! The textbox sheet, its 32-colour palette, and the window-text palette are
//! LZ77 streams; the PP text palette is a raw `.gbapal` run; the textbox
//! tilemap is an LZ77 stream copied through as a blob. Every root is plain, so
//! each goes through the shared codecs.
//!
//! The textbox palette is a single ROM root. The build concatenates
//! `textbox_0.pal` and `textbox_1.pal` into `gBattleTextboxPalette`, so the
//! importer reads it as one 32-colour palette and invents no second asset.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every battle-interface entry.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched. A profile with no battle-interface roots writes nothing.
///
/// # Errors
///
/// Any [`ImportError`] an image, palette or blob read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let family = &roots.battle_interface;
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
