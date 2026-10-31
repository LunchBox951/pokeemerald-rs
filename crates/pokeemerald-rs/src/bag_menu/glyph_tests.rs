//! Glyph-placement tests: a pack that draws only chosen glyphs' left pixel
//! column shows where the bag screen placed each glyph.

use assets::items::{ItemId, Pocket};
use engine::save::bag::Bag;
use platform::Buttons;
use rendering::Framebuffer;

use super::pack_fixture::{
    entries_with_marked_narrow_glyphs, entries_with_marked_normal_glyphs, load, BAG_TEXT,
    MOVE_DATA_TEXT,
};
use super::tests::{frame_of, painted, press, sample_bag, switch_pocket, LIST_X, LIST_Y, ROW};
use super::{open_from_pack, BagScreen, BagScreenMode, BagScreenRequest, BagScreenTick};

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
        switch_pocket(&mut screen, Buttons::RIGHT);
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

#[test]
fn the_list_cursor_draws_from_the_normal_font() {
    // `BagMenu_PrintCursorAtPos` prints the arrow with `FONT_NORMAL` (`item_menu.c:1023`).
    let arrow = narrow_id('▶');
    let pack = load("marked-normal", entries_with_marked_normal_glyphs(&[arrow]));
    let screen = open_from_pack(
        &pack,
        &sample_bag(),
        BagScreenRequest::new(BagScreenMode::Field),
    )
    .expect("the synthetic pack opens");
    assert_eq!(drawn_columns(&frame_of(&screen), 0), vec![0]);
}

/// Columns of the TM/HM value window (`WIN_TMHM_INFO`, left edge 56) in which
/// the normal font drew in the move data's text colour, within the pixel rows
/// `[top, bottom)`. Each text line is a 16 px cell and the lines start 12 px
/// apart, so each band holds only the rows of one line.
fn value_columns(fb: &Framebuffer, band: (usize, usize)) -> Vec<usize> {
    (VALUE_LEFT..VALUE_LEFT + VALUE_WIDTH)
        .filter(|&x| painted(fb, x, band.0, 1, band.1 - band.0, MOVE_DATA_TEXT))
        .map(|x| x - VALUE_LEFT)
        .collect()
}

const VALUE_LEFT: usize = 56;
const VALUE_WIDTH: usize = 32;
/// Where `PrintTMHMMoveData` prints a value (`item_menu.c:2576,2588,2592`).
const VALUE_TEXT_X: usize = 7;
const POWER_BAND: (usize, usize) = (116, 128);
const ACCURACY_BAND: (usize, usize) = (132, 140);
const PP_BAND: (usize, usize) = (144, 152);

fn move_data_glyphs() -> Vec<usize> {
    "-0123456789".chars().map(narrow_id).collect()
}

/// Window-local columns of `text`'s glyphs, each placed after the advance of
/// those before it in the normal font. `\u{E077}` is `CHAR_SPACER`.
fn columns_of(text: &str) -> Vec<usize> {
    let mut x = VALUE_TEXT_X;
    let mut columns = Vec::new();
    for c in text.chars() {
        let id = if c == '\u{E077}' { 0x77 } else { narrow_id(c) };
        let width = assets::fonts::FontId::Normal
            .glyph_width(u16::try_from(id).unwrap())
            .unwrap();
        if c != '\u{E077}' {
            columns.push(x);
        }
        x += usize::from(width);
    }
    columns
}

fn move_data_columns(item: ItemId) -> [Vec<usize>; 3] {
    let pack = load(
        "marked-move-data",
        entries_with_marked_normal_glyphs(&move_data_glyphs()),
    );
    let mut bag = Bag::default();
    assert!(bag.add_item(item, 1));
    let mut screen = open_from_pack(&pack, &bag, BagScreenRequest::new(BagScreenMode::Field))
        .expect("the synthetic pack opens");
    for _ in 0..2 {
        switch_pocket(&mut screen, Buttons::RIGHT);
    }
    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    let fb = frame_of(&screen);
    [POWER_BAND, ACCURACY_BAND, PP_BAND].map(|band| value_columns(&fb, band))
}

#[test]
fn move_stats_print_right_aligned_in_three_positions() {
    // `ConvertIntToDecimalStringN(.., STR_CONV_MODE_RIGHT_ALIGN, 3)` pads with
    // `CHAR_SPACER`; Focus Punch is power 150, accuracy 100, PP 20.
    let spacer = "\u{E077}";
    assert_eq!(
        move_data_columns(ItemId::TM_FOCUS_PUNCH),
        [
            columns_of("150"),
            columns_of("100"),
            columns_of(&format!("{spacer}20")),
        ]
    );
}

#[test]
fn a_power_under_two_or_no_accuracy_prints_three_dashes() {
    // Toxic: power 0, accuracy 85, PP 10. Protect: power 0, accuracy 0, PP 10.
    let spacer = "\u{E077}";
    assert_eq!(
        move_data_columns(ItemId::TM_TOXIC),
        [
            columns_of("---"),
            columns_of(&format!("{spacer}85")),
            columns_of(&format!("{spacer}10")),
        ]
    );
    assert_eq!(
        move_data_columns(ItemId::TM_PROTECT),
        [
            columns_of("---"),
            columns_of("---"),
            columns_of(&format!("{spacer}10")),
        ]
    );
}
