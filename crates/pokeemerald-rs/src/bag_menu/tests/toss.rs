//! The TOSS prompt: the quantity window, the stepping, the yes/no, the
//! "Threw away" message, and the bag the screen returns
//! (`ItemMenu_Toss`, `Task_ChooseHowManyToToss`, `Task_RemoveItemFromBag`,
//! `item_menu.c:1806-1901`).

use assets::items::ItemId;
use engine::save::bag::{Bag, ItemSlot};
use platform::{ButtonState, Buttons};
use rendering::Framebuffer;

use super::super::pack_fixture::{BAG_TEXT, WINDOW_BORDER};
use super::super::toss::{self, TossStage};
use super::super::{BagScreen, BagScreenMode, BagScreenRequest, BagScreenResult, BagScreenTick};
use super::{
    frame_of, open_screen, open_with, painted, press, sample_bag, switch_pocket, LIST_X, LIST_Y,
    ROW,
};

/// The quantity window's content rectangle: tile (24, 17), 5x2
/// (`item_menu.c:517-524`).
pub(super) const QUANTITY: (usize, usize, usize, usize) = (192, 136, 40, 16);
/// The yes/no window's content rectangle: tile (24, 15), 5x4
/// (`item_menu.c:499-506`).
pub(super) const YES_NO: (usize, usize, usize, usize) = (192, 120, 40, 32);
/// The choice window the context menu draws for four entries.
pub(super) const CONTEXT: (usize, usize, usize, usize) = (120, 120, 112, 32);

pub(super) fn tick(screen: &mut BagScreen, buttons: Buttons) -> BagScreenTick {
    screen.tick(press(buttons))
}

pub(super) fn open_ok(screen: &mut BagScreen, buttons: Buttons) {
    assert_eq!(tick(screen, buttons), BagScreenTick::Open);
}

/// Selects the cursor row's item and its TOSS (the Items pocket's third
/// choice).
pub(super) fn choose_toss(screen: &mut BagScreen) {
    for button in [Buttons::A, Buttons::DOWN, Buttons::A] {
        open_ok(screen, button);
    }
}

pub(super) fn potions(quantity: u16) -> Bag {
    let mut bag = Bag::default();
    bag.items[0] = ItemSlot {
        item_id: ItemId::POTION.0,
        quantity,
    };
    bag
}

pub(super) fn quantity_screen() -> BagScreen {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    choose_toss(&mut screen);
    screen
}

pub(super) fn stage(screen: &BagScreen) -> Option<TossStage> {
    screen.toss_stage()
}

pub(super) fn shown(
    fb: &Framebuffer,
    rect: (usize, usize, usize, usize),
    color: rendering::Rgb888,
) -> bool {
    painted(fb, rect.0, rect.1, rect.2, rect.3, color)
}

/// Whether the one-tile border ring stands around a window's content: all
/// four corner tiles, so the window sits at exactly that rectangle.
pub(super) fn bordered(fb: &Framebuffer, rect: (usize, usize, usize, usize)) -> bool {
    let (x, y, w, h) = rect;
    [
        (x - 8, y - 8),
        (x + w, y - 8),
        (x - 8, y + h),
        (x + w, y + h),
    ]
    .into_iter()
    .all(|(left, top)| painted(fb, left, top, 8, 8, WINDOW_BORDER))
}

/// Finishes a toss of `quantity_presses` Ups over `screen`'s prompt, taking
/// Yes and acknowledging the message, and returns the bag.
fn tossed(screen: &mut BagScreen, ups: usize, finish: Buttons) -> Bag {
    for _ in 0..ups {
        open_ok(screen, Buttons::UP);
    }
    open_ok(screen, Buttons::A);
    open_ok(screen, Buttons::A);
    let BagScreenTick::Tossed { bag } = tick(screen, finish) else {
        panic!("acknowledging the message returns the bag");
    };
    *bag
}

#[test]
fn directions_step_the_count_and_the_stack_bounds_it() {
    // Five potions: Up and Down wrap, Right and Left stop at the ends
    // (`menu_helpers.c:180-246`).
    let mut screen = quantity_screen();
    let steps = [
        (Buttons::UP, 2),
        (Buttons::DOWN, 1),
        (Buttons::DOWN, 5),
        (Buttons::UP, 1),
        (Buttons::RIGHT, 5),
        (Buttons::LEFT, 1),
    ];
    for (button, count) in steps {
        open_ok(&mut screen, button);
        assert_eq!(stage(&screen), Some(TossStage::Quantity { count }));
    }
}

#[test]
fn left_and_right_step_by_ten() {
    let mut screen = open_screen(BagScreenMode::Field, &potions(60));
    choose_toss(&mut screen);
    for (button, count) in [
        (Buttons::RIGHT, 11),
        (Buttons::RIGHT, 21),
        (Buttons::LEFT, 11),
        (Buttons::LEFT, 1),
        (Buttons::LEFT, 1),
        (Buttons::DOWN, 60),
        (Buttons::RIGHT, 60),
        (Buttons::UP, 1),
    ] {
        open_ok(&mut screen, button);
        assert_eq!(stage(&screen), Some(TossStage::Quantity { count }));
    }
}

#[test]
fn a_press_that_leaves_the_count_still_confirms() {
    // `Task_ChooseHowManyToToss` reads A only when the count did not change.
    let mut screen = quantity_screen();
    open_ok(&mut screen, Buttons::UP | Buttons::A);
    assert_eq!(stage(&screen), Some(TossStage::Quantity { count: 2 }));
    open_ok(&mut screen, Buttons::RIGHT);
    open_ok(&mut screen, Buttons::RIGHT | Buttons::A);
    assert_eq!(
        stage(&screen),
        Some(TossStage::Confirm {
            count: 5,
            yes: true
        })
    );
}

#[test]
fn two_directions_at_once_do_nothing() {
    let mut screen = quantity_screen();
    open_ok(&mut screen, Buttons::UP | Buttons::RIGHT);
    open_ok(&mut screen, Buttons::UP | Buttons::DOWN);
    assert_eq!(stage(&screen), Some(TossStage::Quantity { count: 1 }));
}

#[test]
fn shoulder_buttons_do_not_step_the_count() {
    let mut request = BagScreenRequest::new(BagScreenMode::Field);
    request.lr_buttons = true;
    let mut screen = open_with(request, &sample_bag());
    choose_toss(&mut screen);
    open_ok(&mut screen, Buttons::R);
    open_ok(&mut screen, Buttons::L);
    assert_eq!(stage(&screen), Some(TossStage::Quantity { count: 1 }));
    assert_eq!(screen.menu.pocket(), assets::items::Pocket::Items);
}

#[test]
fn a_held_direction_repeats_after_forty_frames_then_every_five() {
    let mut bag = potions(99);
    bag.items[1] = ItemSlot {
        item_id: ItemId::ANTIDOTE.0,
        quantity: 1,
    };
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    choose_toss(&mut screen);
    let mut buttons = ButtonState::new();
    let mut count_after = |buttons: &mut ButtonState, held: Buttons| {
        buttons.update(held);
        assert_eq!(screen.tick(*buttons), BagScreenTick::Open);
        match screen.toss_stage() {
            Some(TossStage::Quantity { count }) => count,
            other => panic!("still choosing: {other:?}"),
        }
    };
    assert_eq!(count_after(&mut buttons, Buttons::UP), 2, "the press steps");
    for _ in 0..39 {
        assert_eq!(count_after(&mut buttons, Buttons::UP), 2, "held, waiting");
    }
    assert_eq!(
        count_after(&mut buttons, Buttons::UP),
        3,
        "the 40th repeats"
    );
    for expected in [3, 3, 3, 3] {
        assert_eq!(count_after(&mut buttons, Buttons::UP), expected);
    }
    assert_eq!(
        count_after(&mut buttons, Buttons::UP),
        4,
        "then every fifth"
    );
    // Letting go restarts the wait.
    assert_eq!(count_after(&mut buttons, Buttons::NONE), 4);
    assert_eq!(count_after(&mut buttons, Buttons::UP), 5);
    for _ in 0..39 {
        assert_eq!(count_after(&mut buttons, Buttons::UP), 5);
    }
}

#[test]
fn a_held_a_does_not_run_through_the_stages() {
    let mut screen = quantity_screen();
    let mut buttons = ButtonState::new();
    for _ in 0..90 {
        buttons.update(Buttons::A);
        let _ = screen.tick(buttons);
    }
    assert_eq!(
        stage(&screen),
        Some(TossStage::Confirm {
            count: 1,
            yes: true
        }),
        "A is read on its press only"
    );
}

#[test]
fn a_through_yes_and_the_message_removes_the_count() {
    let mut screen = quantity_screen();
    open_ok(&mut screen, Buttons::UP);
    open_ok(&mut screen, Buttons::UP);
    open_ok(&mut screen, Buttons::A);
    assert_eq!(
        stage(&screen),
        Some(TossStage::Confirm {
            count: 3,
            yes: true
        })
    );
    open_ok(&mut screen, Buttons::A);
    assert_eq!(stage(&screen), Some(TossStage::Done { count: 3 }));
    assert_eq!(
        screen.menu.stacks()[0].quantity,
        5,
        "nothing is removed until the message is acknowledged"
    );
    let BagScreenTick::Tossed { bag } = tick(&mut screen, Buttons::A) else {
        panic!("acknowledging returns the bag");
    };
    assert_eq!(bag.items[0].item_id, ItemId::POTION.0);
    assert_eq!(bag.items[0].quantity, 2);
    assert_eq!(bag.items[1].quantity, 2, "the other stacks are untouched");
    assert_eq!(bag.poke_balls, sample_bag().poke_balls);
    assert_eq!(stage(&screen), None);
    assert_eq!(screen.menu.stacks()[0].quantity, 2, "the list shows it");
    assert_eq!(
        screen.tick(press(Buttons::NONE)),
        BagScreenTick::Open,
        "the bag is returned once"
    );
    // The list is live again.
    open_ok(&mut screen, Buttons::DOWN);
    assert_eq!(screen.menu.cursor(), 1);
}

#[test]
fn b_acknowledges_the_message_too() {
    let mut screen = quantity_screen();
    let bag = tossed(&mut screen, 1, Buttons::B);
    assert_eq!(bag.items[0].quantity, 3);
}

#[test]
fn b_in_the_quantity_window_returns_to_the_list() {
    let mut screen = quantity_screen();
    open_ok(&mut screen, Buttons::UP);
    open_ok(&mut screen, Buttons::B);
    assert_eq!(stage(&screen), None);
    let fb = frame_of(&screen);
    assert!(!bordered(&fb, QUANTITY));
    assert!(
        painted(&fb, LIST_X, LIST_Y, 6, ROW, BAG_TEXT),
        "cursor is back"
    );
    assert_eq!(screen.menu.stacks()[0].quantity, 5);
    // The list answers again, and nothing reaches the host.
    open_ok(&mut screen, Buttons::DOWN);
    assert_eq!(screen.menu.cursor(), 1);
}

#[test]
fn no_and_b_in_the_yes_no_cancel_to_the_list() {
    for cancel in [&[Buttons::B][..], &[Buttons::DOWN, Buttons::A][..]] {
        let mut screen = quantity_screen();
        open_ok(&mut screen, Buttons::UP);
        open_ok(&mut screen, Buttons::A);
        for &button in cancel {
            open_ok(&mut screen, button);
        }
        assert_eq!(stage(&screen), None, "{cancel:?} returns to the list");
        assert!(!bordered(&frame_of(&screen), YES_NO));
        assert_eq!(screen.menu.stacks()[0].quantity, 5);
        open_ok(&mut screen, Buttons::DOWN);
        assert_eq!(screen.menu.cursor(), 1);
    }
}

#[test]
fn the_yes_no_cursor_does_not_wrap_and_ignores_sideways() {
    let mut screen = quantity_screen();
    open_ok(&mut screen, Buttons::A);
    let yes = |screen: &BagScreen| match screen.toss_stage() {
        Some(TossStage::Confirm { yes, .. }) => yes,
        other => panic!("asking: {other:?}"),
    };
    open_ok(&mut screen, Buttons::UP);
    assert!(yes(&screen));
    open_ok(&mut screen, Buttons::DOWN);
    assert!(!yes(&screen));
    open_ok(&mut screen, Buttons::DOWN);
    assert!(!yes(&screen));
    open_ok(&mut screen, Buttons::LEFT);
    open_ok(&mut screen, Buttons::RIGHT);
    assert!(!yes(&screen));
    open_ok(&mut screen, Buttons::UP);
    assert!(yes(&screen));
}

#[test]
fn select_and_pocket_buttons_do_nothing_during_the_prompt() {
    let mut request = BagScreenRequest::new(BagScreenMode::Field);
    request.lr_buttons = true;
    for stage_buttons in [vec![], vec![Buttons::A], vec![Buttons::A, Buttons::A]] {
        let mut screen = open_with(request, &sample_bag());
        choose_toss(&mut screen);
        for button in stage_buttons {
            open_ok(&mut screen, button);
        }
        let before = stage(&screen);
        for button in [Buttons::SELECT, Buttons::L, Buttons::R] {
            open_ok(&mut screen, button);
        }
        assert_eq!(stage(&screen), before);
        assert!(screen.switch.is_none() && screen.swap().is_none());
        assert_eq!(screen.menu.pocket(), assets::items::Pocket::Items);
    }
}

#[test]
fn tossing_a_whole_stack_clears_its_slot_and_lands_on_the_close_row() {
    // The Antidote stack of two is the second row.
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    open_ok(&mut screen, Buttons::DOWN);
    choose_toss(&mut screen);
    open_ok(&mut screen, Buttons::DOWN);
    assert_eq!(stage(&screen), Some(TossStage::Quantity { count: 2 }));
    open_ok(&mut screen, Buttons::A);
    open_ok(&mut screen, Buttons::A);
    let BagScreenTick::Tossed { bag } = tick(&mut screen, Buttons::A) else {
        panic!("returns the bag");
    };
    assert_eq!(bag.items[1], ItemSlot::default());
    assert_eq!(bag.items[0].quantity, 5);
    assert_eq!(screen.menu.stacks().len(), 1);
    assert_eq!(screen.menu.cursor(), 1, "the cursor rests on the close row");
}

#[test]
fn the_selected_stack_is_tossed_not_the_first_of_its_item() {
    // Two Potion stacks in separate slots: list row 2 is slot 2.
    let mut bag = Bag::default();
    for (slot, item, quantity) in [
        (0, ItemId::POTION, 99),
        (1, ItemId::ANTIDOTE, 3),
        (2, ItemId::POTION, 20),
    ] {
        bag.items[slot] = ItemSlot {
            item_id: item.0,
            quantity,
        };
    }
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    open_ok(&mut screen, Buttons::DOWN);
    open_ok(&mut screen, Buttons::DOWN);
    choose_toss(&mut screen);
    let bag = tossed(&mut screen, 2, Buttons::A);
    assert_eq!(bag.items[0].quantity, 99);
    assert_eq!(bag.items[1].quantity, 3);
    assert_eq!(bag.items[2].quantity, 17, "two Ups from 1 toss 3");
}

#[test]
fn a_sparse_pocket_is_packed_after_the_toss() {
    let mut bag = Bag::default();
    bag.items[2] = ItemSlot {
        item_id: ItemId::POTION.0,
        quantity: 5,
    };
    bag.items[5] = ItemSlot {
        item_id: ItemId::ANTIDOTE.0,
        quantity: 1,
    };
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    open_ok(&mut screen, Buttons::DOWN);
    choose_toss(&mut screen);
    // One antidote: the prompt asks at once.
    open_ok(&mut screen, Buttons::A);
    let BagScreenTick::Tossed { bag } = tick(&mut screen, Buttons::A) else {
        panic!("returns the bag");
    };
    assert_eq!(
        bag.items[0].item_id,
        ItemId::POTION.0,
        "packed to the front"
    );
    assert_eq!(bag.items[1], ItemSlot::default());
    assert_eq!(bag.items[5], ItemSlot::default());
}

#[test]
fn berries_toss_by_slot_and_are_packed() {
    // Two Cheri stacks in sparse slots: list row 1 is slot 3.
    let mut bag = Bag::default();
    for (slot, quantity) in [(1, 5), (3, 7)] {
        bag.berries[slot] = ItemSlot {
            item_id: ItemId::CHERI_BERRY.0,
            quantity,
        };
    }
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    for _ in 0..3 {
        switch_pocket(&mut screen, Buttons::RIGHT);
    }
    open_ok(&mut screen, Buttons::DOWN);
    // CHECK TAG, a blank, USE, GIVE, TOSS: two downs reach TOSS.
    for button in [Buttons::A, Buttons::DOWN, Buttons::DOWN, Buttons::A] {
        open_ok(&mut screen, button);
    }
    assert_eq!(stage(&screen), Some(TossStage::Quantity { count: 1 }));
    open_ok(&mut screen, Buttons::DOWN);
    assert_eq!(stage(&screen), Some(TossStage::Quantity { count: 7 }));
    open_ok(&mut screen, Buttons::A);
    open_ok(&mut screen, Buttons::A);
    let BagScreenTick::Tossed { bag } = tick(&mut screen, Buttons::A) else {
        panic!("returns the bag");
    };
    assert_eq!(
        bag.berries[0].quantity, 5,
        "the other stack, packed forward"
    );
    assert_eq!(bag.berries[1], ItemSlot::default());
    assert_eq!(bag.berries[3], ItemSlot::default());
}

#[test]
fn the_screen_keeps_its_place_and_swaps_from_the_new_bag() {
    let mut bag = Bag::default();
    for (slot, quantity) in (0..3).zip([10, 1, 30]) {
        bag.items[slot] = ItemSlot {
            item_id: if slot == 1 {
                ItemId::ANTIDOTE.0
            } else {
                ItemId::POTION.0
            },
            quantity,
        };
    }
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    open_ok(&mut screen, Buttons::DOWN);
    choose_toss(&mut screen);
    open_ok(&mut screen, Buttons::A);
    let BagScreenTick::Tossed { bag } = tick(&mut screen, Buttons::A) else {
        panic!("returns the bag");
    };
    assert_eq!([bag.items[0].quantity, bag.items[1].quantity], [10, 30]);
    assert_eq!(screen.positions(), screen.menu_request.positions);
    // SELECT swap then works from the bag after the toss.
    open_ok(&mut screen, Buttons::UP);
    open_ok(&mut screen, Buttons::SELECT);
    open_ok(&mut screen, Buttons::DOWN);
    open_ok(&mut screen, Buttons::DOWN);
    let BagScreenTick::Reordered { bag } = tick(&mut screen, Buttons::A) else {
        panic!("the swap reorders the bag after the toss");
    };
    assert_eq!([bag.items[0].quantity, bag.items[1].quantity], [30, 10]);
}

#[test]
fn closing_a_prompt_screen_is_not_possible_but_b_on_the_list_still_closes() {
    let mut screen = quantity_screen();
    open_ok(&mut screen, Buttons::B);
    assert_eq!(
        tick(&mut screen, Buttons::B),
        BagScreenTick::Closed(BagScreenResult::ReturnToField)
    );
}

#[test]
fn the_prompt_texts_expand_the_item_and_count() {
    // `strings.c:233-235`.
    assert_eq!(toss::question("POTION"), "Toss out how many\nPOTION(s)?");
    assert_eq!(
        toss::confirmation("POTION", 3),
        "Is it okay to\nthrow away 3\nPOTION(s)?"
    );
    assert_eq!(toss::result("POTION", 12), "Threw away 12\nPOTION(s).");
    assert_eq!(toss::count_text(1, 2), "×01");
    assert_eq!(toss::count_text(7, 3), "×007");
}
