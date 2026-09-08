//! Locating the five Latin glyph sheets.
//!
//! A glyph sheet is neither a raster nor a plain tile sheet: `gbagfx`
//! writes it in its own `.latfont` layout, 64 bytes per 16x16 glyph, and
//! the ROM stores that verbatim. [`super::pack_source::latin_font_bytes`]
//! reproduces the layout from the pack's raster, which makes the sheet a
//! 32 KiB signature -- as unmistakable as a root gets.

use super::error::GenRomProfileError;
use super::locate::{camel_case, exactly_one};
use super::pack_source::latin_font_bytes;
use super::plan::{FontPlan, ReportLine};
use super::tilesets::len32;
use super::Context;

/// Locate every Latin glyph sheet the pack holds.
///
/// # Errors
///
/// [`GenRomProfileError::EntryShape`] if a sheet is not the 256x512 2bpp
/// shape the layout assumes, or [`GenRomProfileError::NotFound`] /
/// [`GenRomProfileError::Ambiguous`] if one does not turn up exactly once.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<FontPlan>, GenRomProfileError> {
    let ids: Vec<String> = ctx
        .pack
        .ids_with_prefix("font/")
        .into_iter()
        .filter(|id| id.ends_with("/glyphs"))
        .collect();
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        needles.push(latin_font_bytes(ctx.pack, id)?);
    }
    let hits = ctx.raw.find_all(&needles);

    let mut plans = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let addr = exactly_one(id, &hits[index])?;
        let (width, height, bit_depth) = ctx.pack.get(id)?.image_shape(id)?;
        let len = len32(&needles[index]);
        let upstream = camel_case(id.trim_start_matches("font/").trim_end_matches("/glyphs"));
        report
            .push(ReportLine::unique(id, addr, len).symbol(format!("gFont{upstream}LatinGlyphs")));
        plans.push(FontPlan {
            id: id.clone(),
            addr,
            len,
            width,
            height,
            bit_depth,
        });
    }
    Ok(plans)
}
