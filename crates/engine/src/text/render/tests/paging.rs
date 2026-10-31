//! Scroll and page-clear prompts, and the terminal state.

use super::*;

#[test]
fn prompt_scroll_waits_for_confirm_then_animates_the_scroll() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::CHAR_PROMPT_SCROLL,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 1));

    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(
        printer.tick(PrinterInput::none()),
        TickEvent::AwaitingScroll
    );
    assert_eq!(
        printer.tick(PrinterInput::none()),
        TickEvent::AwaitingScroll
    );
    assert_eq!(printer.tick(press_a()), TickEvent::ScrollStarted);
    for _ in 0..4 {
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::Scrolling { dy: 4 }
        );
    }
    assert_eq!(
        printer.tick(PrinterInput::none()),
        TickEvent::ScrollFinished
    );
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected printing to resume after the scroll")
    };
    assert_eq!((g.x, g.y), (0, 1));
}

#[test]
fn prompt_clear_waits_for_confirm_then_resets_the_cursor_on_page() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::CHAR_NEWLINE,
        crate::text::CHAR_PROMPT_CLEAR,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Fast, (0, 1));

    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
    assert_eq!(printer.tick(press_a()), TickEvent::Cleared);
    assert_eq!(printer.cursor(), (0, 1));
    assert!(
        printer.cleared_window(),
        "the page-clear confirmation should also report a whole-window clear"
    );
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected printing to resume on the new page")
    };
    assert_eq!((g.x, g.y), (0, 1));
    assert!(
        !printer.cleared_window(),
        "the flag should not persist into the next tick"
    );
}

#[test]
fn page_clear_resumes_after_a_reveal_delay_at_mid_speed() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::CHAR_PROMPT_CLEAR,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0));

    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    for frame in 1..=3 {
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::Idle,
            "frame {frame} should be idle"
        );
    }
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
    assert_eq!(printer.tick(press_a()), TickEvent::Cleared);
    assert_eq!(printer.cursor(), (0, 0));
    for frame in 7..=9 {
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::Idle,
            "resume frame {frame} should be idle"
        );
    }
    let TickEvent::Glyph(first_glyph_on_new_page) = printer.tick(PrinterInput::none()) else {
        panic!("expected the new page's glyph on frame 10")
    };
    assert_eq!(
        (first_glyph_on_new_page.x, first_glyph_on_new_page.y),
        (0, 0)
    );
}

#[test]
fn scroll_resumes_after_a_reveal_delay_at_mid_speed() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::CHAR_PROMPT_SCROLL,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 1));

    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    for _ in 0..3 {
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    }
    assert_eq!(
        printer.tick(PrinterInput::none()),
        TickEvent::AwaitingScroll
    );
    assert_eq!(
        printer.tick(PrinterInput::none()),
        TickEvent::AwaitingScroll
    );
    assert_eq!(printer.tick(press_a()), TickEvent::ScrollStarted);
    for _ in 0..8 {
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::Scrolling { dy: 2 }
        );
    }
    assert_eq!(
        printer.tick(PrinterInput::none()),
        TickEvent::ScrollFinished
    );
    for _ in 0..3 {
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    }
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected printing to resume after the scroll's reveal delay")
    };
    assert_eq!((g.x, g.y), (0, 1));
}

#[test]
fn finished_is_terminal_and_idempotent() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[ENCODED_A, crate::text::EOS]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Finished);
    assert!(printer.is_finished());
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Finished);
}
