//! Cursor arithmetic at the `i32` bounds.

use super::*;

fn extreme_printer<'a>(
    pixels: &'a [u8],
    bytes: &[u8],
    origin: PixelPos,
) -> Printer<FontGlyphSheet<'a>> {
    let sheet = synthetic_sheet(pixels, FontId::Normal);
    Printer::new(decode_tokens(bytes), sheet, TextSpeed::Instant, origin)
}

#[test]
fn newline_saturates_y_at_the_i32_maximum() {
    let pixels = blank_sheet_pixels();
    let mut printer = extreme_printer(
        &pixels,
        &[crate::text::CHAR_NEWLINE, ENCODED_A, crate::text::EOS],
        (0, i32::MAX - 1),
    );
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!((g.x, g.y), (0, i32::MAX));
}

#[test]
fn glyph_advance_saturates_x_at_the_i32_maximum() {
    let pixels = blank_sheet_pixels();
    let mut printer = extreme_printer(
        &pixels,
        &[ENCODED_A, ENCODED_A, crate::text::EOS],
        (i32::MAX, 0),
    );
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(printer.cursor(), (i32::MAX, 0));
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!(g.x, i32::MAX);
}

#[test]
fn shift_controls_saturate_at_the_i32_maximum() {
    let pixels = blank_sheet_pixels();
    let mut printer = extreme_printer(
        &pixels,
        &[
            crate::text::EXT_CTRL_CODE_BEGIN,
            ext_ctrl::SHIFT_RIGHT,
            255,
            crate::text::EXT_CTRL_CODE_BEGIN,
            ext_ctrl::SHIFT_DOWN,
            255,
            ENCODED_A,
            crate::text::EOS,
        ],
        (i32::MAX - 10, i32::MAX - 10),
    );
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!((g.x, g.y), (i32::MAX, i32::MAX));
}

#[test]
fn clear_saturates_the_cursor_at_the_i32_maximum() {
    let pixels = blank_sheet_pixels();
    let mut printer = extreme_printer(
        &pixels,
        &[
            crate::text::EXT_CTRL_CODE_BEGIN,
            ext_ctrl::CLEAR,
            255,
            crate::text::EOS,
        ],
        (i32::MAX - 10, 0),
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(printer.cursor(), (i32::MAX, 0));
    assert_eq!(
        printer.cleared_span().map(|span| (span.x, span.width)),
        Some((i32::MAX - 10, 10))
    );
}

#[test]
fn clear_at_the_saturated_cursor_still_ends_the_frame() {
    let pixels = blank_sheet_pixels();
    let mut printer = extreme_printer(
        &pixels,
        &[
            crate::text::EXT_CTRL_CODE_BEGIN,
            ext_ctrl::CLEAR,
            1,
            ENCODED_A,
            crate::text::EOS,
        ],
        (i32::MAX, 0),
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(printer.cleared_span(), None);
    assert_eq!(printer.cursor(), (i32::MAX, 0));
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
}

#[test]
fn clear_to_saturates_its_target_at_the_i32_maximum() {
    let pixels = blank_sheet_pixels();
    let mut printer = extreme_printer(
        &pixels,
        &[
            crate::text::EXT_CTRL_CODE_BEGIN,
            ext_ctrl::CLEAR_TO,
            255,
            crate::text::EOS,
        ],
        (i32::MAX - 10, 0),
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(printer.cursor(), (i32::MAX, 0));
}
