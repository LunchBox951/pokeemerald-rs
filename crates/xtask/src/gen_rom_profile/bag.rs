//! Locating the ordinary bag's roots.
//!
//! The expectation is the checkout's own bag inventory, which extraction
//! also reads ([`crate::extract::scope::bag_ids`]): the fixed menu and bag
//! assets plus every deduplicated item icon. Every root is found from the
//! pack payload alone. No item-to-icon mapping is kept here, because a root
//! is an image, palette, or blob keyed by file stem, never by item.
//!
//! How a root is stored is read from upstream's C: the rotating ball, the
//! select button, the HM mark, and the rotating ball's palette are linked
//! uncompressed; everything else is an LZ77 stream. Two item-icon images
//! are byte-identical to another and sit in the ROM twice; those are
//! reported as identical copies, which is sound because the importer reads
//! bytes.

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::lz77_blobs::{locate_lz77_blobs, unnamed};
use super::palettes::locate_unique_as;
use super::plan::{BagPlan, BlobPlan, PalettePlan, ReportLine, SymbolExpectation};
use super::Context;

const PREFIX: &str = "bag/";
const IMAGE_PREFIXES: [&str; 2] = ["bag/image/", "bag/item-icon/image/"];
const PALETTE_PREFIXES: [&str; 2] = ["bag/palette/", "bag/item-icon/palette/"];
const RAW_PREFIX: &str = "bag/raw/";

/// Roots linked uncompressed; every other bag root is an LZ77 stream.
/// `item_menu_icons.c:36-37`, `item_menu.c:374`, `graphics.c:1269`.
const UNCOMPRESSED: [&str; 4] = [
    "bag/image/rotating_ball",
    "bag/image/select_button",
    "bag/image/hm",
    "bag/palette/rotating_ball",
];

fn encoding_of(id: &str) -> Encoding {
    if UNCOMPRESSED.contains(&id) {
        Encoding::Raw
    } else {
        Encoding::Lz77
    }
}

/// The expected ids under any of `prefixes`, in id order.
fn ids_under(expected: &[String], prefixes: &[&str]) -> Vec<String> {
    let mut ids: Vec<String> = expected
        .iter()
        .filter(|id| prefixes.iter().any(|prefix| id.starts_with(prefix)))
        .cloned()
        .collect();
    ids.sort();
    ids
}

/// Locate every bag root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for an expected id outside
/// the bag families, [`GenRomProfileError::WrongPackEntryKind`] or
/// [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`]
/// for a root the ROM does not hold as expected.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<BagPlan, GenRomProfileError> {
    locate_expected(ctx, &ctx.bag_ids, report)
}

/// [`locate`] against an explicit expectation.
fn locate_expected(
    ctx: &Context<'_>,
    expected: &[String],
    report: &mut Vec<ReportLine>,
) -> Result<BagPlan, GenRomProfileError> {
    super::inventory::require_bag(ctx.pack, expected)?;
    let images = ids_under(expected, &IMAGE_PREFIXES);
    let palettes = ids_under(expected, &PALETTE_PREFIXES);
    let raws = ids_under(expected, &[RAW_PREFIX]);
    if let Some(stray) = expected.iter().find(|id| {
        !id.starts_with(PREFIX)
            || ![&images, &palettes, &raws]
                .iter()
                .any(|family| family.contains(id))
    }) {
        return Err(GenRomProfileError::StructMismatch {
            id: stray.clone(),
            reason: "is not a bag image, palette, or raw id".to_owned(),
        });
    }

    let queries: Vec<ImageQuery> = images
        .into_iter()
        .map(|id| ImageQuery {
            encoding: encoding_of(&id),
            id,
        })
        .collect();
    Ok(BagPlan {
        images: locate_images(ctx, &queries, report)?,
        palettes: locate_palettes(ctx, &palettes, report)?,
        raws: locate_raws(ctx, &raws, report)?,
    })
}

/// What a linker map should say at one bag palette's address.
///
/// `sRotatingBall_Pal` is `static` (`item_menu_icons.c:36`), so a map omits it.
fn palette_symbol(id: &str) -> SymbolExpectation {
    if id == "bag/palette/rotating_ball" {
        SymbolExpectation::Local("sRotatingBall_Pal".to_owned())
    } else {
        SymbolExpectation::Unnamed
    }
}

/// Locate each palette under the encoding its family stores it in.
fn locate_palettes(
    ctx: &Context<'_>,
    ids: &[String],
    report: &mut Vec<ReportLine>,
) -> Result<Vec<PalettePlan>, GenRomProfileError> {
    let mut plans = Vec::with_capacity(ids.len());
    for encoding in [Encoding::Raw, Encoding::Lz77] {
        let group: Vec<String> = ids
            .iter()
            .filter(|id| encoding_of(id) == encoding)
            .cloned()
            .collect();
        plans.extend(locate_unique_as(
            ctx,
            &group,
            encoding,
            &palette_symbol,
            report,
        )?);
    }
    plans.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(plans)
}

/// Locate each `bag/raw/*` blob as the LZ77 stream it is.
fn locate_raws(
    ctx: &Context<'_>,
    ids: &[String],
    report: &mut Vec<ReportLine>,
) -> Result<Vec<BlobPlan>, GenRomProfileError> {
    locate_lz77_blobs(ctx, ids, &unnamed, report)
}

#[cfg(test)]
#[path = "bag_tests.rs"]
mod tests;
