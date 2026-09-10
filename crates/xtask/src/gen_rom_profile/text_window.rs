//! Locating the text-window frames, the message box, and their palettes.
//!
//! The 21 tile sheets and 24 of the 25 palettes are unique, so they pin
//! themselves. The message box's palette does not: upstream stores it
//! twice, once as `gMessageBox_Pal` beside the message box's own tiles and
//! once as the first bank of `sTextWindowPalettes`. Both hold the same
//! colours, so nothing in the bytes chooses; adjacency does. The pack id
//! names the message box, so the generator records the copy that sits
//! immediately before `gMessageBox_Gfx` -- which is unique -- and records
//! `sTextWindowPalettes` separately, derived from the four `text_pal*`
//! banks that follow its first entry.

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::locate::slice_at_addr;
use super::palettes::locate_unique;
use super::plan::{PalettePlan, ReportLine, Resolution, SymbolExpectation, TextWindowPlan};
use super::tilesets::len32;
use super::Context;

/// The pack id whose palette the ROM stores twice.
const DUPLICATED_PALETTE: &str = "text-window/palette/message_box";
/// The image whose tiles the duplicated palette sits beside.
const ADJACENT_IMAGE: &str = "text-window/image/message_box";
/// The first `text_pal*` bank, which follows `sTextWindowPalettes[0]`.
const FIRST_WINDOW_PALETTE: &str = "text-window/palette/text_pal1";

/// Locate the whole text-window domain.
///
/// # Errors
///
/// Any [`GenRomProfileError`] a locator raises.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<TextWindowPlan, GenRomProfileError> {
    let queries: Vec<ImageQuery> = ctx
        .pack
        .ids_with_prefix("text-window/image/")
        .into_iter()
        .map(ImageQuery::raw)
        .collect();
    let images = locate_images(ctx, &queries, report)?;

    let ids: Vec<String> = ctx
        .pack
        .ids_with_prefix("text-window/palette/")
        .into_iter()
        .filter(|id| id != DUPLICATED_PALETTE)
        .collect();
    let mut palettes = locate_unique(ctx, &ids, &palette_symbol, report)?;

    let message_box_gfx = images
        .iter()
        .find(|plan| plan.id == ADJACENT_IMAGE)
        .ok_or_else(|| GenRomProfileError::MissingPackEntry(ADJACENT_IMAGE.to_owned()))?
        .addr;
    let asset = ctx.pack.get(DUPLICATED_PALETTE)?;
    let bank_bytes = len32(&asset.payload);
    let addr = message_box_gfx
        .checked_sub(bank_bytes)
        .filter(|&addr| slice_at_addr(ctx.rom, addr, asset.payload.len()) == Some(&asset.payload))
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: DUPLICATED_PALETTE.to_owned(),
            reason: format!("no copy of it sits immediately before {message_box_gfx:08X}"),
        })?;
    report.push(
        ReportLine::unique(DUPLICATED_PALETTE, addr, bank_bytes)
            .with(Resolution::StructDerived)
            .symbol("gMessageBox_Pal")
            .note("stored twice; took the copy beside gMessageBox_Gfx"),
    );
    palettes.push(PalettePlan {
        id: DUPLICATED_PALETTE.to_owned(),
        addr,
        color_count: asset.palette_colors(DUPLICATED_PALETTE)?,
    });
    palettes.sort_by(|a, b| a.id.cmp(&b.id));

    let window_palettes = locate_window_palettes(ctx, &palettes, bank_bytes, &asset.payload)?;
    report.push(
        ReportLine::unique("sTextWindowPalettes", window_palettes, bank_bytes * 5)
            .with(Resolution::StructDerived)
            .symbol("sTextWindowPalettes")
            .note("one bank before the first text_pal bank"),
    );

    Ok(TextWindowPlan {
        images,
        palettes,
        window_palettes,
    })
}

/// What a linker map should say at one text-window palette's address.
///
/// The four `text_pal*` banks are entries 1..4 of `sTextWindowPalettes`,
/// so only the array itself is a symbol.
fn palette_symbol(id: &str) -> SymbolExpectation {
    let Some(stem) = id.strip_prefix("text-window/palette/") else {
        return SymbolExpectation::Unnamed;
    };
    if stem.starts_with("text_pal") {
        return SymbolExpectation::Interior;
    }
    SymbolExpectation::Contains(vec![format!("TextWindowFrame{stem}_Pal")])
}

/// Derive `sTextWindowPalettes` from the bank that follows its first entry.
fn locate_window_palettes(
    ctx: &Context<'_>,
    palettes: &[PalettePlan],
    bank_bytes: u32,
    message_box: &[u8],
) -> Result<u32, GenRomProfileError> {
    let first = palettes
        .iter()
        .find(|plan| plan.id == FIRST_WINDOW_PALETTE)
        .ok_or_else(|| GenRomProfileError::MissingPackEntry(FIRST_WINDOW_PALETTE.to_owned()))?
        .addr;
    first
        .checked_sub(bank_bytes)
        .filter(|&addr| slice_at_addr(ctx.rom, addr, message_box.len()) == Some(message_box))
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: "sTextWindowPalettes".to_owned(),
            reason: format!("the bank before {first:08X} is not the message box's"),
        })
}
