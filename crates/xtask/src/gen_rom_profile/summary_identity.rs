//! Locating the summary screen's identity and met-location tables
//! (`src/data/text/nature_names.h`, `src/data/region_map/region_map_sections.json`,
//! `src/strings.c`, `src/pokemon.c`).
//!
//! The pack's `summary-identity/table` entry is the expectation: 25 nature
//! names, 213 region-map section names, the Ferry, Secret Base and Hideout
//! strings, and 411 species conversions. Nothing here records a stream's or a
//! value's bytes; only addresses leave this module.
//!
//! The ROM holds four tables, each found without trusting a symbol:
//!
//! - The two name arrays (`gNatureNamePointers`, `gRegionMapEntries`). Most of
//!   their names are shorter than the byte search can anchor on, and several
//!   repeat inside longer names, so no string identifies them. Every ROM offset
//!   is tested as a base instead, and exactly one must point every row at its
//!   stream at the importer's row stride and name offset. A wrong stride matches
//!   nowhere.
//! - The three special strings. `Ferry` is shorter than the search can anchor
//!   and `Hideout` ends several map names, but upstream defines the three
//!   back to back, so their concatenation is a unique signature and each
//!   address follows from the lengths before it. The importer's reader then
//!   confirms each against the pack, so a layout that breaks the adjacency
//!   fails there rather than passing silently.
//! - `sSpeciesToNationalPokedexNum`. Its 411 `u16`s are the signature.
//!
//! Finally the importer's own reader runs over the finished plan and must yield
//! the pack's payload byte for byte.

use assets::pack::SummaryIdentityTable;

use super::audio::raw_payload;
use super::error::GenRomProfileError;
use super::inventory;
use super::locate::{exactly_one, only_one_matching, to_addr, to_offset};
use super::plan::{ReportLine, Resolution, SummaryIdentityPlan};
use super::Context;

const PACK_ID: &str = rom_import::SUMMARY_IDENTITY_PACK_ID;
/// The pack-id prefix the importer's one entry lives under.
const PACK_PREFIX: &str = "summary-identity/";
/// One nature-table entry is a pointer.
const POINTER_BYTES: usize = 4;
const REGION_ROW_BYTES: usize = rom_import::REGION_ROW_BYTES;
const REGION_NAME_OFFSET: usize = rom_import::REGION_NAME_OFFSET;

/// Locate the summary-identity roots.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] if the pack holds anything under
/// `summary-identity/` but the one table, or the ROM's tables differ from the
/// pack's; [`GenRomProfileError::MissingPackEntry`],
/// [`GenRomProfileError::WrongPackEntryKind`], or
/// [`GenRomProfileError::EntryShape`] for a pack entry that is absent, not
/// raw, or not a valid table.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<SummaryIdentityPlan, GenRomProfileError> {
    inventory::require_summary_identity(ctx.pack)?;
    let ids = ctx.pack.ids_with_prefix(PACK_PREFIX);
    if ids != [PACK_ID] {
        return Err(GenRomProfileError::StructMismatch {
            id: format!("{PACK_PREFIX}*"),
            reason: format!("only [\"{PACK_ID}\"] is supported, the pack holds {ids:?}"),
        });
    }
    let payload = raw_payload(ctx, PACK_ID)?;
    let expected =
        SummaryIdentityTable::decode(payload).map_err(|err| GenRomProfileError::EntryShape {
            id: PACK_ID.to_owned(),
            reason: err.to_string(),
        })?;

    let natures: Vec<&[u8]> = expected.nature_names().collect();
    let sections: Vec<&[u8]> = expected.region_section_names().collect();
    let (ferry, secret_base, hideout) = locate_specials(ctx, &expected, report)?;
    let plan = SummaryIdentityPlan {
        natures: locate_name_table(
            ctx,
            Table {
                symbol: "gNatureNamePointers",
                stride: POINTER_BYTES,
                name_offset: 0,
            },
            &natures,
            report,
        )?,
        region_map_entries: locate_name_table(
            ctx,
            Table {
                symbol: "gRegionMapEntries",
                stride: REGION_ROW_BYTES,
                name_offset: REGION_NAME_OFFSET,
            },
            &sections,
            report,
        )?,
        ferry,
        secret_base,
        hideout,
        species_to_national: locate_conversions(ctx, &expected, report)?,
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

/// A table of name pointers: its symbol, its row stride, and where in a row
/// the pointer sits.
#[derive(Clone, Copy)]
struct Table {
    symbol: &'static str,
    stride: usize,
    name_offset: usize,
}

impl Table {
    /// Whether every row at `base` (a ROM offset) points at its stream.
    fn matches_at(self, rom: &[u8], base: usize, streams: &[&[u8]]) -> bool {
        streams.iter().enumerate().all(|(row, stream)| {
            let at = base + row * self.stride + self.name_offset;
            rom.get(at..at + POINTER_BYTES)
                .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four bytes")))
                .and_then(to_offset)
                .and_then(|name| rom.get(name..name.checked_add(stream.len())?))
                .is_some_and(|found| found == *stream)
        })
    }
}

/// Find a name table by testing every ROM offset as its base.
fn locate_name_table(
    ctx: &Context<'_>,
    table: Table,
    streams: &[&[u8]],
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let bases = (0..ctx.rom.len()).filter(|&base| table.matches_at(ctx.rom, base, streams));
    let base = only_one_matching(
        &line_id(table.symbol),
        "every row's stream",
        bases.map(|base| to_addr(u32::try_from(base).expect("a ROM offset fits in u32"))),
        |_| true,
    )?;
    report.push(
        ReportLine::unique(
            line_id(table.symbol),
            base,
            len_of(streams.len(), table.stride),
        )
        .with(Resolution::StructDerived)
        .symbol(table.symbol)
        .note(format!(
            "{} rows of {} bytes, name pointer at +{}",
            streams.len(),
            table.stride,
            table.name_offset
        )),
    );
    Ok(base)
}

/// Find the three adjacent special strings through their concatenation.
fn locate_specials(
    ctx: &Context<'_>,
    expected: &SummaryIdentityTable<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<(u32, u32, u32), GenRomProfileError> {
    let names = expected.special_location_names();
    let strings = [
        ("gText_Ferry", names.ferry),
        ("gText_SecretBase", names.secret_base),
        ("gText_Hideout", names.hideout),
    ];
    let joined: Vec<u8> = strings
        .iter()
        .flat_map(|(_, s)| s.iter().copied())
        .collect();
    let hits = &ctx.raw.find_all(&[joined])[0];
    let start = exactly_one(&line_id("gText_Ferry"), hits)?;

    let mut addrs = [0u32; 3];
    let mut next = start;
    for ((symbol, stream), addr) in strings.into_iter().zip(&mut addrs) {
        *addr = next;
        let len = u32::try_from(stream.len()).expect("a string fits in u32");
        report.push(
            ReportLine::unique(line_id(symbol), next, len)
                .symbol(symbol)
                .note("adjacent to the other two special strings"),
        );
        next += len;
    }
    Ok((addrs[0], addrs[1], addrs[2]))
}

/// Find the conversion array through its own values.
fn locate_conversions(
    ctx: &Context<'_>,
    expected: &SummaryIdentityTable<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<u32, GenRomProfileError> {
    let symbol = "sSpeciesToNationalPokedexNum";
    let needle: Vec<u8> = expected
        .species_conversions()
        .flat_map(|(_, national)| national.get().to_le_bytes())
        .collect();
    let len = u32::try_from(needle.len()).expect("a table fits in u32");
    let hits = &ctx.raw.find_all(&[needle])[0];
    let base = exactly_one(&line_id(symbol), hits)?;
    report.push(ReportLine::unique(line_id(symbol), base, len).local_symbol(symbol));
    Ok(base)
}

/// Run the importer's own reader over the plan and compare with the pack.
fn check_plan(
    ctx: &Context<'_>,
    plan: &SummaryIdentityPlan,
    payload: &[u8],
) -> Result<(), GenRomProfileError> {
    let ptr = |what: &str, addr: u32| {
        rom_import::GbaPtr::new(addr).ok_or_else(|| GenRomProfileError::StructMismatch {
            id: what.to_owned(),
            reason: format!("{addr:08X} is outside the ROM"),
        })
    };
    let roots = rom_import::SummaryIdentityRoots {
        natures: ptr("gNatureNamePointers", plan.natures)?,
        region_map_entries: ptr("gRegionMapEntries", plan.region_map_entries)?,
        ferry: ptr("gText_Ferry", plan.ferry)?,
        secret_base: ptr("gText_SecretBase", plan.secret_base)?,
        hideout: ptr("gText_Hideout", plan.hideout)?,
        species_to_national: ptr("sSpeciesToNationalPokedexNum", plan.species_to_national)?,
    };
    let reader = rom_import::RomReader::new(ctx.rom);
    let decoded = rom_import::decode_summary_identity_payload(&reader, &roots).map_err(|err| {
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
