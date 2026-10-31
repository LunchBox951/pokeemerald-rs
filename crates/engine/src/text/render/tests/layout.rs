//! Glyph advance, newline, line spacing, and reveal cadence.

use super::*;

const BARD_DELIMITER_ADVANCE_WIDTH: u8 = 3;

#[test]
fn advance_widths_move_the_cursor_by_the_sheets_glyph_width() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[ENCODED_A, ENCODED_A, crate::text::EOS]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

    let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!((first.x, first.y), (0, 1));
    assert_eq!(first.glyph.advance_width, NORMAL_A_ADVANCE_WIDTH);

    let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!((second.x, second.y), (i32::from(NORMAL_A_ADVANCE_WIDTH), 1));
    assert_eq!(printer.cursor(), (i32::from(NORMAL_A_ADVANCE_WIDTH) * 2, 1));
}

#[test]
fn bard_word_delimiter_advances_by_the_blank_glyph_width() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::CHAR_BARD_WORD_DELIMIT,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

    let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!((first.x, first.y), (0, 1));

    let TickEvent::Glyph(delim) = printer.tick(PrinterInput::none()) else {
        panic!("expected the delimiter's blank glyph")
    };
    assert_eq!((delim.x, delim.y), (i32::from(NORMAL_A_ADVANCE_WIDTH), 1));
    assert_eq!(delim.glyph.advance_width, BARD_DELIMITER_ADVANCE_WIDTH);

    let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!(
        (second.x, second.y),
        (
            i32::from(NORMAL_A_ADVANCE_WIDTH + BARD_DELIMITER_ADVANCE_WIDTH),
            1
        ),
        "words must stay separated"
    );
}

#[test]
fn every_font_reports_the_upstream_max_letter_height() {
    assert_eq!(max_letter_height(FontId::Small), 12);
    assert_eq!(max_letter_height(FontId::Normal), 16);
    assert_eq!(max_letter_height(FontId::Short), 14);
    assert_eq!(max_letter_height(FontId::Narrow), 16);
    assert_eq!(max_letter_height(FontId::SmallNarrow), 8);
}

#[test]
fn newline_resets_x_and_advances_y_by_line_height_at_instant_speed() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::CHAR_NEWLINE,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

    let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!((first.x, first.y), (0, 1));

    let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph after the free newline")
    };
    assert_eq!((second.x, second.y), (0, 1 + 16));
}

#[test]
fn newline_costs_a_reveal_delay_period_at_mid_speed() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::CHAR_NEWLINE,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0));

    let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
        panic!("expected the first glyph on frame 0")
    };
    assert_eq!((first.x, first.y), (0, 0));

    for frame in 1..=6 {
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::Idle,
            "frame {frame} should be idle"
        );
    }

    let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
        panic!("expected the second glyph on frame 7")
    };
    assert_eq!((second.x, second.y), (0, 16));
}

#[test]
fn reveal_cadence_matches_frames_per_char_table() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[ENCODED_A, ENCODED_A, crate::text::EOS]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0));

    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    for _ in 0..3 {
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    }
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
}

#[test]
fn fast_speed_reveals_a_glyph_every_frame() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[ENCODED_A, ENCODED_A, ENCODED_A, crate::text::EOS]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 0));

    for _ in 0..3 {
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
    }
}

#[test]
fn slow_speed_waits_seven_frames_between_glyphs() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[ENCODED_A, ENCODED_A, crate::text::EOS]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Slow, (0, 0));

    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    for _ in 0..7 {
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    }
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
}

#[test]
fn line_spacing_extends_the_newline_advance() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::CHAR_NEWLINE,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1)).with_line_spacing(1);
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph after the newline")
    };
    assert_eq!((g.x, g.y), (0, 1 + 16 + 1));
}

#[test]
fn line_spacing_extends_the_scroll_distance() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::CHAR_PROMPT_SCROLL,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 1)).with_line_spacing(1);
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(
        printer.tick(PrinterInput::none()),
        TickEvent::AwaitingScroll
    );
    assert_eq!(printer.tick(press_a()), TickEvent::ScrollStarted);
    let mut total = 0;
    let mut frames = 0;
    loop {
        match printer.tick(PrinterInput::none()) {
            TickEvent::Scrolling { dy } => {
                total += dy;
                frames += 1;
            }
            TickEvent::ScrollFinished => break,
            other => panic!("unexpected event {other:?}"),
        }
    }
    // 17 px at the fast speed's 4 px/frame: 4, 4, 4, 4, 1.
    assert_eq!((total, frames), (17, 5));
}
