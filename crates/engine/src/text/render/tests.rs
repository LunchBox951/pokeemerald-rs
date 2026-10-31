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
