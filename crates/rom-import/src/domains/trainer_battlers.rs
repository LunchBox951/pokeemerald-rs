//! The Brendan and May trainer battlers: `trainer/*`.
//!
//! A trainer's front picture and palette are read and corroborated exactly
//! as the Birch-speech portraits are ([`super::trainer_front`]); the profile
//! records them again under the battler ids. The back picture is the
//! uncompressed four-frame sheet `gTrainerBackPicTable` points at. Its two
//! tables are indexed by `TRAINER_BACK_PIC_*`, not by the front picture's
//! index, and the back palette row points at the same palette stream as the
//! trainer's front palette row (upstream names `gTrainerPalette_Brendan` in
//! both), so a profile whose back row names another copy of the colours is
//! [`ImportError::StructMismatch`] rather than a palette that happens to match.

use pack_format::PackWriter;

use super::image;
use super::trainer_front::{palette_row, picture_row, read_entries, PicTable};
use crate::error::ImportError;
use crate::reader::RomReader;
use crate::rom::Rom;
use crate::roots::{Roots, TrainerBackRoot, TrainerBattlerRoots, TrainerFrontRoot};

/// How many rows each back table holds: `TRAINER_BACK_PIC_STEVEN + 1`.
const BACK_TABLE_ROWS: usize = 8;
/// A back picture row's `size`: four frames of `TRAINER_PIC_SIZE` (0x800).
const BACK_PICTURE_SIZE: u16 = 0x2000;

/// Write every trainer battler image and palette.
///
/// Entries are read before any is queued, so a failing root leaves the
/// writer untouched rather than holding part of the domain. A profile with
/// no trainer battler roots writes nothing.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if a table row disagrees with the profile
/// or a back picture names a trainer the profile has no front for;
/// [`ImportError::IndexOutOfRange`] if a picture is past the end of a table;
/// any [`ImportError`] an image or palette read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let reader = rom.reader();
    let family = &roots.trainer_battlers;
    let mut entries = read_entries(&reader, &family.front)?;
    for back in family.backs {
        corroborate_back(&reader, family, back)?;
        entries.push(image(&reader, &back.back)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

/// Check one back picture's row in each of the two back tables.
fn corroborate_back(
    reader: &RomReader<'_>,
    family: &TrainerBattlerRoots,
    back: &TrainerBackRoot,
) -> Result<(), ImportError> {
    let front = trainer_front(family, back)?;
    picture_row(
        reader,
        PicTable {
            name: "gTrainerBackPicTable",
            base: family.back_table,
            rows: BACK_TABLE_ROWS,
        },
        back.back_pic,
        &back.back,
        BACK_PICTURE_SIZE,
    )?;
    palette_row(
        reader,
        PicTable {
            name: "gTrainerBackPicPaletteTable",
            base: family.back_palette_table,
            rows: BACK_TABLE_ROWS,
        },
        back.back_pic,
        front.palette.id,
        front.palette.addr,
    )
}

/// The front root of the trainer a back picture belongs to.
fn trainer_front<'a>(
    family: &'a TrainerBattlerRoots,
    back: &TrainerBackRoot,
) -> Result<&'a TrainerFrontRoot, ImportError> {
    family
        .front
        .trainers
        .iter()
        .find(|front| front.trainer_pic == back.trainer_pic)
        .ok_or(ImportError::StructMismatch {
            root: back.back.id,
            field: "TrainerBackRoot.trainer_pic",
        })
}

#[cfg(test)]
mod tests;
