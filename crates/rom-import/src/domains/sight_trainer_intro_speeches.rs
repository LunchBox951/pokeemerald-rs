//! The sight-trainer intro-speech table: `sight-trainer-intro-speech/table`.
//!
//! The assembler compiles each `.string` label as raw GBA charmap bytes ending
//! in `EOS` (`0xFF`), with nothing around it. Each of Route 103's nine
//! sight-trainer scripts starts with a `trainerbattle` command whose intro-text
//! pointer addresses its speech. The profile records, per script, the text and
//! the command, and this reader requires that command to be the one upstream
//! assembles for the script and to point at the root's address. Amy and Liv
//! share one trainer ID, and a speech can recur verbatim in a rematch command,
//! so the text alone does not say which script it belongs to; the command
//! does. [`SightTrainerIntroSpeechTable`] validates the inventory and owns the
//! canonical encoding, so the checkout extractor and this reader cannot drift
//! on the wire format.

use assets::pack::SightTrainerIntroSpeechError;
use assets::{
    SightTrainerIntroSpeechTable, TrainerId, SIGHT_TRAINER_INTRO_COUNT, SIGHT_TRAINER_INTRO_SCRIPTS,
};
use pack_format::{raw_entry, PackWriter};

use super::battle_messages::stream;
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{Roots, SightTrainerIntroSpeechRoot};

/// The pack id this domain produces.
pub const PACK_ID: &str = "sight-trainer-intro-speech/table";

/// `SCR_OP_TRAINERBATTLE` (`data/script_cmd_table.inc`).
const SCR_OP_TRAINERBATTLE: u8 = 0x5C;
/// `TRAINER_BATTLE_SINGLE`, `TRAINER_BATTLE_CONTINUE_SCRIPT` and
/// `TRAINER_BATTLE_CONTINUE_SCRIPT_DOUBLE` (`include/constants/battle_setup.h`).
const TRAINER_BATTLE_SINGLE: u8 = 0;
const TRAINER_BATTLE_CONTINUE_SCRIPT: u8 = 2;
const TRAINER_BATTLE_CONTINUE_SCRIPT_DOUBLE: u8 = 6;
/// A `trainerbattle` command's fixed head: opcode, type, trainer, local ID
/// (`asm/macros/event.inc`, `trainerbattle`).
pub const COMMAND_HEAD_LEN: usize = 6;
/// Where every supported command type keeps its intro-text pointer: right
/// after the head.
pub const INTRO_POINTER_OFFSET: usize = COMMAND_HEAD_LEN;
/// The shortest span that holds the head and the intro pointer.
pub const COMMAND_LEN: usize = INTRO_POINTER_OFFSET + 4;

/// The `trainerbattle` head upstream assembles for `script`, or `None` for a
/// script outside the inventory.
///
/// The trainer is corroborating identity, not the key: Amy and Liv share one.
/// Each script's macro and optional callback fix the battle type
/// (`Route103_EventScript_*`, `data/maps/Route103/scripts.inc`), and every
/// command's local ID is `LOCALID_NONE`.
#[must_use]
pub fn witness_head(script: &str) -> Option<[u8; COMMAND_HEAD_LEN]> {
    let (kind, trainer) = match script {
        "Route103_EventScript_Amy" | "Route103_EventScript_Liv" => (
            TRAINER_BATTLE_CONTINUE_SCRIPT_DOUBLE,
            TrainerId::AMY_AND_LIV_1,
        ),
        "Route103_EventScript_Miguel" => (TRAINER_BATTLE_CONTINUE_SCRIPT, TrainerId::MIGUEL_1),
        "Route103_EventScript_Daisy" => (TRAINER_BATTLE_SINGLE, TrainerId::DAISY),
        "Route103_EventScript_Andrew" => (TRAINER_BATTLE_SINGLE, TrainerId::ANDREW),
        "Route103_EventScript_Marcos" => (TRAINER_BATTLE_SINGLE, TrainerId::MARCOS),
        "Route103_EventScript_Rhett" => (TRAINER_BATTLE_SINGLE, TrainerId::RHETT),
        "Route103_EventScript_Pete" => (TRAINER_BATTLE_SINGLE, TrainerId::PETE),
        "Route103_EventScript_Isabelle" => (TRAINER_BATTLE_SINGLE, TrainerId::ISABELLE),
        _ => return None,
    };
    let trainer = trainer.0.to_le_bytes();
    Some([SCR_OP_TRAINERBATTLE, kind, trainer[0], trainer[1], 0, 0])
}

/// Write the sight-trainer intro-speech table, if the profile records one.
///
/// A profile with no intro-speech roots writes nothing, like any other domain
/// with no roots. A profile with some is held to all of them.
///
/// # Errors
///
/// As [`decode_sight_trainer_intro_speech_payload`].
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    if roots.sight_trainer_intro_speeches.is_empty() {
        return Ok(());
    }
    let payload = decode_sight_trainer_intro_speech_payload(
        &rom.reader(),
        roots.sight_trainer_intro_speeches,
    )?;
    writer.push(raw_entry(PACK_ID.to_owned(), payload));
    Ok(())
}

/// The canonical `sight-trainer-intro-speech/table` payload one set of roots
/// yields.
///
/// Public so `cargo xtask gen-rom-profile` can run the importer's own reader
/// over a finished plan and compare it with the checkout pack.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if the roots are not exactly the
/// inventory's, in order, one is unset, or a witness command is not the one
/// upstream assembles for the script or points elsewhere than the root;
/// [`ImportError::Truncated`] or [`ImportError::PointerOutOfRange`] if a read
/// runs off the image; [`ImportError::SightTrainerIntroSpeech`] if the schema
/// rejects the streams the ROM yields.
pub fn decode_sight_trainer_intro_speech_payload(
    reader: &RomReader<'_>,
    roots: &[SightTrainerIntroSpeechRoot],
) -> Result<Vec<u8>, ImportError> {
    check_roots(roots)?;
    let mut streams = Vec::with_capacity(roots.len());
    for root in roots {
        check_witness(reader, root)?;
        streams.push((root.symbol, stream(reader, root.addr)?));
    }
    SightTrainerIntroSpeechTable::from_entries(streams)
        .map(|table| table.encode())
        .map_err(
            |source: SightTrainerIntroSpeechError| ImportError::SightTrainerIntroSpeech {
                id: PACK_ID,
                source,
            },
        )
}

/// Require exactly the inventory's roots, in order, each with both addresses.
fn check_roots(roots: &[SightTrainerIntroSpeechRoot]) -> Result<(), ImportError> {
    let mismatch = |field| ImportError::StructMismatch {
        root: PACK_ID,
        field,
    };
    if roots.len() != SIGHT_TRAINER_INTRO_COUNT {
        return Err(mismatch("sight trainer intro speech roots"));
    }
    for (root, script) in roots.iter().zip(SIGHT_TRAINER_INTRO_SCRIPTS) {
        if root.symbol != script {
            return Err(mismatch("sight trainer intro speech symbol"));
        }
        if root.addr == GbaPtr::AT_BASE {
            return Err(mismatch("sight trainer intro speech address"));
        }
        if root.script == GbaPtr::AT_BASE {
            return Err(mismatch("sight trainer intro speech script"));
        }
    }
    Ok(())
}

/// Hold the root's command to the one upstream assembles for its script, and
/// to the root's text.
fn check_witness(
    reader: &RomReader<'_>,
    root: &SightTrainerIntroSpeechRoot,
) -> Result<(), ImportError> {
    let mismatch = |field| ImportError::StructMismatch {
        root: PACK_ID,
        field,
    };
    let head =
        witness_head(root.symbol).ok_or_else(|| mismatch("sight trainer intro speech symbol"))?;
    let command = reader.slice(root.script, COMMAND_LEN)?;
    if command[..COMMAND_HEAD_LEN] != head {
        return Err(mismatch("sight trainer intro speech command"));
    }
    if reader.ptr(root.script.offset() + INTRO_POINTER_OFFSET)? != root.addr {
        return Err(mismatch("sight trainer intro speech text pointer"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
