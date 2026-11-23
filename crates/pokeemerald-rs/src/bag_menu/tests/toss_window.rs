//! The TOSS prompt's windows: where the quantity and yes/no windows sit,
//! what they print, and what the rest of the screen shows meanwhile.

use assets::items::ItemId;
use engine::save::bag::{Bag, ItemSlot};
use platform::Buttons;

use super::super::pack_fixture::{
    entries_with_marked_normal_glyphs, load, ARROW_FAR, ARROW_NEAR, BAG_TEXT, GRAY_CURSOR_TEXT,
    TEXT, WINDOW_FILL,
};
use super::super::toss::TossStage;
use super::super::{open_from_pack, BagScreen, BagScreenMode, BagScreenRequest};
use super::toss::{
    bordered, choose_toss, open_ok, potions, quantity_screen, shown, stage, CONTEXT, QUANTITY,
    YES_NO,
};
use super::{
    frame_of, icon_color, open_screen, painted, sample_bag, switch_pocket, DESCRIPTION_Y,
    ICON_PIXEL, LIST_X, LIST_Y, ROW,
};

#[test]
fn toss_opens_the_quantity_window_at_upstream_tile() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    open_ok(&mut screen, Buttons::A);
    assert!(shown(&frame_of(&screen), CONTEXT, TEXT));
    assert_eq!(stage(&screen), None);

    open_ok(&mut screen, Buttons::DOWN);
    open_ok(&mut screen, Buttons::A);
    assert_eq!(stage(&screen), Some(TossStage::Quantity { count: 1 }));
    let fb = frame_of(&screen);
    assert!(bordered(&fb, QUANTITY), "the window has its border");
    assert!(shown(&fb, QUANTITY, WINDOW_FILL) && shown(&fb, QUANTITY, TEXT));
    assert!(
        !shown(&fb, (120, 120, 56, 24), TEXT),
        "the choice window is gone"
    );
    assert!(!bordered(&fb, YES_NO), "no yes/no yet");
    assert!(
        painted(&fb, 3, DESCRIPTION_Y + 1, 100, 48, BAG_TEXT),
        "the description asks how many"
    );
}

#[test]
fn the_prompt_keeps_the_row_gray_with_its_icon_and_hides_the_arrows() {
    let mut bag = Bag::default();
    for (index, slot) in bag.items[..10].iter_mut().enumerate() {
        *slot = ItemSlot {
            item_id: ItemId::POTION.0,
            quantity: u16::try_from(index + 2).unwrap(),
        };
    }
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    let any_arrow = |screen: &BagScreen| {
        let fb = frame_of(screen);
        painted(&fb, 0, 0, 240, 160, ARROW_NEAR) || painted(&fb, 0, 0, 240, 160, ARROW_FAR)
    };
    assert!(any_arrow(&screen), "ten stacks overflow the window");

    choose_toss(&mut screen);
    let fb = frame_of(&screen);
    assert!(!any_arrow(&screen), "the arrows are destroyed");
    assert!(painted(&fb, LIST_X, LIST_Y, 6, ROW, GRAY_CURSOR_TEXT));
    assert!(!painted(&fb, LIST_X, LIST_Y, 6, ROW, BAG_TEXT));
    assert_eq!(
        fb.pixel(ICON_PIXEL.0, ICON_PIXEL.1),
        Some(icon_color("potion"))
    );

    open_ok(&mut screen, Buttons::B);
    assert!(any_arrow(&screen), "they return with the list");
}

#[test]
fn a_stack_of_one_asks_without_the_quantity_window() {
    let mut screen = open_screen(BagScreenMode::Field, &potions(1));
    choose_toss(&mut screen);
    assert_eq!(
        stage(&screen),
        Some(TossStage::Confirm {
            count: 1,
            yes: true
        })
    );
    let fb = frame_of(&screen);
    assert!(bordered(&fb, YES_NO) && shown(&fb, YES_NO, WINDOW_FILL));
}

#[test]
fn the_yes_no_window_sits_at_upstream_tile_and_replaces_the_quantity() {
    let mut screen = quantity_screen();
    open_ok(&mut screen, Buttons::A);
    let fb = frame_of(&screen);
    assert!(bordered(&fb, YES_NO), "the window has its border");
    assert!(shown(&fb, YES_NO, TEXT), "YES and NO print");
    assert!(painted(&fb, 3, DESCRIPTION_Y + 1, 100, 48, BAG_TEXT));
}

#[test]
fn the_message_shows_alone_once_yes_is_taken() {
    let mut screen = quantity_screen();
    open_ok(&mut screen, Buttons::A);
    open_ok(&mut screen, Buttons::A);
    let fb = frame_of(&screen);
    assert!(!bordered(&fb, YES_NO) && !bordered(&fb, QUANTITY));
    assert!(painted(&fb, 3, DESCRIPTION_Y + 1, 100, 48, BAG_TEXT));
}

#[test]
fn the_count_prints_centred_with_leading_zeros() {
    // `PrintItemQuantity` (`item_menu.c:1192-1197`): "×01" centred in 40
    // pixels, two digits for an item.
    let mut screen = marked_screen(&sample_bag(), 0);
    choose_toss(&mut screen);
    let width = glyph_width('×') + glyph_width('0') + glyph_width('1');
    let start = (40 - width) / 2;
    assert_eq!(
        quantity_columns(&screen),
        vec![
            start,
            start + glyph_width('×'),
            start + glyph_width('×') + glyph_width('0')
        ]
    );
    open_ok(&mut screen, Buttons::UP);
    let width = glyph_width('×') + glyph_width('0') + glyph_width('2');
    assert_eq!(quantity_columns(&screen)[0], (40 - width) / 2);
}

#[test]
fn a_berry_count_has_three_digits() {
    let mut screen = marked_screen(&sample_bag(), 3);
    for button in [Buttons::A, Buttons::DOWN, Buttons::DOWN, Buttons::A] {
        open_ok(&mut screen, button);
    }
    let width = glyph_width('×') + 3 * glyph_width('0');
    let start = (40 - width) / 2;
    let columns = quantity_columns(&screen);
    assert_eq!(columns.len(), 4, "× and three digits");
    assert_eq!(columns[0], start);
}

#[test]
fn the_yes_no_cursor_sits_left_of_its_row() {
    // `CreateYesNoMenu` prints at (8, 1) and the cursor stands at x 0, 16 rows
    // apart (`menu.c:1623-1645`).
    let marked: Vec<usize> = ['▶', 'Y'].into_iter().map(normal_id).collect();
    let pack = load("toss-yes-no", entries_with_marked_normal_glyphs(&marked));
    let mut screen = open_from_pack(
        &pack,
        &sample_bag(),
        BagScreenRequest::new(BagScreenMode::Field),
    )
    .expect("the synthetic pack opens");
    choose_toss(&mut screen);
    open_ok(&mut screen, Buttons::A);
    let rows_at = |screen: &BagScreen, x: usize| -> Vec<usize> {
        let fb = frame_of(screen);
        (YES_NO.1..YES_NO.1 + YES_NO.3)
            .filter(|&y| painted(&fb, x, y, 1, 1, TEXT))
            .map(|y| y - YES_NO.1)
            .collect()
    };
    let (cursor_rows, label_rows) = (rows_at(&screen, YES_NO.0), rows_at(&screen, YES_NO.0 + 8));
    let top = |rows: &[usize]| rows.iter().copied().min();
    assert_eq!(top(&cursor_rows), Some(1), "the cursor starts a pixel down");
    assert_eq!(top(&label_rows), Some(1), "YES starts a pixel down");
    open_ok(&mut screen, Buttons::DOWN);
    let moved = rows_at(&screen, YES_NO.0);
    assert_eq!(top(&moved), Some(17), "the cursor is on NO, 16 rows down");
}

/// The window-local x of each pixel column the normal font drew in the window
/// text colour over the quantity window.
fn quantity_columns(screen: &BagScreen) -> Vec<usize> {
    let fb = frame_of(screen);
    (QUANTITY.0..QUANTITY.0 + QUANTITY.2)
        .filter(|&x| painted(&fb, x, QUANTITY.1, 1, QUANTITY.3, TEXT))
        .map(|x| x - QUANTITY.0)
        .collect()
}

fn normal_id(c: char) -> usize {
    usize::from(engine::text::char_to_byte(c).expect("the glyph encodes"))
}

fn glyph_width(c: char) -> usize {
    let id = u16::try_from(normal_id(c)).unwrap();
    usize::from(assets::fonts::FontId::Normal.glyph_width(id).unwrap())
}

fn marked_screen(bag: &Bag, pockets: usize) -> BagScreen {
    let marked: Vec<usize> = ['×', '0', '1', '2', '3']
        .into_iter()
        .map(normal_id)
        .collect();
    let pack = load("toss-marked", entries_with_marked_normal_glyphs(&marked));
    let mut screen = open_from_pack(&pack, bag, BagScreenRequest::new(BagScreenMode::Field))
        .expect("the synthetic pack opens");
    for _ in 0..pockets {
        switch_pocket(&mut screen, Buttons::RIGHT);
    }
    screen
}
