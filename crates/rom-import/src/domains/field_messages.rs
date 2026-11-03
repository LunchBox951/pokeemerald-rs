//! The field-message token table: `field-message/table`.
//!
//! `src/strings.c` compiles each string as raw GBA charmap bytes ending in
//! `EOS` (`0xFF`). The ROM holds the [`FieldMessage`] inventory as bare
//! strings the profile addresses one by one. This reader takes each stream
//! through its `EOS` and hands the lot to [`FieldMessageTable`], which
//! validates the inventory and owns the canonical encoding, so the checkout
//! extractor and this reader cannot drift on the wire format.
//!
//! A stream's end is its first `0xFF`. Control sequences whose arguments
//! could hold that byte are rejected by [`FieldMessageTable`]'s own token
//! walk instead of being skipped here, so a cut inside one is a typed
//! [`ImportError::FieldMessage`], never a short string.

use assets::pack::{FieldMessage, FieldMessageTable};
use pack_format::{raw_entry, PackWriter};

use super::battle_messages::stream;
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{FieldMessageRoot, Roots};

/// The pack id this domain produces.
pub const PACK_ID: &str = "field-message/table";

/// Write the field-message table, if the profile records one.
///
/// A profile with no field-message roots writes nothing, like any other
/// domain with no roots. A profile with some is held to all of them.
///
/// # Errors
///
/// As [`decode_field_message_payload`].
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    if roots.field_messages.is_empty() {
        return Ok(());
    }
    let payload = decode_field_message_payload(&rom.reader(), roots.field_messages)?;
    writer.push(raw_entry(PACK_ID.to_owned(), payload));
    Ok(())
}

/// The canonical `field-message/table` payload one set of roots yields.
///
/// Public so `cargo xtask gen-rom-profile` can run the importer's own reader
/// over a finished plan and compare it with the checkout pack, as it does for
/// the battle-message table.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if the roots are not exactly the
/// inventory's, in order, or one is unset; [`ImportError::Truncated`] if a
/// stream runs off the image unterminated; [`ImportError::FieldMessage`] if
/// the schema rejects the streams the ROM yields.
pub fn decode_field_message_payload(
    reader: &RomReader<'_>,
    roots: &[FieldMessageRoot],
) -> Result<Vec<u8>, ImportError> {
    check_roots(roots)?;
    let mut streams = Vec::with_capacity(roots.len());
    for (message, root) in FieldMessage::ALL.into_iter().zip(roots) {
        streams.push((message.id(), stream(reader, root.addr)?));
    }
    FieldMessageTable::from_entries(streams)
        .map(|table| table.encode())
        .map_err(|source| ImportError::FieldMessage {
            id: PACK_ID,
            source,
        })
}

/// Require exactly the inventory's roots, in order, each with an address.
fn check_roots(roots: &[FieldMessageRoot]) -> Result<(), ImportError> {
    let mismatch = |field| ImportError::StructMismatch {
        root: PACK_ID,
        field,
    };
    if roots.len() != FieldMessage::ALL.len() {
        return Err(mismatch("field message roots"));
    }
    for (root, message) in roots.iter().zip(FieldMessage::ALL) {
        if root.symbol != message.symbol() {
            return Err(mismatch("field message symbol"));
        }
        if root.addr == GbaPtr::AT_BASE {
            return Err(mismatch("field message address"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
