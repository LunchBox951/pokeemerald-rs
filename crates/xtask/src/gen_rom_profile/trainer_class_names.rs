//! Locating the trainer class display names (`gTrainerClassNames`,
//! `src/data/text/trainer_class_names.h`).
//!
//! The pack's `trainer-class-name/table` entry is the expectation: one
//! `EOS`-terminated charmap token stream per `TrainerClass`. The ROM stores
//! them as one array of 66 fixed-width 13-byte rows, each name followed by zero
//! padding. The locator rebuilds that whole 858-byte array from the pack and
//! searches the image for it, so only the array as a whole is a signature:
//! individual names are too short for the byte search, and duplicates are
//! legitimate (`COOLTRAINER`, `TUBER`, and the four `{PKMN} TRAINER` classes).
//! Exactly one copy must exist. The importer's own reader then runs over the
//! located address and must yield the pack's payload byte for byte. Nothing
//! here records a name's bytes; only the address leaves this module.

use assets::pack::TrainerClassNameError;
use assets::TrainerClassNameTable;

use super::audio::raw_payload;
use super::error::GenRomProfileError;
use super::inventory;
use super::locate::exactly_one;
use super::plan::{ReportLine, TrainerClassNamePlan};
use super::search::MATCH_LIMIT;
use super::Context;

const PACK_ID: &str = rom_import::TRAINER_CLASS_NAME_PACK_ID;
/// The pack-id prefix the importer's one entry lives under.
const PACK_PREFIX: &str = "trainer-class-name/";
/// The upstream symbol of the array; global, so a linker map lists it.
const SYMBOL: &str = "gTrainerClassNames";

/// Locate the trainer class-name table.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack holds anything under
/// `trainer-class-name/` but the one table, or the ROM's names differ from the
/// pack's; [`GenRomProfileError::MissingPackEntry`],
/// [`GenRomProfileError::WrongPackEntryKind`], or
/// [`GenRomProfileError::EntryShape`] for a pack entry that is absent, not
/// raw, or not a valid table; [`GenRomProfileError::NotFound`] or
/// [`GenRomProfileError::Ambiguous`] for an array the ROM does not hold
/// exactly once.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<TrainerClassNamePlan, GenRomProfileError> {
    inventory::require_trainer_class_names(ctx.pack)?;
    let ids = ctx.pack.ids_with_prefix(PACK_PREFIX);
    if ids != [PACK_ID] {
        return Err(GenRomProfileError::StructMismatch {
            id: format!("{PACK_PREFIX}*"),
            reason: format!("only [\"{PACK_ID}\"] is supported, the pack holds {ids:?}"),
        });
    }
    let payload = raw_payload(ctx, PACK_ID)?;
    let expected =
        TrainerClassNameTable::decode(payload).map_err(|err: TrainerClassNameError| {
            GenRomProfileError::EntryShape {
                id: PACK_ID.to_owned(),
                reason: err.to_string(),
            }
        })?;

    let rows = expected.to_fixed_rows();
    let hits: Vec<u32> = ctx
        .raw
        .find_all(std::slice::from_ref(&rows))
        .into_iter()
        .flatten()
        .collect();
    debug_assert!(hits.len() <= MATCH_LIMIT);
    let addr = exactly_one(SYMBOL, &hits)?;
    let plan = TrainerClassNamePlan { addr };
    check_plan(ctx, &plan, payload)?;
    report.push(
        ReportLine::unique(
            PACK_ID,
            addr,
            u32::try_from(rows.len()).expect("the array fits in u32"),
        )
        .symbol(SYMBOL),
    );
    Ok(plan)
}

/// Run the importer's own reader over the plan and compare with the pack.
fn check_plan(
    ctx: &Context<'_>,
    plan: &TrainerClassNamePlan,
    payload: &[u8],
) -> Result<(), GenRomProfileError> {
    let mismatch = |reason: String| GenRomProfileError::StructMismatch {
        id: PACK_ID.to_owned(),
        reason,
    };
    let table = rom_import::GbaPtr::new(plan.addr)
        .ok_or_else(|| mismatch(format!("{:08X} is outside the ROM", plan.addr)))?;
    let reader = rom_import::RomReader::new(ctx.rom);
    let decoded = rom_import::decode_trainer_class_name_payload(&reader, table)
        .map_err(|err| mismatch(format!("the ROM's class names do not decode: {err}")))?;
    if decoded != payload {
        return Err(mismatch(
            "the ROM's decoded class names differ from the pack entry".to_owned(),
        ));
    }
    Ok(())
}
