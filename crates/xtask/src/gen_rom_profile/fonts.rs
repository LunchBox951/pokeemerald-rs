//! Locates the five Latin glyph sheets and the keypad-icon sheet.
//!
//! `gbagfx` writes a glyph sheet in `.latfont` layout (64 bytes per 16x16
//! glyph), stored verbatim in the ROM.
//! [`super::pack_source::latin_font_bytes`] rebuilds it from the pack raster
//! as a 32 KiB needle. All five shipped sheets are required.

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::inventory;
use super::locate::{camel_case, exactly_one};
use super::pack_source::latin_font_bytes;
use super::plan::{FontPlan, ImagePlan, ReportLine};
use super::tilesets::len32;
use super::Context;

/// Locate every Latin glyph sheet the pack holds.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first of the five
/// shipped sheets the pack lacks, [`GenRomProfileError::EntryShape`] if a
/// sheet is not the 256x512 2bpp shape the layout assumes or holds a pixel
/// index outside `0..=3`, or [`GenRomProfileError::NotFound`] / [`GenRomProfileError::Ambiguous`] if
/// one does not turn up exactly once.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<FontPlan>, GenRomProfileError> {
    inventory::require_fonts(ctx.pack)?;
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

/// Pack id of the keypad-icon sheet.
const KEYPAD_ICONS_ID: &str = "font/keypad_icons";

/// Locate the keypad-icon sheet, which `text.c` includes as plain row-major
/// `.4bpp` tiles (`sKeypadIconTiles`, `text.c:117`).
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] if the pack lacks the sheet,
/// [`GenRomProfileError::EntryShape`] if it is not a raster of whole tiles,
/// or [`GenRomProfileError::NotFound`] / [`GenRomProfileError::Ambiguous`]
/// if it does not turn up exactly once.
pub fn locate_keypad_icons(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<ImagePlan>, GenRomProfileError> {
    locate_images(ctx, &[ImageQuery::raw(KEYPAD_ICONS_ID)], report)
}

#[cfg(test)]
mod tests;
