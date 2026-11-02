//! Extended control codes: escape, shifts, `CLEAR`, `CLEAR_TO`, and `FILL_WINDOW`.

use super::*;

#[test]
fn extra_symbol_addresses_the_extended_glyph_page() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[crate::text::CHAR_EXTRA_SYMBOL, 0, crate::text::EOS]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!(
        g.glyph.advance_width,
        FontId::Normal.glyph_width(EXTRA_SYMBOL_PAGE_START).unwrap()
    );
}

#[test]
fn unsupported_tokens_are_skipped_without_costing_a_glyph() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        crate::text::PLACEHOLDER_BEGIN,
        crate::text::PLACEHOLDER_PLAYER,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected the placeholder to be skipped and 'A' drawn")
    };
    assert_eq!((g.x, g.y), (0, 0));
}

#[test]
fn shift_right_extended_control_moves_the_cursor() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        crate::text::EXT_CTRL_CODE_BEGIN,
        ext_ctrl::SHIFT_RIGHT,
        20,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!((g.x, g.y), (20, 1));
}

#[test]
fn clear_erases_its_width_at_the_cursor_and_advances_past_it() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::EXT_CTRL_CODE_BEGIN,
        ext_ctrl::CLEAR,
        4,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (3, 1));

    let TickEvent::Glyph(before) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!((before.x, before.y), (3, 1));
    assert_eq!(printer.cleared_span(), None);

    let cursor_before_clear = printer.cursor();
    assert_eq!(
        cursor_before_clear,
        (3 + i32::from(NORMAL_A_ADVANCE_WIDTH), 1)
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(
        printer.cleared_span(),
        Some(ClearedSpan {
            x: cursor_before_clear.0,
            y: 1,
            width: 4,
            height: 15,
        })
    );
    assert_eq!(printer.cursor(), (cursor_before_clear.0 + 4, 1));

    let TickEvent::Glyph(after) = printer.tick(PrinterInput::none()) else {
        panic!("expected the glyph after the cleared span")
    };
    assert_eq!((after.x, after.y), (cursor_before_clear.0 + 4, 1));
    assert_eq!(
        printer.cleared_span(),
        None,
        "the span belongs to the tick that erased it"
    );
}

#[test]
fn clear_to_erases_up_to_the_origin_relative_column() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::EXT_CTRL_CODE_BEGIN,
        ext_ctrl::CLEAR_TO,
        20,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (3, 1));

    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    let cursor_before_clear = printer.cursor();
    assert_eq!(cursor_before_clear, (9, 1));

    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    // The target column is relative to the origin, so it is x 23, and the
    // erased width is the distance the cursor still had to travel.
    assert_eq!(
        printer.cleared_span(),
        Some(ClearedSpan {
            x: 9,
            y: 1,
            width: 14,
            height: 15,
        })
    );
    assert_eq!(printer.cursor(), (23, 1));

    let TickEvent::Glyph(aligned) = printer.tick(PrinterInput::none()) else {
        panic!("expected the glyph at the cleared column")
    };
    assert_eq!((aligned.x, aligned.y), (23, 1));
}

#[test]
fn a_clear_with_nothing_ahead_erases_nothing_and_costs_no_frame() {
    // Each case leaves the cursor at x 9 after the leading 'A', so none of
    // them has a positive width to erase (`text.c:1066`, `:1084`). The
    // backward CLEAR_TO is what separates it from its sibling SKIP, which
    // would move the cursor to x 5.
    for (subcode, argument) in [
        (ext_ctrl::CLEAR, 0),
        (ext_ctrl::CLEAR_TO, 6),
        (ext_ctrl::CLEAR_TO, 2),
    ] {
        let pixels = blank_sheet_pixels();
        let sheet = synthetic_sheet(&pixels, FontId::Normal);
        let tokens = decode_tokens(&[
            ENCODED_A,
            crate::text::EXT_CTRL_CODE_BEGIN,
            subcode,
            argument,
            ENCODED_A,
            crate::text::EOS,
        ]);
        let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (3, 1));

        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));

        let TickEvent::Glyph(after) = printer.tick(PrinterInput::none()) else {
            panic!("expected {subcode:#04X} {argument} to be consumed within the frame")
        };
        assert_eq!((after.x, after.y), (9, 1));
        assert_eq!(printer.cleared_span(), None);
    }
}

#[test]
fn fill_window_clears_stale_glyphs_and_resets_the_cursor_within_the_same_frame() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    // The Bard's song appends `EXT_CTRL_CODE_BEGIN, FILL_WINDOW` between
    // paragraphs (`pokeemerald/src/mauville_old_man.c:224-230`).
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::CHAR_NEWLINE,
        ENCODED_A,
        crate::text::EXT_CTRL_CODE_BEGIN,
        ext_ctrl::FILL_WINDOW,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

    let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
        panic!("expected the first glyph")
    };
    assert_eq!((first.x, first.y), (0, 1));
    assert!(!printer.cleared_window());

    let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
        panic!("expected the second-line glyph")
    };
    assert_eq!((second.x, second.y), (0, 17));

    // `text.c:1052-1056` fills the window, resets both cursor axes to the
    // printer origin, and returns `RENDER_REPEAT`, so the glyph after
    // `FILL_WINDOW` still renders within this same tick call.
    let TickEvent::Glyph(after_fill) = printer.tick(PrinterInput::none()) else {
        panic!("expected the glyph after FILL_WINDOW in the same frame")
    };
    assert_eq!((after_fill.x, after_fill.y), (0, 1));
    assert!(
        printer.cleared_window(),
        "FILL_WINDOW must report a whole-window clear"
    );
    assert_eq!(
        printer.cursor(),
        (i32::from(NORMAL_A_ADVANCE_WIDTH), 1),
        "FILL_WINDOW must reset the cursor to the origin before the next glyph advances it"
    );
}

#[test]
fn restart_discards_the_span_the_previous_stream_cleared() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        crate::text::EXT_CTRL_CODE_BEGIN,
        ext_ctrl::CLEAR,
        4,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert!(printer.cleared_span().is_some());

    printer.restart(decode_tokens(&[ENCODED_A, crate::text::EOS]));
    assert_eq!(printer.cleared_span(), None);

    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!((g.x, g.y), (0, 1));
}

#[test]
fn cleared_span_height_is_the_fonts_latin_glyph_height() {
    for (font, height) in [
        (FontId::Small, 13),
        (FontId::Normal, 15),
        (FontId::Narrow, 15),
        (FontId::Short, 14),
        (FontId::SmallNarrow, 12),
    ] {
        assert_eq!(clear_span_height(font), height);
    }
    // `gCurGlyph.height` is not `sFontInfos`' `maxLetterHeight`; only
    // Short's two metrics agree.
    assert_eq!(
        clear_span_height(FontId::Short),
        max_letter_height(FontId::Short)
    );
    for font in [
        FontId::Small,
        FontId::Normal,
        FontId::Narrow,
        FontId::SmallNarrow,
    ] {
        assert_ne!(clear_span_height(font), max_letter_height(font));
    }
}
