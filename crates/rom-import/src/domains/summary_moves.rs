//! The summary screen's move and contest records: `summary-move/table`.
//!
//! The ROM holds four arrays. `gMoveDescriptionPointers` and
//! `gContestEffectDescriptionPointers` are pointer arrays to `EOS`-terminated
//! charmap streams; `gContestMoves` and `gContestEffects` are arrays of small
//! structs. This reader follows the pointers, takes each stream through its
//! `EOS`, projects the struct rows onto the fields the schema carries, and
//! hands the lot to [`SummaryMoveTable`], which validates the tables and owns
//! the canonical encoding, so the checkout extractor and this reader cannot
//! drift on the wire format.
//!
//! # Struct layout
//!
//! `include/contest_effect.h` declares `ContestMove` as seven bytes (`effect`,
//! `contestCategory:3`, `comboStarterId`, `comboMoves[4]`) and `ContestEffect`
//! as three. The retail compiler (agbcc, `STRUCTURE_SIZE_BOUNDARY 32`) rounds
//! a struct's size up to a multiple of four bytes, so the arrays step by
//! [`CONTEST_MOVE_BYTES`] and [`CONTEST_EFFECT_BYTES`]: eight and four. The
//! profile generator confirms both against the supported ROM, and this reader
//! never serializes the padding or the combo fields.
//!
//! `contestCategory` is the low three bits of byte 1 (the first bit-field of
//! its storage unit on a little-endian target); the unit's upper bits are
//! padding and are not read.

use assets::pack::{
    ContestEffectRecord, ContestMoveRecord, SummaryMoveTable, CONTEST_EFFECT_COUNT,
    CONTEST_MOVE_COUNT, MOVE_DESCRIPTION_COUNT,
};
use pack_format::{raw_entry, PackWriter};

use super::battle_messages::stream;
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{Roots, SummaryMoveRoots};

/// The pack id this domain produces.
pub const PACK_ID: &str = "summary-move/table";
/// Bytes between `gContestMoves` rows: seven declared, rounded up to the word.
pub const CONTEST_MOVE_BYTES: usize = 8;
/// Bytes between `gContestEffects` rows: three declared, rounded up to the word.
pub const CONTEST_EFFECT_BYTES: usize = 4;
/// One description-table entry is a `const u8 *`.
const POINTER_BYTES: usize = 4;
/// `contestCategory:3`.
const CATEGORY_MASK: u8 = 0b111;

/// Write the summary-move table, if the profile records one.
///
/// A profile with no summary-move roots writes nothing, like any other domain
/// with no roots. A profile with some is held to all of them.
///
/// # Errors
///
/// As [`decode_summary_move_payload`].
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.summary_moves;
    if *family == SummaryMoveRoots::NONE {
        return Ok(());
    }
    let payload = decode_summary_move_payload(&rom.reader(), family)?;
    writer.push(raw_entry(PACK_ID.to_owned(), payload));
    Ok(())
}

/// The canonical `summary-move/table` payload one family of roots yields.
///
/// Public so `cargo xtask gen-rom-profile` can run the importer's own reader
/// over a finished plan and compare it with the checkout pack, as it does for
/// the battle-message table.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if any root is unset;
/// [`ImportError::PointerOutOfRange`] or [`ImportError::Truncated`] if a table
/// slot is not a cartridge pointer, a row runs off the image, or a stream runs
/// off it unterminated; [`ImportError::SummaryMove`] if the schema rejects what
/// the ROM yields.
pub fn decode_summary_move_payload(
    reader: &RomReader<'_>,
    family: &SummaryMoveRoots,
) -> Result<Vec<u8>, ImportError> {
    check_roots(family)?;
    let descriptions = pointed_streams(reader, family.descriptions, MOVE_DESCRIPTION_COUNT)?;
    let texts = pointed_streams(reader, family.effect_descriptions, CONTEST_EFFECT_COUNT)?;

    let mut moves = Vec::with_capacity(CONTEST_MOVE_COUNT);
    for index in 0..CONTEST_MOVE_COUNT {
        let row = reader.table_entry(
            "gContestMoves",
            family.contest_moves,
            index,
            CONTEST_MOVE_COUNT,
            CONTEST_MOVE_BYTES,
        )?;
        moves.push(ContestMoveRecord {
            move_id: index_u16(index, CONTEST_MOVE_COUNT)?,
            effect: row[0],
            category: row[1] & CATEGORY_MASK,
        });
    }

    let mut effects = Vec::with_capacity(CONTEST_EFFECT_COUNT);
    for (index, description) in texts.iter().enumerate() {
        let row = reader.table_entry(
            "gContestEffects",
            family.contest_effects,
            index,
            CONTEST_EFFECT_COUNT,
            CONTEST_EFFECT_BYTES,
        )?;
        effects.push(ContestEffectRecord {
            id: index_u16(index, CONTEST_EFFECT_COUNT)?,
            effect_type: row[0],
            appeal: row[1],
            jam: row[2],
            description,
        });
    }

    let descriptions = descriptions
        .iter()
        .enumerate()
        .map(|(slot, bytes)| Ok((index_u16(slot, MOVE_DESCRIPTION_COUNT)?, *bytes)))
        .collect::<Result<Vec<_>, ImportError>>()?;
    SummaryMoveTable::from_entries(descriptions, moves, effects)
        .map(|table| table.encode())
        .map_err(|source| ImportError::SummaryMove {
            id: PACK_ID,
            source,
        })
}

/// All four roots must be set.
fn check_roots(family: &SummaryMoveRoots) -> Result<(), ImportError> {
    for (root, field) in [
        (family.descriptions, "gMoveDescriptionPointers"),
        (family.contest_moves, "gContestMoves"),
        (family.contest_effects, "gContestEffects"),
        (
            family.effect_descriptions,
            "gContestEffectDescriptionPointers",
        ),
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

/// The streams `count` consecutive pointers from `base` name.
fn pointed_streams<'a>(
    reader: &RomReader<'a>,
    base: GbaPtr,
    count: usize,
) -> Result<Vec<&'a [u8]>, ImportError> {
    (0..count)
        .map(|index| {
            let entry = base
                .offset()
                .saturating_add(index.saturating_mul(POINTER_BYTES));
            stream(reader, reader.ptr(entry)?)
        })
        .collect()
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
