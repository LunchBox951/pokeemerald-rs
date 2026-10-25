//! Glyph-placement tests: a pack that draws only chosen glyphs' left pixel
//! column shows where the bag screen placed each glyph.

use assets::items::{ItemId, Pocket};
use engine::save::bag::Bag;
use platform::Buttons;
use rendering::Framebuffer;

use super::pack_fixture::{entries_with_marked_narrow_glyphs, load, BAG_TEXT};
use super::tests::{frame_of, painted, press, sample_bag, LIST_X, LIST_Y, ROW};
use super::{open_from_pack, BagScreen, BagScreenMode, BagScreenRequest};

/// The window-local x of every pixel column of list row `row` that the narrow
/// font drew in the text colour. Under a pack that marks only some glyphs'
/// left columns, each is one glyph's placement.
fn drawn_columns(fb: &Framebuffer, row: usize) -> Vec<usize> {
    (LIST_X..Framebuffer::WIDTH)
        .filter(|&x| painted(fb, x, LIST_Y + row * ROW, 1, ROW, BAG_TEXT))
        .map(|x| x - LIST_X)
        .collect()
}

fn marked_screen(marked: &[usize], bag: &Bag, pockets_right: usize) -> BagScreen {
    let pack = load("marked", entries_with_marked_narrow_glyphs(marked));
    let mut screen = open_from_pack(&pack, bag, BagScreenRequest::new(BagScreenMode::Field))
        .expect("the synthetic pack opens");
    for _ in 0..pockets_right {
        screen.tick(press(Buttons::RIGHT));
    }
    screen
}

fn narrow_id(c: char) -> usize {
    usize::from(engine::text::char_to_byte(c).expect("the glyph encodes"))
}

#[test]
fn the_pokeblock_case_row_draws_the_pokeblock_glyphs() {
    use engine::text::Token;
    // `{POKEBLOCK}` is the five glyphs 0x55-0x59 (`charmap.txt:52`); the
    // placeholder never reaches the font as letters.
    let tokens = super::chrome::tokens_of("{POKEBLOCK} CASE");
    assert!(tokens[..5]
        .iter()
        .all(|token| matches!(token, Token::Symbol(_))));
    assert_eq!(tokens.len(), 5 + " CASE".len());

    let mut bag = Bag::default();
    assert!(bag.add_item(ItemId::POKEBLOCK_CASE, 1));
    let screen = marked_screen(&[0x55, 0x56, 0x57, 0x58, 0x59], &bag, 4);
    assert_eq!(screen.menu.pocket(), Pocket::KeyItems);
    // The name starts at x 8 and each glyph advances by its `gFontNarrow`
    // width (fonts.c): 8, 7, 8, 8, 4.
    assert_eq!(
        drawn_columns(&frame_of(&screen), 0),
        vec![8, 16, 23, 31, 39]
    );
}

#[test]
fn quantities_pad_with_the_five_pixel_spacer_not_a_space() {
    // `gFontNarrowLatinGlyphWidths`: CHAR_SPACER (0x77) is 5 wide, a space 3.
    assert_eq!(assets::fonts::FontId::Narrow.glyph_width(0x77), Some(5));
    assert_eq!(assets::fonts::FontId::Narrow.glyph_width(0x00), Some(3));
    let times = i32::from(
        assets::fonts::FontId::Narrow
            .glyph_width(u16::try_from(narrow_id('×')).unwrap())
            .unwrap(),
    );
    let marked = [narrow_id('×'), narrow_id('5'), narrow_id('7'), 0x77];
    let column = |x: i32| usize::try_from(x).unwrap();

    // Two digits wide in the items pocket: "×", one spacer, "5", ending at 119
    // (`item_menu.c:965`, `BAG_ITEM_CAPACITY_DIGITS`).
    let screen = marked_screen(&marked, &sample_bag(), 0);
    let start = 119 - times - 5 - 5;
    assert_eq!(
        drawn_columns(&frame_of(&screen), 0),
        vec![
            column(start),
            column(start + times),
            column(start + times + 5)
        ],
    );

    // Three digits wide in the berry pocket: two spacers before the "7".
    let screen = marked_screen(&marked, &sample_bag(), 3);
    assert_eq!(screen.menu.pocket(), Pocket::Berries);
    let start = 119 - times - 5 - 5 - 5;
    assert_eq!(
        drawn_columns(&frame_of(&screen), 0),
        vec![
            column(start),
            column(start + times),
            column(start + times + 5),
            column(start + times + 10),
        ],
    );
}
