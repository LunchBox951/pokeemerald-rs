//! Screen tests over synthetic packs loaded through real `AssetPack` handles.

use assets::items::{ItemId, Pocket};
use engine::bag_menu::{BagAction, BagPositions};
use engine::save::bag::Bag;
use platform::{ButtonState, Buttons};
use rendering::{Framebuffer, Rgb888};

use super::pack_fixture::{
    bag_frame_color, bag_pack, entries, icon_color, load, BACKGROUND, BAG_TEXT, GRAY_CURSOR_SHADOW,
    GRAY_CURSOR_TEXT, HM_MARK, INDICATOR_CURRENT, INDICATOR_IDLE, NORMAL_SHADOW, TEXT,
    WINDOW_BORDER, WINDOW_FILL,
};
use super::{
    open, open_from_pack, BagScreen, BagScreenError, BagScreenMode, BagScreenRequest,
    BagScreenResult, BagScreenTick, SWITCH_TICKS,
};

pub(super) const LIST_X: usize = 112;
pub(super) const LIST_Y: usize = 17;
pub(super) const ROW: usize = 16;
const ICON_PIXEL: (usize, usize) = (24, 88);
const BAG_PIXEL: (usize, usize) = (68, 66);
const DESCRIPTION_Y: usize = 104;
const SHOWN_BASE: Rgb888 = Rgb888 { r: 7, g: 77, b: 7 };

pub(super) fn press(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state
}

/// Ticks with nothing pressed.
fn idle(screen: &mut BagScreen, ticks: u8) {
    for _ in 0..ticks {
        assert_eq!(screen.tick(ButtonState::new()), BagScreenTick::Open);
    }
}

/// Presses a pocket button and lets the switch it starts run out.
pub(super) fn switch_pocket(screen: &mut BagScreen, button: Buttons) {
    assert_eq!(screen.tick(press(button)), BagScreenTick::Open);
    idle(screen, SWITCH_TICKS);
}

/// Presses a pocket button, then idles until the switch is `ticks` old.
fn switch_for(screen: &mut BagScreen, button: Buttons, ticks: u8) {
    assert_eq!(screen.tick(press(button)), BagScreenTick::Open);
    idle(screen, ticks);
}

fn region(fb: &Framebuffer, x: usize, y: usize, w: usize, h: usize) -> Vec<Rgb888> {
    (y..y + h)
        .flat_map(|row| (x..x + w).map(move |col| at(fb, col, row)))
        .collect()
}

pub(super) fn sample_bag() -> Bag {
    let mut bag = Bag::default();
    assert!(bag.add_item(ItemId::POTION, 5));
    assert!(bag.add_item(ItemId::ANTIDOTE, 2));
    assert!(bag.add_item(ItemId::POKE_BALL, 3));
    assert!(bag.add_item(ItemId::CHERI_BERRY, 7));
    bag
}

fn open_screen(mode: BagScreenMode, bag: &Bag) -> BagScreen {
    open_with(BagScreenRequest::new(mode), bag)
}

fn open_with(request: BagScreenRequest, bag: &Bag) -> BagScreen {
    open_from_pack(&bag_pack("screen"), bag, request).expect("the synthetic pack opens")
}

pub(super) fn frame_of(screen: &BagScreen) -> Framebuffer {
    let mut base = Framebuffer::new();
    base.fill(SHOWN_BASE);
    screen.compose_over(base)
}

fn at(fb: &Framebuffer, x: usize, y: usize) -> Rgb888 {
    fb.pixel(x, y).expect("the pixel is on screen")
}

/// Whether any pixel of a region is `color`.
pub(super) fn painted(
    fb: &Framebuffer,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: Rgb888,
) -> bool {
    (y..y + h).any(|row| (x..x + w).any(|col| at(fb, col, row) == color))
}

fn row_has_text(fb: &Framebuffer, row: usize) -> bool {
    painted(fb, LIST_X + 8, LIST_Y + row * ROW, 100, ROW, BAG_TEXT)
}

fn cursor_row(fb: &Framebuffer) -> Option<usize> {
    (0..8).find(|&row| painted(fb, LIST_X, LIST_Y + row * ROW, 6, ROW, BAG_TEXT))
}

#[test]
fn selected_row_shows_cursor_icon_and_description() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    let fb = frame_of(&screen);

    assert_eq!(cursor_row(&fb), Some(0));
    assert!(row_has_text(&fb, 0) && row_has_text(&fb, 1) && row_has_text(&fb, 2));
    assert!(
        !row_has_text(&fb, 3),
        "the third row is past the two items and the close row"
    );
    assert_eq!(at(&fb, ICON_PIXEL.0, ICON_PIXEL.1), icon_color("potion"));
    assert!(painted(&fb, 3, DESCRIPTION_Y + 1, 100, 48, BAG_TEXT));

    assert_eq!(screen.tick(press(Buttons::DOWN)), BagScreenTick::Open);
    let fb = frame_of(&screen);
    assert_eq!(cursor_row(&fb), Some(1));
    assert_eq!(at(&fb, ICON_PIXEL.0, ICON_PIXEL.1), icon_color("antidote"));
}

#[test]
fn context_menu_grays_the_list_cursor_until_it_closes() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    let cursor =
        |screen: &BagScreen, color| painted(&frame_of(screen), LIST_X, LIST_Y, 6, ROW, color);
    assert!(cursor(&screen, BAG_TEXT) && cursor(&screen, NORMAL_SHADOW));
    assert!(!cursor(&screen, GRAY_CURSOR_SHADOW));

    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    assert!(
        cursor(&screen, GRAY_CURSOR_TEXT) && cursor(&screen, GRAY_CURSOR_SHADOW),
        "the cursor takes the gray text and green shadow from bank 1"
    );
    assert!(!cursor(&screen, BAG_TEXT), "no normal cursor pixel remains");

    assert_eq!(screen.tick(press(Buttons::B)), BagScreenTick::Open);
    assert!(cursor(&screen, BAG_TEXT) && cursor(&screen, NORMAL_SHADOW));
    assert!(!cursor(&screen, GRAY_CURSOR_SHADOW));
}

#[test]
fn the_supplied_frame_shows_through_transparent_screen_pixels() {
    let screen = open_screen(BagScreenMode::Battle, &sample_bag());
    let fb = frame_of(&screen);
    // The all-zero tilemap paints tile 0 everywhere but the tile-index pixels
    // the fixture leaves clear, so BACKGROUND must appear over SHOWN_BASE.
    assert_eq!(at(&fb, 1, 1), BACKGROUND);
    assert_ne!(at(&fb, 1, 1), SHOWN_BASE);
}

#[test]
fn quantity_prints_on_rows_but_not_for_key_items() {
    let mut bag = sample_bag();
    assert!(bag.add_item(ItemId::MACH_BIKE, 1));
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    let fb = frame_of(&screen);
    assert!(
        painted(&fb, LIST_X + 90, LIST_Y, 29, ROW, BAG_TEXT),
        "the item row has a quantity"
    );
    for _ in 0..4 {
        switch_pocket(&mut screen, Buttons::RIGHT);
    }
    assert_eq!(screen.menu.pocket(), Pocket::KeyItems);
    let fb = frame_of(&screen);
    assert!(
        !painted(&fb, LIST_X + 90, LIST_Y, 29, ROW, BAG_TEXT),
        "a key item shows no quantity"
    );
}

#[test]
fn pocket_movement_changes_bag_picture_indicator_and_rows() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    let fb = frame_of(&screen);
    assert_eq!(at(&fb, BAG_PIXEL.0, BAG_PIXEL.1), bag_frame_color(1));
    assert_eq!(at(&fb, 5 * 8 + 1, 3 * 8 + 1), INDICATOR_CURRENT);
    assert_eq!(at(&fb, 6 * 8 + 1, 3 * 8 + 1), INDICATOR_IDLE);

    switch_pocket(&mut screen, Buttons::RIGHT);
    let fb = frame_of(&screen);
    assert_eq!(screen.menu.pocket(), Pocket::PokeBalls);
    assert_eq!(at(&fb, BAG_PIXEL.0, BAG_PIXEL.1), bag_frame_color(3));
    assert_eq!(at(&fb, 5 * 8 + 1, 3 * 8 + 1), INDICATOR_IDLE);
    assert_eq!(at(&fb, 6 * 8 + 1, 3 * 8 + 1), INDICATOR_CURRENT);
    assert_eq!(at(&fb, ICON_PIXEL.0, ICON_PIXEL.1), icon_color("poke_ball"));

    // L is inert outside the L/R button mode.
    assert_eq!(screen.tick(press(Buttons::L)), BagScreenTick::Open);
    assert_eq!(screen.menu.pocket(), Pocket::PokeBalls);
    switch_pocket(&mut screen, Buttons::LEFT);
    assert_eq!(screen.menu.pocket(), Pocket::Items);
    switch_pocket(&mut screen, Buttons::LEFT);
    assert_eq!(screen.menu.pocket(), Pocket::KeyItems);
}

#[test]
fn shoulder_buttons_switch_pockets_only_in_lr_mode() {
    let mut request = BagScreenRequest::new(BagScreenMode::Field);
    request.lr_buttons = true;
    let mut screen = open_with(request, &sample_bag());
    switch_pocket(&mut screen, Buttons::R);
    assert_eq!(screen.menu.pocket(), Pocket::PokeBalls);
    switch_pocket(&mut screen, Buttons::L);
    assert_eq!(screen.menu.pocket(), Pocket::Items);
}

#[test]
fn an_empty_pocket_shows_only_the_close_row_and_closes() {
    let mut screen = open_screen(BagScreenMode::Field, &Bag::default());
    let fb = frame_of(&screen);
    assert_eq!(cursor_row(&fb), Some(0));
    assert!(
        row_has_text(&fb, 0) && !row_has_text(&fb, 1),
        "only CLOSE BAG is listed"
    );
    assert_eq!(
        at(&fb, ICON_PIXEL.0, ICON_PIXEL.1),
        icon_color("return_to_field_arrow")
    );

    assert_eq!(
        screen.tick(press(Buttons::A)),
        BagScreenTick::Closed(BagScreenResult::ReturnToField)
    );
}

#[test]
fn the_close_row_words_its_description_for_each_mode() {
    let description_width = |mode| {
        let fb = frame_of(&open_screen(mode, &Bag::default()));
        (0..110)
            .rev()
            .find(|&x| painted(&fb, x, DESCRIPTION_Y + 17, 1, 15, BAG_TEXT))
    };
    let field = description_width(BagScreenMode::Field);
    let battle = description_width(BagScreenMode::Battle);
    assert!(field.is_some() && battle.is_some());
    assert!(
        battle > field,
        "\"the battle.\" is wider than \"the field.\""
    );
}

#[test]
fn contextual_selection_opens_a_choice_window_then_returns_the_action() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    assert!(!painted(
        &frame_of(&screen),
        15 * 8,
        13 * 8,
        8,
        8,
        WINDOW_FILL
    ));

    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    let fb = frame_of(&screen);
    assert!(
        painted(&fb, 15 * 8, 13 * 8, 8 * 14, 8 * 6, WINDOW_BORDER),
        "the choice window has a border"
    );
    assert!(painted(&fb, 15 * 8 + 8, 15 * 8, 8 * 14, 8, WINDOW_FILL));
    assert!(
        painted(&fb, 15 * 8, 15 * 8, 14 * 8, 4 * 8, TEXT),
        "choices are printed"
    );

    // B closes only the choices.
    assert_eq!(screen.tick(press(Buttons::B)), BagScreenTick::Open);
    assert!(!painted(
        &frame_of(&screen),
        15 * 8,
        13 * 8,
        14 * 8,
        6 * 8,
        WINDOW_BORDER
    ));

    screen.tick(press(Buttons::A));
    let BagScreenTick::Action { action, selection } = screen.tick(press(Buttons::A)) else {
        panic!("confirming the first choice returns it");
    };
    assert_eq!(action, BagAction::Use);
    assert_eq!(selection.item, ItemId::POTION);
    assert_eq!(selection.quantity, 5);
    // The screen is still open, back to browsing.
    assert_eq!(screen.tick(press(Buttons::DOWN)), BagScreenTick::Open);
}

#[test]
fn cancel_returns_to_the_field_or_cancels_the_battle() {
    let mut field = open_screen(BagScreenMode::Field, &sample_bag());
    assert_eq!(
        field.tick(press(Buttons::B)),
        BagScreenTick::Closed(BagScreenResult::ReturnToField)
    );
    let mut battle = open_screen(BagScreenMode::Battle, &sample_bag());
    assert_eq!(
        battle.tick(press(Buttons::B)),
        BagScreenTick::Closed(BagScreenResult::Cancelled)
    );

    // A closed screen draws nothing and keeps reporting its result.
    let mut base = Framebuffer::new();
    base.fill(SHOWN_BASE);
    let after = battle.compose_over(base.clone());
    assert_eq!(after.pixels(), base.pixels());
    assert_eq!(
        battle.tick(press(Buttons::A)),
        BagScreenTick::Closed(BagScreenResult::Cancelled)
    );
}

#[test]
fn field_and_battle_modes_yield_the_same_typed_selection() {
    let bag = sample_bag();
    let mut field = open_screen(BagScreenMode::Field, &bag);
    field.tick(press(Buttons::DOWN));
    field.tick(press(Buttons::A));
    let BagScreenTick::Action {
        selection: from_field,
        ..
    } = field.tick(press(Buttons::A))
    else {
        panic!("field USE returns an action");
    };

    let mut battle = open_screen(BagScreenMode::Battle, &bag);
    battle.tick(press(Buttons::DOWN));
    battle.tick(press(Buttons::A));
    let BagScreenTick::Closed(BagScreenResult::Item(from_battle)) = battle.tick(press(Buttons::A))
    else {
        panic!("battle USE selects an item");
    };
    assert_eq!(from_field, from_battle);
    assert_eq!(from_battle.item, ItemId::ANTIDOTE);
}

#[test]
fn battle_selection_in_the_ball_pocket_is_a_ball() {
    let mut screen = open_screen(BagScreenMode::Battle, &sample_bag());
    switch_pocket(&mut screen, Buttons::RIGHT);
    screen.tick(press(Buttons::A));
    let BagScreenTick::Closed(BagScreenResult::Ball(selection)) = screen.tick(press(Buttons::A))
    else {
        panic!("a ball choice returns a ball");
    };
    assert_eq!(selection.item, ItemId::POKE_BALL);
    assert_eq!(selection.pocket, Pocket::PokeBalls);
}

#[test]
fn reopening_keeps_every_pockets_position() {
    let mut bag = sample_bag();
    assert!(bag.add_item(ItemId::GREAT_BALL, 1));
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    screen.tick(press(Buttons::DOWN));
    switch_pocket(&mut screen, Buttons::RIGHT);
    screen.tick(press(Buttons::DOWN));
    let before = frame_of(&screen);
    let kept = screen.positions();
    assert_eq!(kept.pocket, Pocket::PokeBalls);
    assert_ne!(kept, BagPositions::default());

    let mut request = BagScreenRequest::new(BagScreenMode::Field);
    request.positions = kept;
    let mut reopened = open_with(request, &bag);
    assert_eq!(frame_of(&reopened).pixels(), before.pixels());
    assert_eq!(reopened.positions(), kept);
    reopened.tick(press(Buttons::LEFT));
    assert_eq!(
        reopened.positions().cursor[0],
        1,
        "the first pocket kept its own cursor"
    );
}

#[test]
fn open_reads_the_pack_its_source_names() {
    let path: &'static std::path::Path = Box::leak(
        std::env::temp_dir()
            .join(format!(
                "pokeemerald-rs-bag-menu-source-{}.pack",
                std::process::id()
            ))
            .into_boxed_path(),
    );
    std::fs::write(path, crate::pack_test_support::pack_bytes(entries())).unwrap();
    let source = crate::pack_source::PackSource::Test(path);
    let opened = open(
        &source,
        &sample_bag(),
        BagScreenRequest::new(BagScreenMode::Field),
    );
    let _ = std::fs::remove_file(path);
    assert!(opened.is_ok());
}

#[test]
fn opening_fails_on_a_pack_without_the_bag_graphics() {
    let without_tilemap = entries()
        .into_iter()
        .filter(|entry| entry.id != "bag/raw/menu")
        .collect();
    let pack = load("missing", without_tilemap);
    let result = open_from_pack(
        &pack,
        &sample_bag(),
        BagScreenRequest::new(BagScreenMode::Field),
    );
    assert!(matches!(result, Err(BagScreenError::Pack(_))));
}

/// The checkout's extracted pack draws every pocket in both modes with
/// production assets.
#[test]
#[ignore = "needs a local pack extracted by `cargo xtask extract`"]
fn production_pack_draws_every_pocket_in_both_modes() {
    let pack = assets::AssetPack::load_repo().expect("the checkout pack loads");
    let mut bag = sample_bag();
    assert!(bag.add_item(ItemId::MACH_BIKE, 1));
    assert!(bag.add_item(ItemId::TM_FOCUS_PUNCH, 1));
    assert!(bag.add_item(ItemId::HM_CUT, 1));
    for mode in [BagScreenMode::Field, BagScreenMode::Battle] {
        let mut screen =
            open_from_pack(&pack, &bag, BagScreenRequest::new(mode)).expect("the real pack opens");
        let mut previous = None;
        for _ in 0..5 {
            let frame = frame_of(&screen);
            assert_ne!(
                frame.pixels(),
                Framebuffer::new().pixels(),
                "the pocket draws"
            );
            assert_ne!(
                Some(frame.pixels().to_vec()),
                previous,
                "each pocket looks different"
            );
            previous = Some(frame.pixels().to_vec());
            switch_pocket(&mut screen, Buttons::RIGHT);
        }
        screen.tick(press(Buttons::A));
        assert_ne!(screen.menu.context_grid(), None);
    }
}

#[test]
fn bag_window_text_uses_screen_palette_bank_1() {
    let screen = open_screen(BagScreenMode::Field, &sample_bag());
    let fb = frame_of(&screen);
    let fg = screen.chrome.window_text_colors()[1].expect("bank 1 index 1 exists");
    assert_eq!(fg, BAG_TEXT);
    assert!(painted(&fb, 32, 8, 64, 16, fg), "pocket name");
    assert!(painted(&fb, LIST_X + 8, LIST_Y, 100, ROW, fg), "list text");
    assert!(
        painted(&fb, 3, DESCRIPTION_Y + 1, 100, 48, fg),
        "description"
    );
}

#[test]
fn tmhm_context_menu_does_not_print_is_selected_in_description() {
    let mut bag = Bag::default();
    assert!(bag.add_item(ItemId::TM_FOCUS_PUNCH, 1));
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    switch_pocket(&mut screen, Buttons::RIGHT);
    switch_pocket(&mut screen, Buttons::RIGHT);
    assert_eq!(screen.menu.pocket(), Pocket::TmHm);
    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    assert!(screen.menu.context_grid().is_some());
    let fb = frame_of(&screen);
    // `item_menu.c:1642-1648`: the TM/HM pocket clears the description window.
    assert!(!painted(&fb, 3, DESCRIPTION_Y + 1, 100, 48, BAG_TEXT));
}

#[test]
fn simultaneous_down_and_a_in_a_multi_row_context_moves_to_toss() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    // `Task_ItemContext_MultipleRows` checks the D-pad before A.
    assert_eq!(
        screen.tick(press(Buttons::DOWN | Buttons::A)),
        BagScreenTick::Open,
        "DOWN wins over A in the choice grid"
    );
    let BagScreenTick::Action { action, .. } = screen.tick(press(Buttons::A)) else {
        panic!("confirming returns the choice");
    };
    assert_eq!(action, BagAction::Toss);
}

#[test]
fn tm_hm_and_berry_rows_are_numbered_and_hms_carry_the_mark() {
    use super::chrome::NO_SYMBOL;
    use super::labels::row_label;
    let tm = row_label(Pocket::TmHm, ItemId::TM_FOCUS_PUNCH, Some("TM01"));
    assert_eq!(
        (tm.number, tm.is_hm),
        (Some(format!("{NO_SYMBOL}01")), false)
    );
    let hm = row_label(Pocket::TmHm, ItemId::HM_CUT, Some("HM01"));
    assert_eq!((hm.number.as_deref(), hm.is_hm), (Some("1"), true));
    assert_eq!(hm.name, "CUT");
    let berry = row_label(Pocket::Berries, ItemId::CHERI_BERRY, Some("CHERI BERRY"));
    assert_eq!(berry.number, Some(format!("{NO_SYMBOL}01")));
    assert!(!berry.is_hm);

    let mut bag = sample_bag();
    assert!(bag.add_item(ItemId::TM_FOCUS_PUNCH, 1));
    assert!(bag.add_item(ItemId::HM_CUT, 1));
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    switch_pocket(&mut screen, Buttons::RIGHT);
    switch_pocket(&mut screen, Buttons::RIGHT);
    assert_eq!(screen.menu.pocket(), Pocket::TmHm);
    let fb = frame_of(&screen);
    // The HM mark (screen palette bank 1 index 1) sits at list x 8 on exactly
    // one of the two item rows.
    let marked: Vec<usize> = (0..2)
        .filter(|&row| painted(&fb, LIST_X + 8, LIST_Y + row * ROW - 1, 16, 16, HM_MARK))
        .collect();
    assert_eq!(marked.len(), 1, "exactly the HM row carries the mark");
    assert!(row_has_text(&fb, 0) && row_has_text(&fb, 1));

    switch_pocket(&mut screen, Buttons::RIGHT);
    assert_eq!(screen.menu.pocket(), Pocket::Berries);
    assert!(
        row_has_text(&frame_of(&screen), 0),
        "the berry row prints its number and name"
    );
}

#[test]
fn bag_list_and_description_shadow_is_screen_bank_1_index_3() {
    let screen = open_screen(BagScreenMode::Field, &sample_bag());
    // `COLORID_NORMAL` (`item_menu.c:386`): foreground 1, shadow 3.
    assert_eq!(
        screen.chrome.normal_text_colors()[2],
        Some(NORMAL_SHADOW),
        "bank 1 index 3"
    );
    let fb = frame_of(&screen);
    assert!(
        painted(&fb, LIST_X + 8, LIST_Y, 100, ROW, NORMAL_SHADOW),
        "list text shadow"
    );
    assert!(
        painted(&fb, 3, DESCRIPTION_Y + 1, 100, 48, NORMAL_SHADOW),
        "description shadow"
    );
    // The pocket header keeps `COLORID_POCKET_NAME`'s shadow, index 4.
    assert_ne!(screen.chrome.window_text_colors()[2], Some(NORMAL_SHADOW));
}

#[test]
fn r_moves_right_in_a_multi_row_context_in_lr_mode() {
    let mut request = BagScreenRequest::new(BagScreenMode::Field);
    request.lr_buttons = true;
    let mut screen = open_with(request, &sample_bag());
    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    assert_eq!(screen.tick(press(Buttons::R)), BagScreenTick::Open);
    let BagScreenTick::Action { action, .. } = screen.tick(press(Buttons::A)) else {
        panic!("confirming returns the choice");
    };
    assert_eq!(action, BagAction::Give, "R moved from Use to Give");
    // Without L/R mode the shoulder buttons do nothing in the grid.
    let mut plain = open_screen(BagScreenMode::Field, &sample_bag());
    plain.tick(press(Buttons::A));
    plain.tick(press(Buttons::R));
    let BagScreenTick::Action { action, .. } = plain.tick(press(Buttons::A)) else {
        panic!("confirming returns the choice");
    };
    assert_eq!(action, BagAction::Use);
}

#[test]
fn simultaneous_down_and_a_in_a_single_row_context_confirms() {
    let mut screen = open_screen(BagScreenMode::Battle, &sample_bag());
    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    let grid = screen.menu.context_grid().expect("the choices are open");
    assert_eq!(grid.columns, 1, "a battle item has Use and Cancel only");
    // `Menu_ProcessInputNoWrap` tests A before the D-pad.
    assert!(matches!(
        screen.tick(press(Buttons::DOWN | Buttons::A)),
        BagScreenTick::Closed(BagScreenResult::Item(_))
    ));
}

mod move_data;
mod scroll;
mod swap;
mod switch;
