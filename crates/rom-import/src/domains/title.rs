//! The title screen: `title/image/*`, `title/raw/*`, `title/palette/*`.
//!
//! The first domain the importer reads, and the one that settles the shape
//! of the rest. Everything it needs is ordinary: six LZ77 tile sheets,
//! three LZ77 tilemaps copied through as opaque blobs, and five palettes
//! stored as GBA-native BGR555 runs.
//!
//! Two details are not ordinary, and both live in the profile table rather
//! than here:
//!
//! - `title/image/press_start` is 4bpp in the ROM and 8bpp in the pack.
//!   `ImageRoot`'s `rom_bit_depth`/`pack_bit_depth` split carries that, and
//!   [`super::image`] applies it.
//! - `title/palette/pokemon_logo` is 224 colours, not the 256 its upstream
//!   `.pal` file holds. Upstream's own build cuts it
//!   (`graphics_file_rules.mk`'s `-num_colors 224`) and the game never
//!   reads past 224 (`pokeemerald_rs::title`'s `LOGO_PALETTE_COLORS`), so
//!   both pack backends emit the cut palette and neither has anything to
//!   reconcile.
//!
//! `TitleScreenRoots::bg_palettes` is deliberately not written. It is
//! `gTitleScreenBgPalettes`, the address the logo palette and the
//! rayquaza/clouds palette are loaded from as one block; both already have
//! their own ids in `palettes`, and emitting the concatenation again would
//! put the same bytes in the pack twice.

use pack_format::PackWriter;

use crate::error::ImportError;
use crate::rom::Rom;
use crate::roots::Roots;

/// Write every title-screen entry.
///
/// # Errors
///
/// Any [`ImportError`] a root's read raises. Fails on the first one: a
/// partial title screen is not a useful pack.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let title = &roots.title_screen;
    for root in title.images {
        writer.push(super::image(&reader, root)?);
    }
    for root in title.tilemaps {
        writer.push(super::blob(&reader, root)?);
    }
    for root in title.palettes {
        writer.push(super::palette(&reader, root)?);
    }
    Ok(())
}
