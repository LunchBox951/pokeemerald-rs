//! Locating the summary screen's move and contest tables
//! (`src/data/text/move_descriptions.h`, `src/data/contest_moves.h`,
//! `src/data/contest_text_tables.h`).
//!
//! The pack's `summary-move/table` entry is the expectation: 354 move
//! descriptions, 355 contest moves, 48 contest effects, and 48 effect
//! descriptions. Nothing here records a stream's or a row's bytes; only
//! addresses leave this module.
//!
//! The ROM holds four arrays, each found without trusting a symbol:
//!
//! - The two pointer arrays (`gMoveDescriptionPointers`,
//!   `gContestEffectDescriptionPointers`). A table stream that appears exactly
//!   once in the ROM anchors a candidate: the pointers to that string, each
//!   biased back by the slot's index, are candidate table bases, and exactly
//!   one base must point every slot at its stream. Repeated or empty texts need
//!   no unique-string assumption, since any slot with a unique stream will do.
//! - The two struct arrays (`gContestMoves`, `gContestEffects`). Their rows
//!   carry fields the pack leaves out (combo moves, padding), so no byte string
//!   exists to search for; instead every ROM offset is tested as a base, and
//!   exactly one must reproduce the pack's projected field of every row at the
//!   importer's row stride. A wrong stride matches nowhere.
//!
//! Finally the importer's own reader runs over the finished plan and must yield
//! the pack's payload byte for byte.

use std::collections::BTreeSet;

use assets::pack::{SummaryMoveTable, CONTEST_EFFECT_COUNT, CONTEST_MOVE_COUNT};

use super::audio::raw_payload;
use super::error::GenRomProfileError;
use super::inventory;
use super::locate::{only_one_matching, slice_at_addr, to_addr, u32_at_addr};
use super::plan::{ReportLine, Resolution, SummaryMovePlan};
use super::Context;

const PACK_ID: &str = rom_import::SUMMARY_MOVE_PACK_ID;
/// The pack-id prefix the importer's one entry lives under.
const PACK_PREFIX: &str = "summary-move/";
/// One pointer-table entry is a pointer.
const POINTER_BYTES: u32 = 4;
const CONTEST_MOVE_BYTES: usize = rom_import::CONTEST_MOVE_BYTES;
const CONTEST_EFFECT_BYTES: usize = rom_import::CONTEST_EFFECT_BYTES;
/// `contestCategory:3`, the low three bits of a row's second byte.
const CATEGORY_MASK: u8 = 0b111;

/// Locate the summary-move roots.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack holds anything under
/// `summary-move/` but the one table, or the ROM's tables differ from the
/// pack's; [`GenRomProfileError::MissingPackEntry`],
/// [`GenRomProfileError::WrongPackEntryKind`], or
/// [`GenRomProfileError::EntryShape`] for a pack entry that is absent, not
/// raw, or not a valid table.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<SummaryMovePlan, GenRomProfileError> {
    inventory::require_summary_moves(ctx.pack)?;
    let ids = ctx.pack.ids_with_prefix(PACK_PREFIX);
    if ids != [PACK_ID] {
        return Err(GenRomProfileError::StructMismatch {
            id: format!("{PACK_PREFIX}*"),
            reason: format!("only [\"{PACK_ID}\"] is supported, the pack holds {ids:?}"),
        });
    }
    let payload = raw_payload(ctx, PACK_ID)?;
    let expected =
        SummaryMoveTable::decode(payload).map_err(|err| GenRomProfileError::EntryShape {
            id: PACK_ID.to_owned(),
            reason: err.to_string(),
        })?;

    let descriptions: Vec<&[u8]> = expected.descriptions().collect();
    let effect_texts: Vec<&[u8]> = expected.contest_effects().map(|e| e.description).collect();
    let plan = SummaryMovePlan {
        descriptions: locate_pointer_table(ctx, "gMoveDescriptionPointers", &descriptions, report)?,
        contest_moves: locate_contest_moves(ctx, &expected, report)?,
        contest_effects: locate_contest_effects(ctx, &expected, report)?,
        effect_descriptions: locate_pointer_table(
            ctx,
            "gContestEffectDescriptionPointers",
            &effect_texts,
            report,
        )?,
    };
    check_plan(ctx, &plan, payload)?;
    Ok(plan)
}

fn line_id(symbol: &str) -> String {
    format!("{PACK_ID}#{symbol}")
}

fn len_of(count: usize, stride: usize) -> u32 {
    u32::try_from(count * stride).expect("a table fits in u32")
}

/// Find a pointer array through the table strings only one place holds.
fn locate_pointer_table(
    ctx: &Context<'_>,
    symbol: &str,
    streams: &[&[u8]],
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let needles: Vec<Vec<u8>> = streams.iter().map(|stream| stream.to_vec()).collect();
    let mut candidates = BTreeSet::new();
    for (slot, found) in ctx.raw.find_all(&needles).iter().enumerate() {
        let [only] = found.as_slice() else { continue };
        let slot_offset = u32::try_from(slot).expect("a slot index fits in u32") * POINTER_BYTES;
        candidates.extend(
            ctx.pointers
                .refs_to(to_addr(*only))
                .iter()
                .filter_map(|at| at.checked_sub(slot_offset)),
        );
    }
    let base = only_one_matching(
        &line_id(symbol),
        "every slot's stream",
        candidates,
        |&base| {
            streams.iter().enumerate().all(|(index, stream)| {
                let entry =
                    base + u32::try_from(index).expect("a slot fits in u32") * POINTER_BYTES;
                u32_at_addr(ctx.rom, entry)
                    .and_then(|ptr| slice_at_addr(ctx.rom, ptr, stream.len()))
                    .is_some_and(|found| found == *stream)
            })
        },
    )?;
    report.push(
        ReportLine::unique(line_id(symbol), base, len_of(streams.len(), 4))
            .with(Resolution::StructDerived)
            .symbol(symbol)
            .note(format!(
                "{} slots, anchored on a unique stream",
                streams.len()
            )),
    );
    Ok(base)
}

/// Find a struct array by testing every ROM offset as its base.
fn locate_rows(
    ctx: &Context<'_>,
    symbol: &str,
    count: usize,
    stride: usize,
    row_matches: impl Fn(&[u8], usize) -> bool,
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let span = stride * (count - 1) + 3;
    let last_base = ctx.rom.len().saturating_sub(span);
    let bases = (0..=last_base).filter(|&base| {
        (0..count).all(|row| {
            ctx.rom
                .get(base + row * stride..)
                .is_some_and(|tail| row_matches(tail, row))
        })
    });
    let base = only_one_matching(
        &line_id(symbol),
        "the pack's field of every row",
        bases.map(|base| to_addr(u32::try_from(base).expect("a ROM offset fits in u32"))),
        |_| true,
    )?;
    report.push(
        ReportLine::unique(line_id(symbol), base, len_of(count, stride))
            .with(Resolution::StructDerived)
            .symbol(symbol)
            .note(format!("{count} rows of {stride} bytes")),
    );
    Ok(base)
}

fn locate_contest_moves(
    ctx: &Context<'_>,
    expected: &SummaryMoveTable<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let rows: Vec<(u8, u8)> = expected
        .contest_moves()
        .map(|row| (row.effect.index(), row.category.raw()))
        .collect();
    locate_rows(
        ctx,
        "gContestMoves",
        CONTEST_MOVE_COUNT,
        CONTEST_MOVE_BYTES,
        |tail, row| {
            let (effect, category) = rows[row];
            tail[0] == effect && tail[1] & CATEGORY_MASK == category
        },
        report,
    )
}

fn locate_contest_effects(
    ctx: &Context<'_>,
    expected: &SummaryMoveTable<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let rows: Vec<[u8; 3]> = expected
        .contest_effects()
        .map(|row| [row.effect_type.raw(), row.appeal, row.jam])
        .collect();
    locate_rows(
        ctx,
        "gContestEffects",
        CONTEST_EFFECT_COUNT,
        CONTEST_EFFECT_BYTES,
        |tail, row| tail[..3] == rows[row],
        report,
    )
}

/// Run the importer's own reader over the plan and compare with the pack.
fn check_plan(
    ctx: &Context<'_>,
    plan: &SummaryMovePlan,
    payload: &[u8],
) -> Result<(), GenRomProfileError> {
    let ptr = |what: &str, addr: u32| {
        rom_import::GbaPtr::new(addr).ok_or_else(|| GenRomProfileError::StructMismatch {
            id: what.to_owned(),
            reason: format!("{addr:08X} is outside the ROM"),
        })
    };
    let roots = rom_import::SummaryMoveRoots {
        descriptions: ptr("gMoveDescriptionPointers", plan.descriptions)?,
        contest_moves: ptr("gContestMoves", plan.contest_moves)?,
        contest_effects: ptr("gContestEffects", plan.contest_effects)?,
        effect_descriptions: ptr(
            "gContestEffectDescriptionPointers",
            plan.effect_descriptions,
        )?,
    };
    let reader = rom_import::RomReader::new(ctx.rom);
    let decoded = rom_import::decode_summary_move_payload(&reader, &roots).map_err(|err| {
        GenRomProfileError::StructMismatch {
            id: PACK_ID.to_owned(),
            reason: format!("the ROM's tables do not decode: {err}"),
        }
    })?;
    if decoded != payload {
        return Err(GenRomProfileError::StructMismatch {
            id: PACK_ID.to_owned(),
            reason: "the ROM's decoded tables differ from the pack entry".to_owned(),
        });
    }
    Ok(())
}
