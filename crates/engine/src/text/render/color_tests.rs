//! Inline `COLOR`, `HIGHLIGHT`, `SHADOW`, and `COLOR_HIGHLIGHT_SHADOW` state
//! carried by [`Printer`] onto glyph, erase, and window-clear output.

use super::tests::{blank_sheet_pixels, decode_tokens, synthetic_sheet, ENCODED_A};
use super::*;
use assets::fonts::{FontGlyphSheet, FontId};

fn colored(foreground: u8, background: u8, shadow: u8) -> TextColors {
    TextColors {
        foreground,
        background,
        shadow,
    }
}

fn printer_for(bytes: &[u8], speed: TextSpeed) -> Printer<FontGlyphSheet<'static>> {
    let pixels: &'static [u8] = Box::leak(blank_sheet_pixels().into_boxed_slice());
    Printer::new(
        decode_tokens(bytes),
        synthetic_sheet(pixels, FontId::Normal),
        speed,
        (0, 1),
    )
}

/// Every revealed glyph's colours, ticking until the stream finishes.
fn glyph_colors_of(bytes: &[u8]) -> Vec<((i32, i32), TextColors)> {
    let mut printer = printer_for(bytes, TextSpeed::Instant);
    let mut out = Vec::new();
    for _ in 0..64 {
        match printer.tick(PrinterInput::none()) {
            TickEvent::Glyph(g) => out.push(((g.x, g.y), g.colors)),
            TickEvent::Finished => break,
            _ => {}
        }
    }
    out
}

#[test]
fn color_blue_prefix_is_carried_on_the_revealed_glyph() {
    // gText_ColorBlue is `{COLOR BLUE}`: FC 01 08 (strings.c:1234).
    let plain = glyph_colors_of(&[ENCODED_A, super::super::EOS]);
    let blue = glyph_colors_of(&[0xFC, 0x01, 0x08, ENCODED_A, super::super::EOS]);
    assert_eq!(plain, vec![((0, 1), colored(2, 1, 3))]);
    assert_eq!(blue, vec![((0, 1), colored(8, 1, 3))]);
}

#[test]
fn each_color_control_updates_its_own_index() {
    let eos = super::super::EOS;
    let cases: [(&[u8], TextColors); 4] = [
        (&[0xFC, 0x01, 0x08, ENCODED_A, eos], colored(8, 1, 3)),
        (&[0xFC, 0x02, 0x04, ENCODED_A, eos], colored(2, 4, 3)),
        (&[0xFC, 0x03, 0x09, ENCODED_A, eos], colored(2, 1, 9)),
        (
            &[0xFC, 0x04, 0x08, 0x04, 0x09, ENCODED_A, eos],
            colored(8, 4, 9),
        ),
    ];
    for (bytes, expected) in cases {
        assert_eq!(
            glyph_colors_of(bytes),
            vec![((0, 1), expected)],
            "{bytes:?}"
        );
    }
}

#[test]
fn colors_persist_across_glyphs_and_newlines_without_rewriting_earlier_glyphs() {
    let eos = super::super::EOS;
    let got = glyph_colors_of(&[ENCODED_A, 0xFC, 0x01, 0x08, ENCODED_A, 0xFE, ENCODED_A, eos]);
    assert_eq!(
        got,
        vec![
            ((0, 1), colored(2, 1, 3)),
            ((6, 1), colored(8, 1, 3)),
            ((0, 17), colored(8, 1, 3)),
        ]
    );
}

#[test]
fn a_highlight_paints_glyph_backgrounds_from_then_on_even_back_to_white() {
    let eos = super::super::EOS;
    let mut printer = printer_for(
        &[
            ENCODED_A, 0xFC, 0x02, 0x04, ENCODED_A, 0xFC, 0x02, 0x01, ENCODED_A, eos,
        ],
        TextSpeed::Instant,
    );
    let mut paint = Vec::new();
    while !printer.is_finished() {
        if let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) {
            paint.push((g.colors.background, g.paint_background, g.raster_height));
        }
    }
    assert_eq!(paint, vec![(1, false, 15), (4, true, 15), (1, true, 15)]);
}

#[test]
fn color_controls_keep_upstreams_repeat_timing() {
    let eos = super::super::EOS;
    let prefixes: [&[u8]; 4] = [
        &[0xFC, 0x01, 0x08],
        &[0xFC, 0x02, 0x04],
        &[0xFC, 0x03, 0x09],
        &[0xFC, 0x04, 0x08, 0x04, 0x09],
    ];
    for prefix in prefixes {
        let mut bytes = prefix.to_vec();
        bytes.extend([ENCODED_A, eos]);
        for (speed, idle_ticks) in [
            (TextSpeed::Slow, 7),
            (TextSpeed::Mid, 3),
            (TextSpeed::Fast, 0),
            (TextSpeed::Instant, 0),
        ] {
            let mut printer = printer_for(&bytes, speed);
            for tick in 0..idle_ticks {
                assert_eq!(
                    printer.tick(PrinterInput::none()),
                    TickEvent::Idle,
                    "{prefix:?} {speed:?} tick {tick}"
                );
            }
            assert!(
                matches!(printer.tick(PrinterInput::none()), TickEvent::Glyph(_)),
                "{prefix:?} {speed:?}"
            );
        }
    }
}

#[test]
fn clear_reports_the_highlight_in_force() {
    let mut printer = printer_for(
        &[0xFC, 0x02, 0x04, 0xFC, 0x11, 0x03, super::super::EOS],
        TextSpeed::Instant,
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(
        printer.cleared_span(),
        Some(ClearedSpan {
            x: 0,
            y: 1,
            width: 3,
            height: 15,
            colors: colored(2, 4, 3),
        })
    );
}

#[test]
fn fill_window_reports_the_background_at_the_clear_not_the_one_after() {
    let mut printer = printer_for(
        &[
            0xFC,
            0x02,
            0x04,
            0xFC,
            0x0F,
            0xFC,
            0x02,
            0x08,
            ENCODED_A,
            super::super::EOS,
        ],
        TextSpeed::Instant,
    );
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert!(printer.cleared_window());
    assert_eq!(printer.cleared_window_colors(), Some(colored(2, 4, 3)));
    assert_eq!(g.colors.background, 8);
    printer.tick(PrinterInput::none());
    assert_eq!(printer.cleared_window_colors(), None);
}

#[test]
fn short_hand_built_color_tokens_change_nothing() {
    let tokens = vec![
        Token::ExtCtrl {
            sub: ext_ctrl::COLOR,
            args: Vec::new(),
        },
        Token::ExtCtrl {
            sub: ext_ctrl::COLOR_HIGHLIGHT_SHADOW,
            args: vec![8, 4],
        },
        Token::End,
    ];
    let pixels = blank_sheet_pixels();
    let mut printer = Printer::new(
        tokens,
        synthetic_sheet(&pixels, FontId::Normal),
        TextSpeed::Instant,
        (0, 1),
    );
    while printer.tick(PrinterInput::none()) != TickEvent::Finished {}
    assert_eq!(printer.colors(), TextColors::default());
}

#[test]
fn configured_colors_apply_until_restart_resets_them() {
    let eos = super::super::EOS;
    let pixels = blank_sheet_pixels();
    let mut printer = Printer::new(
        decode_tokens(&[0xFC, 0x01, 0x08, ENCODED_A, eos]),
        synthetic_sheet(&pixels, FontId::Normal),
        TextSpeed::Instant,
        (0, 1),
    )
    .with_text_colors(colored(5, 6, 7));
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!(g.colors, colored(8, 6, 7));
    assert!(g.paint_background);
    printer.restart(decode_tokens(&[ENCODED_A, eos]));
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!(g.colors, colored(5, 6, 7));
}
