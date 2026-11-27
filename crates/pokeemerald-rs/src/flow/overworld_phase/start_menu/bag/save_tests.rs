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

/// Play time is counted once a frame through the fades, the bag and the
/// handoff, as on every other frame (`PlayTimeCounter_Update`,
/// `src/main.c:131-167`).
#[test]
fn play_time_counts_every_frame_of_the_trip() {
    fn vblanks(session: &Session) -> u64 {
        let time = session.phase().save2().play_time;
        ((u64::from(time.hours) * 60 + u64::from(time.minutes)) * 60 + u64::from(time.seconds)) * 60
            + u64::from(time.vblanks)
    }
    let mut session = session("bag-play-time");
    session.open_start_menu();
    let mut expected = vblanks(&session);
    let mut tick = |session: &mut Session, button| {
        session.frame(button);
        expected += 1;
        assert_eq!(vblanks(session), expected);
    };

    tick(&mut session, Buttons::A);
    assert!(
        session.phase().bag_trip_active(),
        "A on BAG starts the trip"
    );
    while !session.browsing() {
        tick(&mut session, Buttons::NONE);
    }
    tick(&mut session, Buttons::B);
    let mut frames = 0;
    while session.phase().bag_trip_active() {
        frames += 1;
        assert!(frames < 200, "the trip never returned to the field");
        tick(&mut session, Buttons::NONE);
    }
    // The frame the menu takes input back.
    tick(&mut session, Buttons::NONE);
}

/// `SetupBagMenu` state 10 runs `UpdatePocketItemLists` on the live pockets
/// (`item_menu.c:725-726,1102-1114`): a gap left by `RemoveBagItem` is packed
/// away by a BAG visit, so a later `AddBagItem` lands after the Antidote.
#[test]
fn a_bag_visit_packs_the_live_pockets() {
    let mut s = session("bag-verify-pack");
    let mut bag = Bag::default();
    bag.items[1] = ItemSlot {
        item_id: ItemId::ANTIDOTE.0,
        quantity: 3,
    };
    s.phase_mut().save1.bag = bag;
    s.open_the_bag();
    s.close_the_bag();
    let live = &s.phase().save1.bag;
    assert_eq!(
        live.items[0].item_id,
        ItemId::ANTIDOTE.0,
        "live bag: {:?}",
        &live.items[..2]
    );
    let mut after = live.clone();
    assert!(after.add_item(ItemId::POTION, 1));
    assert_eq!(after.items[1].item_id, ItemId::POTION.0);
}

/// The TM/HM and berry pockets are ordered by item id instead
/// (`SortBerriesOrTMHMs`, `item.c:615-633`; `item_menu.c:1107-1110`). The
/// scratch pack draws one berry only, so the berries are packed without a
/// visit.
#[test]
fn a_bag_visit_sorts_the_live_machines_by_item_id_as_packing_sorts_berries() {
    let one = |item: ItemId| ItemSlot {
        item_id: item.0,
        quantity: 1,
    };
    let ids = |slots: &[ItemSlot]| [slots[0].item_id, slots[1].item_id, slots[2].item_id];
    let mut s = session("bag-sort-pockets");
    let mut bag = Bag::default();
    bag.tms_hms[2] = one(ItemId::HM_CUT);
    bag.tms_hms[5] = one(ItemId::TM_ROAR);
    s.phase_mut().save1.bag = bag;
    s.open_the_bag();
    s.close_the_bag();
    assert_eq!(
        ids(&s.phase().save1.bag.tms_hms),
        [ItemId::TM_ROAR.0, ItemId::HM_CUT.0, 0]
    );

    let mut berries = Bag::default();
    berries.berries[0] = one(ItemId::ORAN_BERRY);
    berries.berries[3] = one(ItemId::CHERI_BERRY);
    crate::bag_menu::pack_pockets(&mut berries);
    assert_eq!(
        ids(&berries.berries),
        [ItemId::CHERI_BERRY.0, ItemId::ORAN_BERRY.0, 0]
    );
}

/// The packing is `SetupBagMenu`'s, which a failed `GoToBagMenu` never
/// installs (`item_menu.c:614-621`): the live pockets keep their gap.
#[test]
fn a_bag_that_cannot_open_leaves_the_live_pockets_unpacked() {
    let mut s = Session::new(
        "bag-unpacked",
        &host_fixture::pack_bytes_without_the_bag_screen(),
    );
    let mut bag = Bag::default();
    bag.items[1] = ItemSlot {
        item_id: ItemId::ANTIDOTE.0,
        quantity: 3,
    };
    s.phase_mut().save1.bag = bag.clone();
    s.open_start_menu();
    s.choose_bag();
    for _ in 0..200 {
        if !s.phase().bag_trip_active() {
            break;
        }
        s.frame(Buttons::NONE);
    }
    assert!(!s.phase().bag_trip_active(), "the trip never returned");
    assert_eq!(s.phase().save1.bag, bag);
}
