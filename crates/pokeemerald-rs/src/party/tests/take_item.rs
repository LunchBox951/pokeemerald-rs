//! `TryTakeMonItem` / `CursorCb_TakeItem` (`pokeemerald/src/party_menu.c:1813-1825`,
//! `:3273-3298`): the held item moves into the bag, and the save follows.

use super::super::{
    from_save_pokemon, merge_into_save_pokemon, take_held_item, LoadedLead, TakeHeldItem,
};
use super::common::{saved_treecko_holding, EXPECTED_GROWTH_HELD_ITEM};
use assets::items::ItemId;
use battle::Dex;
use engine::save::bag::{ItemSlot, MAX_BERRY_CAPACITY};
use engine::save::Bag;

fn berry_quantity(bag: &Bag, item: ItemId) -> u16 {
    bag.berries
        .iter()
        .filter(|slot| slot.item_id == item.0)
        .map(|slot| slot.quantity)
        .sum()
}

#[test]
fn taking_the_item_puts_one_in_the_bag_and_empties_the_battler() {
    let dex = Dex::new();
    let mut lead = from_save_pokemon(&dex, &saved_treecko_holding(ItemId::ORAN_BERRY)).unwrap();
    let mut bag = Bag::default();
    bag.add_item(ItemId::ORAN_BERRY, 2);

    let outcome = take_held_item(&mut lead, &mut bag);

    assert_eq!(outcome, TakeHeldItem::Took(ItemId::ORAN_BERRY));
    assert_eq!(lead.held_item(), ItemId::NONE);
    assert_eq!(berry_quantity(&bag, ItemId::ORAN_BERRY), 3, "two plus one");
}

#[test]
fn a_mon_holding_nothing_changes_nothing() {
    let dex = Dex::new();
    let mut lead = from_save_pokemon(&dex, &saved_treecko_holding(ItemId::NONE)).unwrap();
    let mut bag = Bag::default();

    assert_eq!(
        take_held_item(&mut lead, &mut bag),
        TakeHeldItem::NotHolding
    );
    assert_eq!(bag, Bag::default());
    assert_eq!(lead.held_item(), ItemId::NONE);
}

/// Upstream offers TAKE MAIL, not TAKE ITEM, for a mail holder (`SetPartyMonFieldSelectionActions`, `party_menu.c:2631-2634`).
#[test]
fn a_mon_holding_mail_is_refused_untouched() {
    let dex = Dex::new();
    let mut lead = from_save_pokemon(&dex, &saved_treecko_holding(ItemId::ORANGE_MAIL)).unwrap();
    let mut bag = Bag::default();

    assert_eq!(
        take_held_item(&mut lead, &mut bag),
        TakeHeldItem::HoldsMail(ItemId::ORANGE_MAIL)
    );
    assert_eq!(bag, Bag::default());
    assert_eq!(lead.held_item(), ItemId::ORANGE_MAIL);
}

/// `ItemIsMail` checks mail ids, not `ItemType(0)`, which Master Ball shares.
#[test]
fn a_master_ball_is_taken_not_refused_as_mail() {
    let dex = Dex::new();
    let mut lead = from_save_pokemon(&dex, &saved_treecko_holding(ItemId::MASTER_BALL)).unwrap();
    let mut bag = Bag::default();

    assert_eq!(
        take_held_item(&mut lead, &mut bag),
        TakeHeldItem::Took(ItemId::MASTER_BALL)
    );
    assert_eq!(lead.held_item(), ItemId::NONE);
}

/// Both ends of the mail id range are mail.
#[test]
fn the_first_and_last_mail_ids_are_refused() {
    let dex = Dex::new();
    for mail in [ItemId::ORANGE_MAIL, ItemId::RETRO_MAIL] {
        let mut lead = from_save_pokemon(&dex, &saved_treecko_holding(mail)).unwrap();
        let mut bag = Bag::default();
        assert_eq!(
            take_held_item(&mut lead, &mut bag),
            TakeHeldItem::HoldsMail(mail)
        );
    }
}

/// `AddBagItem` fails ahead of the clear (`party_menu.c:1818-1822`): the item
/// stays on the Pokemon rather than vanishing.
#[test]
fn a_bag_that_cannot_hold_the_item_refuses_the_take_and_the_lead_keeps_it() {
    let dex = Dex::new();
    let mut lead = from_save_pokemon(&dex, &saved_treecko_holding(ItemId::ORAN_BERRY)).unwrap();
    let mut bag = Bag::default();
    bag.berries[0] = ItemSlot {
        item_id: ItemId::ORAN_BERRY.0,
        quantity: MAX_BERRY_CAPACITY,
    };
    let full = bag.clone();

    let outcome = take_held_item(&mut lead, &mut bag);

    assert_eq!(outcome, TakeHeldItem::BagFull(ItemId::ORAN_BERRY));
    assert_eq!(lead.held_item(), ItemId::ORAN_BERRY);
    assert_eq!(bag, full, "a refused take leaves the bag as it was");
}

/// The merge files the battler's item over the record it was loaded from, so
/// a save made after the take no longer holds it, and a reload sees none.
#[test]
fn the_saved_record_reflects_the_take_after_the_merge() {
    let dex = Dex::new();
    let stored = saved_treecko_holding(ItemId::ORAN_BERRY);
    let mut loaded = LoadedLead::load(&dex, std::slice::from_ref(&stored)).unwrap();
    let mut bag = Bag::default();

    assert_eq!(
        take_held_item(loaded.battler_mut(), &mut bag),
        TakeHeldItem::Took(ItemId::ORAN_BERRY)
    );
    let merged = loaded.merge_and_save(&dex);

    let growth = merged.box_data.substructures().unwrap().growth;
    assert_eq!(
        growth[EXPECTED_GROWTH_HELD_ITEM],
        ItemId::NONE.0.to_le_bytes()
    );
    assert_eq!(
        from_save_pokemon(&dex, &merged).unwrap().held_item(),
        ItemId::NONE
    );
    assert_eq!(berry_quantity(&bag, ItemId::ORAN_BERRY), 1);
}

#[test]
fn a_merge_without_a_take_keeps_the_saved_item() {
    let dex = Dex::new();
    let stored = saved_treecko_holding(ItemId::ORAN_BERRY);
    let lead = from_save_pokemon(&dex, &stored).unwrap();

    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut 0);

    assert_eq!(
        from_save_pokemon(&dex, &merged).unwrap().held_item(),
        ItemId::ORAN_BERRY
    );
}
