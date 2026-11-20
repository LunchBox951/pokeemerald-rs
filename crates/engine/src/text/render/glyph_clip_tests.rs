//! Glyph-cell clipping tests: `CopyGlyphToWindow` copies only
//! `gCurGlyph.width` columns and `gCurGlyph.height` rows of a glyph's 16x16
//! cell (`pokeemerald/src/text.c:608-612`), so the printer hands out glyphs
//! with the rest of the cell cleared.

use super::tests::{decode_tokens, synthetic_sheet};
use super::*;
use assets::fonts::{FontId, GLYPH_SIZE, SHEET_HEIGHT, SHEET_WIDTH};

const SIDE: usize = GLYPH_SIZE as usize;

/// Narrow `S`: upstream's encoded byte 0xCD is glyph id 0xCD (`charmap.txt`).
const NARROW_S: u8 = 0xCD;

/// `graphics/fonts/latin_narrow.png` cell 0xCD as palette indices, one row
/// per string. The glyph's width is 5 (`gFontNarrowLatinGlyphWidths[0xCD]`),
/// yet the cell carries a second stroke of foreground (1) and shadow (2)
/// pixels at columns 8-11.
const NARROW_S_CELL: [&str; SIDE] = [
    "3333300033330000",
    "3333300033330000",
    "3333300033330000",
    "3112300011120000",
    "1221200021220000",
    "1232200031230000",
    "2123300031230000",
    "3212300031230000",
    "3321200031230000",
    "3331200031230000",
    "1231200031230000",
    "2112200031230000",
    "3222300032230000",
    "3333300033330000",
    "3333300033330000",
    "0000000000000000",
];

fn sheet_with_cell(glyph_id: usize, cell: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    let stride = SHEET_WIDTH as usize;
    let mut pixels = vec![0u8; stride * SHEET_HEIGHT as usize];
    let origin_x = (glyph_id % 16) * SIDE;
    let origin_y = (glyph_id / 16) * SIDE;
    for row in 0..SIDE {
        for column in 0..SIDE {
            pixels[(origin_y + row) * stride + origin_x + column] = cell(column, row);
        }
    }
    pixels
}

fn first_glyph(font: FontId, pixels: &[u8], glyph_byte: u8) -> RevealedGlyph {
    let sheet = synthetic_sheet(pixels, font);
    let tokens = decode_tokens(&[glyph_byte, crate::text::EOS]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));
    match printer.tick(PrinterInput::none()) {
        TickEvent::Glyph(g) => *g,
        other => panic!("expected a glyph, got {other:?}"),
    }
}

#[test]
fn canonical_narrow_s_keeps_its_glyph_and_hides_the_second_stroke_in_the_cell_padding() {
    let digit = |column: usize, row: usize| -> u8 { NARROW_S_CELL[row].as_bytes()[column] - b'0' };
    let pixels = sheet_with_cell(usize::from(NARROW_S), digit);
    let revealed = first_glyph(FontId::Narrow, &pixels, NARROW_S);
    assert_eq!(revealed.glyph.advance_width, 5);

    let mut hidden_artwork = 0;
    for row in 0..SIDE {
        for column in 0..SIDE {
            let drawn = revealed.glyph.pixels[row * SIDE + column];
            if column < 5 {
                assert_eq!(drawn, digit(column, row), "in-bounds ({column}, {row})");
            } else {
                hidden_artwork += usize::from(digit(column, row) != 0);
                assert_eq!(drawn, 0, "padding ({column}, {row}) must be cleared");
            }
        }
    }
    // The fixture's padding really does hold artwork (columns 8-11), so the
    // zeros above are the clip's doing.
    assert!(hidden_artwork > 0);
    assert_eq!(digit(8, 3), 1);
    assert_eq!(revealed.glyph.pixels[3 * SIDE + 8], 0);
}

#[test]
fn each_fonts_glyph_keeps_exactly_its_width_by_height_rectangle() {
    // Upstream heights: Small 13, Normal 15, Short 14, Narrow 15, SmallNarrow
    // 12 (`text.c:1713,1883,1841,1755,1797`).
    for (font, height) in [
        (FontId::Small, 13),
        (FontId::Normal, 15),
        (FontId::Short, 14),
        (FontId::Narrow, 15),
        (FontId::SmallNarrow, 12),
    ] {
        let glyph_id = u16::from(super::tests::ENCODED_A);
        let pixels = sheet_with_cell(usize::from(glyph_id), |_, _| 3);
        let revealed = first_glyph(font, &pixels, super::tests::ENCODED_A);
        let width = usize::from(font.glyph_width(glyph_id).unwrap());
        assert!(width > 0 && width < SIDE);

        for row in 0..SIDE {
            for column in 0..SIDE {
                let expected = if column < width && row < height { 3 } else { 0 };
                assert_eq!(
                    revealed.glyph.pixels[row * SIDE + column],
                    expected,
                    "{font:?} ({column}, {row})"
                );
            }
        }
    }
}

#[test]
fn small_narrow_row_twelve_is_cut_while_row_eleven_stays() {
    let pixels = sheet_with_cell(usize::from(super::tests::ENCODED_A), |column, row| {
        u8::from(column == 0 && (row == 11 || row == 12))
    });
    let revealed = first_glyph(FontId::SmallNarrow, &pixels, super::tests::ENCODED_A);
    assert_eq!(revealed.glyph.pixels[11 * SIDE], 1);
    assert_eq!(revealed.glyph.pixels[12 * SIDE], 0);
}

#[test]
fn clipping_leaves_the_cursor_advance_unchanged() {
    let pixels = sheet_with_cell(usize::from(NARROW_S), |_, _| 3);
    let sheet = synthetic_sheet(&pixels, FontId::Narrow);
    let tokens = decode_tokens(&[NARROW_S, NARROW_S, crate::text::EOS]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));
    let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!(second.x - first.x, 5);
}
