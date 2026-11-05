//! ITEM: the submenu and the host steps behind GIVE and TAKE.

use assets::items::ItemId;
use engine::save::bag::Bag;

use super::*;
use crate::party::records::SlotFacts;
use crate::party_menu::item::{take_held_item, TakeItemOutcome};

fn item_menu() -> (PartyScreen, Pad) {
    let mut screen = field(party(vec![Mon::new(1).held(POTION), Mon::new(2)]));
    let mut pad = Pad::new();
    // SUMMARY, SWITCH, ITEM, CANCEL
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::A, Buttons::DOWN, Buttons::DOWN, Buttons::A],
    );
    (screen, pad)
}

#[test]
fn item_opens_give_take_cancel_even_when_nothing_is_held() {
    use PartyAction::{CancelToActions, Give, TakeItem};
    let (screen, _) = item_menu();
    assert_eq!(screen.menu().unwrap().0, [Give, TakeItem, CancelToActions]);
}

#[test]
fn the_submenus_cancel_returns_to_the_members_action_menu() {
    use PartyAction::{Cancel, Item, Summary, Switch};
    let (mut screen, mut pad) = item_menu();
    tap(&mut screen, &mut pad, Buttons::B);
    assert_eq!(screen.menu().unwrap().0, [Summary, Switch, Item, Cancel]);
    assert_eq!(
        screen.menu().unwrap().1,
        0,
        "the menu starts over on its first line"
    );
}

#[test]
fn give_leaves_for_the_bag_and_the_screen_waits_for_the_host() {
    let (mut screen, mut pad) = item_menu();
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Host(PartyHostRequest::GiveItem { slot: 0 })
    );
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Open
    );
    assert!(
        screen.menu().is_none(),
        "input is ignored while the host works"
    );
    screen.resume(PartyResume::ActionMenu);
    assert!(
        screen.menu().is_some(),
        "backing out of the bag reopens the action menu"
    );
}

#[test]
fn take_is_run_by_the_host_and_the_list_returns_with_the_member_updated() {
    let (mut screen, mut pad) = item_menu();
    tap(&mut screen, &mut pad, Buttons::DOWN);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Host(PartyHostRequest::TakeItem { slot: 0 })
    );
    let mut bag = Bag::default();
    let mut record = *screen.records().get(0).unwrap();
    assert_eq!(
        take_held_item(&mut record, &mut bag),
        TakeItemOutcome::Took(ItemId(POTION))
    );
    screen.replace_record(0, record);
    screen.resume(PartyResume::List);
    assert!(screen.menu().is_none());
    assert_eq!(screen.cursor(), PartyCursor::Mon(0));
    assert_eq!(
        SlotFacts::read(screen.records().get(0).unwrap()).held_item,
        ItemId::NONE
    );
}

#[test]
fn resume_without_a_pending_request_does_nothing() {
    let mut screen = field(three());
    screen.resume(PartyResume::ActionMenu);
    assert!(screen.menu().is_none());
}

#[test]
fn taking_an_item_moves_it_to_the_bag() {
    let mut record = Mon::new(1).held(POTION).build();
    let mut bag = Bag::default();
    assert_eq!(
        take_held_item(&mut record, &mut bag),
        TakeItemOutcome::Took(ItemId(POTION))
    );
    assert_eq!(SlotFacts::read(&record).held_item, ItemId::NONE);
    assert!(bag
        .items
        .iter()
        .any(|stack| stack.item_id == POTION && stack.quantity == 1));
}

#[test]
fn taking_from_a_member_holding_nothing_changes_nothing() {
    let mut record = Mon::new(1).build();
    let before = record;
    let mut bag = Bag::default();
    assert_eq!(
        take_held_item(&mut record, &mut bag),
        TakeItemOutcome::NotHolding
    );
    assert_eq!(record, before);
}

#[test]
fn a_full_bag_keeps_the_item_on_the_member() {
    let mut record = Mon::new(1).held(POTION).build();
    let before = record;
    let mut bag = Bag::default();
    for slot in &mut bag.items {
        slot.item_id = POTION + 1;
        slot.quantity = 1;
    }
    let bag_before = bag.clone();
    assert_eq!(
        take_held_item(&mut record, &mut bag),
        TakeItemOutcome::BagFull(ItemId(POTION))
    );
    assert_eq!(record, before);
    assert_eq!(bag, bag_before);
}

#[test]
fn backing_out_of_gives_bag_forgets_the_remembered_side_panel() {
    let mut screen = field(party((1..=3).map(Mon::new).collect()));
    let mut pad = Pad::new();
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::DOWN, Buttons::DOWN, Buttons::LEFT],
    );
    assert_eq!(screen.cursor(), PartyCursor::Mon(0));
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::A, Buttons::DOWN, Buttons::DOWN, Buttons::A],
    );
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Host(PartyHostRequest::GiveItem { slot: 0 })
    );
    screen.resume(PartyResume::ActionMenu);
    assert!(screen.menu().is_some());
    tap(&mut screen, &mut pad, Buttons::B);
    assert_eq!(screen.cursor(), PartyCursor::Mon(0));
    tap(&mut screen, &mut pad, Buttons::RIGHT);
    assert_eq!(
        screen.cursor(),
        PartyCursor::Mon(1),
        "slot 1, not the old slot 2"
    );
}

#[test]
fn a_resume_within_the_session_keeps_the_remembered_side_panel() {
    let mut screen = field(party((1..=3).map(Mon::new).collect()));
    let mut pad = Pad::new();
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::DOWN, Buttons::DOWN, Buttons::LEFT],
    );
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::A, Buttons::DOWN, Buttons::DOWN, Buttons::A],
    );
    tap(&mut screen, &mut pad, Buttons::DOWN);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Host(PartyHostRequest::TakeItem { slot: 0 })
    );
    screen.resume(PartyResume::List);
    tap(&mut screen, &mut pad, Buttons::RIGHT);
    assert_eq!(screen.cursor(), PartyCursor::Mon(2));
}
