//! The Battle Frontier held-item index lookup.
//!
//! Source: `gBattleFrontierHeldItems` (`src/battle_tower.c:82-147`), indexed by
//! the `BATTLE_FRONTIER_ITEM_*` constants of `include/constants/battle_frontier.h`.
//! A Frontier facility's party record does not store an item id. It stores a
//! one-byte index (`itemTableId`, `include/battle_tower.h:31`) into this
//! 63-entry list, and generation resolves it to the real item
//! (`src/battle_factory.c:365-373`).
//!
//! The index and the item id are different spaces: index `1` is King's Rock,
//! whose [`ItemId`] is `187`. [`FrontierItemIndex`] keeps the two apart at the
//! type level. Index `0` is a real entry that resolves to [`ItemId::NONE`]
//! (no held item), so a record can say "holds nothing" through the same path
//! as any other.
//!
//! Only the lookup is modelled. Facility party generation, the duplicate-item
//! check against the rest of a team, held-item effects and randomness are not.

use std::error::Error;
use std::fmt;

use crate::items::ItemId;

/// The number of Frontier held-item indices (`0` through `62`).
pub const FRONTIER_HELD_ITEM_COUNT: usize = 63;

/// An index into the Frontier held-item list (a `BATTLE_FRONTIER_ITEM_*`
/// value, upstream's `itemTableId`).
///
/// This is not an [`ItemId`]: resolve it with [`frontier_held_item`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrontierItemIndex(pub u8);

impl FrontierItemIndex {
    /// `BATTLE_FRONTIER_ITEM_NONE`: the entry that holds no item.
    pub const NONE: FrontierItemIndex = FrontierItemIndex(0);

    /// `BATTLE_FRONTIER_ITEM_GANLON_BERRY`: the last entry.
    pub const GANLON_BERRY: FrontierItemIndex = FrontierItemIndex(62);

    /// Returns the numeric list index.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// A Frontier held-item index outside the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierHeldItemError {
    /// The index is not below [`FRONTIER_HELD_ITEM_COUNT`].
    InvalidIndex(FrontierItemIndex),
}

impl fmt::Display for FrontierHeldItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIndex(index) => write!(
                f,
                "Battle Frontier held-item index `{}` is out of range (there are \
                 {FRONTIER_HELD_ITEM_COUNT})",
                index.index()
            ),
        }
    }
}

impl Error for FrontierHeldItemError {}

/// The item each Frontier index resolves to, in index order.
#[rustfmt::skip]
const HELD_ITEMS: [ItemId; FRONTIER_HELD_ITEM_COUNT] = [
    ItemId::NONE,
    ItemId::KINGS_ROCK,
    ItemId::SITRUS_BERRY,
    ItemId::ORAN_BERRY,
    ItemId::CHESTO_BERRY,
    ItemId::HARD_STONE,
    ItemId::FOCUS_BAND,
    ItemId::PERSIM_BERRY,
    ItemId::MIRACLE_SEED,
    ItemId::BERRY_JUICE,
    ItemId::MACHO_BRACE,
    ItemId::SILVER_POWDER,
    ItemId::CHERI_BERRY,
    ItemId::BLACK_GLASSES,
    ItemId::BLACK_BELT,
    ItemId::SOUL_DEW,
    ItemId::CHOICE_BAND,
    ItemId::MAGNET,
    ItemId::SILK_SCARF,
    ItemId::WHITE_HERB,
    ItemId::DEEP_SEA_SCALE,
    ItemId::DEEP_SEA_TOOTH,
    ItemId::MYSTIC_WATER,
    ItemId::SHARP_BEAK,
    ItemId::QUICK_CLAW,
    ItemId::LEFTOVERS,
    ItemId::RAWST_BERRY,
    ItemId::LIGHT_BALL,
    ItemId::POISON_BARB,
    ItemId::NEVER_MELT_ICE,
    ItemId::ASPEAR_BERRY,
    ItemId::SPELL_TAG,
    ItemId::BRIGHT_POWDER,
    ItemId::LEPPA_BERRY,
    ItemId::SCOPE_LENS,
    ItemId::TWISTED_SPOON,
    ItemId::METAL_COAT,
    ItemId::MENTAL_HERB,
    ItemId::CHARCOAL,
    ItemId::PECHA_BERRY,
    ItemId::SOFT_SAND,
    ItemId::LUM_BERRY,
    ItemId::DRAGON_SCALE,
    ItemId::DRAGON_FANG,
    ItemId::IAPAPA_BERRY,
    ItemId::WIKI_BERRY,
    ItemId::SEA_INCENSE,
    ItemId::SHELL_BELL,
    ItemId::SALAC_BERRY,
    ItemId::LANSAT_BERRY,
    ItemId::APICOT_BERRY,
    ItemId::STARF_BERRY,
    ItemId::LIECHI_BERRY,
    ItemId::STICK,
    ItemId::LAX_INCENSE,
    ItemId::AGUAV_BERRY,
    ItemId::FIGY_BERRY,
    ItemId::THICK_CLUB,
    ItemId::MAGO_BERRY,
    ItemId::METAL_POWDER,
    ItemId::PETAYA_BERRY,
    ItemId::LUCKY_PUNCH,
    ItemId::GANLON_BERRY,
];

/// Returns the item a Frontier held-item `index` resolves to.
///
/// Index `0` resolves to [`ItemId::NONE`], which is a valid answer meaning no
/// held item, not an error.
///
/// # Errors
///
/// Returns [`FrontierHeldItemError::InvalidIndex`] when `index` is not below
/// [`FRONTIER_HELD_ITEM_COUNT`].
pub fn frontier_held_item(index: FrontierItemIndex) -> Result<ItemId, FrontierHeldItemError> {
    HELD_ITEMS
        .get(usize::from(index.index()))
        .copied()
        .ok_or(FrontierHeldItemError::InvalidIndex(index))
}

#[cfg(test)]
mod tests {
    use super::{
        frontier_held_item, FrontierHeldItemError, FrontierItemIndex, FRONTIER_HELD_ITEM_COUNT,
    };
    use crate::items::ItemId;

    /// Every Frontier index and the numeric upstream item id it resolves to,
    /// generated from `gBattleFrontierHeldItems` (`src/battle_tower.c:82-147`)
    /// and the `ITEM_*` values of `include/constants/items.h`.
    const TABLE: [(u8, u16); FRONTIER_HELD_ITEM_COUNT] = [
        (0, 0),    // NONE
        (1, 187),  // KINGS_ROCK
        (2, 142),  // SITRUS_BERRY
        (3, 139),  // ORAN_BERRY
        (4, 134),  // CHESTO_BERRY
        (5, 204),  // HARD_STONE
        (6, 196),  // FOCUS_BAND
        (7, 140),  // PERSIM_BERRY
        (8, 205),  // MIRACLE_SEED
        (9, 44),   // BERRY_JUICE
        (10, 181), // MACHO_BRACE
        (11, 188), // SILVER_POWDER
        (12, 133), // CHERI_BERRY
        (13, 206), // BLACK_GLASSES
        (14, 207), // BLACK_BELT
        (15, 191), // SOUL_DEW
        (16, 186), // CHOICE_BAND
        (17, 208), // MAGNET
        (18, 217), // SILK_SCARF
        (19, 180), // WHITE_HERB
        (20, 193), // DEEP_SEA_SCALE
        (21, 192), // DEEP_SEA_TOOTH
        (22, 209), // MYSTIC_WATER
        (23, 210), // SHARP_BEAK
        (24, 183), // QUICK_CLAW
        (25, 200), // LEFTOVERS
        (26, 136), // RAWST_BERRY
        (27, 202), // LIGHT_BALL
        (28, 211), // POISON_BARB
        (29, 212), // NEVER_MELT_ICE
        (30, 137), // ASPEAR_BERRY
        (31, 213), // SPELL_TAG
        (32, 179), // BRIGHT_POWDER
        (33, 138), // LEPPA_BERRY
        (34, 198), // SCOPE_LENS
        (35, 214), // TWISTED_SPOON
        (36, 199), // METAL_COAT
        (37, 185), // MENTAL_HERB
        (38, 215), // CHARCOAL
        (39, 135), // PECHA_BERRY
        (40, 203), // SOFT_SAND
        (41, 141), // LUM_BERRY
        (42, 201), // DRAGON_SCALE
        (43, 216), // DRAGON_FANG
        (44, 147), // IAPAPA_BERRY
        (45, 144), // WIKI_BERRY
        (46, 220), // SEA_INCENSE
        (47, 219), // SHELL_BELL
        (48, 170), // SALAC_BERRY
        (49, 173), // LANSAT_BERRY
        (50, 172), // APICOT_BERRY
        (51, 174), // STARF_BERRY
        (52, 168), // LIECHI_BERRY
        (53, 225), // STICK
        (54, 221), // LAX_INCENSE
        (55, 146), // AGUAV_BERRY
        (56, 143), // FIGY_BERRY
        (57, 224), // THICK_CLUB
        (58, 145), // MAGO_BERRY
        (59, 223), // METAL_POWDER
        (60, 171), // PETAYA_BERRY
        (61, 222), // LUCKY_PUNCH
        (62, 169), // GANLON_BERRY
    ];

    #[test]
    fn every_frontier_index_resolves_to_its_upstream_item() {
        for (index, item) in TABLE {
            assert_eq!(
                frontier_held_item(FrontierItemIndex(index)),
                Ok(ItemId(item)),
                "frontier index {index}"
            );
        }
    }

    #[test]
    fn fixture_covers_each_index_once_in_order() {
        for (position, (index, _)) in TABLE.iter().enumerate() {
            assert_eq!(usize::from(*index), position);
        }
    }

    #[test]
    fn the_index_space_is_not_the_item_space() {
        // Index 0 is the no-item entry, so it is skipped; every other entry's
        // item id differs from its own index.
        for (index, item) in TABLE.into_iter().skip(1) {
            assert_ne!(u16::from(index), item, "frontier index {index}");
        }
        assert_eq!(
            frontier_held_item(FrontierItemIndex(1)),
            Ok(ItemId::KINGS_ROCK)
        );
        assert_ne!(ItemId::KINGS_ROCK, ItemId(1));
    }

    #[test]
    fn index_zero_is_the_no_item_entry() {
        assert_eq!(
            frontier_held_item(FrontierItemIndex::NONE),
            Ok(ItemId::NONE)
        );
    }

    #[test]
    fn the_last_index_is_ganlon_berry() {
        assert_eq!(
            frontier_held_item(FrontierItemIndex::GANLON_BERRY),
            Ok(ItemId::GANLON_BERRY)
        );
        assert_eq!(FrontierItemIndex::GANLON_BERRY.index(), 62);
    }

    #[test]
    fn indices_past_the_list_are_refused() {
        for raw in [63_u8, 64, 255] {
            let index = FrontierItemIndex(raw);
            assert_eq!(
                frontier_held_item(index),
                Err(FrontierHeldItemError::InvalidIndex(index))
            );
        }
    }

    #[test]
    fn the_error_names_the_index_and_the_count() {
        let message = FrontierHeldItemError::InvalidIndex(FrontierItemIndex(63)).to_string();
        assert!(message.contains("63"), "{message}");
        assert!(message.contains("there are 63"), "{message}");
    }
}
