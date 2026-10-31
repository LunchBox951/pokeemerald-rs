//! The battle-message token table: `battle-message/table`.
//!
//! `src/battle_message.c` compiles every string as raw GBA charmap bytes
//! ending in `EOS` (`0xFF`). The ROM holds the 369-slot `gBattleStringsTable`
//! as a pointer array keyed by battle-string ID, and the special templates
//! `BufferStringBattle` selects from as bare strings the profile addresses one
//! by one. This reader follows those addresses, takes each stream through its
//! `EOS`, and hands the lot to [`BattleMessageTable`], which validates the
//! inventory and owns the canonical encoding, so the checkout extractor and
//! this reader cannot drift on the wire format.
//!
//! A stream's end is its first `0xFF`. Control sequences whose arguments
//! could hold that byte are rejected by [`BattleMessageTable`]'s own token
//! walk instead of being skipped here, so a cut inside one is a typed
//! [`ImportError::BattleMessage`], never a short string.

use assets::pack::{
    BattleMessageTable, BATTLE_MESSAGE_SLOT_COUNT, BATTLE_MESSAGE_SPECIAL_TEMPLATES,
    BATTLE_MESSAGE_TABLE_START,
};
use pack_format::{raw_entry, PackWriter};

use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{BattleMessageRoots, Roots};

/// The pack id this domain produces.
pub const PACK_ID: &str = "battle-message/table";
/// The end-of-string byte every stream carries.
const EOS: u8 = 0xFF;
/// One `gBattleStringsTable` entry is a `const u8 *`.
const POINTER_BYTES: usize = 4;

/// Write the battle-message table, if the profile records one.
///
/// A profile with no battle-message roots writes nothing, like any other
/// domain with no roots. A profile with some is held to all of them.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if the table root is unset, or the special
/// roots are not the inventory's, in order;
/// [`ImportError::PointerOutOfRange`] or [`ImportError::Truncated`] if a table
/// slot is not a cartridge pointer, or a stream runs off the image
/// unterminated; [`ImportError::BattleMessage`] if the schema rejects the
/// streams the ROM yields.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.battle_messages;
    if *family == BattleMessageRoots::NONE {
        return Ok(());
    }
    let payload = decode_battle_message_payload(&rom.reader(), family)?;
    writer.push(raw_entry(PACK_ID.to_owned(), payload));
    Ok(())
}

/// The canonical `battle-message/table` payload one family of roots yields.
///
/// Public so `cargo xtask gen-rom-profile` can run the importer's own reader
/// over a finished plan and compare it with the checkout pack, as it does for
/// voicegroups.
///
/// # Errors
///
/// As [`write`], and [`ImportError::StructMismatch`] for an empty family.
pub fn decode_battle_message_payload(
    reader: &RomReader<'_>,
    family: &BattleMessageRoots,
) -> Result<Vec<u8>, ImportError> {
    check_roots(family)?;
    let mut slots = Vec::with_capacity(BATTLE_MESSAGE_SLOT_COUNT);
    for index in 0..BATTLE_MESSAGE_SLOT_COUNT {
        let entry = family
            .table
            .offset()
            .saturating_add(index.saturating_mul(POINTER_BYTES));
        let id = u16::try_from(index)
            .ok()
            .and_then(|offset| BATTLE_MESSAGE_TABLE_START.checked_add(offset))
            .ok_or(ImportError::IndexOutOfRange {
                table: PACK_ID,
                index,
                count: BATTLE_MESSAGE_SLOT_COUNT,
            })?;
        slots.push((id, stream(reader, reader.ptr(entry)?)?));
    }
    let mut special = Vec::with_capacity(family.specials.len());
    for (index, root) in family.specials.iter().enumerate() {
        let index = u16::try_from(index).map_err(|_| ImportError::IndexOutOfRange {
            table: PACK_ID,
            index,
            count: BATTLE_MESSAGE_SPECIAL_TEMPLATES.len(),
        })?;
        special.push((index, stream(reader, root.addr)?));
    }
    BattleMessageTable::from_entries(slots, special)
        .map(|table| table.encode())
        .map_err(|source| ImportError::BattleMessage {
            id: PACK_ID,
            source,
        })
}

/// Require a table root and exactly the inventory's special roots, in order.
fn check_roots(family: &BattleMessageRoots) -> Result<(), ImportError> {
    let mismatch = |field| ImportError::StructMismatch {
        root: PACK_ID,
        field,
    };
    if family.table == GbaPtr::AT_BASE {
        return Err(mismatch("gBattleStringsTable"));
    }
    if family.specials.len() != BATTLE_MESSAGE_SPECIAL_TEMPLATES.len() {
        return Err(mismatch("special roots"));
    }
    for (root, template) in family.specials.iter().zip(BATTLE_MESSAGE_SPECIAL_TEMPLATES) {
        if root.symbol != template.symbol {
            return Err(mismatch("special symbol"));
        }
    }
    Ok(())
}

/// The bytes from `ptr` through the first `EOS`.
///
/// # Errors
///
/// [`ImportError::Truncated`] if the image ends before an `EOS`.
pub(super) fn stream<'a>(reader: &RomReader<'a>, ptr: GbaPtr) -> Result<&'a [u8], ImportError> {
    let start = ptr.offset();
    let tail = reader.tail(start)?;
    let end = tail
        .iter()
        .position(|&byte| byte == EOS)
        .ok_or(ImportError::Truncated {
            at: start,
            len: tail.len().saturating_add(1),
        })?;
    Ok(&tail[..=end])
}

#[cfg(test)]
mod tests;
