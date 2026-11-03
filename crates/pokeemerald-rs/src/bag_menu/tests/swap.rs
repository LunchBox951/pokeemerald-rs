//! The SELECT swap: which pockets pick an item up, where it is put down,
//! what the screen returns, and what it shows meanwhile.

use assets::items::{ItemId, Pocket};
use engine::bag_menu::BagPositions;
use engine::save::bag::{Bag, ItemSlot};
use platform::{ButtonState, Buttons};
use rendering::Framebuffer;

use super::super::pack_fixture::{
    entries, load, GRAY_CURSOR_SHADOW, SWAP_END, SWAP_END_AT, SWAP_SEGMENT, SWAP_SEGMENT_AT,
};
use super::super::swap::move_in_list;
use super::super::{
    open_from_pack, BagScreen, BagScreenError, BagScreenMode, BagScreenRequest, BagScreenResult,
    BagScreenTick,
};
use super::{
    at, cursor_row, frame_of, icon_color, idle, open_screen, open_with, painted, press, sample_bag,
    switch_pocket, DESCRIPTION_Y, ICON_PIXEL, LIST_X, LIST_Y, ROW,
};

/// Four stacks told apart by quantity: A, B, C and D.
const QUANTITIES: [u16; 4] = [10, 20, 30, 40];

fn stack(index: usize, quantity: u16) -> ItemSlot {
    // Alternate two items so the fixture has the icons.
    let item = if index.is_multiple_of(2) {
        ItemId::POTION
    } else {
        ItemId::ANTIDOTE
    };
    ItemSlot {
        item_id: item.0,
        quantity,
    }
}

fn abcd() -> Bag {
    let mut bag = Bag::default();
    for (index, &quantity) in QUANTITIES.iter().enumerate() {
        bag.items[index] = stack(index, quantity);
    }
    bag
}

/// The quantities of the Items pocket in slot order, empty slots left out.
fn items_of(bag: &Bag) -> Vec<u16> {
    bag.items
        .iter()
        .map(|slot| slot.quantity)
        .filter(|&quantity| quantity != 0)
        .collect()
}

fn tick(screen: &mut BagScreen, buttons: Buttons) -> BagScreenTick {
    screen.tick(press(buttons))
}

/// Presses each button in turn, each a fresh press that must leave the
/// screen open without a result.
fn presses(screen: &mut BagScreen, buttons: &[Buttons]) {
    for &button in buttons {
        assert_eq!(tick(screen, button), BagScreenTick::Open);
    }
}

/// The bag a swap started at `from` and confirmed after `moves` returns.
fn swapped(bag: &Bag, from: usize, moves: &[Buttons], confirm: Buttons) -> (BagScreen, Bag) {
    let mut screen = open_screen(BagScreenMode::Field, bag);
    presses(&mut screen, &vec![Buttons::DOWN; from]);
    presses(&mut screen, &[Buttons::SELECT]);
    presses(&mut screen, moves);
    let BagScreenTick::Reordered { bag } = tick(&mut screen, confirm) else {
        panic!("confirming the swap returns the new bag");
    };
    (screen, *bag)
}

#[test]
fn moving_down_puts_the_item_before_the_row_under_the_cursor() {
    let (screen, bag) = swapped(&abcd(), 0, &[Buttons::DOWN, Buttons::DOWN], Buttons::A);
    assert_eq!(items_of(&bag), [20, 10, 30, 40], "A lands between B and C");
    assert_eq!(screen.menu.cursor(), 1, "the cursor stays on A's new row");
    let ordered: Vec<u16> = screen.menu.stacks().iter().map(|s| s.quantity).collect();
    assert_eq!(ordered, [20, 10, 30, 40], "the list shows the new order");
}

#[test]
fn moving_up_puts_the_item_before_the_row_under_the_cursor() {
    let (screen, bag) = swapped(&abcd(), 2, &[Buttons::UP, Buttons::UP], Buttons::A);
    assert_eq!(items_of(&bag), [30, 10, 20, 40]);
    assert_eq!(screen.menu.cursor(), 0);
}

#[test]
fn select_puts_the_item_down_as_a_does() {
    let (_, bag) = swapped(&abcd(), 0, &[Buttons::DOWN, Buttons::DOWN], Buttons::SELECT);
    assert_eq!(items_of(&bag), [20, 10, 30, 40]);
}

#[test]
fn the_close_row_is_a_place_to_put_an_item_down() {
    let moves = [Buttons::DOWN; 4];
    let (screen, bag) = swapped(&abcd(), 0, &moves, Buttons::A);
    assert_eq!(items_of(&bag), [20, 30, 40, 10]);
    assert_eq!(
        screen.menu.cursor(),
        3,
        "A is the last item, above CLOSE BAG"
    );
}

#[test]
fn the_screen_stays_open_and_acts_on_the_new_order() {
    let (mut screen, _) = swapped(&abcd(), 0, &[Buttons::DOWN, Buttons::DOWN], Buttons::A);
    presses(&mut screen, &[Buttons::A]);
    let BagScreenTick::Action { selection, .. } = tick(&mut screen, Buttons::A) else {
        panic!("the first choice is returned");
    };
    assert_eq!(
        (selection.quantity, selection.list_position, selection.slot),
        (10, 1, 1),
        "the cursor is on A, now the second row and second slot"
    );
}

#[test]
fn putting_an_item_down_where_it_was_changes_nothing() {
    for (moves, cursor) in [(vec![], 0), (vec![Buttons::DOWN], 0)] {
        let mut screen = open_screen(BagScreenMode::Field, &abcd());
        presses(&mut screen, &[Buttons::SELECT]);
        presses(&mut screen, &moves);
        assert_eq!(tick(&mut screen, Buttons::A), BagScreenTick::Open);
        assert_eq!(items_of(&screen.bag), QUANTITIES, "{moves:?}");
        assert_eq!(screen.menu.cursor(), cursor, "the cursor is back on A");
        assert!(screen.swap().is_none());
    }
}

#[test]
fn cancelling_leaves_the_order_and_steps_back_only_from_below() {
    let mut screen = open_screen(BagScreenMode::Field, &abcd());
    presses(
        &mut screen,
        &[Buttons::SELECT, Buttons::DOWN, Buttons::DOWN],
    );
    assert_eq!(tick(&mut screen, Buttons::B), BagScreenTick::Open);
    assert_eq!(items_of(&screen.bag), QUANTITIES);
    assert_eq!(screen.menu.cursor(), 1, "below the item the row steps back");
    assert!(
        matches!(
            tick(&mut screen, Buttons::B),
            BagScreenTick::Closed(BagScreenResult::ReturnToField)
        ),
        "the screen was still open after cancelling"
    );

    let mut screen = open_screen(BagScreenMode::Field, &abcd());
    presses(
        &mut screen,
        &[Buttons::DOWN, Buttons::DOWN, Buttons::SELECT],
    );
    presses(&mut screen, &[Buttons::UP, Buttons::B]);
    assert_eq!(screen.menu.cursor(), 1, "above the item the row stays");
}

#[test]
fn cancelling_below_a_scrolled_list_keeps_the_scroll() {
    // A list of twelve scrolls; the cursor row steps back, not the scroll.
    let mut bag = Bag::default();
    for index in 0..12 {
        bag.items[index] = stack(index, u16::try_from(index + 1).unwrap());
    }
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    presses(&mut screen, &[Buttons::SELECT]);
    presses(&mut screen, &[Buttons::DOWN; 9]);
    presses(&mut screen, &[Buttons::UP]);
    assert_eq!((screen.menu.scroll(), screen.menu.cursor()), (5, 3));
    presses(&mut screen, &[Buttons::B]);
    assert_eq!((screen.menu.scroll(), screen.menu.cursor()), (5, 2));
}

#[test]
fn a_list_position_is_not_a_slot_number() {
    // Slots 0, 2, 4 and 5 hold A, B, C and D with empty slots between.
    let mut bag = Bag::default();
    for (slot, (index, &quantity)) in [0usize, 2, 4, 5]
        .into_iter()
        .zip(QUANTITIES.iter().enumerate())
    {
        bag.items[slot] = stack(index, quantity);
    }
    let before = bag.clone();
    let (mut screen, first) = swapped(&bag, 2, &[Buttons::UP, Buttons::UP], Buttons::A);
    assert_eq!(items_of(&first), [30, 10, 20, 40], "C moved to the top");
    assert_eq!(
        first.items[..4]
            .iter()
            .map(|s| s.quantity)
            .collect::<Vec<_>>(),
        [30, 10, 20, 40],
        "the pocket is packed in list order"
    );
    assert_eq!(bag, before, "the host's bag is not written");
    assert_eq!(first.key_items, before.key_items);
    assert_eq!(first.poke_balls, before.poke_balls);
    assert_eq!(first.tms_hms, before.tms_hms);
    assert_eq!(first.berries, before.berries);
    assert_eq!(screen.bag, first, "the screen holds what it returned");

    // A second swap works from the first one's order: D, now last, goes up.
    presses(&mut screen, &[Buttons::DOWN, Buttons::DOWN, Buttons::DOWN]);
    presses(&mut screen, &[Buttons::SELECT, Buttons::UP, Buttons::UP]);
    let BagScreenTick::Reordered { bag: second } = tick(&mut screen, Buttons::A) else {
        panic!("the second swap returns a bag");
    };
    assert_eq!(items_of(&second), [30, 40, 10, 20]);
    assert_eq!(*second, screen.bag);
}

#[test]
fn only_the_items_balls_and_key_items_pockets_swap() {
    let mut bag = abcd();
    assert!(bag.add_item(ItemId::POKE_BALL, 3));
    assert!(bag.add_item(ItemId::MACH_BIKE, 1));
    assert!(bag.add_item(ItemId::TM_FOCUS_PUNCH, 1));
    assert!(bag.add_item(ItemId::CHERI_BERRY, 7));
    for mode in [BagScreenMode::Field, BagScreenMode::Battle] {
        let mut screen = open_screen(mode, &bag);
        let mut swaps = Vec::new();
        for _ in 0..5 {
            presses(&mut screen, &[Buttons::SELECT]);
            swaps.push((screen.menu.pocket(), screen.swap().is_some()));
            screen.swap = None;
            switch_pocket(&mut screen, Buttons::RIGHT);
        }
        assert_eq!(
            swaps,
            [
                (Pocket::Items, true),
                (Pocket::PokeBalls, true),
                (Pocket::TmHm, false),
                (Pocket::Berries, false),
                (Pocket::KeyItems, true),
            ],
            "{mode:?}"
        );
    }
}

#[test]
fn select_on_the_close_row_or_an_empty_pocket_starts_nothing() {
    let mut screen = open_screen(BagScreenMode::Field, &abcd());
    presses(&mut screen, &[Buttons::DOWN; 4]);
    presses(&mut screen, &[Buttons::SELECT]);
    assert!(screen.swap().is_none(), "the close row cannot be picked up");

    let mut empty = open_screen(BagScreenMode::Field, &Bag::default());
    presses(&mut empty, &[Buttons::SELECT]);
    assert!(empty.swap().is_none());
}

#[test]
fn a_select_press_that_starts_nothing_still_takes_the_frame() {
    // SELECT with A on the close row neither opens a choice nor closes.
    let mut screen = open_screen(BagScreenMode::Field, &abcd());
    presses(&mut screen, &[Buttons::DOWN; 4]);
    assert_eq!(
        screen.tick(press(Buttons::SELECT | Buttons::A)),
        BagScreenTick::Open
    );
    assert!(screen.menu.context_grid().is_none());
    assert!(screen.swap().is_none());
}

#[test]
fn a_pocket_press_beats_select_and_a_context_menu_ignores_it() {
    let mut screen = open_screen(BagScreenMode::Field, &abcd());
    assert_eq!(
        screen.tick(press(Buttons::SELECT | Buttons::RIGHT)),
        BagScreenTick::Open
    );
    assert!(screen.swap().is_none() && screen.switch.is_some());

    let mut screen = open_screen(BagScreenMode::Field, &abcd());
    presses(&mut screen, &[Buttons::A, Buttons::SELECT]);
    assert!(screen.swap().is_none() && screen.menu.context_grid().is_some());
}

#[test]
fn while_swapping_select_beats_the_other_buttons() {
    let place = |buttons: ButtonState| {
        let mut screen = open_screen(BagScreenMode::Field, &abcd());
        presses(
            &mut screen,
            &[Buttons::SELECT, Buttons::DOWN, Buttons::DOWN],
        );
        (screen.tick(buttons), screen)
    };
    for buttons in [
        Buttons::SELECT | Buttons::B,
        Buttons::A | Buttons::B,
        Buttons::A | Buttons::DOWN,
        Buttons::SELECT | Buttons::DOWN,
    ] {
        let (outcome, screen) = place(press(buttons));
        assert!(
            matches!(outcome, BagScreenTick::Reordered { .. }),
            "{buttons:?} puts the item down where the cursor is"
        );
        assert_eq!(items_of(&screen.bag), [20, 10, 30, 40], "{buttons:?}");
    }
}

#[test]
fn while_swapping_the_pocket_buttons_do_nothing() {
    let mut request = BagScreenRequest::new(BagScreenMode::Field);
    request.lr_buttons = true;
    let mut screen = open_with(request, &abcd());
    presses(&mut screen, &[Buttons::SELECT]);
    for button in [Buttons::LEFT, Buttons::RIGHT, Buttons::L, Buttons::R] {
        presses(&mut screen, &[button]);
        assert!(screen.switch.is_none(), "{button:?}");
        assert_eq!(screen.menu.pocket(), Pocket::Items);
        assert_eq!(screen.menu.cursor(), 0);
    }
    presses(&mut screen, &[Buttons::RIGHT | Buttons::DOWN]);
    assert_eq!(screen.menu.cursor(), 1, "Down still moves beside Right");
}

#[test]
fn moving_the_place_restores_the_cursor_to_where_the_screen_would_open_it() {
    // Every place the cursor can reach from every item of lists of several
    // lengths, put down or cancelled: the cursor row steps back exactly when
    // it is below the item, and the scroll never changes.
    for len in [1, 5, 8, 9, 12, 17, 30] {
        let mut bag = Bag::default();
        for index in 0..len {
            bag.items[index] = stack(index, u16::try_from(index + 1).unwrap());
        }
        for from in 0..len {
            for downs in 0..=len - from {
                for confirm in [Buttons::A, Buttons::B] {
                    let mut screen = open_screen(BagScreenMode::Field, &bag);
                    presses(&mut screen, &vec![Buttons::DOWN; from]);
                    presses(&mut screen, &[Buttons::SELECT]);
                    presses(&mut screen, &vec![Buttons::DOWN; downs]);
                    let (scroll, cursor) = (screen.menu.scroll(), screen.menu.cursor());
                    screen.tick(press(confirm));
                    let expected = cursor - u16::from(downs > 0);
                    assert_eq!(
                        (screen.menu.scroll(), screen.menu.cursor()),
                        (scroll, expected),
                        "{len} stacks, from {from}, {downs} down, {confirm:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_swap_leaves_the_other_pockets_where_the_player_put_them() {
    // 16 balls with the cursor on row 7 open at scroll 2 / row 5; opening
    // that position again would move it to 3 / 4.
    let mut bag = abcd();
    for index in 0..16 {
        bag.poke_balls[index] = ItemSlot {
            item_id: ItemId::POKE_BALL.0,
            quantity: 1,
        };
    }
    let mut positions = BagPositions::default();
    positions.cursor[1] = 7;
    let mut request = BagScreenRequest::new(BagScreenMode::Field);
    request.positions = positions;
    let mut screen = open_with(request, &bag);
    presses(
        &mut screen,
        &[Buttons::SELECT, Buttons::DOWN, Buttons::DOWN],
    );
    assert!(matches!(
        tick(&mut screen, Buttons::A),
        BagScreenTick::Reordered { .. }
    ));
    assert_eq!(screen.menu.cursor(), 1);
    switch_pocket(&mut screen, Buttons::RIGHT);
    assert_eq!(screen.menu.pocket(), Pocket::PokeBalls);
    assert_eq!((screen.menu.scroll(), screen.menu.cursor()), (2, 5));
}

#[test]
fn the_screen_remembers_where_the_swap_left_the_pocket() {
    let (screen, _) = swapped(&abcd(), 0, &[Buttons::DOWN, Buttons::DOWN], Buttons::A);
    assert_eq!(screen.positions().cursor[0], 1);
}

#[test]
fn moving_an_item_in_a_list_matches_taking_it_out_and_putting_it_back() {
    for len in 1..=6usize {
        for from in 0..len {
            for to in 0..=len {
                let mut list: Vec<usize> = (0..len).collect();
                assert!(move_in_list(&mut list, from, to));
                let mut expected: Vec<usize> = (0..len).collect();
                let item = expected.remove(from);
                expected.insert(if to > from { to - 1 } else { to }, item);
                assert_eq!(list, expected, "{len} rows, from {from} to {to}");
            }
        }
    }
    let mut list = vec![0, 1, 2];
    assert!(!move_in_list(&mut list, 3, 0) && !move_in_list(&mut list, 0, 4));
    assert_eq!(list, [0, 1, 2], "an out-of-range move changes nothing");
}

// -- What the screen shows -------------------------------------------------

fn swap_line_columns(fb: &Framebuffer, row: usize, color: rendering::Rgb888) -> Vec<usize> {
    (0..Framebuffer::WIDTH)
        .filter(|&x| at(fb, x, row) == color)
        .collect()
}

fn gray_cursor_on(fb: &Framebuffer, row: usize) -> bool {
    painted(fb, LIST_X, LIST_Y + row * ROW, 6, ROW, GRAY_CURSOR_SHADOW)
}

#[test]
fn picking_an_item_up_grays_its_cursor_and_freezes_the_icon_and_prompt() {
    let mut screen = open_screen(BagScreenMode::Field, &abcd());
    let before = frame_of(&screen);
    presses(&mut screen, &[Buttons::SELECT]);
    let picked = frame_of(&screen);
    assert!(gray_cursor_on(&picked, 0));
    assert_eq!(cursor_row(&picked), None, "the black cursor is hidden");
    assert!(
        (0..Framebuffer::WIDTH).all(|x| (0..160).all(|y| at(&picked, x, y) != SWAP_END)),
        "the swap line shows from the next frame"
    );
    let description = |fb: &Framebuffer| super::region(fb, 0, DESCRIPTION_Y, 112, 48);
    assert_ne!(description(&picked), description(&before), "the prompt");

    presses(&mut screen, &[Buttons::DOWN, Buttons::DOWN]);
    let moved = frame_of(&screen);
    assert!(
        gray_cursor_on(&moved, 0),
        "the gray cursor stays on the item"
    );
    assert_eq!(cursor_row(&moved), None);
    assert_eq!(at(&moved, ICON_PIXEL.0, ICON_PIXEL.1), icon_color("potion"));
    assert_eq!(
        description(&moved),
        description(&picked),
        "the prompt stays"
    );

    presses(&mut screen, &[Buttons::B]);
    let after = frame_of(&screen);
    assert_eq!(cursor_row(&after), Some(1));
    assert!(!gray_cursor_on(&after, 1));
    assert_eq!(
        at(&after, ICON_PIXEL.0, ICON_PIXEL.1),
        icon_color("antidote")
    );
    assert_ne!(description(&after), description(&picked));
}

#[test]
fn the_gray_cursor_is_gone_once_the_item_scrolls_out_of_the_window() {
    let mut bag = Bag::default();
    for index in 0..12 {
        bag.items[index] = stack(index, 1);
    }
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    presses(&mut screen, &[Buttons::SELECT]);
    presses(&mut screen, &[Buttons::DOWN; 8]);
    assert!(screen.menu.scroll() > 0);
    let fb = frame_of(&screen);
    assert!((0..8).all(|row| !gray_cursor_on(&fb, row)));
}

#[test]
fn the_swap_line_runs_from_the_end_cap_across_the_list_at_the_cursor() {
    let mut screen = open_screen(BagScreenMode::Field, &abcd());
    presses(&mut screen, &[Buttons::SELECT]);
    // Eight 16-pixel sprites from x 112, the last pulled in 8 pixels. The end
    // cap is the first sprite's frame and the body is the others'.
    let line = |screen: &BagScreen, row: usize| {
        let fb = frame_of(screen);
        let top = 9 + 16 * row;
        (
            swap_line_columns(&fb, top + SWAP_END_AT.1, SWAP_END),
            swap_line_columns(&fb, top + SWAP_SEGMENT_AT.1, SWAP_SEGMENT),
        )
    };
    assert_eq!(line(&screen, 0), (vec![], vec![]), "hidden at the start");

    presses(&mut screen, &[Buttons::DOWN]);
    let (end, body) = line(&screen, 1);
    assert_eq!(end, [112 + SWAP_END_AT.0]);
    let expected: Vec<usize> = [128, 144, 160, 176, 192, 208, 216]
        .into_iter()
        .map(|x| x + SWAP_SEGMENT_AT.0)
        .collect();
    assert_eq!(body, expected, "the last sprite overlaps the one before");

    presses(&mut screen, &[Buttons::DOWN]);
    assert_eq!(
        line(&screen, 2).0,
        [112 + SWAP_END_AT.0],
        "it follows the cursor"
    );

    idle(&mut screen, 2);
    presses(&mut screen, &[Buttons::B]);
    assert_eq!(
        line(&screen, 1),
        (vec![], vec![]),
        "gone once the swap ends"
    );
}

#[test]
fn putting_an_item_down_with_select_never_shows_the_line() {
    let mut screen = open_screen(BagScreenMode::Field, &abcd());
    presses(&mut screen, &[Buttons::SELECT, Buttons::SELECT]);
    assert!(screen.swap().is_none());
    let fb = frame_of(&screen);
    assert!((0..Framebuffer::WIDTH).all(|x| (0..160).all(|y| at(&fb, x, y) != SWAP_SEGMENT)));
}

#[test]
fn the_scroll_arrows_stay_while_swapping() {
    let mut bag = Bag::default();
    for index in 0..9 {
        bag.items[index] = stack(index, 1);
    }
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    let shown = |screen: &BagScreen| {
        let fb = frame_of(screen);
        super::scroll::arrows_of(&fb)
    };
    let before = shown(&screen);
    presses(&mut screen, &[Buttons::SELECT]);
    assert_eq!(shown(&screen), before);
}

#[test]
fn opening_fails_when_the_swap_line_graphics_are_missing_or_malformed() {
    for broken in ["bag/image/swap_line", "bag/palette/swap_line"] {
        let without = entries()
            .into_iter()
            .filter(|entry| entry.id != broken)
            .collect();
        let result = open_from_pack(
            &load("swap-missing", without),
            &sample_bag(),
            BagScreenRequest::new(BagScreenMode::Field),
        );
        assert!(matches!(result, Err(BagScreenError::Pack(_))), "{broken}");
    }
    let mut short = entries();
    short.retain(|entry| entry.id != "bag/image/swap_line");
    short.push(
        pack_format::image_entry("bag/image/swap_line".into(), 16, 16, 4, vec![0; 256]).unwrap(),
    );
    let result = open_from_pack(
        &load("swap-short", short),
        &sample_bag(),
        BagScreenRequest::new(BagScreenMode::Field),
    );
    assert!(matches!(result, Err(BagScreenError::Pack(_))));
}
