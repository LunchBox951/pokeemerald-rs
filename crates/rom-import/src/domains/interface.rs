//! Interface art: `interface/image/*` and `interface/palette/*`.
//!
//! Two kinds of entry. `interface/palette/main_menu_bg` is the single
//! per-pixel colour source the no-save main menu needs (I-3; the rest of
//! that scene's colours are upstream's own runtime `LoadPalette` literals,
//! not file-sourced). `interface/image/down_arrow` is the text printer's
//! page-prompt arrow, `sDownArrowTiles` (`pokeemerald/src/text.c:71`): an
//! 8x48 raw 4bpp sheet with no palette of its own, since the printer blits
//! its indices into the window and the window owns the colours. It is its
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
    let family = &roots.interface;
    super::write_plain(rom, family.images, family.palettes, &[], writer)
}

#[cfg(test)]
mod tests;
