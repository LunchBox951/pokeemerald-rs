//! What the bag changes in the live save, and what survives between visits:
//! the request built from the save, reordered and tossed bags, retained
//! positions, and the saved file.

use assets::items::{ItemId, Pocket};
use engine::save::bag::{Bag, ItemSlot};
use engine::save::{ButtonMode, PlayerGender};
use platform::Buttons;

use super::test_support::Session;
use crate::bag_menu::{host_fixture, BagScreenMode};
use crate::flow::save_continue_support::{drive_start_menu, new_game_phase};
use crate::start_menu::StartMenuItem;

fn session(label: &str) -> Session {
    Session::new(label, &host_fixture::pack_bytes())
}

/// Four stacks told apart by quantity.
fn abcd() -> Bag {
    let mut bag = Bag::default();
    for (index, quantity) in [10u16, 20, 30, 40].into_iter().enumerate() {
        let item = if index % 2 == 0 {
            ItemId::POTION
        } else {
            ItemId::ANTIDOTE
        };
        bag.items[index] = ItemSlot {
            item_id: item.0,
            quantity,
        };
    }
    bag
}

fn quantities(bag: &Bag) -> Vec<u16> {
    bag.items
        .iter()
        .map(|slot| slot.quantity)
        .filter(|&quantity| quantity != 0)
        .collect()
}

#[test]
fn the_request_comes_from_the_live_save() {
    let mut phase = new_game_phase();
    phase.save1.registered_item = ItemId(0x1AB);
    phase.save2.player_gender = PlayerGender::Female;
    phase.save2.options_window_frame_type = 7;
    phase.save2.options_button_mode = ButtonMode::Lr;
    phase.field_bag.positions.pocket = Pocket::PokeBalls;

    let request = phase.field_bag_request();
    assert_eq!(request.mode, BagScreenMode::Field);
    assert_eq!(request.registered_item, Some(ItemId(0x1AB)));
    assert!(request.female && request.lr_buttons);
    assert_eq!(request.window_frame, 7);
    assert_eq!(request.positions.pocket, Pocket::PokeBalls);
    assert!(!request.bike_avatar && request.link.is_none());

    phase.save1.registered_item = ItemId::NONE;
    phase.save2.player_gender = PlayerGender::Male;
    phase.save2.options_button_mode = ButtonMode::Normal;
    let request = phase.field_bag_request();
    assert_eq!(request.registered_item, None);
    assert!(!request.female && !request.lr_buttons);
}

/// L is A in the L=A button mode: it chooses BAG instead of doing nothing.
#[test]
fn l_chooses_bag_in_the_l_equals_a_mode() {
    let mut s = session("bag-l-equals-a");
    s.phase_mut().save2.options_button_mode = ButtonMode::LEqualsA;
    s.open_start_menu();
    s.idle(2);
    s.frame(Buttons::L);
    assert!(s.phase().bag_trip_active());
}

/// SELECT, DOWN, DOWN and A move the first stack below the second, and the
/// live bag has the new order on the frame the screen reports it, while the
/// screen is still up. Putting the item back with B changes nothing.
#[test]
fn a_reordered_bag_reaches_the_live_save_and_a_cancelled_swap_does_not() {
    let mut s = session("bag-reorder");
    s.phase_mut().save1.bag = abcd();
    s.open_the_bag();

    for button in [Buttons::SELECT, Buttons::DOWN, Buttons::B] {
        s.tap(button);
    }
    assert!(
        s.browsing(),
        "B puts the item back and leaves the screen up"
    );
    assert_eq!(quantities(&s.phase().save1.bag), [10, 20, 30, 40]);

    for button in [Buttons::SELECT, Buttons::DOWN, Buttons::DOWN] {
        s.tap(button);
    }
    assert_eq!(
        quantities(&s.phase().save1.bag),
        [10, 20, 30, 40],
        "not yet"
    );
    s.frame(Buttons::A);
    assert_eq!(quantities(&s.phase().save1.bag), [20, 10, 30, 40]);
    assert!(s.browsing(), "the screen stays up on the new order");
}

/// Five potions, TOSS, two of them: the live bag keeps three, but only once
/// the message has been acknowledged.
#[test]
fn a_tossed_bag_reaches_the_live_save_on_the_acknowledgement() {
    let mut s = session("bag-toss");
    let mut bag = Bag::default();
    bag.items[0] = ItemSlot {
        item_id: ItemId::POTION.0,
        quantity: 5,
    };
    s.phase_mut().save1.bag = bag;
    s.open_the_bag();

    for button in [
        Buttons::A,
        Buttons::DOWN,
        Buttons::A,
        Buttons::UP,
        Buttons::A,
        Buttons::A,
    ] {
        s.tap(button);
    }
    assert_eq!(
        s.phase().save1.bag.items[0].quantity,
        5,
        "the message is still up"
    );
    s.frame(Buttons::A);
    assert_eq!(s.phase().save1.bag.items[0].quantity, 3);
    assert!(s.browsing());
}

/// The pocket and rows the player left the bag on come back, through a
/// reopened bag and through a start menu that was closed in between.
#[test]
fn positions_are_kept_between_visits_and_between_menus() {
    let mut s = session("bag-positions");
    let mut bag = Bag::default();
    for index in 0..12 {
        bag.items[index] = ItemSlot {
            item_id: ItemId::POTION.0,
            quantity: 1 + u16::try_from(index).unwrap(),
        };
    }
    s.phase_mut().save1.bag = bag;
    s.open_the_bag();
    for _ in 0..10 {
        s.tap(Buttons::DOWN);
    }
    s.close_the_bag();
    let left = s.phase().field_bag.positions;
    assert_eq!(left.pocket, Pocket::Items);
    assert!(left.cursor.iter().sum::<u16>() > 0 && left.scroll.iter().sum::<u16>() > 0);

    // Reopen, switch pocket, close.
    s.tap(Buttons::A);
    s.wait_for_the_bag();
    assert_eq!(
        s.phase().field_bag.positions,
        left,
        "kept while the bag is up"
    );
    s.tap(Buttons::RIGHT);
    s.idle(host_fixture::SWITCH_TICKS.into());
    s.close_the_bag();
    let switched = s.phase().field_bag.positions;
    assert_ne!(switched.pocket, Pocket::Items);
    assert_eq!(switched.cursor, left.cursor);
    assert_eq!(switched.scroll, left.scroll);

    // Close the start menu, open it again, and open the bag on its pocket.
    s.tap(Buttons::START);
    assert!(s.phase().start_menu().is_none());
    s.open_start_menu();
    assert_eq!(
        s.phase()
            .start_menu()
            .map(crate::start_menu::StartMenu::selected),
        Some(StartMenuItem::Bag)
    );
    s.choose_bag();
    s.wait_for_the_bag();
    s.close_the_bag();
    assert_eq!(s.phase().field_bag.positions, switched);
}

/// The reordered bag is in the file a SAVE from the returned menu writes,
/// and nothing was written before it.
#[test]
fn the_reordered_bag_is_saved_from_the_returned_menu() {
    let mut s = session("bag-saved");
    s.phase_mut().save1.bag = abcd();
    s.open_the_bag();
    for button in [Buttons::SELECT, Buttons::DOWN, Buttons::DOWN, Buttons::A] {
        s.tap(button);
    }
    s.close_the_bag();
    assert!(
        !s.save.slot().load().status.menu_shows_continue(),
        "visiting the bag writes nothing"
    );

    s.tap(Buttons::DOWN);
    assert_eq!(
        s.phase()
            .start_menu()
            .map(crate::start_menu::StartMenu::selected),
        Some(StartMenuItem::Save),
        "one DOWN from BAG is SAVE"
    );
    let mut slot = s.save.slot();
    let phase = s.phase_mut();
    drive_start_menu(phase, &mut slot, &[]);
    let saved = s.save.slot().load();
    assert_eq!(quantities(&saved.block1.bag), [20, 10, 30, 40]);
}
