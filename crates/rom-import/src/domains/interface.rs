//! Interface art: `interface/image/*` and `interface/palette/*`.
//!
//! Two kinds of entry. `interface/palette/main_menu_bg` is the single
//! per-pixel colour source the no-save main menu needs (I-3; the rest of
//! that scene's colours are upstream's own runtime `LoadPalette` literals,
//! not file-sourced). `interface/image/down_arrow` and
//! `interface/image/down_arrow_alt` are the text printer's light and dark
//! page-prompt arrows, `sDownArrowTiles` and `sDarkDownArrowTiles`
//! (`pokeemerald/src/text.c:71-72`): 8x48 raw 4bpp sheets with no palette of
//! their own, since the printer blits their indices into the window and the
//! window owns the colours. It is its
//! own domain rather than a title-screen afterthought because the interface
//! grows its own roots as more menus land, and the profile table already
//! groups it that way.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every interface image and palette.
///
/// # Errors
///
/// Any [`ImportError`] an image or palette read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    for root in roots.interface.images {
        writer.push(super::image(&reader, root)?);
    }
    for root in roots.interface.palettes {
        writer.push(super::palette(&reader, root)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
