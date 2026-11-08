//! Locating the battle-message token table (`src/battle_message.c`).
//!
//! The pack's `battle-message/table` entry is the expectation: 369 table
//! streams and the fixed special-template inventory, each an `EOS`-terminated
//! charmap token stream. The ROM stores the table slots behind
//! `gBattleStringsTable`, a pointer array, and the special templates as bare
//! strings. Nothing here records a stream's bytes; only addresses leave this
//! module.
//!
//! The walk:
//!
//! 1. A table stream that appears exactly once in the ROM anchors the table:
//!    the pointers to that string, each biased back by the slot's index, are
//!    the candidate table bases, and exactly one base must point every one of
//!    the 369 slots at its stream.
//! 2. Each special template is a string start (the byte before it is an
//!    `EOS`) whose bytes are the pack's stream. A template found once is
//!    unique; one found in several places must have byte-identical copies,
//!    which the importer reads the same either way.
//! 3. The importer's own reader runs over the finished plan and must yield the
//!    pack's payload byte for byte.

use assets::pack::{
    BattleMessageTable, BATTLE_MESSAGE_SPECIAL_TEMPLATES, BATTLE_MESSAGE_TABLE_START,
};

use super::audio::raw_payload;
use super::error::GenRomProfileError;
use super::inventory;
use super::locate::{only_one_matching, slice_at_addr, to_addr, u32_at_addr};
use super::plan::{BattleMessagePlan, ReportLine, Resolution, SpecialMessagePlan};
use super::Context;

const PACK_ID: &str = rom_import::BATTLE_MESSAGE_PACK_ID;
/// The pack-id prefix the importer's one entry lives under.
const PACK_PREFIX: &str = "battle-message/";
/// The end-of-string byte every stream carries.
const EOS: u8 = 0xFF;
/// One `gBattleStringsTable` entry is a pointer.
const POINTER_BYTES: u32 = 4;

/// Locate the battle-message roots.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack holds anything under
/// `battle-message/` but the one table, or the ROM's table or special
/// templates differ from the pack's; [`GenRomProfileError::MissingPackEntry`],
/// [`GenRomProfileError::WrongPackEntryKind`], or
/// [`GenRomProfileError::EntryShape`] for a pack entry that is absent, not
/// raw, or not a valid table; [`GenRomProfileError::NotFound`] or
/// [`GenRomProfileError::Ambiguous`] for a root the ROM does not hold once.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<BattleMessagePlan, GenRomProfileError> {
    inventory::require_battle_messages(ctx.pack)?;
    let ids = ctx.pack.ids_with_prefix(PACK_PREFIX);
    if ids != [PACK_ID] {
        return Err(GenRomProfileError::StructMismatch {
            id: format!("{PACK_PREFIX}*"),
            reason: format!("only [\"{PACK_ID}\"] is supported, the pack holds {ids:?}"),
        });
    }
    let payload = raw_payload(ctx, PACK_ID)?;
    let expected =
        BattleMessageTable::decode(payload).map_err(|err| GenRomProfileError::EntryShape {
            id: PACK_ID.to_owned(),
            reason: err.to_string(),
        })?;

    let table = locate_table(ctx, &expected, report)?;
    let specials = locate_specials(ctx, &expected, report)?;
    let plan = BattleMessagePlan { table, specials };
    check_plan(ctx, &plan, payload)?;
    Ok(plan)
}

/// Find `gBattleStringsTable` through a table string only one place holds.
fn locate_table(
    ctx: &Context<'_>,
    expected: &BattleMessageTable<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let strings: Vec<(u16, &[u8])> = expected.strings().collect();
    let needles: Vec<Vec<u8>> = strings.iter().map(|(_, stream)| stream.to_vec()).collect();
    let hits = ctx.raw.find_all(&needles);
    let (slot, anchor) = hits
        .iter()
        .enumerate()
        .find_map(|(slot, found)| match found.as_slice() {
            [only] => Some((slot, to_addr(*only))),
            _ => None,
        })
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: "gBattleStringsTable".to_owned(),
            reason: "no table string is unique in the ROM, so nothing anchors the table".to_owned(),
        })?;
    let slot_offset = u32::try_from(slot).expect("a slot index fits in u32") * POINTER_BYTES;

    let base = only_one_matching(
        "gBattleStringsTable",
        "every table slot's stream",
        ctx.pointers
            .refs_to(anchor)
            .iter()
            .filter_map(|at| at.checked_sub(slot_offset)),
        |&base| {
            strings.iter().enumerate().all(|(index, (_, stream))| {
                let entry =
                    base + u32::try_from(index).expect("a slot fits in u32") * POINTER_BYTES;
                u32_at_addr(ctx.rom, entry)
                    .and_then(|ptr| slice_at_addr(ctx.rom, ptr, stream.len()))
                    .is_some_and(|found| found == *stream)
            })
        },
    )?;
    let slot_count = u32::try_from(strings.len()).expect("a table fits in u32");
    report.push(
        ReportLine::unique(PACK_ID, base, slot_count * POINTER_BYTES)
            .with(Resolution::StructDerived)
            .symbol("gBattleStringsTable")
            .note(format!(
                "{slot_count} slots from id {BATTLE_MESSAGE_TABLE_START}, anchored on slot {slot}"
            )),
    );
    Ok(base)
}

/// A special template that no byte search settles alone, placed against a
/// neighbour the same translation unit declares beside it.
///
/// `battle_message.c` defines these as consecutive `static const u8` arrays,
/// which the linker lays out back to back with no padding.
#[derive(Debug, Clone, Copy)]
enum Neighbor {
    /// Starts exactly where the named template's stream ends.
    After(&'static str),
    /// Ends exactly where the named template's stream starts.
    Before(&'static str),
}

/// Specials whose bytes appear at several string starts, with the neighbour
/// that picks one: declaration order in `src/battle_message.c`.
const NEIGHBORS: [(&str, Neighbor); 8] = [
    (
        "sText_WildPkmnAppeared",
        Neighbor::Before("sText_LegendaryPkmnAppeared"),
    ),
    (
        "sText_LegendaryPkmnAppeared",
        Neighbor::Before("sText_WildPkmnAppearedPause"),
    ),
    (
        "sText_ExclamationMark",
        Neighbor::After("sText_AttackerUsedX"),
    ),
    (
        "sText_ExclamationMark2",
        Neighbor::After("sText_ExclamationMark"),
    ),
    (
        "sText_ExclamationMark3",
        Neighbor::After("sText_ExclamationMark2"),
    ),
    (
        "sText_ExclamationMark4",
        Neighbor::After("sText_ExclamationMark3"),
    ),
    (
        "sText_ExclamationMark5",
        Neighbor::After("sText_ExclamationMark4"),
    ),
    ("sText_SpaceIs", Neighbor::Before("sText_ApostropheS")),
];

/// Upstream's `sATypeMove_Table[NUMBER_OF_MON_TYPES][17]` spelling of a row.
const TYPE_ROW_PREFIX: &str = "sATypeMove_Table[";
/// Bytes per `sATypeMove_Table` row: the stream, then zero padding.
const TYPE_ROW_BYTES: u32 = 17;

/// Where a special template sits, for the importer and the report.
#[derive(Clone, Copy)]
struct Placed {
    addr: u32,
    how: Placement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// The only string start with these bytes.
    Unique,
    /// Chosen by a neighbour's address.
    Neighbor,
    /// A row of the fixed-stride type table.
    TypeRow,
}

/// Find each special template.
///
/// Most are the only string start holding their bytes. Byte-identical
/// templates resolve against a [`NEIGHBORS`] entry, and the `sATypeMove_Table`
/// rows, which sit in zero-padded 17-byte cells and so start after no `EOS`,
/// resolve against the first row.
fn locate_specials(
    ctx: &Context<'_>,
    expected: &BattleMessageTable<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<SpecialMessagePlan>, GenRomProfileError> {
    let templates = &BATTLE_MESSAGE_SPECIAL_TEMPLATES;
    let streams: Vec<&[u8]> = (0..templates.len())
        .map(|index| {
            expected
                .special(index)
                .ok_or_else(|| GenRomProfileError::NotFound {
                    id: special_id(templates[index].symbol),
                })
        })
        .collect::<Result<_, _>>()?;
    let candidates: Vec<Vec<u32>> = templates
        .iter()
        .zip(&streams)
        .map(|(template, stream)| {
            if is_type_row(template.symbol) && !is_first_type_row(template.symbol) {
                Vec::new()
            } else {
                string_starts(ctx.rom, stream)
            }
        })
        .collect();

    let mut placed: Vec<Option<Placed>> = candidates
        .iter()
        .map(|found| match found.as_slice() {
            [only] => Some(Placed {
                addr: *only,
                how: Placement::Unique,
            }),
            _ => None,
        })
        .collect();
    place_type_rows(ctx, &streams, &mut placed)?;
    loop {
        let mut progressed = false;
        for (index, template) in templates.iter().enumerate() {
            if placed[index].is_some() || candidates[index].len() < 2 {
                continue;
            }
            let Some(&(_, neighbor)) = NEIGHBORS
                .iter()
                .find(|(symbol, _)| *symbol == template.symbol)
            else {
                continue;
            };
            if let Some(addr) = by_neighbor(
                template.symbol,
                templates,
                &streams,
                &placed,
                &candidates[index],
                streams[index].len(),
                neighbor,
            )? {
                placed[index] = Some(Placed {
                    addr,
                    how: Placement::Neighbor,
                });
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    templates
        .iter()
        .enumerate()
        .map(|(index, template)| {
            let id = special_id(template.symbol);
            let Some(Placed { addr, how }) = placed[index] else {
                return Err(match candidates[index].as_slice() {
                    [] => GenRomProfileError::NotFound { id },
                    many => GenRomProfileError::Ambiguous {
                        id,
                        addrs: many.to_vec(),
                    },
                });
            };
            let len = u32::try_from(streams[index].len()).expect("a stream fits in u32");
            let line = ReportLine::unique(id, addr, len).interior();
            report.push(match how {
                Placement::Unique => line,
                Placement::Neighbor => line
                    .with(Resolution::StructDerived)
                    .note("identical bytes elsewhere; placed against its declared neighbour"),
                Placement::TypeRow => line
                    .with(Resolution::StructDerived)
                    .note("row of the 17-byte type table"),
            });
            Ok(SpecialMessagePlan {
                symbol: template.symbol,
                addr,
            })
        })
        .collect()
}

fn special_id(symbol: &str) -> String {
    format!("{PACK_ID}#{symbol}")
}

fn is_type_row(symbol: &str) -> bool {
    symbol.starts_with(TYPE_ROW_PREFIX)
}

fn is_first_type_row(symbol: &str) -> bool {
    symbol == "sATypeMove_Table[TYPE_NORMAL]"
}

/// Place every type-table row at its stride from `TYPE_NORMAL`'s, requiring the
/// ROM to hold the row's stream and then zero padding to the cell's end.
fn place_type_rows(
    ctx: &Context<'_>,
    streams: &[&[u8]],
    placed: &mut [Option<Placed>],
) -> Result<(), GenRomProfileError> {
    let templates = &BATTLE_MESSAGE_SPECIAL_TEMPLATES;
    let Some(first) = templates.iter().position(|t| is_first_type_row(t.symbol)) else {
        return Ok(());
    };
    let Some(base) = placed[first].as_ref().map(|p| p.addr) else {
        return Ok(());
    };
    for (row, index) in (first..templates.len())
        .filter(|&index| is_type_row(templates[index].symbol))
        .enumerate()
    {
        let id = special_id(templates[index].symbol);
        let addr = base + u32::try_from(row).expect("a row fits in u32") * TYPE_ROW_BYTES;
        let cell = slice_at_addr(ctx.rom, addr, TYPE_ROW_BYTES as usize)
            .ok_or_else(|| GenRomProfileError::NotFound { id: id.clone() })?;
        let stream = streams[index];
        let padded = cell.get(..stream.len()) == Some(stream)
            && cell[stream.len()..].iter().all(|&byte| byte == 0);
        if !padded {
            return Err(GenRomProfileError::StructMismatch {
                id,
                reason: format!(
                    "the cell {row} rows from the first type row is not the row's stream \
                     followed by zero padding"
                ),
            });
        }
        if index != first {
            placed[index] = Some(Placed {
                addr,
                how: Placement::TypeRow,
            });
        }
    }
    Ok(())
}

/// The one candidate `neighbor` allows for the template `own_symbol`, if its
/// anchor is placed yet. A refusal blames `own_symbol`, the template that did
/// not resolve; the anchor appears only in the reason.
fn by_neighbor(
    own_symbol: &str,
    templates: &[assets::pack::SpecialTemplate],
    streams: &[&[u8]],
    placed: &[Option<Placed>],
    candidates: &[u32],
    own_len: usize,
    neighbor: Neighbor,
) -> Result<Option<u32>, GenRomProfileError> {
    let (symbol, after) = match neighbor {
        Neighbor::After(symbol) => (symbol, true),
        Neighbor::Before(symbol) => (symbol, false),
    };
    let Some(anchor) = templates.iter().position(|t| t.symbol == symbol) else {
        return Err(GenRomProfileError::StructMismatch {
            id: special_id(symbol),
            reason: "a neighbour names a template the inventory lacks".to_owned(),
        });
    };
    let Some(anchor_addr) = placed[anchor].as_ref().map(|p| p.addr) else {
        return Ok(None);
    };
    let anchor_len = u32::try_from(streams[anchor].len()).expect("a stream fits in u32");
    let own_len = u32::try_from(own_len).expect("a stream fits in u32");
    let wanted = if after {
        anchor_addr.checked_add(anchor_len)
    } else {
        anchor_addr.checked_sub(own_len)
    };
    only_one_matching(
        &special_id(own_symbol),
        &format!("the address beside its declared neighbour {symbol}"),
        candidates.iter().copied(),
        |&candidate| Some(candidate) == wanted,
    )
    .map(Some)
}

/// Every bus address where `stream` begins a string: the byte before it is an
/// `EOS`, or it starts the image.
fn string_starts(rom: &[u8], stream: &[u8]) -> Vec<u32> {
    let Some((&first, _)) = stream.split_first() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut from = 0usize;
    while let Some(at) = rom[from..].iter().position(|&byte| byte == first) {
        let start = from + at;
        from = start + 1;
        let starts_string = start == 0 || rom[start - 1] == EOS;
        if starts_string && rom[start..].starts_with(stream) {
            found.push(to_addr(
                u32::try_from(start).expect("a ROM offset fits in u32"),
            ));
        }
    }
    found
}

/// Run the importer's own reader over the plan and compare with the pack.
fn check_plan(
    ctx: &Context<'_>,
    plan: &BattleMessagePlan,
    payload: &[u8],
) -> Result<(), GenRomProfileError> {
    let ptr = |what: &str, addr: u32| {
        rom_import::GbaPtr::new(addr).ok_or_else(|| GenRomProfileError::StructMismatch {
            id: what.to_owned(),
            reason: format!("{addr:08X} is outside the ROM"),
        })
    };
    let specials: Vec<rom_import::SpecialMessageRoot> = plan
        .specials
        .iter()
        .map(|special| {
            Ok(rom_import::SpecialMessageRoot {
                symbol: special.symbol,
                addr: ptr(special.symbol, special.addr)?,
            })
        })
        .collect::<Result<_, GenRomProfileError>>()?;
    let roots = rom_import::BattleMessageRoots {
        table: ptr("gBattleStringsTable", plan.table)?,
        specials: Box::leak(specials.into_boxed_slice()),
    };
    let reader = rom_import::RomReader::new(ctx.rom);
    let decoded = rom_import::decode_battle_message_payload(&reader, &roots).map_err(|err| {
        GenRomProfileError::StructMismatch {
            id: PACK_ID.to_owned(),
            reason: format!("the ROM's table does not decode: {err}"),
        }
    })?;
    if decoded != payload {
        return Err(GenRomProfileError::StructMismatch {
            id: PACK_ID.to_owned(),
            reason: "the ROM's decoded table differs from the pack entry".to_owned(),
        });
    }
    Ok(())
}
