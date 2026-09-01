//! Locating palette banks.
//!
//! A pack palette entry is already GBA-native BGR555, so its payload *is*
//! the ROM's bytes and no repacking is needed. A 16-colour bank is only 32
//! bytes, which is short enough to repeat, so a bank that turns up twice is
//! a hard failure here: the caller that knows a structural way to choose
//! (a `{data, tag}` table, adjacency to an already-unique root) resolves it
//! itself.

use super::error::GenRomProfileError;
use super::locate::exactly_one;
use super::plan::{PalettePlan, ReportLine, SymbolExpectation};
use super::tilesets::len32;
use super::Context;

/// Locate every palette in `ids`, requiring each to be unique.
///
/// `expect` says what a linker map should find at each id's address; the
/// caller knows its own domain's naming, this module does not.
///
/// # Errors
///
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`]
/// for a bank that does not turn up exactly once, or
/// [`GenRomProfileError::WrongPackEntryKind`] for an id that is not a
/// palette.
pub fn locate_unique(
    ctx: &Context<'_>,
    ids: &[String],
    expect: &dyn Fn(&str) -> SymbolExpectation,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<PalettePlan>, GenRomProfileError> {
    let mut needles = Vec::with_capacity(ids.len());
    for id in ids {
        needles.push(ctx.pack.get(id)?.payload.clone());
    }
    let hits = ctx.raw.find_all(&needles);

    let mut plans = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let asset = ctx.pack.get(id)?;
        let addr = exactly_one(id, &hits[index])?;
        let mut line = ReportLine::unique(id, addr, len32(&asset.payload));
        line.symbol = expect(id);
        report.push(line);
        plans.push(PalettePlan {
            id: id.clone(),
            addr,
            color_count: asset.palette_colors(id)?,
        });
    }
    Ok(plans)
}
