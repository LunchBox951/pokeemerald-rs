//! Interface palettes: `interface/palette/*`.
//!
//! One entry so far, `interface/palette/main_menu_bg`, the single per-pixel
//! colour source the no-save main menu needs (issue #216, I-3; the rest of
//! that scene's colours are upstream's own runtime `LoadPalette` literals,
//! not file-sourced). It is its own domain rather than a title-screen
//! afterthought because the interface grows its own roots as more menus
//! land, and the profile table already groups it that way.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every interface palette.
///
/// # Errors
///
/// Any [`ImportError`] a palette read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    for root in roots.interface.palettes {
        writer.push(super::palette(&reader, root)?);
    }
    Ok(())
}
