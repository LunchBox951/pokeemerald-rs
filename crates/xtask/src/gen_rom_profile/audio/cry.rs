//! Locating the species cry tables and their samples, with their colocated tests.
//!
//! The cry samples are DPCM-compressed in the ROM (`wav2agb -c`), so their
//! decoded PCM occurs nowhere in it and the PCM search that places instrument
//! samples cannot find them. They are reached from the tables instead:
//! `gCryTable` and `gCryTable_Reverse` are each a run of [`CRY_TABLE_LEN`]
//! `cry` or `cry_reverse` `ToneData` records
//! (`pokeemerald/asm/macros/music_voice.inc:142-152`), found by that shape
//! alone and accepted only when exactly one run of that exact length has it.
//! Position `n` of the pack's `audio/cry/table` binds to record `n`, and the
//! record's pointer is the sample's address. The importer's own cry decoder
//! then reads the planned roots, and every entry it would write must equal the
//! pack's byte for byte, guard sample included, so the generator and the
//! importer cannot drift on the DPCM contract.

use std::collections::BTreeSet;

use assets::audio::{CryTable, CRY_TABLE_LEN, CRY_TABLE_PACK_ID};

use crate::extract::cry::SAMPLE_ID_PREFIX as CRY_SAMPLE_PREFIX;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::locate::{exactly_one, to_offset, u32_at};
use crate::gen_rom_profile::plan::{CryPlan, ReportLine, Resolution, SamplePlan};
use crate::gen_rom_profile::Context;

use super::samples::direct_sound_of;
use super::validation::importer_cry_roots;
use super::{raw_payload, WAVE_HEADER_BYTES};

const NORMAL_SYMBOL: &str = "gCryTable";
const REVERSE_SYMBOL: &str = "gCryTable_Reverse";
const RECORD_BYTES: usize = 12;
/// `ToneData.wav`'s offset.
const RECORD_WAV: usize = 4;
/// The `cry` macro's `type` byte.
const TYPE_NORMAL: u8 = 0x20;
/// The `cry_reverse` macro's `type` byte.
const TYPE_REVERSE: u8 = 0x30;

/// Plan the cries, or `None` when the pack holds none.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for cry samples without a table;
/// [`GenRomProfileError::EntryShape`] for a table that does not decode;
/// [`GenRomProfileError::StructMismatch`] for a table that repeats a sample
/// or disagrees with the pack's cry samples, for a ROM table record outside
/// the ROM, and for a ROM cry the importer cannot decode or that differs from
/// the pack's; [`GenRomProfileError::NotFound`] or
/// [`GenRomProfileError::Ambiguous`] unless exactly one run of records has
/// each table's shape.
pub(super) fn locate_cry(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Option<CryPlan>, GenRomProfileError> {
    let sample_ids = ctx.pack.ids_with_prefix(CRY_SAMPLE_PREFIX);
    let has_table = !ctx.pack.ids_with_prefix(CRY_TABLE_PACK_ID).is_empty();
    if sample_ids.is_empty() && !has_table {
        return Ok(None);
    }
    let table = CryTable::decode(raw_payload(ctx, CRY_TABLE_PACK_ID)?).map_err(|err| {
        GenRomProfileError::EntryShape {
            id: CRY_TABLE_PACK_ID.to_owned(),
            reason: err.to_string(),
        }
    })?;
    let ids: Vec<&str> = table.samples().iter().map(|id| id.0.as_str()).collect();
    check_inventory(&ids, &sample_ids)?;

    let normal = exactly_one(NORMAL_SYMBOL, &table_offsets(ctx.rom, TYPE_NORMAL))?;
    let reverse = exactly_one(REVERSE_SYMBOL, &table_offsets(ctx.rom, TYPE_REVERSE))?;
    let samples = ids
        .iter()
        .enumerate()
        .map(|(position, id)| sample_plan(ctx, normal, position, id))
        .collect::<Result<Vec<_>, _>>()?;
    let plan = CryPlan {
        normal,
        reverse,
        samples,
    };
    check_decoded(ctx, &plan, &ids)?;
    report_roots(&plan, report);
    Ok(Some(plan))
}

/// Require the table to name each of the pack's cry samples exactly once.
fn check_inventory(table: &[&str], pack: &[String]) -> Result<(), GenRomProfileError> {
    let mismatch = |reason: String| GenRomProfileError::StructMismatch {
        id: CRY_TABLE_PACK_ID.to_owned(),
        reason,
    };
    let named: BTreeSet<&str> = table.iter().copied().collect();
    if named.len() != table.len() {
        return Err(mismatch("the table names a sample twice".to_owned()));
    }
    if let Some(id) = table.iter().find(|id| !id.starts_with(CRY_SAMPLE_PREFIX)) {
        return Err(mismatch(format!(
            "`{id}` is not under `{CRY_SAMPLE_PREFIX}`"
        )));
    }
    let held: BTreeSet<&str> = pack.iter().map(String::as_str).collect();
    if named != held {
        let unnamed: Vec<&&str> = held.difference(&named).collect();
        let missing: Vec<&&str> = named.difference(&held).collect();
        return Err(mismatch(format!(
            "the table and the pack's cry samples disagree: {unnamed:?} are unnamed, \
             {missing:?} are absent"
        )));
    }
    Ok(())
}

/// The ROM offsets where a run of exactly [`CRY_TABLE_LEN`] records of `kind`
/// starts.
///
/// A run is every consecutive 12-byte record with `kind`'s fixed bytes and a
/// cartridge pointer. A longer or shorter run is no table: the record before
/// `gCryTable_Reverse`'s first is the last normal one, and its type differs.
fn table_offsets(rom: &[u8], kind: u8) -> Vec<u32> {
    let is_record = |offset: usize| {
        rom.get(offset..offset + RECORD_BYTES)
            .is_some_and(|record| {
                record[..4] == [kind, 60, 0, 0]
                    && record[8..] == [0xFF, 0, 0xFF, 0]
                    && u32_at(rom, offset + RECORD_WAV).is_some_and(|ptr| to_offset(ptr).is_some())
            })
    };
    let mut hits = Vec::new();
    for start in 0..rom.len() {
        if !is_record(start) || (start >= RECORD_BYTES && is_record(start - RECORD_BYTES)) {
            continue;
        }
        let mut length = 1;
        while is_record(start + length * RECORD_BYTES) {
            length += 1;
        }
        if length == CRY_TABLE_LEN {
            hits.push(u32::try_from(start).expect("a ROM offset fits in u32"));
        }
    }
    hits
}

/// The sample root for table position `position`: its address is the record's
/// pointer and its count the pack sample's.
fn sample_plan(
    ctx: &Context<'_>,
    normal: u32,
    position: usize,
    id: &str,
) -> Result<SamplePlan, GenRomProfileError> {
    let record = normal + u32::try_from(position * RECORD_BYTES).expect("a table offset fits");
    let addr =
        crate::gen_rom_profile::locate::u32_at_addr(ctx.rom, record + 4).ok_or_else(|| {
            GenRomProfileError::StructMismatch {
                id: id.to_owned(),
                reason: format!("record {position} at {record:08X} runs off the ROM"),
            }
        })?;
    Ok(SamplePlan {
        id: id.to_owned(),
        addr,
        header_len: WAVE_HEADER_BYTES,
        data_len: direct_sound_of(ctx, id)?.sample_count(),
    })
}

/// Require the importer's decode of `plan` to equal the pack's entries.
fn check_decoded(
    ctx: &Context<'_>,
    plan: &CryPlan,
    ids: &[&str],
) -> Result<(), GenRomProfileError> {
    let roots = importer_cry_roots(plan)?;
    let entries = rom_import::decode_cry_payloads(&rom_import::RomReader::new(ctx.rom), &roots)
        .map_err(|err| GenRomProfileError::StructMismatch {
            id: CRY_TABLE_PACK_ID.to_owned(),
            reason: format!("the ROM cries do not decode: {err}"),
        })?;
    let expected = std::iter::once(CRY_TABLE_PACK_ID).chain(ids.iter().copied());
    for (entry, id) in entries.iter().zip(expected) {
        if entry.id != id || entry.payload != raw_payload(ctx, id)? {
            return Err(GenRomProfileError::StructMismatch {
                id: id.to_owned(),
                reason: "the ROM's decoded cry differs from the pack entry".to_owned(),
            });
        }
    }
    Ok(())
}

fn report_roots(plan: &CryPlan, report: &mut Vec<ReportLine>) {
    let span = u32::try_from(CRY_TABLE_LEN * RECORD_BYTES).expect("a table span fits in u32");
    report.push(
        ReportLine::unique(CRY_TABLE_PACK_ID, plan.normal, span)
            .symbol(NORMAL_SYMBOL)
            .note("388 cry records matched by shape; samples read from their pointers"),
    );
    report.push(
        ReportLine::unique(format!("{CRY_TABLE_PACK_ID} (reverse)"), plan.reverse, span)
            .symbol(REVERSE_SYMBOL)
            .note("388 cry_reverse records matched by shape"),
    );
    for sample in &plan.samples {
        report.push(
            ReportLine::unique(
                &sample.id,
                sample.addr,
                sample.header_len + sample.data_len + 1,
            )
            .with(Resolution::PointerWalk)
            .note("DPCM decoded; matches the pack"),
        );
    }
}

#[cfg(test)]
mod tests;
