//! Locating the sight-trainer intro speeches (`data/text/trainers.inc` and
//! `data/maps/Route103/scripts.inc`).
//!
//! The pack's `sight-trainer-intro-speech/table` entry is the expectation: one
//! `EOS`-terminated charmap token stream per sight-trainer script. The ROM
//! stores each as a bare string. Nothing here records a stream's bytes; only
//! addresses leave this module.
//!
//! Text alone cannot place a speech: Amy's and Liv's rematch commands name
//! copies of their intros, and both share one trainer. A speech is found
//! through the script's own `trainerbattle` command instead. The command's
//! fixed head (opcode, battle type, trainer, local ID) is unaligned, as script
//! operands are, so it is searched byte by byte; among the commands with that
//! head, the one whose intro pointer addresses the expected stream at a string
//! start is the script's, and exactly one may. The importer's own reader then
//! runs over the finished plan, checks that witness again, and must yield the
//! pack's payload byte for byte.

use assets::pack::SightTrainerIntroSpeechError;
use assets::{SightTrainerIntroSpeechTable, SIGHT_TRAINER_INTRO_SCRIPTS};

use super::audio::raw_payload;
use super::error::GenRomProfileError;
use super::inventory;
use super::locate::{exactly_one, to_offset, u32_at};
use super::plan::{
    ReportLine, Resolution, SightTrainerIntroSpeechPlan, SightTrainerIntroSpeechRootPlan,
};
use super::Context;

const PACK_ID: &str = rom_import::SIGHT_TRAINER_INTRO_SPEECH_PACK_ID;
/// The pack-id prefix the importer's one entry lives under.
const PACK_PREFIX: &str = "sight-trainer-intro-speech/";
/// The end-of-string byte every stream carries.
const EOS: u8 = 0xFF;

/// Locate the sight-trainer intro-speech roots.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack holds anything under
/// `sight-trainer-intro-speech/` but the one table, or the ROM's speeches
/// differ from the pack's; [`GenRomProfileError::MissingPackEntry`],
/// [`GenRomProfileError::WrongPackEntryKind`], or
/// [`GenRomProfileError::EntryShape`] for a pack entry that is absent, not
/// raw, or not a valid table; [`GenRomProfileError::NotFound`] or
/// [`GenRomProfileError::Ambiguous`] for a script whose command naming the
/// expected text the ROM does not hold exactly once.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<SightTrainerIntroSpeechPlan, GenRomProfileError> {
    inventory::require_sight_trainer_intro_speeches(ctx.pack)?;
    let ids = ctx.pack.ids_with_prefix(PACK_PREFIX);
    if ids != [PACK_ID] {
        return Err(GenRomProfileError::StructMismatch {
            id: format!("{PACK_PREFIX}*"),
            reason: format!("only [\"{PACK_ID}\"] is supported, the pack holds {ids:?}"),
        });
    }
    let payload = raw_payload(ctx, PACK_ID)?;
    let expected = SightTrainerIntroSpeechTable::decode(payload).map_err(
        |err: SightTrainerIntroSpeechError| GenRomProfileError::EntryShape {
            id: PACK_ID.to_owned(),
            reason: err.to_string(),
        },
    )?;

    let mut speeches = Vec::with_capacity(SIGHT_TRAINER_INTRO_SCRIPTS.len());
    let mut lines = Vec::with_capacity(SIGHT_TRAINER_INTRO_SCRIPTS.len());
    for script in SIGHT_TRAINER_INTRO_SCRIPTS {
        let stream = expected
            .speech(script)
            .expect("a decoded table holds every inventory script");
        let (root, line) = by_command(ctx, script, stream)?;
        speeches.push(root);
        lines.push(line);
    }
    let plan = SightTrainerIntroSpeechPlan { speeches };
    check_plan(ctx, &plan, payload)?;
    report.append(&mut lines);
    Ok(plan)
}

/// The speech the one `trainerbattle` command of `script`'s shape names.
fn by_command(
    ctx: &Context<'_>,
    script: &'static str,
    stream: &[u8],
) -> Result<(SightTrainerIntroSpeechRootPlan, ReportLine), GenRomProfileError> {
    let head = rom_import::sight_trainer_intro_witness_head(script).ok_or_else(|| {
        GenRomProfileError::StructMismatch {
            id: script.to_owned(),
            reason: "the importer knows no witness command for this script".to_owned(),
        }
    })?;
    // `RawSearch` skips needles this short and the head is unaligned, so
    // `PointerIndex` cannot see the pointer behind it either.
    let commands: Vec<u32> = ctx
        .rom
        .windows(head.len())
        .enumerate()
        .filter(|(_, window)| *window == head)
        .map(|(offset, _)| u32::try_from(offset).expect("a ROM offset fits in u32"))
        .filter(|offset| names_stream(ctx.rom, *offset, stream))
        .collect();
    let command = exactly_one(script, &commands)?;
    let offset = to_offset(command).expect("the command was found inside the ROM");
    let addr = u32_at(
        ctx.rom,
        offset + rom_import::SIGHT_TRAINER_INTRO_POINTER_OFFSET,
    )
    .expect("the command was bounds-checked");
    let len = u32::try_from(stream.len()).expect("a stream fits in u32");
    Ok((
        SightTrainerIntroSpeechRootPlan {
            symbol: script,
            addr,
            script: command,
        },
        // The scripts are assembler-local, so a linker map cannot list them.
        ReportLine::unique(PACK_ID, addr, len)
            .with(Resolution::StructDerived)
            .local_symbol(script)
            .note(format!(
                "named by the trainerbattle command at {command:08X}"
            )),
    ))
}

/// Whether the command at ROM offset `command` points at `stream`, at a
/// string start.
fn names_stream(rom: &[u8], command: u32, stream: &[u8]) -> bool {
    let Some(pointer) = usize::try_from(command)
        .ok()
        .and_then(|command| command.checked_add(rom_import::SIGHT_TRAINER_INTRO_POINTER_OFFSET))
        .and_then(|at| u32_at(rom, at))
    else {
        return false;
    };
    let Some(offset) = to_offset(pointer) else {
        return false;
    };
    rom.get(offset..offset.saturating_add(stream.len())) == Some(stream)
        && (offset == 0 || rom.get(offset - 1) == Some(&EOS))
}

/// Run the importer's own reader over the plan and compare with the pack.
fn check_plan(
    ctx: &Context<'_>,
    plan: &SightTrainerIntroSpeechPlan,
    payload: &[u8],
) -> Result<(), GenRomProfileError> {
    let outside = |symbol: &str, addr: u32| GenRomProfileError::StructMismatch {
        id: symbol.to_owned(),
        reason: format!("{addr:08X} is outside the ROM"),
    };
    let roots: Vec<rom_import::SightTrainerIntroSpeechRoot> = plan
        .speeches
        .iter()
        .map(|speech| {
            Ok(rom_import::SightTrainerIntroSpeechRoot {
                symbol: speech.symbol,
                addr: rom_import::GbaPtr::new(speech.addr)
                    .ok_or_else(|| outside(speech.symbol, speech.addr))?,
                script: rom_import::GbaPtr::new(speech.script)
                    .ok_or_else(|| outside(speech.symbol, speech.script))?,
            })
        })
        .collect::<Result<_, GenRomProfileError>>()?;
    let reader = rom_import::RomReader::new(ctx.rom);
    let decoded =
        rom_import::decode_sight_trainer_intro_speech_payload(&reader, &roots).map_err(|err| {
            GenRomProfileError::StructMismatch {
                id: PACK_ID.to_owned(),
                reason: format!("the ROM's speeches do not decode: {err}"),
            }
        })?;
    if decoded != payload {
        return Err(GenRomProfileError::StructMismatch {
            id: PACK_ID.to_owned(),
            reason: "the ROM's decoded speeches differ from the pack entry".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
