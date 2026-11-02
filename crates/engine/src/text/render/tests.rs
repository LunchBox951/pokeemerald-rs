//! Regression tests for [`Printer`], grouped by concern, and the fixtures they share.

mod controls;
mod layout;
mod paging;
mod pause;
mod saturation;
mod sources;
mod text_speed;

use super::*;
use assets::fonts::{FontGlyphSheet, FontId, FontImageRef, SHEET_HEIGHT, SHEET_WIDTH};
use assets::pack::ImageRef;

pub(super) const ENCODED_A: u8 = 0xBB;
pub(super) const NORMAL_A_ADVANCE_WIDTH: u8 = 6;

pub(super) fn blank_sheet_pixels() -> Vec<u8> {
    vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize]
}

pub(super) fn synthetic_sheet(pixels: &[u8], font: FontId) -> FontGlyphSheet<'_> {
    let image = ImageRef {
        width: SHEET_WIDTH,
        height: SHEET_HEIGHT,
        bit_depth: 2,
        pixels,
    };
    FontGlyphSheet::new(FontImageRef::new_for_tests(font, image)).unwrap()
}

pub(super) fn decode_tokens(bytes: &[u8]) -> Vec<Token> {
    super::super::decode(bytes).unwrap()
}

pub(super) const fn press_a() -> PrinterInput {
    PrinterInput {
        a_pressed: true,
        b_pressed: false,
        a_held: false,
        b_held: false,
    }
}

/// Per-font pixel pairs, each distinct within the 2-bit depth, so a
/// glyph's first two pixels name its source.
pub(super) fn fill_for(font: FontId) -> (u8, u8) {
    match font {
        FontId::Small => (1, 0),
        FontId::Normal => (2, 0),
        FontId::Short => (3, 0),
        FontId::Narrow => (0, 1),
        FontId::SmallNarrow => (1, 1),
    }
}

/// A sheet alternating `fill_for(font)` across each row, so every glyph
/// starts with that pair.
pub(super) fn multi_font_pixels(font: FontId) -> Vec<u8> {
    let (even, odd) = fill_for(font);
    (0..SHEET_WIDTH * SHEET_HEIGHT)
        .map(|index| if index % 2 == 0 { even } else { odd })
        .collect()
}

pub(super) fn multi_font_printer_over(
    initial: FontId,
    bytes: &[u8],
) -> Printer<assets::fonts::OwnedFontGlyphSheet> {
    let sources = GlyphSources::try_from_fn(|font| {
        let pixels = multi_font_pixels(font);
        Ok::<_, assets::error::AssetError>(synthetic_sheet(&pixels, font).to_owned_sheet())
    })
    .unwrap();
    Printer::with_sources(
        decode_tokens(bytes),
        sources,
        initial,
        TextSpeed::Instant,
        (0, 0),
    )
}
