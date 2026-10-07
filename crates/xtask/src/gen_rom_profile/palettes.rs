//! Locating palette banks.
//!
//! A pack palette payload is the ROM's own BGR555 bytes. A 16-colour bank is
//! short enough to repeat, so a bank found twice is an error here; callers
//! with a structural tie-break (a `{data, tag}` table, adjacency to a unique
//! root) resolve it themselves.

use super::error::GenRomProfileError;
use super::locate::exactly_one;
use super::plan::{PalettePlan, ReportLine, SymbolExpectation};
use super::tilesets::len32;
use super::Context;

/// Locate every palette in `ids`, requiring each to be unique.
///
/// `expect` gives the linker-map symbol expected at each id's address; the
/// caller owns the naming.
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
