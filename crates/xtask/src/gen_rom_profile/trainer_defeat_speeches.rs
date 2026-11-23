//! Locating the trainer defeat speeches (`data/text/trainers.inc` and
//! `data/maps/Route103/scripts.inc`).
//!
//! The pack's `trainer-defeat-speech/table` entry is the expectation: one
//! `EOS`-terminated charmap token stream per `TrainerDefeatSpeechContext`. The
//! ROM stores each as a bare string. Nothing here records a stream's bytes;
//! only addresses leave this module.
//!
//! A speech is found where its stream begins a string (the byte before it is
//! an `EOS`, or it starts the image) and exactly one such place holds it. One
//! that appears in several places is refused rather than guessed at.
//!
//! The exception is a speech too short for the byte search or recurring
//! verbatim (Isabelle's seven-byte `Groan…` occurs three times, each at a
//! string start). Such a speech is found through the `trainerbattle` command
//! that names it instead: the command's fixed head occurs exactly once, and
//! its defeat-text pointer, unaligned as script operands are, selects the
//! copy. The importer's own reader then runs over the finished plan, checks
//! that witness again, and must yield the pack's payload byte for byte.

use assets::pack::TrainerDefeatSpeechError;
use assets::{TrainerDefeatSpeechContext, TrainerDefeatSpeechTable};

use super::audio::raw_payload;
use super::error::GenRomProfileError;
use super::inventory;
use super::locate::{exactly_one, slice_at_addr, to_offset, u32_at};
use super::plan::{ReportLine, Resolution, TrainerDefeatSpeechPlan, TrainerDefeatSpeechRootPlan};
use super::search::MATCH_LIMIT;
use super::Context;

const PACK_ID: &str = rom_import::TRAINER_DEFEAT_SPEECH_PACK_ID;
/// The pack-id prefix the importer's one entry lives under.
const PACK_PREFIX: &str = "trainer-defeat-speech/";
/// The end-of-string byte every stream carries.
const EOS: u8 = 0xFF;

/// Locate the trainer defeat-speech roots.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack holds anything under
/// `trainer-defeat-speech/` but the one table, a speech's `trainerbattle`
/// command is cut by the image or names other text, or the ROM's speeches
/// differ from the pack's; [`GenRomProfileError::MissingPackEntry`],
/// [`GenRomProfileError::WrongPackEntryKind`], or
/// [`GenRomProfileError::EntryShape`] for a pack entry that is absent, not
/// raw, or not a valid table; [`GenRomProfileError::NotFound`] or
/// [`GenRomProfileError::Ambiguous`] for a speech (or the command that names
/// it) the ROM does not hold exactly once.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<TrainerDefeatSpeechPlan, GenRomProfileError> {
    inventory::require_trainer_defeat_speeches(ctx.pack)?;
    let ids = ctx.pack.ids_with_prefix(PACK_PREFIX);
    if ids != [PACK_ID] {
        return Err(GenRomProfileError::StructMismatch {
            id: format!("{PACK_PREFIX}*"),
            reason: format!("only [\"{PACK_ID}\"] is supported, the pack holds {ids:?}"),
        });
    }
    let payload = raw_payload(ctx, PACK_ID)?;
    let expected =
        TrainerDefeatSpeechTable::decode(payload).map_err(|err: TrainerDefeatSpeechError| {
            GenRomProfileError::EntryShape {
                id: PACK_ID.to_owned(),
                reason: err.to_string(),
            }
        })?;

    let mut speeches = Vec::with_capacity(TrainerDefeatSpeechContext::ALL.len());
    let mut lines = Vec::with_capacity(TrainerDefeatSpeechContext::ALL.len());
    for context in TrainerDefeatSpeechContext::ALL {
        let stream = expected.speech(context);
        let (root, line) = match rom_import::trainer_defeat_speech_witness_head(context) {
            Some(head) => by_command(ctx, context, &head, stream)?,
            None => by_text(ctx, context, stream)?,
        };
        speeches.push(root);
        lines.push(line);
    }
    let plan = TrainerDefeatSpeechPlan { speeches };
    check_plan(ctx, &plan, payload)?;
    report.append(&mut lines);
    Ok(plan)
}

/// The speech's unique string start in the image.
fn by_text(
    ctx: &Context<'_>,
    context: TrainerDefeatSpeechContext,
    stream: &[u8],
) -> Result<(TrainerDefeatSpeechRootPlan, ReportLine), GenRomProfileError> {
    // The string-start test runs inside the search, before the hit cap:
    // suffix-only matches at lower offsets must not use up the cap and hide a
    // second start or the only one.
    let hits: Vec<u32> = ctx
        .raw
        .find_all_up_to_where(&[stream.to_vec()], MATCH_LIMIT, |at| {
            starts_string(ctx.rom, at)
        })
        .into_iter()
        .flatten()
        .collect();
    let addr = exactly_one(context.symbol(), &hits)?;
    Ok((
        TrainerDefeatSpeechRootPlan {
            symbol: context.symbol(),
            addr,
            script: None,
        },
        line(context, addr, stream, Resolution::UniqueSignature),
    ))
}

/// The speech the one `trainerbattle` command with `head` names.
fn by_command(
    ctx: &Context<'_>,
    context: TrainerDefeatSpeechContext,
    head: &[u8],
    stream: &[u8],
) -> Result<(TrainerDefeatSpeechRootPlan, ReportLine), GenRomProfileError> {
    let symbol = context.symbol();
    let mismatch = |reason: String| GenRomProfileError::StructMismatch {
        id: symbol.to_owned(),
        reason,
    };
    // `RawSearch` skips needles this short, so every offset is tried; the head
    // is unaligned, so `PointerIndex` cannot see the pointer behind it either.
    let commands: Vec<u32> = ctx
        .rom
        .windows(head.len())
        .enumerate()
        .filter(|(_, window)| *window == head)
        .map(|(offset, _)| u32::try_from(offset).expect("a ROM offset fits in u32"))
        .collect();
    let script = exactly_one(symbol, &commands)?;

    let defeat_offset = to_offset(script)
        .and_then(|offset| offset.checked_add(rom_import::TRAINER_BATTLE_DEFEAT_POINTER_OFFSET))
        .ok_or_else(|| mismatch(format!("the command at {script:08X} is outside the ROM")))?;
    if slice_at_addr(ctx.rom, script, rom_import::TRAINER_BATTLE_COMMAND_LEN).is_none() {
        return Err(mismatch(format!(
            "the command at {script:08X} runs off the image"
        )));
    }
    let addr = u32_at(ctx.rom, defeat_offset).expect("the command was bounds-checked");
    let offset = to_offset(addr).ok_or_else(|| {
        mismatch(format!(
            "the defeat pointer {addr:08X} is not a cartridge address"
        ))
    })?;
    if ctx.rom.get(offset..offset.saturating_add(stream.len())) != Some(stream)
        || !starts_string(
            ctx.rom,
            u32::try_from(offset).expect("a ROM offset fits in u32"),
        )
    {
        return Err(mismatch(format!(
            "the defeat pointer {addr:08X} does not address the expected text"
        )));
    }
    Ok((
        TrainerDefeatSpeechRootPlan {
            symbol,
            addr,
            script: Some(script),
        },
        line(context, addr, stream, Resolution::StructDerived).note(format!(
            "named by the trainerbattle command at {script:08X}"
        )),
    ))
}

fn line(
    context: TrainerDefeatSpeechContext,
    addr: u32,
    stream: &[u8],
    resolution: Resolution,
) -> ReportLine {
    let len = u32::try_from(stream.len()).expect("a stream fits in u32");
    // The labels are assembler-local, so a linker map cannot list them.
    ReportLine::unique(PACK_ID, addr, len)
        .with(resolution)
        .local_symbol(context.symbol())
}

/// Whether the byte before ROM offset `at` is an `EOS`, or `at` is the start.
fn starts_string(rom: &[u8], at: u32) -> bool {
    let at = usize::try_from(at).expect("a ROM offset fits in usize");
    at == 0 || rom.get(at - 1) == Some(&EOS)
}

/// Run the importer's own reader over the plan and compare with the pack.
fn check_plan(
    ctx: &Context<'_>,
    plan: &TrainerDefeatSpeechPlan,
    payload: &[u8],
) -> Result<(), GenRomProfileError> {
    let outside = |symbol: &str, addr: u32| GenRomProfileError::StructMismatch {
        id: symbol.to_owned(),
        reason: format!("{addr:08X} is outside the ROM"),
    };
    let roots: Vec<rom_import::TrainerDefeatSpeechRoot> = plan
        .speeches
        .iter()
        .map(|speech| {
            let addr = rom_import::GbaPtr::new(speech.addr)
                .ok_or_else(|| outside(speech.symbol, speech.addr))?;
            let script = speech
                .script
                .map(|script| {
                    rom_import::GbaPtr::new(script).ok_or_else(|| outside(speech.symbol, script))
                })
                .transpose()?;
            Ok(rom_import::TrainerDefeatSpeechRoot {
                symbol: speech.symbol,
                addr,
                script,
            })
        })
        .collect::<Result<_, GenRomProfileError>>()?;
    let reader = rom_import::RomReader::new(ctx.rom);
    let decoded =
        rom_import::decode_trainer_defeat_speech_payload(&reader, &roots).map_err(|err| {
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
