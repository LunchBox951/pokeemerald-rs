//! `TryTakeMonItem` (`pokeemerald/src/party_menu.c:1813-1825`), the effect
//! of the party menu's TAKE ITEM entry, which this port reaches from the
//! start menu: the lead's held item moves into the bag, and the save follows
//! through [`super::merge_into_save_pokemon`].

use battle::BattlePokemon;

/// What [`take_held_item`] did, mirroring `TryTakeMonItem`'s return
/// (`pokeemerald/src/party_menu.c:1813-1825`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TakeHeldItem {
    /// The Pokemon holds nothing (`return 0`); nothing changed.
    NotHolding,
    /// The Pokemon holds mail (`ItemIsMail`, `mail_data.c:185`). Not a
    /// `TryTakeMonItem` outcome: the party menu offers `CursorCb_TakeMail`
    /// in place of TAKE ITEM for a mail holder (`SetPartyMonFieldSelectionActions`, `party_menu.c:2631-2634`), so this
    /// port refuses here, unchanged, rather than take mail without clearing
    /// its record.
    HoldsMail(assets::items::ItemId),
    /// `AddBagItem` refused the item (`return 1`, "bag full, could not
    /// remove"); the bag and the Pokemon are exactly as they were.
    BagFull(assets::items::ItemId),
    /// The bag gained one of the item and the Pokemon holds nothing
    /// (`return 2`).
    Took(assets::items::ItemId),
}

/// `TryTakeMonItem` (`pokeemerald/src/party_menu.c:1813-1825`), the effect of
/// `CursorCb_TakeItem` (`:3273-3298`): moves `mon`'s held item into `bag`.
///
/// Mail is refused untouched ([`TakeHeldItem::HoldsMail`]); that check is the
/// port's stand-in for the menu never offering TAKE ITEM there, not part of
/// `TryTakeMonItem` itself.
///
/// `AddBagItem(item, 1)` runs before the item is cleared, so a bag that cannot
/// take it leaves the item on the Pokemon rather than losing it. The change
/// reaches the save through [`super::merge_into_save_pokemon`].
pub(crate) fn take_held_item(mon: &mut BattlePokemon, bag: &mut engine::save::Bag) -> TakeHeldItem {
    let item = mon.held_item();
    if item == assets::items::ItemId::NONE {
        return TakeHeldItem::NotHolding;
    }
    if item_is_mail(item) {
        return TakeHeldItem::HoldsMail(item);
    }
    if !bag.add_item(item, 1) {
        return TakeHeldItem::BagFull(item);
    }
    *mon = mon.clone().with_held_item(assets::items::ItemId::NONE);
    TakeHeldItem::Took(item)
}

/// `ItemIsMail` (`pokeemerald/src/mail_data.c:185`): the twelve mail ids,
/// `ITEM_ORANGE_MAIL..=ITEM_RETRO_MAIL`. Not an item-type test: Master Ball
/// shares mail's `ItemType(0)`.
fn item_is_mail(item: assets::items::ItemId) -> bool {
    (assets::items::ItemId::ORANGE_MAIL..=assets::items::ItemId::RETRO_MAIL).contains(&item)
}
