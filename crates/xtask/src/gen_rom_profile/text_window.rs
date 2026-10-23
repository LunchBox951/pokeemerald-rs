//! Locates the text-window frames, the message box, and their palettes.
//!
//! The 21 tile sheets and 24 of 25 palettes are unique. The message box
//! palette is stored twice, as `gMessageBox_Pal` and as bank 0 of
//! `sTextWindowPalettes`, with identical bytes. The copy immediately before
//! the unique `gMessageBox_Gfx` is recorded for it; `sTextWindowPalettes` is
//! derived from `text_pal1` and must hold all four banks in order.

use rom_import::Encoding;

use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::locate::slice_at_addr;
use super::palettes::locate_unique;
use super::plan::{PalettePlan, ReportLine, Resolution, SymbolExpectation, TextWindowPlan};
use super::tilesets::len32;
use super::Context;

/// Pack id of the palette stored twice in the ROM.
const DUPLICATED_PALETTE: &str = "text-window/palette/message_box";
/// Image that the `gMessageBox_Pal` copy immediately precedes.
const ADJACENT_IMAGE: &str = "text-window/image/message_box";
/// Pack ids of `sTextWindowPalettes[1..=4]`, in table order.
const WINDOW_BANKS: [&str; 4] = [
    "text-window/palette/text_pal1",
    "text-window/palette/text_pal2",
    "text-window/palette/text_pal3",
    "text-window/palette/text_pal4",
];

/// Colours in each `sTextWindowPalettes` bank: the table is `u16[][16]`
/// (`text_window.c:51-58`), and the importer's `corroborate` rejects any other
/// shape.
const BANK_COLORS: u16 = 16;
/// Bytes in each `sTextWindowPalettes` bank, the table's fixed stride.
const BANK_BYTES: u32 = 32;

/// Locates the text-window frames, message box, and palettes.
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
    report.push(window_palettes_line(window_palettes, bank_bytes));

    Ok(TextWindowPlan {
        images,
        palettes,
        window_palettes,
    })
}

/// The report line for `sTextWindowPalettes`, a `static` absent from linker maps.
fn window_palettes_line(addr: u32, bank_bytes: u32) -> ReportLine {
    ReportLine::unique("sTextWindowPalettes", addr, bank_bytes * 5)
        .with(Resolution::StructDerived)
        .local_symbol("sTextWindowPalettes")
        .note("one bank before the first text_pal bank")
}

/// The linker-map expectation at a palette's address; `text_pal*` banks are
/// interior to `sTextWindowPalettes`.
fn palette_symbol(id: &str) -> SymbolExpectation {
    let Some(stem) = id.strip_prefix("text-window/palette/") else {
        return SymbolExpectation::Unnamed;
    };
    if stem.starts_with("text_pal") {
        return SymbolExpectation::Interior;
    }
    SymbolExpectation::Contains(vec![format!("TextWindowFrame{stem}_Pal")])
}

/// Requires a bank to hold exactly one full 16-colour, 32-byte palette.
///
/// A shorter bank still matches the ROM by prefix and passes the address
/// checks, but `rom-import` refuses it with `StructMismatch`.
fn require_full_bank(
    id: &str,
    color_count: u16,
    payload_bytes: u32,
) -> Result<(), GenRomProfileError> {
    if color_count == BANK_COLORS && payload_bytes == BANK_BYTES {
        return Ok(());
    }
    Err(GenRomProfileError::StructMismatch {
        id: id.to_owned(),
        reason: format!(
            "sTextWindowPalettes banks must hold exactly {BANK_COLORS} colours \
             ({BANK_BYTES} bytes), found {color_count} colours ({payload_bytes} bytes)"
        ),
    })
}

/// Derives `sTextWindowPalettes` from the bank before `text_pal1`, then
/// requires each `text_pal*` bank at its table index.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] when the message box or a bank is not exactly
/// 16 colours / 32 bytes, or the banks are not the table's in order.
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
    for id in std::iter::once(DUPLICATED_PALETTE).chain(WINDOW_BANKS) {
        let plan = palettes
            .iter()
            .find(|plan| plan.id == id)
            .ok_or_else(|| GenRomProfileError::MissingPackEntry(id.to_owned()))?;
        require_full_bank(id, plan.color_count, bank_bytes)?;
    }
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
