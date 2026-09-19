//! Text windows: `text-window/image/*` and `text-window/palette/*`.
//!
//! Twenty border frames, the message box, and a palette for each, plus the
//! four `text_pal*` banks. All ordinary: raw 4bpp tiles and BGR555 runs,
//! through [`super::image`] and [`super::palette`].
//!
//! The one struct is `sTextWindowPalettes` (`pokeemerald/src/text_window.c`),
//! five consecutive banks: the message box's colours, then `text_pal1` to
//! `text_pal4`. The message box's palette is stored twice in the ROM, and
//! the profile records the copy beside `gMessageBox_Gfx`, so the bank here
//! is not read as an entry; it is read to check that the profile's message
//! box palette agrees with it, and the four banks after it are checked to
//! be where the profile says the `text_pal*` entries are. Either
//! disagreement is [`ImportError::StructMismatch`].

use pack_format::PackWriter;

use super::{image, palette};
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{PaletteRoot, Roots, TextWindowRoots};

/// One 16-colour bank, in bytes.
const BANK_BYTES: usize = 32;
/// The pack id of the bank `sTextWindowPalettes[0]` duplicates.
const MESSAGE_BOX: &str = "text-window/palette/message_box";
/// The pack ids of `sTextWindowPalettes[1..=4]`, in table order.
const WINDOW_BANKS: [&str; 4] = [
    "text-window/palette/text_pal1",
    "text-window/palette/text_pal2",
    "text-window/palette/text_pal3",
    "text-window/palette/text_pal4",
];
/// The table's name in an error.
const TABLE: &str = "sTextWindowPalettes";

/// Write every text-window image and palette.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if `sTextWindowPalettes` disagrees with
/// the profile; [`ImportError::Truncated`] if any root runs past the end of
/// the ROM; [`ImportError::EntryShape`] if a sheet's tiles do not fit its
/// raster.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let text_window = &roots.text_window;
    corroborate(&reader, text_window)?;

    for root in text_window.images {
        writer.push(image(&reader, root)?);
    }
    for root in text_window.palettes {
        writer.push(palette(&reader, root)?);
    }
    Ok(())
}

/// Check `sTextWindowPalettes` against the profile.
///
/// Skipped for a profile with no palettes: there is nothing to corroborate
/// and no table to read.
fn corroborate(reader: &RomReader<'_>, text_window: &TextWindowRoots) -> Result<(), ImportError> {
    if text_window.palettes.is_empty() {
        return Ok(());
    }
    let table = text_window.window_palettes;

    // Bank 0 duplicates the message box's palette; the bytes have to agree.
    let message_box = find(text_window, MESSAGE_BOX)?;
    let recorded = reader.slice(message_box.addr, BANK_BYTES)?;
    if reader.slice(table, BANK_BYTES)? != recorded {
        return Err(ImportError::StructMismatch {
            root: TABLE,
            field: "message_box",
        });
    }

    // Banks 1 to 4 are the `text_pal*` entries themselves.
    for (index, id) in WINDOW_BANKS.iter().enumerate() {
        let root = find(text_window, id)?;
        let expected = bank(table, index + 1)?;
        if root.addr != expected {
            return Err(ImportError::StructMismatch {
                root: TABLE,
                field: id,
            });
        }
    }
    Ok(())
}

/// The address of bank `index` of a table at `table`.
///
/// A bank past the cartridge window is reported as a truncated read at the
/// offset it would have started from.
fn bank(table: GbaPtr, index: usize) -> Result<GbaPtr, ImportError> {
    let delta = u32::try_from(index * BANK_BYTES).unwrap_or(u32::MAX);
    table
        .raw()
        .checked_add(delta)
        .and_then(GbaPtr::new)
        .ok_or(ImportError::Truncated {
            at: table.offset().saturating_add(index * BANK_BYTES),
            len: BANK_BYTES,
        })
}

/// The profile's palette root with pack id `id`.
///
/// A profile that records any text-window palette records all of them,
/// so a missing one is a profile mismatch rather than an empty domain.
fn find<'a>(
    text_window: &'a TextWindowRoots,
    id: &'static str,
) -> Result<&'a PaletteRoot, ImportError> {
    text_window
        .palettes
        .iter()
        .find(|root| root.id == id)
        .ok_or(ImportError::StructMismatch {
            root: TABLE,
            field: id,
        })
}

#[cfg(test)]
mod tests;
