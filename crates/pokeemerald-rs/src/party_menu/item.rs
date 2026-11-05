//! The held-item transactions the ITEM submenu hands to the host.
//!
//! The screen never owns the bag, so TAKE is a host step the host runs on its
//! bag and the screen's records and reports back with
//! [`super::PartyScreen::resume`].

use assets::items::ItemId;
use engine::save::bag::Bag;
use engine::save::Pokemon;

use crate::party::records::{set_held_item, SlotFacts};

/// What taking a member's held item came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TakeItemOutcome {
    /// The member holds nothing; nothing changed.
    NotHolding,
    /// The bag has no room; nothing changed.
    BagFull(ItemId),
    /// The item moved to the bag and the member holds nothing.
    Took(ItemId),
}

/// `TryTakeMonItem` (`pokeemerald/src/party_menu.c:1813-1824`): the held item
/// leaves the member only once the bag has accepted it.
pub(crate) fn take_held_item(record: &mut Pokemon, bag: &mut Bag) -> TakeItemOutcome {
    let item = SlotFacts::read(record).held_item;
    if item == ItemId::NONE {
        return TakeItemOutcome::NotHolding;
    }
    if !bag.add_item(item, 1) {
        return TakeItemOutcome::BagFull(item);
    }
    set_held_item(record, ItemId::NONE);
    TakeItemOutcome::Took(item)
}
