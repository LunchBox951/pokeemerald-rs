//! Pause controls, A/B confirmation, and the A/B speed-up latch.

use super::*;

const PAUSE_DURATION: u8 = 96;

#[test]
fn b_alone_advances_a_prompt_clear_exactly_like_a_alone() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
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
    let b_only = PrinterInput {
        a_pressed: false,
        b_pressed: true,
        a_held: false,
        b_held: false,
    };
    assert_eq!(printer.tick(b_only), TickEvent::Cleared);
    assert_eq!(printer.cursor(), (0, 1));
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected printing to resume on the new page")
    };
    assert_eq!((g.x, g.y), (0, 1));
}

#[test]
fn pause_until_press_requires_a_new_press_and_preserves_the_cursor() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::EXT_CTRL_CODE_BEGIN,
        crate::text::EXT_CTRL_CODE_PAUSE_UNTIL_PRESS,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

    assert!(
        matches!(printer.tick(PrinterInput::none()), TickEvent::Glyph(_)),
        "the first glyph reveals before the wait"
    );
    let cursor_at_wait = printer.cursor();

    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingPress);
    let held_a = PrinterInput {
        a_pressed: false,
        b_pressed: false,
        a_held: true,
        b_held: false,
    };
    assert_eq!(
        printer.tick(held_a),
        TickEvent::AwaitingPress,
        "a held button with no fresh edge must not release the wait"
    );
    assert_eq!(
        printer.tick(PrinterInput::none()),
        TickEvent::AwaitingPress,
        "releasing the held button must not release the wait either"
    );
    assert_eq!(
        printer.cursor(),
        cursor_at_wait,
        "waiting must not move the cursor"
    );

    assert_eq!(printer.tick(press_a()), TickEvent::PressAccepted);
    assert_eq!(
        printer.cursor(),
        cursor_at_wait,
        "accepting the press must not move the cursor"
    );

    let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
        panic!("expected the next glyph the tick after the press was accepted")
    };
    assert_eq!(
        (second.x, second.y),
        (i32::from(NORMAL_A_ADVANCE_WIDTH), 1),
        "advanced past the first glyph"
    );
}

#[test]
fn mid_speed_pause_until_press_preserves_the_pending_reveal_delay() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::EXT_CTRL_CODE_BEGIN,
        crate::text::EXT_CTRL_CODE_PAUSE_UNTIL_PRESS,
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
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::AwaitingPress);
    assert_eq!(printer.tick(press_a()), TickEvent::PressAccepted);

    // Unlike timed PAUSE, this control does not reset the reveal delay.
    for _ in 0..3 {
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    }
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
}

#[test]
fn pause_control_code_blocks_for_exactly_the_argument_frame_count() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::EXT_CTRL_CODE_BEGIN,
        crate::text::EXT_CTRL_CODE_PAUSE,
        PAUSE_DURATION,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));

    assert!(
        matches!(printer.tick(PrinterInput::none()), TickEvent::Glyph(_)),
        "the first glyph reveals before the pause"
    );

    let mut paused_ticks = 0;
    loop {
        match printer.tick(PrinterInput::none()) {
            TickEvent::Paused => paused_ticks += 1,
            TickEvent::PauseFinished => break,
            other => panic!("expected Paused or PauseFinished, got {other:?}"),
        }
    }
    assert_eq!(
        paused_ticks,
        usize::from(PAUSE_DURATION),
        "pause duration must include the control code's frame"
    );

    let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
        panic!("expected the second glyph the tick after PauseFinished")
    };
    assert_eq!(
        (second.x, second.y),
        (i32::from(NORMAL_A_ADVANCE_WIDTH), 1),
        "advanced past the first glyph"
    );
}

#[test]
fn mid_speed_pause_resumes_without_an_extra_reveal_delay() {
    const TWO_FRAME_PAUSE: u8 = 2;

    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        ENCODED_A,
        crate::text::EXT_CTRL_CODE_BEGIN,
        crate::text::EXT_CTRL_CODE_PAUSE,
        TWO_FRAME_PAUSE,
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
    for _ in 0..TWO_FRAME_PAUSE {
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Paused);
    }
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::PauseFinished);
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
}

#[test]
fn held_ab_speed_up_only_affects_printers_that_opt_in() {
    let pixels = blank_sheet_pixels();

    let opted_out_sheet = synthetic_sheet(&pixels, FontId::Normal);
    let opted_out_tokens = decode_tokens(&[ENCODED_A, ENCODED_A, ENCODED_A, crate::text::EOS]);
    let mut opted_out_printer =
        Printer::new(opted_out_tokens, opted_out_sheet, TextSpeed::Mid, (0, 0));

    let held_a = PrinterInput {
        a_pressed: true,
        b_pressed: false,
        a_held: true,
        b_held: false,
    };
    assert!(
        matches!(opted_out_printer.tick(held_a), TickEvent::Glyph(_)),
        "the first glyph reveals on frame 0 regardless"
    );
    for frame in 1..=3 {
        assert_eq!(
            opted_out_printer.tick(held_a),
            TickEvent::Idle,
            "an opted-out printer's frame {frame} must stay un-accelerated"
        );
    }
    assert!(
        matches!(opted_out_printer.tick(held_a), TickEvent::Glyph(_)),
        "the second glyph still waits out the full MID delay"
    );

    let intro_sheet = synthetic_sheet(&pixels, FontId::Normal);
    let intro_tokens = decode_tokens(&[ENCODED_A, ENCODED_A, ENCODED_A, crate::text::EOS]);
    let mut intro_printer =
        Printer::new(intro_tokens, intro_sheet, TextSpeed::Mid, (0, 0)).with_ab_speed_up_print();

    assert!(
        matches!(
            intro_printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ),
        "the first glyph still reveals on frame 0"
    );
    assert_eq!(intro_printer.tick(held_a), TickEvent::Idle);
    assert!(
        matches!(
            intro_printer.tick(PrinterInput::none()),
            TickEvent::Glyph(_)
        ),
        "the second glyph reveals early once the speed-up latches"
    );
    assert!(
        matches!(intro_printer.tick(held_a), TickEvent::Glyph(_)),
        "the third glyph reveals immediately while A remains held"
    );
}

#[test]
fn restart_clears_a_stuck_pause_state() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);

    let tokens = decode_tokens(&[
        crate::text::EXT_CTRL_CODE_BEGIN,
        crate::text::EXT_CTRL_CODE_PAUSE,
        PAUSE_DURATION,
        ENCODED_A,
        crate::text::EOS,
    ]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 0));

    for tick in 1..=5 {
        assert_eq!(
            printer.tick(PrinterInput::none()),
            TickEvent::Paused,
            "tick {tick} should still be mid-pause"
        );
    }

    printer.restart(decode_tokens(&[ENCODED_A, crate::text::EOS]));
    assert!(!printer.is_finished());
    assert_eq!(printer.cursor(), (0, 0));

    assert!(
        matches!(printer.tick(PrinterInput::none()), TickEvent::Glyph(_)),
        "restart must not leave the printer stuck mid-pause"
    );
}

#[test]
fn restart_clears_the_latched_speed_up_flag() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[ENCODED_A, ENCODED_A, crate::text::EOS]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Mid, (0, 0)).with_ab_speed_up_print();

    let press_and_hold_a = PrinterInput {
        a_pressed: true,
        b_pressed: false,
        a_held: true,
        b_held: false,
    };
    let hold_a = PrinterInput {
        a_pressed: false,
        b_pressed: false,
        a_held: true,
        b_held: false,
    };

    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(printer.tick(press_and_hold_a), TickEvent::Idle);
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));

    printer.restart(decode_tokens(&[ENCODED_A, ENCODED_A, crate::text::EOS]));
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));

    for frame in 1..=3 {
        assert_eq!(
            printer.tick(hold_a),
            TickEvent::Idle,
            "restart frame {frame}: the speed-up latch must not survive"
        );
    }
    assert!(matches!(printer.tick(hold_a), TickEvent::Glyph(_)));
}
