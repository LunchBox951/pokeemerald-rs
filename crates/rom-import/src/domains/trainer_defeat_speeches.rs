//! The trainer defeat-speech table: `trainer-defeat-speech/table`.
//!
//! The assembler compiles each `.string` label as raw GBA charmap bytes ending
//! in `EOS` (`0xFF`), with nothing around it. The profile addresses each
//! [`TrainerDefeatSpeechContext`]'s text; this reader takes each stream
//! through its `EOS` and hands the lot to [`TrainerDefeatSpeechTable`], which
//! validates the inventory and owns the canonical encoding, so the checkout
//! extractor and this reader cannot drift on the wire format.
//!
//! A text short enough to recur elsewhere in the image cannot be told from its
//! copies by content (Isabelle's `Groan…` occurs three times, each at a string
//! start). Such a context also records the address of the `trainerbattle`
//! command that names the text, and this reader requires that command to be
//! the one upstream assembles for the context and to point at the root's
//! address, so a profile whose root is a lookalike copy is refused.

use assets::pack::TrainerDefeatSpeechError;
use assets::{TrainerDefeatSpeechContext, TrainerDefeatSpeechTable, TrainerId};
use pack_format::{raw_entry, PackWriter};

use super::battle_messages::stream;
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{Roots, TrainerDefeatSpeechRoot};

/// The pack id this domain produces.
pub const PACK_ID: &str = "trainer-defeat-speech/table";

/// `SCR_OP_TRAINERBATTLE` (`data/script_cmd_table.inc`).
const SCR_OP_TRAINERBATTLE: u8 = 0x5C;
/// `TRAINER_BATTLE_SINGLE` (`include/constants/battle_setup.h`).
const TRAINER_BATTLE_SINGLE: u8 = 0;
/// A `trainerbattle` command's fixed head: opcode, type, trainer, local ID
/// (`asm/macros/event.inc`, `trainerbattle`).
pub const COMMAND_HEAD_LEN: usize = 6;
/// Where a `TRAINER_BATTLE_SINGLE` command's defeat-text pointer sits: after
/// the head and the four-byte intro-text pointer.
pub const DEFEAT_POINTER_OFFSET: usize = COMMAND_HEAD_LEN + 4;
/// A `TRAINER_BATTLE_SINGLE` command's length: the head and two text pointers.
pub const COMMAND_LEN: usize = DEFEAT_POINTER_OFFSET + 4;

/// The `trainerbattle` head naming the context's text, if the context's text
/// needs that command as its witness.
///
/// Only Isabelle's `trainerbattle_single` (`Route103_EventScript_Isabelle`,
/// local ID `LOCALID_NONE`) qualifies: her text is the one that recurs.
#[must_use]
pub fn witness_head(context: TrainerDefeatSpeechContext) -> Option<[u8; COMMAND_HEAD_LEN]> {
    let trainer = match context {
        TrainerDefeatSpeechContext::Route103Isabelle => TrainerId::ISABELLE.0.to_le_bytes(),
        _ => return None,
    };
    Some([
        SCR_OP_TRAINERBATTLE,
        TRAINER_BATTLE_SINGLE,
        trainer[0],
        trainer[1],
        0,
        0,
    ])
}

/// Write the trainer defeat-speech table, if the profile records one.
///
/// A profile with no defeat-speech roots writes nothing, like any other domain
/// with no roots. A profile with some is held to all of them.
///
/// # Errors
///
/// As [`decode_trainer_defeat_speech_payload`].
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    if roots.trainer_defeat_speeches.is_empty() {
        return Ok(());
    }
    let payload =
        decode_trainer_defeat_speech_payload(&rom.reader(), roots.trainer_defeat_speeches)?;
    writer.push(raw_entry(PACK_ID.to_owned(), payload));
    Ok(())
}

/// The canonical `trainer-defeat-speech/table` payload one set of roots yields.
///
/// Public so `cargo xtask gen-rom-profile` can run the importer's own reader
/// over a finished plan and compare it with the checkout pack, as it does for
/// the field-message table.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if the roots are not exactly the
/// inventory's, in order, one is unset, or a witness command is missing,
/// unexpected, not the one upstream assembles, or points elsewhere than the
/// root; [`ImportError::Truncated`] or [`ImportError::PointerOutOfRange`] if a
/// read runs off the image; [`ImportError::TrainerDefeatSpeech`] if the schema
/// rejects the streams the ROM yields.
pub fn decode_trainer_defeat_speech_payload(
    reader: &RomReader<'_>,
    roots: &[TrainerDefeatSpeechRoot],
) -> Result<Vec<u8>, ImportError> {
    check_roots(roots)?;
    let mut streams = Vec::with_capacity(roots.len());
    for (context, root) in TrainerDefeatSpeechContext::ALL.into_iter().zip(roots) {
        check_witness(reader, context, root)?;
        streams.push((context.id(), stream(reader, root.addr)?));
    }
    TrainerDefeatSpeechTable::from_entries(streams)
        .map(|table| table.encode())
        .map_err(
            |source: TrainerDefeatSpeechError| ImportError::TrainerDefeatSpeech {
                id: PACK_ID,
                source,
            },
        )
}

/// Require exactly the inventory's roots, in order, each with an address.
fn check_roots(roots: &[TrainerDefeatSpeechRoot]) -> Result<(), ImportError> {
    let mismatch = |field| ImportError::StructMismatch {
        root: PACK_ID,
        field,
    };
    if roots.len() != TrainerDefeatSpeechContext::ALL.len() {
        return Err(mismatch("trainer defeat speech roots"));
    }
    for (root, context) in roots.iter().zip(TrainerDefeatSpeechContext::ALL) {
        if root.symbol != context.symbol() {
            return Err(mismatch("trainer defeat speech symbol"));
        }
        if root.addr == GbaPtr::AT_BASE {
            return Err(mismatch("trainer defeat speech address"));
        }
    }
    Ok(())
}

/// Hold a context that needs a witness to it, and every other context to none.
fn check_witness(
    reader: &RomReader<'_>,
    context: TrainerDefeatSpeechContext,
    root: &TrainerDefeatSpeechRoot,
) -> Result<(), ImportError> {
    let mismatch = |field| ImportError::StructMismatch {
        root: PACK_ID,
        field,
    };
    let Some(head) = witness_head(context) else {
        return match root.script {
            None => Ok(()),
            Some(_) => Err(mismatch("trainer defeat speech script")),
        };
    };
    let script = root
        .script
        .filter(|script| *script != GbaPtr::AT_BASE)
        .ok_or_else(|| mismatch("trainer defeat speech script"))?;
    let command = reader.slice(script, COMMAND_LEN)?;
    if command[..COMMAND_HEAD_LEN] != head {
        return Err(mismatch("trainer defeat speech command"));
    }
    if reader.ptr(script.offset() + DEFEAT_POINTER_OFFSET)? != root.addr {
        return Err(mismatch("trainer defeat speech text pointer"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
