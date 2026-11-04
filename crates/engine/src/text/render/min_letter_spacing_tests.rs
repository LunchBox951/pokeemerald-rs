//! `MIN_LETTER_SPACING` extended-control tests (`pokeemerald/src/text.c:1091-1092`,
//! post-glyph clear and advance `:1149-1156`).

use super::tests::{decode_tokens, synthetic_sheet, ENCODED_A};
use super::*;
use assets::fonts::{FontId, OwnedFontGlyphSheet};

const EXT: u8 = crate::text::EXT_CTRL_CODE_BEGIN;
const MIN_SPACING: u8 = crate::text::EXT_CTRL_CODE_MIN_LETTER_SPACING;
const FONT_CODE: u8 = crate::text::EXT_CTRL_CODE_FONT;
const CLEAR_CODE: u8 = crate::text::EXT_CTRL_CODE_CLEAR;
const NORMAL_A_ADVANCE: i32 = 6;
const NORMAL_CLEAR_HEIGHT: i32 = 15;

fn normal_printer(bytes: &[u8]) -> Printer<OwnedFontGlyphSheet> {
    let pixels = super::tests::blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal).to_owned_sheet();
    Printer::new(decode_tokens(bytes), sheet, TextSpeed::Instant, (0, 1))
}

fn next_glyph(printer: &mut Printer<OwnedFontGlyphSheet>) -> RevealedGlyph {
    match printer.tick(PrinterInput::none()) {
        TickEvent::Glyph(g) => *g,
        other => panic!("expected a glyph, got {other:?}"),
    }
}

#[test]
fn a_minimum_wider_than_the_glyph_places_the_next_glyph_at_the_minimum() {
    // `FC 14 12 A A` with the argument 12: the confirmed regression, where
    // the control was a no-op and the second glyph landed at x=6.
    let mut printer =
        normal_printer(&[EXT, MIN_SPACING, 12, ENCODED_A, ENCODED_A, crate::text::EOS]);

    assert_eq!(next_glyph(&mut printer).x, 0);
    assert_eq!(next_glyph(&mut printer).x, 12);
    assert_eq!(printer.cursor().0, 24);
}

#[test]
fn the_gap_span_accompanies_its_glyph_in_the_same_tick() {
    let mut printer = normal_printer(&[EXT, MIN_SPACING, 12, ENCODED_A, crate::text::EOS]);

    let glyph = next_glyph(&mut printer);
    assert_eq!((glyph.x, glyph.y), (0, 1));
    // Only the gap beyond the glyph's own advance is erased, at the glyph's
    // font height, and the cursor already sits at the minimum.
    assert_eq!(
        printer.cleared_span(),
        Some(ClearedSpan {
            x: NORMAL_A_ADVANCE,
            y: 1,
            width: 12 - NORMAL_A_ADVANCE,
            height: NORMAL_CLEAR_HEIGHT,
        })
    );
    assert_eq!(printer.cursor(), (12, 1));
}

#[test]
fn the_span_is_scoped_to_the_tick_that_drew_the_glyph() {
    let mut printer = normal_printer(&[EXT, MIN_SPACING, 12, ENCODED_A, crate::text::EOS]);
    next_glyph(&mut printer);
    assert!(printer.cleared_span().is_some());

    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Finished);
    assert_eq!(printer.cleared_span(), None);
}

#[test]
fn a_minimum_the_glyph_already_meets_erases_nothing() {
    for minimum in [1, 4, NORMAL_A_ADVANCE as u8] {
        let mut printer = normal_printer(&[
            EXT,
            MIN_SPACING,
            minimum,
            ENCODED_A,
            ENCODED_A,
            crate::text::EOS,
        ]);

        next_glyph(&mut printer);
        assert_eq!(printer.cleared_span(), None, "minimum {minimum}");
        assert_eq!(
            next_glyph(&mut printer).x,
            NORMAL_A_ADVANCE,
            "minimum {minimum}"
        );
        assert_eq!(printer.cleared_span(), None, "minimum {minimum}");
    }
}

#[test]
fn a_zero_argument_restores_the_ordinary_latin_advance() {
    let mut printer = normal_printer(&[
        EXT,
        MIN_SPACING,
        12,
        ENCODED_A,
        EXT,
        MIN_SPACING,
        0,
        ENCODED_A,
        ENCODED_A,
        crate::text::EOS,
    ]);

    assert_eq!(next_glyph(&mut printer).x, 0);
    assert_eq!(next_glyph(&mut printer).x, 12);
    assert_eq!(printer.cleared_span(), None);
    assert_eq!(next_glyph(&mut printer).x, 12 + NORMAL_A_ADVANCE);
    assert_eq!(printer.cleared_span(), None);
}

#[test]
fn the_gap_clear_height_follows_the_glyphs_font() {
    // Short A is narrower than 12 and clears 14 rows rather than Normal's 15.
    let sources = assets::fonts::GlyphSources::try_from_fn(|font| {
        let pixels = super::tests::multi_font_pixels(font);
        Ok::<_, assets::error::AssetError>(synthetic_sheet(&pixels, font).to_owned_sheet())
    })
    .unwrap();
    let mut printer = Printer::with_sources(
        decode_tokens(&[
            EXT,
            MIN_SPACING,
            20,
            EXT,
            FONT_CODE,
            2,
            ENCODED_A,
            crate::text::EOS,
        ]),
        sources,
        FontId::Normal,
        TextSpeed::Instant,
        (0, 0),
    );

    let glyph = next_glyph(&mut printer);
    let span = printer.cleared_span().unwrap();
    assert_eq!(span.height, clear_span_height(FontId::Short));
    assert_eq!(span.x, i32::from(glyph.glyph.advance_width));
    assert_eq!(span.width, 20 - i32::from(glyph.glyph.advance_width));
}

#[test]
fn explicit_clear_still_reports_its_span_on_an_idle_tick() {
    let mut printer = normal_printer(&[EXT, MIN_SPACING, 12, EXT, CLEAR_CODE, 5, crate::text::EOS]);

    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(
        printer.cleared_span(),
        Some(ClearedSpan {
            x: 0,
            y: 1,
            width: 5,
            height: NORMAL_CLEAR_HEIGHT,
        })
    );
    assert_eq!(printer.cursor().0, 5);
}

#[test]
fn restart_returns_to_the_ordinary_advance() {
    let mut printer = normal_printer(&[EXT, MIN_SPACING, 12, ENCODED_A, crate::text::EOS]);
    next_glyph(&mut printer);

    printer.restart(decode_tokens(&[ENCODED_A, ENCODED_A, crate::text::EOS]));

    assert_eq!(next_glyph(&mut printer).x, 0);
    assert_eq!(printer.cleared_span(), None);
    assert_eq!(next_glyph(&mut printer).x, NORMAL_A_ADVANCE);
}

#[test]
fn the_gap_span_stops_where_the_cursor_saturates() {
    let pixels = super::tests::blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal).to_owned_sheet();
    let mut printer = Printer::new(
        decode_tokens(&[EXT, MIN_SPACING, 12, ENCODED_A, crate::text::EOS]),
        sheet,
        TextSpeed::Instant,
        (i32::MAX - 8, 0),
    );

    next_glyph(&mut printer);
    let span = printer.cleared_span().unwrap();
    assert_eq!(span.x, i32::MAX - 8 + NORMAL_A_ADVANCE);
    assert_eq!(span.x + span.width, i32::MAX);
    assert_eq!(printer.cursor().0, i32::MAX);
}
