//! Locating Birch's speech scene and the wall clock.
//!
//! The expectation is the checkout's fixed scene inventory, which
//! extraction also reads ([`crate::extract::scope::opening_scene_ids`]):
//! five sheets, eight palettes, and three screen tilemaps. Every root is found
//! from the pack payload alone.
//!
//! How each is stored is read from upstream's C: Birch's sprite sheet is
//! linked uncompressed (`src/field_effect.c:245`) and every other sheet is a
//! `.4bpp.lz` stream; Lotad's normal palette is a `.gbapal.lz` stream
//! (`src/data/graphics/pokemon.h:1889`) and the other palettes are raw
//! `.gbapal` runs; the tilemaps are `.lz` streams. The 16-colour banks are
//! all 4-bit in the ROM and in the pack, so no depth split is needed.
//!
//! Linkage is read from the same files. A linker map lists only global
//! symbols, so a root upstream declares `static` is marked
//! [`super::plan::SymbolExpectation::Local`] ([`local_symbol`]) and the rest keep the
//! any-symbol check.

use std::collections::BTreeMap;

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::locate::{exactly_one, to_addr};
use super::lz77_blobs::{locate_lz77_blobs, unnamed};
use super::plan::{BlobPlan, OpeningScenePlan, PalettePlan, ReportLine, Resolution};
use super::tilesets::len32;
use super::Context;

const IMAGE_PREFIX: &str = "opening-scene/image/";
const PALETTE_PREFIX: &str = "opening-scene/palette/";
const TILEMAP_PREFIX: &str = "opening-scene/raw/";

/// The only sheet linked uncompressed.
const UNCOMPRESSED_IMAGE: &str = "opening-scene/image/birch";
/// The only palette linked compressed.
const COMPRESSED_PALETTE: &str = "opening-scene/palette/lotad";

/// The fixed ids under `prefix`, in id order.
fn ids_under(prefix: &str) -> Vec<String> {
    let mut ids: Vec<String> = crate::extract::scope::opening_scene_ids()
        .into_iter()
        .filter(|id| id.starts_with(prefix))
        .collect();
    ids.sort();
    ids
}

/// The upstream name of a scene root declared `static` (so a linker map
/// cannot list it), or `None` for a global root.
///
/// Static: `main_menu.c:250-257` (`sBirchSpeechBgPals`, whose rows are `bg0`
/// and `bg1`, then the shadow sheet, the speech tilemap and the `bg2`
/// gradient), `field_effect.c:245,247` (Birch's sheet and palette) and
/// `wallclock.c:74-75` (the hand sheet and the text-prompt palette). Global:
/// `pokemon.h:1888-1889` (Lotad's sheet and palette) and `graphics.c:1350-1354`
/// (the clock sheet, the male and female palettes, both tilemaps).
fn local_symbol(id: &str) -> Option<&'static str> {
    match id {
        "opening-scene/image/shadow" => Some("sBirchSpeechShadowGfx"),
        "opening-scene/image/birch" => Some("sNewGameBirch_Gfx"),
        "opening-scene/image/clock_hand" => Some("sHand_Gfx"),
        "opening-scene/palette/bg0" | "opening-scene/palette/bg1" => Some("sBirchSpeechBgPals"),
        "opening-scene/palette/bg2" => Some("sBirchSpeechBgGradientPal"),
        "opening-scene/palette/birch" => Some("sNewGameBirch_Pal"),
        "opening-scene/palette/clock_text_prompt" => Some("sTextPrompt_Pal"),
        "opening-scene/raw/birch_speech_map" => Some("sBirchSpeechBgMap"),
        _ => None,
    }
}

/// Locate every opening-scene root the checkout expects.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] for an expected entry the pack
/// lacks, [`GenRomProfileError::StructMismatch`] for a surplus
/// `opening-scene/` pack entry, [`GenRomProfileError::WrongPackEntryKind`]
/// or [`GenRomProfileError::EntryShape`] for a malformed entry, and
/// [`GenRomProfileError::NotFound`] or [`GenRomProfileError::Ambiguous`]
/// for a root the ROM does not hold as expected.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<OpeningScenePlan, GenRomProfileError> {
    super::inventory::require_opening_scene(ctx.pack)?;
    let queries: Vec<ImageQuery> = ids_under(IMAGE_PREFIX)
        .into_iter()
        .map(|id| {
            if id == UNCOMPRESSED_IMAGE {
                ImageQuery::raw(id)
            } else {
                ImageQuery::lz77(id)
            }
        })
        .collect();
    let first = report.len();
    let plan = OpeningScenePlan {
        images: locate_images(ctx, &queries, report)?,
        palettes: locate_palettes(ctx, report)?,
        tilemaps: locate_tilemaps(ctx, report)?,
    };
    super::mark_local_roots(&mut report[first..], &local_symbol);
    Ok(plan)
}

/// Locate each palette under the encoding its symbol is linked with.
///
/// Palettes with identical colours share their hits: `bg0.pal` and `bg1.pal`
/// are byte-identical rows of `sBirchSpeechBgPals`, so they pair with the
/// copies in address order, as [`super::images`] pairs identical sheets.
fn locate_palettes(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<PalettePlan>, GenRomProfileError> {
    let (compressed, raw): (Vec<String>, Vec<String>) = ids_under(PALETTE_PREFIX)
        .into_iter()
        .partition(|id| id == COMPRESSED_PALETTE);
    let mut plans = Vec::with_capacity(raw.len() + compressed.len());
    for (encoding, ids) in [(Encoding::Raw, raw), (Encoding::Lz77, compressed)] {
        plans.extend(locate_encoded_palettes(ctx, &ids, encoding, report)?);
    }
    plans.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(plans)
}

fn locate_encoded_palettes(
    ctx: &Context<'_>,
    ids: &[String],
    encoding: Encoding,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<PalettePlan>, GenRomProfileError> {
    let mut needles = Vec::with_capacity(ids.len());
    for id in ids {
        needles.push(ctx.pack.get(id)?.palette_payload(id)?.0.to_vec());
    }
    let hits = match encoding {
        Encoding::Raw => ctx.raw.find_all(&needles),
        Encoding::Lz77 => ctx.lz77.find_all(&needles),
    };

    // Ids with the same address list hold the same colours.
    let mut groups: BTreeMap<&[u32], Vec<usize>> = BTreeMap::new();
    for (index, found) in hits.iter().enumerate() {
        groups.entry(found).or_default().push(index);
    }
    let mut plans = Vec::with_capacity(ids.len());
    for (addrs, members) in groups {
        let id = &ids[members[0]];
        if addrs.is_empty() {
            return Err(GenRomProfileError::NotFound { id: id.clone() });
        }
        if addrs.len() != members.len() {
            // Fewer copies than ids, or a spare copy nothing distinguishes.
            return Err(GenRomProfileError::Ambiguous {
                id: id.clone(),
                addrs: addrs.iter().copied().map(to_addr).collect(),
            });
        }
        for (slot, &index) in members.iter().enumerate() {
            let id = &ids[index];
            let (payload, color_count) = ctx.pack.get(id)?.palette_payload(id)?;
            let addr = exactly_one(id, &addrs[slot..=slot])?;
            let mut line = ReportLine::unique(id, addr, len32(payload));
            if members.len() > 1 {
                line = line.with(Resolution::ArbitraryAmongIdentical).note(format!(
                    "the colours repeat at {} addresses; nothing distinguishes them, so this \
                     is the one at index {slot} in address order",
                    members.len()
                ));
            }
            report.push(line);
            plans.push(PalettePlan {
                id: id.clone(),
                addr,
                color_count,
                encoding,
            });
        }
    }
    Ok(plans)
}

/// Locate each `opening-scene/raw/*` tilemap as the LZ77 stream it is.
fn locate_tilemaps(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<BlobPlan>, GenRomProfileError> {
    locate_lz77_blobs(ctx, &ids_under(TILEMAP_PREFIX), &unnamed, report)
}

#[cfg(test)]
#[path = "opening_scene_tests.rs"]
mod tests;
