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
    /// in place of TAKE ITEM for a mail holder
    /// (`SetPartyMonFieldSelectionActions`, `party_menu.c:2631-2634`), and the
    /// start menu sends a mail holder to that flow (see [`super::mail`])
    /// before this runs. The refusal is a guard against a caller that skips
    /// that dispatch: mail is never taken without clearing its record.
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
/// Mail is refused untouched ([`TakeHeldItem::HoldsMail`]); that check is a
/// guard for callers that skip the menu's mail dispatch, not part of
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
    if engine::save::item_is_mail(item) {
        return TakeHeldItem::HoldsMail(item);
    }
    if !bag.add_item(item, 1) {
        return TakeHeldItem::BagFull(item);
    }
    *mon = mon.clone().with_held_item(assets::items::ItemId::NONE);
    TakeHeldItem::Took(item)
}
