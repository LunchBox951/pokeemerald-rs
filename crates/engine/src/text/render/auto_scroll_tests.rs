//! Auto-scroll wait timing (`pokeemerald/src/text.c:772-785`, `:850-899`,
//! `:1018-1022`, `:1167-1188`).
//!
//! `TextPrinterWaitAutoMode` leaves the delay counting 0 to 48 as false and
//! returns true when it reads 49, so the prompt confirms on the fiftieth
//! waiting call whatever A or B do.

use super::tests::{decode_tokens, press_a, synthetic_sheet, ENCODED_A};
use super::*;
use assets::fonts::{FontId, OwnedFontGlyphSheet};

const EXT: u8 = crate::text::EXT_CTRL_CODE_BEGIN;
const PAUSE_UNTIL_PRESS: u8 = crate::text::EXT_CTRL_CODE_PAUSE_UNTIL_PRESS;
const PROMPT_CLEAR: u8 = crate::text::CHAR_PROMPT_CLEAR;
const PROMPT_SCROLL: u8 = crate::text::CHAR_PROMPT_SCROLL;
const EOS: u8 = crate::text::EOS;
/// Waiting calls that leave the prompt waiting (`autoScrollDelay` 0 to 48).
const WAITING_CALLS_BEFORE_RELEASE: usize = 49;

fn auto_printer(bytes: &[u8]) -> Printer<OwnedFontGlyphSheet> {
    manual_printer(bytes).with_auto_scroll()
}

fn manual_printer(bytes: &[u8]) -> Printer<OwnedFontGlyphSheet> {
    let pixels = super::tests::blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal).to_owned_sheet();
    Printer::new(decode_tokens(bytes), sheet, TextSpeed::Fast, (0, 1))
}

fn held_ab() -> PrinterInput {
    PrinterInput {
        a_pressed: true,
        b_pressed: true,
        a_held: true,
        b_held: true,
    }
}

fn press_b() -> PrinterInput {
    PrinterInput {
        a_pressed: false,
        b_pressed: true,
        a_held: false,
        b_held: true,
    }
}

fn clear_stream() -> [u8; 4] {
    [ENCODED_A, PROMPT_CLEAR, ENCODED_A, EOS]
}

fn scroll_stream() -> [u8; 4] {
    [ENCODED_A, PROMPT_SCROLL, ENCODED_A, EOS]
}

fn press_wait_stream() -> [u8; 5] {
    [ENCODED_A, EXT, PAUSE_UNTIL_PRESS, ENCODED_A, EOS]
}

/// Ticks the arming frame, then 49 waiting calls with `input`, asserting each
/// reports `waiting`. Returns the printer ready for the fiftieth call.
fn arm_and_wait(
    bytes: &[u8],
    waiting: &TickEvent,
    input: PrinterInput,
) -> Printer<OwnedFontGlyphSheet> {
    let mut printer = auto_printer(bytes);
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(
        &printer.tick(PrinterInput::none()),
        waiting,
        "the arming tick is not a waiting call"
    );
    wait_calls(&mut printer, waiting, input, WAITING_CALLS_BEFORE_RELEASE);
    printer
}

fn wait_calls(
    printer: &mut Printer<OwnedFontGlyphSheet>,
    waiting: &TickEvent,
    input: PrinterInput,
    count: usize,
) {
    for call in 1..=count {
        assert_eq!(
            &printer.tick(input),
            waiting,
            "waiting call {call} must not release the prompt"
        );
    }
}

#[test]
fn manual_printers_report_no_auto_scroll() {
    assert!(!manual_printer(&clear_stream()).is_auto_scroll());
    assert!(auto_printer(&clear_stream()).is_auto_scroll());
}

#[test]
fn a_clear_prompt_releases_on_the_fiftieth_waiting_call() {
    let mut printer = arm_and_wait(
        &clear_stream(),
        &TickEvent::AwaitingClear,
        PrinterInput::none(),
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Cleared);
    assert_eq!(printer.cursor(), (0, 1));
    assert!(printer.cleared_window());
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected printing to resume on the new page")
    };
    assert_eq!((g.x, g.y), (0, 1));
}

#[test]
fn a_scroll_prompt_releases_on_the_fiftieth_waiting_call() {
    let mut printer = arm_and_wait(
        &scroll_stream(),
        &TickEvent::AwaitingScroll,
        PrinterInput::none(),
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::ScrollStarted);
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
fn pause_until_press_releases_on_the_fiftieth_waiting_call() {
    let mut printer = arm_and_wait(
        &press_wait_stream(),
        &TickEvent::AwaitingPress,
        PrinterInput::none(),
    );
    let cursor_at_wait = printer.cursor();
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::PressAccepted);
    assert_eq!(printer.cursor(), cursor_at_wait);
    let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
        panic!("expected the next glyph after the wait")
    };
    assert_eq!((second.x, second.y), (6, 1));
}

#[test]
fn a_or_b_never_shortens_the_wait_held_or_pressed() {
    for (bytes, waiting, released) in [
        (
            clear_stream().to_vec(),
            TickEvent::AwaitingClear,
            TickEvent::Cleared,
        ),
        (
            scroll_stream().to_vec(),
            TickEvent::AwaitingScroll,
            TickEvent::ScrollStarted,
        ),
        (
            press_wait_stream().to_vec(),
            TickEvent::AwaitingPress,
            TickEvent::PressAccepted,
        ),
    ] {
        for input in [press_a(), press_b(), held_ab()] {
            let mut printer = arm_and_wait(&bytes, &waiting, input);
            assert_eq!(printer.tick(input), released, "{waiting:?} with {input:?}");
        }
    }
}

#[test]
fn successive_prompts_each_wait_a_full_delay() {
    let mut printer = auto_printer(&[
        ENCODED_A,
        PROMPT_CLEAR,
        ENCODED_A,
        PROMPT_CLEAR,
        ENCODED_A,
        EOS,
    ]);
    for _ in 0..2 {
        assert!(matches!(
            printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ));
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
        wait_calls(
            &mut printer,
            &TickEvent::AwaitingClear,
            PrinterInput::none(),
            WAITING_CALLS_BEFORE_RELEASE,
        );
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Cleared);
    }
}

#[test]
fn a_press_wait_after_a_prompt_restarts_the_delay() {
    let mut printer = auto_printer(&[
        ENCODED_A,
        PROMPT_SCROLL,
        EXT,
        PAUSE_UNTIL_PRESS,
        ENCODED_A,
        EOS,
    ]);
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(
        printer.tick(PrinterInput::none()),
        TickEvent::AwaitingScroll
    );
    wait_calls(
        &mut printer,
        &TickEvent::AwaitingScroll,
        PrinterInput::none(),
        WAITING_CALLS_BEFORE_RELEASE,
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::ScrollStarted);
    loop {
        match printer.tick(PrinterInput::none()) {
            TickEvent::Scrolling { .. } => {}
            TickEvent::ScrollFinished => break,
            other => panic!("unexpected event {other:?}"),
        }
    }
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingPress);
    wait_calls(
        &mut printer,
        &TickEvent::AwaitingPress,
        PrinterInput::none(),
        WAITING_CALLS_BEFORE_RELEASE,
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::PressAccepted);
}

#[test]
fn restart_resets_a_partly_counted_wait() {
    let mut printer = auto_printer(&clear_stream());
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
    wait_calls(
        &mut printer,
        &TickEvent::AwaitingClear,
        PrinterInput::none(),
        WAITING_CALLS_BEFORE_RELEASE - 1,
    );

    printer.restart(decode_tokens(&clear_stream()));
    assert!(printer.is_auto_scroll(), "restart keeps the selected mode");
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
    wait_calls(
        &mut printer,
        &TickEvent::AwaitingClear,
        PrinterInput::none(),
        WAITING_CALLS_BEFORE_RELEASE,
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Cleared);
}

#[test]
fn manual_mode_keeps_its_fresh_edge_confirmation() {
    let mut printer = manual_printer(&clear_stream());
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingClear);
    wait_calls(
        &mut printer,
        &TickEvent::AwaitingClear,
        PrinterInput::none(),
        WAITING_CALLS_BEFORE_RELEASE + 10,
    );
    let held_only = PrinterInput {
        a_held: true,
        ..PrinterInput::none()
    };
    assert_eq!(printer.tick(held_only), TickEvent::AwaitingClear);
    assert_eq!(printer.tick(press_a()), TickEvent::Cleared);
}
