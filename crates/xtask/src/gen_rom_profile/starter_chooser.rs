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
//!
//! The pokeball-selection and starter-circle sheets are named by the
//! `CompressedSpriteSheet` records upstream loads them through
//! (`sSpriteSheet_PokeballSelect`, `sSpriteSheet_StarterCircle`,
//! `starter_choose.c:284-302`, loaded at `:420-422`): `{data, size, tag}`. A
//! copy of a sheet qualifies only when a pointer to it sits in a record whose
//! own size and tag match, and exactly one copy must qualify, so an identical
//! stray copy at a lower address cannot displace the one the record names. The
//! bag/grass sheet has no record and is found from its pixels alone. Palettes
//! and tilemaps are unchanged.
//!
//! Linkage is read from the same file. A linker map lists only global
//! symbols, so a root upstream declares `static` is marked
//! [`SymbolExpectation::Local`] ([`local_symbol`]) and the rest keep the
//! any-symbol check.

use super::error::GenRomProfileError;
use super::images::{locate_image_copies, locate_images, ImageQuery};
use super::locate::{only_one_matching, u16_at_addr};
use super::lz77_blobs::{locate_lz77_blobs, unnamed};
use super::palettes::locate_unique;
use super::plan::{BlobPlan, ImagePlan, ReportLine, StarterChooserPlan, SymbolExpectation};
use super::Context;

const IMAGE_PREFIX: &str = "starter-chooser/image/";
const PALETTE_PREFIX: &str = "starter-chooser/palette/";
const TILEMAP_PREFIX: &str = "starter-chooser/raw/";

/// A `CompressedSpriteSheet` is `{data: u32, size: u16, tag: u16}`.
const SHEET_SIZE_FIELD: u32 = 4;
const SHEET_TAG_FIELD: u32 = 6;
/// Both chooser sheets are declared `.size = 0x0800` (`starter_choose.c:289,299`).
const SHEET_SIZE: u16 = 0x0800;
/// `TAG_POKEBALL_SELECT` and `TAG_STARTER_CIRCLE` (`starter_choose.c:33-34`).
const POKEBALL_SELECT_TAG: u16 = 0x1000;
const STARTER_CIRCLE_TAG: u16 = 0x1001;

/// The tag in the sprite-sheet record that names the sheet `id`, or `None`
/// for a sheet no record names (the bag/grass tiles, loaded as a plain
/// `LoadBgTiles` source).
const fn sheet_tag(id: &str) -> Option<u16> {
    match id.as_bytes() {
        b"starter-chooser/image/pokeball_selection" => Some(POKEBALL_SELECT_TAG),
        b"starter-chooser/image/starter_circle" => Some(STARTER_CIRCLE_TAG),
        _ => None,
    }
}

/// Whether the record at `record` carries the chooser's size and `tag`.
fn is_sheet_record(ctx: &Context<'_>, record: u32, tag: u16) -> bool {
    u16_at_addr(ctx.rom, record + SHEET_SIZE_FIELD) == Some(SHEET_SIZE)
        && u16_at_addr(ctx.rom, record + SHEET_TAG_FIELD) == Some(tag)
}

/// Locate the sheet `id` through the record that names it.
fn locate_named_sheet(
    ctx: &Context<'_>,
    id: &str,
    tag: u16,
    report: &mut Vec<ReportLine>,
) -> Result<ImagePlan, GenRomProfileError> {
    let mut found = locate_image_copies(ctx, &[ImageQuery::lz77(id)])?.into_iter();
    let Some(copies) = found.next() else {
        unreachable!("one set of copies comes back per query");
    };
    let addr = only_one_matching(
        id,
        "a {data, size, tag} sprite-sheet record pointing at the copy",
        copies.addrs(),
        |&copy| {
            ctx.pointers
                .refs_to(copy)
                .iter()
                .any(|&record| is_sheet_record(ctx, record, tag))
        },
    )?;
    copies.plan_at(ctx, addr, report)
}

/// Locate each chooser sheet: through its record where upstream has one.
fn locate_sheets(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<ImagePlan>, GenRomProfileError> {
    let mut plans = Vec::new();
    for id in ids_under(IMAGE_PREFIX) {
        if let Some(tag) = sheet_tag(&id) {
            plans.push(locate_named_sheet(ctx, &id, tag, report)?);
        } else {
            plans.extend(locate_images(ctx, &[ImageQuery::lz77(id)], report)?);
        }
    }
    Ok(plans)
}

/// The fixed ids under `prefix`, in id order.
fn ids_under(prefix: &str) -> Vec<String> {
    let mut ids: Vec<String> = crate::extract::scope::starter_chooser_ids()
        .into_iter()
        .filter(|id| id.starts_with(prefix))
        .collect();
    ids.sort();
    ids
}

/// The upstream name of a chooser root declared `static` (so a linker map
/// cannot list it), or `None` for a global root.
///
/// `starter_choose.c:55-56` makes the pokeball-selection and starter-circle
/// palettes `static`, and `:61` makes the starter-circle sheet `static`. The
/// bag/grass palette and sheet (`:54,59`), the pokeball-selection sheet
/// (`:60`) and both tilemaps (`:57-58`) are global.
fn local_symbol(id: &str) -> Option<&'static str> {
    match id {
        "starter-chooser/palette/pokeball_selection" => Some("sPokeballSelection_Pal"),
        "starter-chooser/palette/starter_circle" => Some("sStarterCircle_Pal"),
        "starter-chooser/image/starter_circle" => Some("sStarterCircle_Gfx"),
        _ => None,
    }
}

/// Locate every starter-chooser root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for a surplus
/// `starter-chooser/` pack entry or a sheet that no sprite-sheet record (or
/// more than one copy's record) names, [`GenRomProfileError::WrongPackEntryKind`]
/// or [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`]
/// for a root the ROM does not hold as expected.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<StarterChooserPlan, GenRomProfileError> {
    super::inventory::require_starter_chooser(ctx.pack)?;
    let first = report.len();
    let plan = StarterChooserPlan {
        images: locate_sheets(ctx, report)?,
        palettes: locate_palettes(ctx, report)?,
        tilemaps: locate_tilemaps(ctx, report)?,
    };
    super::mark_local_roots(&mut report[first..], &local_symbol);
    Ok(plan)
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
