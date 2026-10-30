//! Restart, owned sheets, and multi-source printers.

use super::*;

#[test]
fn restart_reprints_a_new_stream_from_the_top_keeping_the_sheet() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[ENCODED_A, crate::text::EOS]);
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (3, 1));

    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Glyph(_)
    ));
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Finished
    ));
    assert!(printer.is_finished());

    printer.restart(decode_tokens(&[ENCODED_A, ENCODED_A, crate::text::EOS]));
    assert!(!printer.is_finished(), "a restarted printer prints again");
    assert_eq!(printer.cursor(), (3, 1), "the cursor is back at the origin");

    let TickEvent::Glyph(first) = printer.tick(PrinterInput::none()) else {
        panic!("expected the restarted stream's first glyph")
    };
    assert_eq!((first.x, first.y), (3, 1));
    let TickEvent::Glyph(second) = printer.tick(PrinterInput::none()) else {
        panic!("expected the restarted stream's second glyph")
    };
    assert_eq!(
        (second.x, second.y),
        (3 + i32::from(NORMAL_A_ADVANCE_WIDTH), 1),
        "advanced by the first glyph's width"
    );
    assert!(matches!(
        printer.tick(PrinterInput::none()),
        TickEvent::Finished
    ));
}

#[test]
fn a_printer_over_an_owned_sheet_outlives_the_pack_bytes_it_came_from() {
    let mut printer = {
        let pixels = blank_sheet_pixels();
        let owned = synthetic_sheet(&pixels, FontId::Normal).to_owned_sheet();
        Printer::new(
            decode_tokens(&[ENCODED_A, crate::text::EOS]),
            owned,
            TextSpeed::Instant,
            (0, 1),
        )
    };

    let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
        panic!("expected a glyph from the owned sheet")
    };
    assert_eq!((g.x, g.y), (0, 1));
    assert_eq!(g.glyph.advance_width, NORMAL_A_ADVANCE_WIDTH);
}

/// Per-font pixel pairs, each distinct within the 2-bit depth, so a
/// glyph's first two pixels name its source.
fn fill_for(font: FontId) -> (u8, u8) {
    match font {
        FontId::Small => (1, 0),
        FontId::Normal => (2, 0),
        FontId::Short => (3, 0),
        FontId::Narrow => (0, 1),
        FontId::SmallNarrow => (1, 1),
    }
}

/// A sheet alternating `fill_for(font)` across each row, so every glyph
/// starts with that pair.
fn multi_font_pixels(font: FontId) -> Vec<u8> {
    let (even, odd) = fill_for(font);
    (0..SHEET_WIDTH * SHEET_HEIGHT)
        .map(|index| if index % 2 == 0 { even } else { odd })
        .collect()
}

fn multi_font_printer(initial: FontId) -> Printer<assets::fonts::OwnedFontGlyphSheet> {
    let sources = GlyphSources::try_from_fn(|font| {
        let pixels = multi_font_pixels(font);
        Ok::<_, assets::error::AssetError>(synthetic_sheet(&pixels, font).to_owned_sheet())
    })
    .unwrap();
    Printer::with_sources(
        decode_tokens(&[ENCODED_A, crate::text::EOS]),
        sources,
        initial,
        TextSpeed::Instant,
        (0, 0),
    )
}

#[test]
fn a_multi_source_printer_retains_separately_identified_font_data() {
    let printer = multi_font_printer(FontId::Normal);
    assert_eq!(printer.selected_font(), FontId::Normal);
    for font in FontId::ALL {
        let source = printer
            .source(font)
            .unwrap_or_else(|| panic!("{font:?} source must be carried"));
        assert_eq!(source.font(), font);
        let glyph = source.glyph(1).unwrap();
        assert_eq!(
            (glyph.pixels[0], glyph.pixels[1]),
            fill_for(font),
            "{font:?} pixels"
        );
        assert_eq!(glyph.advance_width, font.glyph_width(1).unwrap());
    }
    // Normal and an alternate font are different data, not one aliased sheet.
    let normal = printer.source(FontId::Normal).unwrap().glyph(1).unwrap();
    let short = printer.source(FontId::Short).unwrap().glyph(1).unwrap();
    assert_ne!(normal.pixels[0], short.pixels[0]);
    assert_ne!(normal.advance_width, 0);
}

#[test]
fn unchanged_text_renders_from_the_initial_source_not_an_alternate() {
    for initial in FontId::ALL {
        let mut printer = multi_font_printer(initial);
        let TickEvent::Glyph(g) = printer.tick(PrinterInput::none()) else {
            panic!("expected a glyph")
        };
        assert_eq!(
            (g.glyph.pixels[0], g.glyph.pixels[1]),
            fill_for(initial),
            "{initial:?}"
        );
        assert_eq!(
            g.glyph.advance_width,
            initial.glyph_width(u16::from(ENCODED_A)).unwrap()
        );
        assert_eq!(printer.selected_font(), initial);
    }
}

#[test]
fn a_single_source_printer_carries_only_its_one_source() {
    let pixels = blank_sheet_pixels();
    let printer = Printer::new(
        decode_tokens(&[crate::text::EOS]),
        synthetic_sheet(&pixels, FontId::Normal),
        TextSpeed::Instant,
        (0, 0),
    );
    assert!(printer.source(FontId::Normal).is_some());
    assert!(printer.source(FontId::Small).is_none());
}
