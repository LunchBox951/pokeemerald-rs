//! The trainer class display names: `trainer-class-name/table`.
//!
//! `gTrainerClassNames` is one array of 66 fixed-width rows
//! (`const u8 gTrainerClassNames[][13]`), not 66 pointers: each name is its
//! charmap bytes, an `EOS` (`0xFF`), and zero padding to the row's 13 bytes.
//! The profile addresses the array once. This reader takes exactly the array's
//! 858 bytes and hands them to [`TrainerClassNameTable::from_fixed_rows`],
//! which splits each row at its first token-boundary `EOS`, requires zero
//! padding after it, and owns the canonical encoding, so the checkout
//! extractor and this reader cannot drift on the wire format.
//!
//! The read is bounded to the array: a row with no terminator inside its width
//! is refused rather than borrowing one from the next row, and padding is never
//! trimmed, because a zero byte is a space in the charmap.

use assets::pack::TrainerClassNameError;
use assets::{TrainerClassNameTable, TRAINER_CLASS_NAME_COUNT, TRAINER_CLASS_NAME_ROW_BYTES};
use pack_format::{raw_entry, PackWriter};

use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::Roots;

/// The pack id this domain produces.
pub const PACK_ID: &str = "trainer-class-name/table";

/// The byte length of `gTrainerClassNames`.
pub const TABLE_BYTES: usize = TRAINER_CLASS_NAME_COUNT * TRAINER_CLASS_NAME_ROW_BYTES;

/// Write the trainer class-name table, if the profile records one.
///
/// A profile with no class-name root writes nothing, like any other domain
/// with no roots.
///
/// # Errors
///
/// As [`decode_trainer_class_name_payload`].
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let Some(table) = roots.trainer_class_names else {
        return Ok(());
    };
    let payload = decode_trainer_class_name_payload(&rom.reader(), table)?;
    writer.push(raw_entry(PACK_ID.to_owned(), payload));
    Ok(())
}

/// The canonical `trainer-class-name/table` payload the array at `table` yields.
///
/// Public so `cargo xtask gen-rom-profile` can run the importer's own reader
/// over a located address and compare it with the checkout pack, as it does for
/// the field-message table.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if `table` is the unset placeholder;
/// [`ImportError::Truncated`] or [`ImportError::PointerOutOfRange`] if the
/// array runs off the image; [`ImportError::TrainerClassName`] if a row has no
/// well-formed terminator inside its width or a nonzero byte after its name.
pub fn decode_trainer_class_name_payload(
    reader: &RomReader<'_>,
    table: GbaPtr,
) -> Result<Vec<u8>, ImportError> {
    if table == GbaPtr::AT_BASE {
        return Err(ImportError::StructMismatch {
            root: PACK_ID,
            field: "trainer class name address",
        });
    }
    let rows = reader.slice(table, TABLE_BYTES)?;
    TrainerClassNameTable::from_fixed_rows(rows)
        .map(|table| table.encode())
        .map_err(
            |source: TrainerClassNameError| ImportError::TrainerClassName {
                id: PACK_ID,
                source,
            },
        )
}

#[cfg(test)]
mod tests;
