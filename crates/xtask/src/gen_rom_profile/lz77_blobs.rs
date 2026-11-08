//! The shared locator for `raw/*` blobs the game stores as LZ77 streams.
//!
//! Tilemaps are the usual case: the pack holds the decompressed bytes and the
//! ROM holds the compressed stream, so the search runs through the LZ77 index.
//! Every domain module that locates such blobs goes through
//! [`locate_lz77_blobs`], so the matching policy and the report line are
//! written once. A domain owns only its id selection and, where a linker map
//! names the blob, its symbol expectation.

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::locate::exactly_one;
use super::plan::{BlobPlan, ReportLine, SymbolExpectation};
use super::tilesets::len32;
use super::Context;

/// Locate each of `ids` as the LZ77 stream whose decompressed bytes are the
/// pack's `raw` payload.
///
/// Each blob must decompress from exactly one place in the ROM. The plan and
/// the report line both record the decompressed (pack payload) length.
/// `expect` derives the symbol a linker map should show at each id; pass
/// [`unnamed`] when the blob has no symbol of its own.
///
/// # Errors
///
/// [`GenRomProfileError`] when an id is missing from the pack or is not a raw
/// blob, and [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`]
/// when the stream is not found exactly once.
pub fn locate_lz77_blobs(
    ctx: &Context<'_>,
    ids: &[String],
    expect: &dyn Fn(&str) -> SymbolExpectation,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<BlobPlan>, GenRomProfileError> {
    let mut needles = Vec::with_capacity(ids.len());
    for id in ids {
        needles.push(ctx.pack.raw_payload(id)?.to_vec());
    }
    let hits = ctx.lz77.find_all(&needles);

    let mut plans = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let addr = exactly_one(id, &hits[index])?;
        let len = len32(&needles[index]);
        report.push(ReportLine::unique(id, addr, len).with_symbol(expect(id)));
        plans.push(BlobPlan {
            id: id.clone(),
            addr,
            encoding: Encoding::Lz77,
            len,
        });
    }
    Ok(plans)
}

/// The symbol expectation for a blob a linker map does not name.
pub fn unnamed(_id: &str) -> SymbolExpectation {
    SymbolExpectation::Unnamed
}

#[cfg(test)]
#[path = "lz77_blobs_tests.rs"]
mod tests;
