//! Locating the text-window frames, the message box, and their palettes.
//!
//! The 21 tile sheets and 24 of the 25 palettes are unique, so they pin
//! themselves. The message box's palette does not: upstream stores it
//! twice, once as `gMessageBox_Pal` beside the message box's own tiles and
//! once as the first bank of `sTextWindowPalettes`. Both hold the same
//! colours, so nothing in the bytes chooses; adjacency does. The pack id
//! names the message box, so the generator records the copy that sits
//! immediately before `gMessageBox_Gfx` -- which is unique -- and records
//! `sTextWindowPalettes` separately, derived from the first `text_pal*`
//! bank and then required to hold all four in order.

use rom_import::Encoding;

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
/// The pack ids of `sTextWindowPalettes[1..=4]`, in table order. The first
/// follows `sTextWindowPalettes[0]`.
const WINDOW_BANKS: [&str; 4] = [
    "text-window/palette/text_pal1",
    "text-window/palette/text_pal2",
    "text-window/palette/text_pal3",
    "text-window/palette/text_pal4",
];

/// Locate the whole text-window domain.
///
/// # Errors
///
/// [`GenRomProfileError::MissingPackEntry`] naming the first fixed frame,
/// message box, or palette the pack lacks, or any other
/// [`GenRomProfileError`] a locator raises.
pub fn locate(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<TextWindowPlan, GenRomProfileError> {
    super::inventory::require_text_window(ctx.pack)?;
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
        encoding: Encoding::Raw,
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

/// Derive `sTextWindowPalettes` from the bank that follows its first entry,
/// then require every `text_pal*` bank at its indexed place in the table.
fn locate_window_palettes(
    ctx: &Context<'_>,
    palettes: &[PalettePlan],
    bank_bytes: u32,
    message_box: &[u8],
) -> Result<u32, GenRomProfileError> {
    let bank_addr = |id: &str| {
        palettes
            .iter()
            .find(|plan| plan.id == id)
            .map(|plan| plan.addr)
            .ok_or_else(|| GenRomProfileError::MissingPackEntry(id.to_owned()))
    };
    let first = bank_addr(WINDOW_BANKS[0])?;
    let table = first
        .checked_sub(bank_bytes)
        .filter(|&addr| slice_at_addr(ctx.rom, addr, message_box.len()) == Some(message_box))
        .ok_or_else(|| GenRomProfileError::StructMismatch {
            id: "sTextWindowPalettes".to_owned(),
            reason: format!("the bank before {first:08X} is not the message box's"),
        })?;
    for (index, id) in (1u32..).zip(WINDOW_BANKS) {
        let expected = table.checked_add(index * bank_bytes);
        let found = bank_addr(id)?;
        if expected != Some(found) {
            return Err(GenRomProfileError::StructMismatch {
                id: id.to_owned(),
                reason: format!(
                    "sTextWindowPalettes[{index}] is not at {found:08X}; the table at {table:08X} \
                     expects it at bank {index}"
                ),
            });
        }
    }
    Ok(table)
}

#[cfg(test)]
mod tests;
