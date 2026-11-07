//! The summary screen's identity and met-location records:
//! `summary-identity/table`.
//!
//! The ROM holds four tables. `gNatureNamePointers` is a pointer array to
//! `EOS`-terminated charmap streams; `gRegionMapEntries` is an array of
//! `RegionMapLocation` rows, of which only the name pointer is read;
//! `gText_Ferry`, `gText_SecretBase` and `gText_Hideout` are bare streams; and
//! `sSpeciesToNationalPokedexNum` is an array of `u16` National Dex identities.
//! This reader follows the pointers, takes each stream through its `EOS`, and
//! hands the lot to [`SummaryIdentityTable`], which validates the tables and
//! owns the canonical encoding, so the checkout extractor and this reader
//! cannot drift on the wire format.
//!
//! # Struct layout
//!
//! `include/region_map.h` declares `RegionMapLocation` as four `u8`s (`x`, `y`,
//! `width`, `height`) then `const u8 *name`. The pointer is word-aligned and
//! the struct is already a multiple of four, so rows are [`REGION_ROW_BYTES`]
//! apart and the name sits [`REGION_NAME_OFFSET`] bytes in. The profile
//! generator confirms both against the supported ROM, and the geometry is
//! never read.

use assets::pack::{
    SpecialLocationNames, SummaryIdentityTable, NATURE_COUNT, REGION_MAP_SECTION_COUNT,
    SPECIES_TO_NATIONAL_COUNT,
};
use pack_format::{raw_entry, PackWriter};

use super::battle_messages::stream;
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{Roots, SummaryIdentityRoots};

/// The pack id this domain produces.
pub const PACK_ID: &str = "summary-identity/table";
/// Bytes between `gRegionMapEntries` rows: four `u8`s and a pointer.
pub const REGION_ROW_BYTES: usize = 8;
/// Where a `gRegionMapEntries` row keeps its `name` pointer.
pub const REGION_NAME_OFFSET: usize = 4;
/// One pointer-table entry is a `const u8 *`.
const POINTER_BYTES: usize = 4;
/// One conversion-table entry is a `u16`.
const CONVERSION_BYTES: usize = 2;

/// Write the summary-identity table, if the profile records one.
///
/// A profile with no summary-identity roots writes nothing, like any other
/// domain with no roots. A profile with some is held to all of them.
///
/// # Errors
///
/// As [`decode_summary_identity_payload`].
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.summary_identity;
    if *family == SummaryIdentityRoots::NONE {
        return Ok(());
    }
    let payload = decode_summary_identity_payload(&rom.reader(), family)?;
    writer.push(raw_entry(PACK_ID.to_owned(), payload));
    Ok(())
}

/// The canonical `summary-identity/table` payload one family of roots yields.
///
/// Public so `cargo xtask gen-rom-profile` can run the importer's own reader
/// over a finished plan and compare it with the checkout pack, as it does for
/// the other text tables.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if any root is unset;
/// [`ImportError::PointerOutOfRange`] or [`ImportError::Truncated`] if a table
/// slot is not a cartridge pointer, a row runs off the image, or a stream runs
/// off it unterminated; [`ImportError::SummaryIdentity`] if the schema rejects
/// what the ROM yields.
pub fn decode_summary_identity_payload(
    reader: &RomReader<'_>,
    family: &SummaryIdentityRoots,
) -> Result<Vec<u8>, ImportError> {
    check_roots(family)?;
    let natures = (0..NATURE_COUNT)
        .map(|index| {
            let entry = row_offset(family.natures, index, POINTER_BYTES);
            Ok((
                index_u16(index, NATURE_COUNT)?,
                stream(reader, reader.ptr(entry)?)?,
            ))
        })
        .collect::<Result<Vec<_>, ImportError>>()?;

    let sections = (0..REGION_MAP_SECTION_COUNT)
        .map(|index| {
            let row = row_offset(family.region_map_entries, index, REGION_ROW_BYTES);
            let name = reader.ptr(row.saturating_add(REGION_NAME_OFFSET))?;
            Ok((
                index_u16(index, REGION_MAP_SECTION_COUNT)?,
                stream(reader, name)?,
            ))
        })
        .collect::<Result<Vec<_>, ImportError>>()?;

    let specials = SpecialLocationNames {
        ferry: stream(reader, family.ferry)?,
        secret_base: stream(reader, family.secret_base)?,
        hideout: stream(reader, family.hideout)?,
    };

    let conversions = (0..SPECIES_TO_NATIONAL_COUNT)
        .map(|index| {
            let entry = row_offset(family.species_to_national, index, CONVERSION_BYTES);
            // Table slot `index` converts species `index + 1`.
            Ok((
                index_u16(index + 1, SPECIES_TO_NATIONAL_COUNT)?,
                reader.u16_le(entry)?,
            ))
        })
        .collect::<Result<Vec<_>, ImportError>>()?;

    SummaryIdentityTable::from_entries(natures, sections, specials, conversions)
        .map(|table| table.encode())
        .map_err(|source| ImportError::SummaryIdentity {
            id: PACK_ID,
            source,
        })
}

/// All six roots must be set.
fn check_roots(family: &SummaryIdentityRoots) -> Result<(), ImportError> {
    for (root, field) in [
        (family.natures, "gNatureNamePointers"),
        (family.region_map_entries, "gRegionMapEntries"),
        (family.ferry, "gText_Ferry"),
        (family.secret_base, "gText_SecretBase"),
        (family.hideout, "gText_Hideout"),
        (family.species_to_national, "sSpeciesToNationalPokedexNum"),
    ] {
        if root == GbaPtr::AT_BASE {
            return Err(ImportError::StructMismatch {
                root: PACK_ID,
                field,
            });
        }
    }
    Ok(())
}

/// The ROM offset of row `index` of a table at `base`, saturating so an
/// address past the image reads as truncated rather than wrapping.
fn row_offset(base: GbaPtr, index: usize, stride: usize) -> usize {
    base.offset().saturating_add(index.saturating_mul(stride))
}

fn index_u16(index: usize, count: usize) -> Result<u16, ImportError> {
    u16::try_from(index).map_err(|_| ImportError::IndexOutOfRange {
        table: PACK_ID,
        index,
        count,
    })
}

#[cfg(test)]
mod tests;
