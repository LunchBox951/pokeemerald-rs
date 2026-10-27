//! `FONT` extended-control tests (`pokeemerald/src/text.c:1007-1010`).

use super::tests::{
    decode_tokens, fill_for, multi_font_pixels, multi_font_printer_over, press_a, synthetic_sheet,
    ENCODED_A,
};
use super::*;
use assets::fonts::{FontId, GlyphSources};

const EXT: u8 = crate::text::EXT_CTRL_CODE_BEGIN;
const FONT_CODE: u8 = crate::text::EXT_CTRL_CODE_FONT;
const CLEAR_CODE: u8 = crate::text::EXT_CTRL_CODE_CLEAR;
const CLEAR_TO_CODE: u8 = crate::text::EXT_CTRL_CODE_CLEAR_TO;

fn next_glyph(printer: &mut Printer<assets::fonts::OwnedFontGlyphSheet>) -> RevealedGlyph {
    match printer.tick(PrinterInput::none()) {
        TickEvent::Glyph(g) => *g,
        other => panic!("expected a glyph, got {other:?}"),
    }
}

fn pair(g: &RevealedGlyph) -> (u8, u8) {
    (g.glyph.pixels[0], g.glyph.pixels[1])
}

#[test]
fn font_switches_the_source_and_advance_and_the_following_clear_height() {
    let mut printer = multi_font_printer_over(
        FontId::Normal,
        &[
            ENCODED_A,
            EXT,
            FONT_CODE,
            8,
            ENCODED_A,
            EXT,
            CLEAR_CODE,
            4,
            EXT,
            CLEAR_TO_CODE,
            0x20,
            crate::text::EOS,
        ],
    );
    let normal_advance = FontId::Normal.glyph_width(u16::from(ENCODED_A)).unwrap();
    let narrow_advance = FontId::SmallNarrow
        .glyph_width(u16::from(ENCODED_A))
        .unwrap();
    // The metric-distinct sheets are what make the assertions below mean
    // something.
    assert_ne!(normal_advance, narrow_advance);
    assert_ne!(
        clear_span_height(FontId::Normal),
        clear_span_height(FontId::SmallNarrow)
    );

    let g = next_glyph(&mut printer);
    assert_eq!(pair(&g), fill_for(FontId::Normal));
    assert_eq!(g.glyph.advance_width, normal_advance);
    let after_first = i32::from(normal_advance);
    assert_eq!(printer.cursor().0, after_first);

    // FONT is consumed within the same frame as the glyph after it.
    let g = next_glyph(&mut printer);
    assert_eq!(g.x, after_first);
    assert_eq!(pair(&g), fill_for(FontId::SmallNarrow));
    assert_eq!(g.glyph.advance_width, narrow_advance);
    assert_eq!(printer.active_font(), Some(FontId::SmallNarrow));
    let x = after_first + i32::from(narrow_advance);
    assert_eq!(printer.cursor().0, x);

    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(
        printer.cleared_span(),
        Some(ClearedSpan {
            x,
            y: 0,
            width: 4,
            height: 12
        })
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(
        printer.cleared_span(),
        Some(ClearedSpan {
            x: x + 4,
            y: 0,
            width: 0x20 - x - 4,
            height: 12
        })
    );
}

#[test]
fn a_clear_straight_after_font_keeps_the_last_glyphs_height_until_the_next_glyph() {
    // Upstream's `gCurGlyph.height` is set by glyph decompression, not by
    // `FONT` (`text.c:1007-1010`, `:649-674`).
    let mut printer = multi_font_printer_over(
        FontId::Normal,
        &[
            EXT,
            FONT_CODE,
            0,
            EXT,
            CLEAR_CODE,
            3,
            ENCODED_A,
            EXT,
            CLEAR_CODE,
            3,
            crate::text::EOS,
        ],
    );
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(printer.cleared_span().unwrap().height, 15);
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(printer.cleared_span().unwrap().height, 13);
}

#[test]
fn every_modelled_upstream_font_id_selects_its_source() {
    // `include/text.h:10-21`: the three short copies draw as Short.
    for (raw, font) in [
        (0, FontId::Small),
        (1, FontId::Normal),
        (2, FontId::Short),
        (3, FontId::Short),
        (4, FontId::Short),
        (5, FontId::Short),
        (7, FontId::Narrow),
        (8, FontId::SmallNarrow),
    ] {
        let mut printer = multi_font_printer_over(
            FontId::Normal,
            &[
                EXT,
                FONT_CODE,
                raw,
                ENCODED_A,
                EXT,
                CLEAR_CODE,
                1,
                crate::text::EOS,
            ],
        );
        let g = next_glyph(&mut printer);
        assert_eq!(pair(&g), fill_for(font), "id {raw}");
        assert_eq!(
            g.glyph.advance_width,
            font.glyph_width(u16::from(ENCODED_A)).unwrap(),
            "id {raw}"
        );
        assert_eq!(printer.active_font(), Some(font));
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
        assert_eq!(
            printer.cleared_span().unwrap().height,
            clear_span_height(font),
            "id {raw}"
        );
    }
}

#[test]
fn font_can_return_to_the_initial_source() {
    let mut printer = multi_font_printer_over(
        FontId::Normal,
        &[
            EXT,
            FONT_CODE,
            2,
            ENCODED_A,
            EXT,
            FONT_CODE,
            1,
            ENCODED_A,
            crate::text::EOS,
        ],
    );
    assert_eq!(pair(&next_glyph(&mut printer)), fill_for(FontId::Short));
    assert_eq!(pair(&next_glyph(&mut printer)), fill_for(FontId::Normal));
}

#[test]
fn an_unmodelled_font_id_draws_no_fallback_glyph_until_a_modelled_one_is_selected() {
    for raw in [6, 9, 10, 255] {
        let mut printer = multi_font_printer_over(
            FontId::Normal,
            &[
                EXT,
                FONT_CODE,
                raw,
                ENCODED_A,
                ENCODED_A,
                EXT,
                FONT_CODE,
                7,
                ENCODED_A,
                crate::text::EOS,
            ],
        );
        // Both glyphs under the unmodelled id are dropped without
        // advancing the cursor; the first event is the Narrow glyph.
        let g = next_glyph(&mut printer);
        assert_eq!((g.x, pair(&g)), (0, fill_for(FontId::Narrow)), "id {raw}");
        assert_eq!(printer.active_font(), Some(FontId::Narrow));
    }
    let mut printer =
        multi_font_printer_over(FontId::Normal, &[EXT, FONT_CODE, 6, crate::text::EOS]);
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Finished);
    assert_eq!(printer.active_font(), None);
}

#[test]
fn a_font_a_single_source_printer_does_not_carry_draws_nothing() {
    let pixels = multi_font_pixels(FontId::Normal);
    let mut printer = Printer::new(
        decode_tokens(&[
            EXT,
            FONT_CODE,
            0,
            ENCODED_A,
            EXT,
            FONT_CODE,
            1,
            ENCODED_A,
            crate::text::EOS,
        ]),
        synthetic_sheet(&pixels, FontId::Normal),
        TextSpeed::Instant,
        (0, 0),
    );
    // The Small glyph is dropped; the Normal one that follows renders.
    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph")
    };
    assert_eq!((g.x, g.glyph.pixels[0]), (0, fill_for(FontId::Normal).0));
    assert_eq!(printer.active_font(), Some(FontId::Normal));
}

#[test]
fn font_leaves_newline_and_scroll_metrics_on_the_template_font() {
    let mut printer = multi_font_printer_over(
        FontId::Normal,
        &[
            EXT,
            FONT_CODE,
            8,
            crate::text::CHAR_NEWLINE,
            ENCODED_A,
            crate::text::EOS,
        ],
    )
    .with_line_spacing(1);
    let g = next_glyph(&mut printer);
    // Normal's 16 + 1, not SmallNarrow's 8 + 1.
    assert_eq!((g.x, g.y), (0, 17));
    assert_eq!(pair(&g), fill_for(FontId::SmallNarrow));
    assert_eq!(printer.selected_font(), FontId::Normal);
}

#[test]
fn font_keeps_the_scroll_distance_of_the_template_font() {
    let sources = GlyphSources::try_from_fn(|font| {
        let pixels = multi_font_pixels(font);
        Ok::<_, assets::error::AssetError>(synthetic_sheet(&pixels, font).to_owned_sheet())
    })
    .unwrap();
    let mut printer = Printer::with_sources(
        decode_tokens(&[
            EXT,
            FONT_CODE,
            8,
            crate::text::CHAR_PROMPT_SCROLL,
            crate::text::EOS,
        ]),
        sources,
        FontId::Normal,
        TextSpeed::Fast,
        (0, 1),
    )
    .with_line_spacing(1);
    assert_eq!(
        printer.tick(PrinterInput::none()),
        TickEvent::AwaitingScroll
    );
    assert_eq!(printer.tick(press_a()), TickEvent::ScrollStarted);
    let mut total = 0;
    loop {
        match printer.tick(PrinterInput::none()) {
            TickEvent::Scrolling { dy } => total += dy,
            TickEvent::ScrollFinished => break,
            other => panic!("unexpected event {other:?}"),
        }
    }
    // Normal's 16 + 1, not SmallNarrow's 8 + 1.
    assert_eq!(total, 17);
}

#[test]
fn font_costs_a_reveal_delay_period_like_other_repeat_controls() {
    fn frames_to_second_glyph(bytes: &[u8]) -> u32 {
        let sources = GlyphSources::try_from_fn(|font| {
            let pixels = multi_font_pixels(font);
            Ok::<_, assets::error::AssetError>(synthetic_sheet(&pixels, font).to_owned_sheet())
        })
        .unwrap();
        let mut printer = Printer::with_sources(
            decode_tokens(bytes),
            sources,
            FontId::Normal,
            TextSpeed::Mid,
            (0, 0),
        );
        let mut glyphs = 0;
        for frame in 1..200 {
            if matches!(printer.tick(PrinterInput::none()), TickEvent::Glyph(_)) {
                glyphs += 1;
                if glyphs == 2 {
                    return frame;
                }
            }
        }
        panic!("second glyph never revealed")
    }
    let plain = frames_to_second_glyph(&[ENCODED_A, ENCODED_A, crate::text::EOS]);
    let with_font =
        frames_to_second_glyph(&[ENCODED_A, EXT, FONT_CODE, 0, ENCODED_A, crate::text::EOS]);
    // One more token handled before the glyph, each preceded by the Mid
    // period of 4 reveal-delay frames.
    assert_eq!(with_font - plain, u32::from(TextSpeed::Mid.wait_frames()));
}

#[test]
fn restart_returns_to_the_initial_font_and_clear_height() {
    for raw in [8, 6] {
        let mut printer = multi_font_printer_over(
            FontId::Normal,
            &[
                EXT,
                FONT_CODE,
                raw,
                ENCODED_A,
                EXT,
                CLEAR_CODE,
                2,
                crate::text::EOS,
            ],
        );
        let _ = printer.tick(PrinterInput::none());
        while printer.tick(PrinterInput::none()) != TickEvent::Finished {}
        printer.restart(decode_tokens(&[
            ENCODED_A,
            EXT,
            CLEAR_CODE,
            2,
            crate::text::EOS,
        ]));
        assert_eq!(printer.active_font(), Some(FontId::Normal));
        let g = next_glyph(&mut printer);
        assert_eq!(pair(&g), fill_for(FontId::Normal));
        assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
        assert_eq!(printer.cleared_span().unwrap().height, 15);
    }
}
