use super::super::tests::{blank_sheet_pixels, decode_tokens, synthetic_sheet, ENCODED_A};
use super::super::{Printer, PrinterInput, TextSpeed, TickEvent};
use super::*;
use crate::text::Token;
use assets::fonts::FontId;

#[test]
fn keypad_icon_reserves_its_width_before_the_next_glyph() {
    let pixels = blank_sheet_pixels();
    let sheet = synthetic_sheet(&pixels, FontId::Normal);
    let tokens = decode_tokens(&[
        crate::text::CHAR_KEYPAD_ICON,
        0x00,
        ENCODED_A,
        crate::text::EOS,
    ]);
    assert_eq!(tokens[0], Token::KeypadIcon(0));
    let mut printer = Printer::new(tokens, sheet, TextSpeed::Instant, (0, 1));
    assert_eq!(printer.tick(PrinterInput::none()), TickEvent::Idle);
    assert_eq!(
        printer.keypad_icon(),
        Some(KeypadIconPlacement {
            x: 0,
            y: 1,
            id: 0,
            width: 8,
            height: 12
        })
    );
    assert_eq!(printer.cursor(), (8, 1));
    let TickEvent::Glyph(a) = printer.tick(PrinterInput::none()) else {
        panic!("expected glyph");
    };
    assert_eq!(a.x, 8);
    assert_eq!(printer.keypad_icon(), None);
}

#[test]
fn keypad_icon_widths_follow_the_source_table() {
    let widths: Vec<u8> = (0..=12).map(|i| keypad_icon_size(i).unwrap().0).collect();
    assert_eq!(widths, [8, 8, 16, 16, 24, 24, 8, 8, 8, 8, 8, 8, 8]);
    assert_eq!(keypad_icon_size(13), None);
}
