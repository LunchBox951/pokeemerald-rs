//! Locating the evolution scene's roots.
//!
//! The expectation is the checkout's fixed scene inventory, which extraction
//! also reads ([`crate::extract::scope::evolution_scene_ids`]): two images,
//! three palettes, and two background tilemaps. Every root is found from the
//! pack payload alone.
//!
//! How each is stored is read from upstream's C (`src/evolution_scene.c:71-78`,
//! `src/evolution_graphics.c:41-42`): the images are `.4bpp.lz` and the
//! tilemaps `.lz`, both LZ77 streams, while the palettes are raw `.gbapal`
//! runs. Every one is a `static` array, so a linker map cannot confirm its
//! symbol. Both images are 4-bit in the ROM and in the pack, so the located
//! depth needs no split.

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::locate::exactly_one;
use super::palettes::locate_unique;
use super::plan::{BlobPlan, EvolutionScenePlan, ReportLine, SymbolExpectation};
use super::tilesets::len32;
use super::Context;

const IMAGE_PREFIX: &str = "evolution-scene/image/";
const PALETTE_PREFIX: &str = "evolution-scene/palette/";
const TILEMAP_PREFIX: &str = "evolution-scene/raw/";

/// The fixed ids under `prefix`, in id order.
fn ids_under(prefix: &str) -> Vec<String> {
    let mut ids: Vec<String> = crate::extract::scope::evolution_scene_ids()
        .into_iter()
        .filter(|id| id.starts_with(prefix))
        .collect();
    ids.sort();
    ids
}

/// The `static` symbol upstream stores a palette or tilemap id in.
fn local_symbol(id: &str) -> Option<&'static str> {
    match id {
        "evolution-scene/palette/bg_anim_intro" => Some("sBgAnim_Intro_Pal"),
        "evolution-scene/palette/bg_anim" => Some("sBgAnim_Pal"),
        "evolution-scene/palette/evo_sparkle" => Some("sEvoSparkle_Pal"),
        "evolution-scene/raw/bg_inner" => Some("sBgAnim_Inner_Tilemap"),
        "evolution-scene/raw/bg_outer" => Some("sBgAnim_Outer_Tilemap"),
        _ => None,
    }
}

/// Locate every evolution-scene root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for a surplus
/// `evolution-scene/` pack entry, [`GenRomProfileError::WrongPackEntryKind`]
/// or [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`]
/// for a root the ROM does not hold as expected.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<EvolutionScenePlan, GenRomProfileError> {
    super::inventory::require_evolution_scene(ctx.pack)?;
    let queries: Vec<ImageQuery> = ids_under(IMAGE_PREFIX)
        .into_iter()
        .map(ImageQuery::lz77)
        .collect();
    Ok(EvolutionScenePlan {
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
        &|id| match local_symbol(id) {
            Some(name) => SymbolExpectation::Local(name.to_owned()),
            None => SymbolExpectation::Unnamed,
        },
        report,
    )?;
    plans.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(plans)
}

/// Locate each `evolution-scene/raw/*` tilemap as the LZ77 stream it is.
fn locate_tilemaps(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<BlobPlan>, GenRomProfileError> {
    let ids = ids_under(TILEMAP_PREFIX);
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        needles.push(ctx.pack.raw_payload(id)?.to_vec());
    }
    let hits = ctx.lz77.find_all(&needles);

    let mut plans = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let addr = exactly_one(id, &hits[index])?;
        let len = len32(&needles[index]);
        let mut line = ReportLine::unique(id, addr, len);
        if let Some(name) = local_symbol(id) {
            line = line.local_symbol(name);
        }
        report.push(line);
        plans.push(BlobPlan {
            id: id.clone(),
            addr,
            encoding: Encoding::Lz77,
            len,
        });
    }
    Ok(plans)
}

#[cfg(test)]
#[path = "evolution_scene_tests.rs"]
mod tests;
