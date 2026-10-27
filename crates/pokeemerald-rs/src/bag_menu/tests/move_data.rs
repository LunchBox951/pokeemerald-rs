//! The TM/HM move-data windows shown in place of the description while a
//! machine's context menu is open.

use assets::items::{ItemId, Pocket};
use engine::save::bag::Bag;
use platform::Buttons;
use rendering::Framebuffer;

use super::super::pack_fixture::{
    ACCURACY_LABEL, BAG_TEXT, FIGHTING_TYPE_ICON, NORMAL_TYPE_ICON, POISON_TYPE_ICON, POWER_LABEL,
    PP_LABEL, TYPE_LABEL,
};
use super::super::{BagScreen, BagScreenMode, BagScreenTick};
use super::{at, frame_of, open_screen, painted, press, sample_bag, switch_pocket, DESCRIPTION_Y};

/// The TM/HM move-data window's two windows (`sDefaultBagWindows`,
/// `item_menu.c:422-437`): the label window at tile (1, 13), 5x6 tiles, and the
/// value window at tile (7, 13), 4x6 tiles.
const LABEL_WINDOW: (usize, usize, usize) = (8, DESCRIPTION_Y, 40);
const VALUE_WINDOW: (usize, usize, usize) = (56, DESCRIPTION_Y, 32);
const MOVE_DATA_ROW: usize = 12;
const MOVE_DATA_WINDOWS_HEIGHT: usize = 48;

/// A bag holding only `item`, in its pocket with the context menu open.
fn context_screen(item: ItemId) -> BagScreen {
    let mut bag = Bag::default();
    assert!(bag.add_item(item, 1));
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    for _ in 0..2 {
        switch_pocket(&mut screen, Buttons::RIGHT);
    }
    assert_eq!(screen.menu.pocket(), Pocket::TmHm);
    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    assert!(screen.menu.context_grid().is_some());
    screen
}

fn window_has_move_data(fb: &Framebuffer) -> bool {
    [
        TYPE_LABEL,
        POWER_LABEL,
        ACCURACY_LABEL,
        PP_LABEL,
        NORMAL_TYPE_ICON,
        FIGHTING_TYPE_ICON,
        POISON_TYPE_ICON,
    ]
    .into_iter()
    .any(|color| {
        painted(
            fb,
            LABEL_WINDOW.0,
            DESCRIPTION_Y,
            VALUE_WINDOW.0 + VALUE_WINDOW.2 - LABEL_WINDOW.0,
            MOVE_DATA_WINDOWS_HEIGHT,
            color,
        )
    })
}

#[test]
fn tmhm_context_menu_draws_the_move_data_labels() {
    // `PrepareTMHMMoveWindow` blits TYPE, POWER, ACCURACY and PP down the label
    // window at y 0, 12, 24 and 36; its 5 tiles (40 px) clip the 42 px icons.
    let fb = frame_of(&context_screen(ItemId::TM_FOCUS_PUNCH));
    let (left, top, width) = LABEL_WINDOW;
    for (row, color) in [TYPE_LABEL, POWER_LABEL, ACCURACY_LABEL, PP_LABEL]
        .into_iter()
        .enumerate()
    {
        let y = top + row * MOVE_DATA_ROW;
        assert_eq!(at(&fb, left, y), color, "row {row} top-left");
        assert_eq!(
            at(&fb, left + width - 1, y + MOVE_DATA_ROW - 1),
            color,
            "row {row} bottom-right"
        );
        assert_ne!(at(&fb, left + width, y), color, "row {row} clips at 40 px");
        assert_ne!(
            at(&fb, left - 1, y),
            color,
            "row {row} starts at the window"
        );
    }
}

#[test]
fn tmhm_context_menu_draws_the_moves_type_icon() {
    // `BlitMenuInfoIcon(WIN_TMHM_INFO, gBattleMoves[move].type + 1, 0, 0)`.
    for (item, color) in [
        (ItemId::TM_FOCUS_PUNCH, FIGHTING_TYPE_ICON),
        (ItemId::TM_TOXIC, POISON_TYPE_ICON),
        (ItemId::TM_PROTECT, NORMAL_TYPE_ICON),
        (ItemId::HM_CUT, NORMAL_TYPE_ICON),
    ] {
        let fb = frame_of(&context_screen(item));
        let (left, top, width) = VALUE_WINDOW;
        assert_eq!(at(&fb, left, top), color, "{item:?} top-left");
        assert_eq!(
            at(&fb, left + width - 1, top + MOVE_DATA_ROW - 1),
            color,
            "{item:?} bottom-right"
        );
        assert_ne!(at(&fb, left + width, top), color, "{item:?} is 32 px wide");
        assert_ne!(
            at(&fb, left, top + MOVE_DATA_ROW),
            color,
            "{item:?} is 12 px tall"
        );
    }
}

#[test]
fn the_move_data_window_replaces_the_description_only_in_the_open_context_menu() {
    let mut screen = context_screen(ItemId::TM_FOCUS_PUNCH);
    assert!(window_has_move_data(&frame_of(&screen)));
    assert_eq!(screen.tick(press(Buttons::B)), BagScreenTick::Open);
    assert!(screen.menu.context_grid().is_none());
    let fb = frame_of(&screen);
    assert!(!window_has_move_data(&fb), "ReturnToItemList clears it");
    assert!(painted(&fb, 3, DESCRIPTION_Y + 1, 100, 48, BAG_TEXT));

    // Browsing the TM/HM pocket shows the description, not the move data.
    let mut bag = Bag::default();
    assert!(bag.add_item(ItemId::TM_FOCUS_PUNCH, 1));
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    for _ in 0..2 {
        switch_pocket(&mut screen, Buttons::RIGHT);
    }
    assert!(!window_has_move_data(&frame_of(&screen)));
}

#[test]
fn other_pockets_do_not_draw_the_move_data_window() {
    // `OpenContextMenu` prints "X is selected." outside the TM/HM pocket.
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    assert!(screen.menu.context_grid().is_some());
    assert!(!window_has_move_data(&frame_of(&screen)));
}
