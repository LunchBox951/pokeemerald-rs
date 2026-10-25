//! Locating the starter chooser's roots.
//!
//! The expectation is the checkout's fixed chooser inventory, which
//! extraction also reads ([`crate::extract::scope::starter_chooser_ids`]):
//! three sheets, their palettes, and two Birch background tilemaps. Every
//! root is found from the pack payload alone.
//!
//! How each is stored is read from upstream's C (`src/starter_choose.c`):
//! the sheets are `.4bpp.lz` and the tilemaps `.lz`, both LZ77 streams, while
//! the palettes are raw `.gbapal` runs. The chooser tiles are 4-bit in the
//! ROM and in the pack, so the located depth needs no split.

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::lz77_blobs::{locate_lz77_blobs, unnamed};
use super::palettes::locate_unique;
use super::plan::{BlobPlan, ReportLine, StarterChooserPlan, SymbolExpectation};
use super::Context;

const IMAGE_PREFIX: &str = "starter-chooser/image/";
const PALETTE_PREFIX: &str = "starter-chooser/palette/";
const TILEMAP_PREFIX: &str = "starter-chooser/raw/";

/// The fixed ids under `prefix`, in id order.
fn ids_under(prefix: &str) -> Vec<String> {
    let mut ids: Vec<String> = crate::extract::scope::starter_chooser_ids()
        .into_iter()
        .filter(|id| id.starts_with(prefix))
        .collect();
    ids.sort();
    ids
}

/// Locate every starter-chooser root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for a surplus
/// `starter-chooser/` pack entry, [`GenRomProfileError::WrongPackEntryKind`]
/// or [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`]
/// for a root the ROM does not hold as expected.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<StarterChooserPlan, GenRomProfileError> {
    super::inventory::require_starter_chooser(ctx.pack)?;
    let queries: Vec<ImageQuery> = ids_under(IMAGE_PREFIX)
        .into_iter()
        .map(ImageQuery::lz77)
        .collect();
    Ok(StarterChooserPlan {
        images: locate_images(ctx, &queries, report)?,
        palettes: locate_palettes(ctx, report)?,
        tilemaps: locate_tilemaps(ctx, report)?,
    })
}

/// Locate each palette as the raw `.gbapal` run it is.
fn locate_palettes(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<super::plan::PalettePlan>, GenRomProfileError> {
    let mut plans = locate_unique(
        ctx,
        &ids_under(PALETTE_PREFIX),
        &|_| SymbolExpectation::Unnamed,
        report,
    )?;
    plans.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(plans)
}

/// Locate each `starter-chooser/raw/*` tilemap as the LZ77 stream it is.
fn locate_tilemaps(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<BlobPlan>, GenRomProfileError> {
    locate_lz77_blobs(ctx, &ids_under(TILEMAP_PREFIX), &unnamed, report)
}

#[cfg(test)]
#[path = "starter_chooser_tests.rs"]
mod tests;
