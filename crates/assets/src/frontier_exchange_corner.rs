//! The Battle Frontier exchange corner's four reward menus: the ordered
//! rewards, each paired with its description, and the closing Exit entry.
//!
//! Source: `src/data/battle_frontier/battle_frontier_exchange_corner.h`, the
//! `BattleFrontier_ExchangeServiceCorner_Text_*Desc` strings in
//! `data/maps/BattleFrontier_ExchangeServiceCorner/scripts.inc`, and
//! `gText_Exit` (`src/strings.c`). BP prices, purchases, icons, and menu
//! rendering are not part of this table.

use std::error::Error;
use std::fmt;

use crate::decorations::DecorationId;
use crate::items::ItemId;

/// The `ITEM_LIST_END` value (`include/constants/items.h`) that ends each
/// upstream reward list in place of an Exit entry's reward.
pub const ITEM_LIST_END: u16 = 0xFFFF;

/// The text of `gText_Exit`, the description of every list's Exit entry.
pub const EXIT_DESCRIPTION: &str = "EXIT";

/// A failed lookup in the exchange corner menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExchangeCornerError {
    /// The value is not a `SCROLL_MULTI_BF_EXCHANGE_CORNER_*` menu ID.
    UnknownMenu(u16),
    /// The selection is past the menu's Exit entry.
    SelectionOutOfRange {
        /// The menu that was indexed.
        menu: ExchangeCornerMenu,
        /// The rejected selection.
        selection: u16,
    },
}

impl fmt::Display for ExchangeCornerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownMenu(id) => write!(f, "unknown exchange corner menu `{id}`"),
            Self::SelectionOutOfRange { menu, selection } => {
                write!(f, "selection `{selection}` is past the end of {menu:?}")
            }
        }
    }
}

impl Error for ExchangeCornerError {}

/// One exchange corner menu (`SCROLL_MULTI_BF_EXCHANGE_CORNER_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExchangeCornerMenu {
    /// `DECOR_VENDOR_1`: the small decorations.
    SmallDecorations,
    /// `DECOR_VENDOR_2`: the large decorations.
    LargeDecorations,
    /// `VITAMIN_VENDOR`.
    Vitamins,
    /// `HOLD_ITEM_VENDOR`.
    HeldItems,
}

impl ExchangeCornerMenu {
    /// The menus in ascending menu ID order.
    pub const ALL: [Self; 4] = [
        Self::SmallDecorations,
        Self::LargeDecorations,
        Self::Vitamins,
        Self::HeldItems,
    ];

    /// Decodes an upstream menu ID (3 through 6).
    ///
    /// # Errors
    ///
    /// Returns [`ExchangeCornerError::UnknownMenu`] for any other value.
    pub const fn from_id(id: u16) -> Result<Self, ExchangeCornerError> {
        match id {
            3 => Ok(Self::SmallDecorations),
            4 => Ok(Self::LargeDecorations),
            5 => Ok(Self::Vitamins),
            6 => Ok(Self::HeldItems),
            _ => Err(ExchangeCornerError::UnknownMenu(id)),
        }
    }

    /// Returns the upstream `SCROLL_MULTI_BF_EXCHANGE_CORNER_*` value.
    #[must_use]
    pub const fn id(self) -> u16 {
        self as u16 + 3
    }
}

/// What a menu entry hands over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExchangeCornerReward {
    /// A decoration (`DECOR_*`).
    Decoration(DecorationId),
    /// An item (`ITEM_*`).
    Item(ItemId),
    /// The Exit entry, stored upstream as [`ITEM_LIST_END`].
    Exit,
}

impl ExchangeCornerReward {
    /// Returns the value upstream stores in the reward list: the decoration
    /// or item ID, or [`ITEM_LIST_END`] for [`Self::Exit`].
    #[must_use]
    pub const fn raw_id(self) -> u16 {
        match self {
            Self::Decoration(id) => id.index() as u16,
            Self::Item(id) => id.index(),
            Self::Exit => ITEM_LIST_END,
        }
    }
}

/// A reward and the description shown while it is highlighted. Text is in the
/// game's charmap notation: `\n` is a real line break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExchangeCornerEntry {
    /// The reward list value.
    pub reward: ExchangeCornerReward,
    /// The paired entry of the description list.
    pub description: &'static str,
}

/// One menu's entries in source order, ending with its Exit entry.
#[derive(Debug, Clone, Copy)]
pub struct ExchangeCornerList {
    menu: ExchangeCornerMenu,
    entries: &'static [ExchangeCornerEntry],
}

impl ExchangeCornerList {
    /// Returns the menu this list belongs to.
    #[must_use]
    pub const fn menu(&self) -> ExchangeCornerMenu {
        self.menu
    }

    /// Returns every entry in source order; the last is the Exit entry.
    #[must_use]
    pub const fn entries(&self) -> &'static [ExchangeCornerEntry] {
        self.entries
    }

    /// Returns the number of entries, including Exit.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `false`; every list holds at least its Exit entry.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the entry at `selection`.
    ///
    /// # Errors
    ///
    /// Returns [`ExchangeCornerError::SelectionOutOfRange`] when `selection`
    /// is past the Exit entry.
    pub fn entry(
        &self,
        selection: u16,
    ) -> Result<&'static ExchangeCornerEntry, ExchangeCornerError> {
        self.entries
            .get(usize::from(selection))
            .ok_or(ExchangeCornerError::SelectionOutOfRange {
                menu: self.menu,
                selection,
            })
    }

    /// Returns the Exit entry.
    #[must_use]
    pub const fn exit(&self) -> &'static ExchangeCornerEntry {
        &self.entries[self.entries.len() - 1]
    }

    /// Iterates the rewards that can be bought, excluding Exit.
    pub fn rewards(&self) -> impl Iterator<Item = &'static ExchangeCornerEntry> {
        self.entries[..self.entries.len() - 1].iter()
    }
}

/// Lookup over the four exchange corner menus.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExchangeCornerTable;

impl ExchangeCornerTable {
    /// Creates a handle on the canonical menus.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns the list for `menu`.
    #[must_use]
    pub const fn list(&self, menu: ExchangeCornerMenu) -> ExchangeCornerList {
        let entries: &'static [ExchangeCornerEntry] = match menu {
            ExchangeCornerMenu::SmallDecorations => &SMALL_DECORATIONS,
            ExchangeCornerMenu::LargeDecorations => &LARGE_DECORATIONS,
            ExchangeCornerMenu::Vitamins => &VITAMINS,
            ExchangeCornerMenu::HeldItems => &HELD_ITEMS,
        };
        ExchangeCornerList { menu, entries }
    }

    /// Returns the entry `FillFrontierExchangeCornerWindowAndItemIcon` reads
    /// for a raw menu ID and selection.
    ///
    /// # Errors
    ///
    /// Returns [`ExchangeCornerError::UnknownMenu`] for a menu ID outside 3
    /// through 6, or [`ExchangeCornerError::SelectionOutOfRange`] for a
    /// selection past the Exit entry.
    pub fn entry(
        &self,
        menu: u16,
        selection: u16,
    ) -> Result<&'static ExchangeCornerEntry, ExchangeCornerError> {
        self.list(ExchangeCornerMenu::from_id(menu)?)
            .entry(selection)
    }
}

const fn decoration(id: DecorationId, description: &'static str) -> ExchangeCornerEntry {
    ExchangeCornerEntry {
        reward: ExchangeCornerReward::Decoration(id),
        description,
    }
}

const fn item(id: ItemId, description: &'static str) -> ExchangeCornerEntry {
    ExchangeCornerEntry {
        reward: ExchangeCornerReward::Item(id),
        description,
    }
}

const EXIT: ExchangeCornerEntry = ExchangeCornerEntry {
    reward: ExchangeCornerReward::Exit,
    description: EXIT_DESCRIPTION,
};

const LARGE_DOLL_DESCRIPTION: &str = "A large DOLL.\nPlace it on a mat or a desk.";

const SMALL_DECORATIONS: [ExchangeCornerEntry; 11] = [
    decoration(
        DecorationId::KISS_POSTER,
        "A large poster with a SMOOCHUM print.",
    ),
    decoration(
        DecorationId::KISS_CUSHION,
        "A SMOOCHUM cushion.\nPlace it on a mat or a desk.",
    ),
    decoration(
        DecorationId::SMOOCHUM_DOLL,
        "A SMOOCHUM DOLL.\nPlace it on a mat or a desk.",
    ),
    decoration(
        DecorationId::TOGEPI_DOLL,
        "A TOGEPI DOLL.\nPlace it on a mat or a desk.",
    ),
    decoration(
        DecorationId::MEOWTH_DOLL,
        "A MEOWTH DOLL.\nPlace it on a mat or a desk.",
    ),
    decoration(
        DecorationId::CLEFAIRY_DOLL,
        "A CLEFAIRY DOLL.\nPlace it on a mat or a desk.",
    ),
    decoration(
        DecorationId::DITTO_DOLL,
        "A DITTO DOLL.\nPlace it on a mat or a desk.",
    ),
    decoration(
        DecorationId::CYNDAQUIL_DOLL,
        "A CYNDAQUIL DOLL.\nPlace it on a mat or a desk.",
    ),
    decoration(
        DecorationId::CHIKORITA_DOLL,
        "A CHIKORITA DOLL.\nPlace it on a mat or a desk.",
    ),
    decoration(
        DecorationId::TOTODILE_DOLL,
        "A TOTODILE DOLL.\nPlace it on a mat or a desk.",
    ),
    EXIT,
];

const LARGE_DECORATIONS: [ExchangeCornerEntry; 6] = [
    decoration(DecorationId::LAPRAS_DOLL, LARGE_DOLL_DESCRIPTION),
    decoration(DecorationId::SNORLAX_DOLL, LARGE_DOLL_DESCRIPTION),
    decoration(DecorationId::VENUSAUR_DOLL, LARGE_DOLL_DESCRIPTION),
    decoration(DecorationId::CHARIZARD_DOLL, LARGE_DOLL_DESCRIPTION),
    decoration(DecorationId::BLASTOISE_DOLL, LARGE_DOLL_DESCRIPTION),
    EXIT,
];

const VITAMINS: [ExchangeCornerEntry; 7] = [
    item(ItemId::PROTEIN, "Raises the stat ATTACK of one\nPOKéMON."),
    item(ItemId::CALCIUM, "Raises the stat SP. ATK of one\nPOKéMON."),
    item(ItemId::IRON, "Raises the stat DEFENSE of one\nPOKéMON."),
    item(ItemId::ZINC, "Raises the stat SP. DEF of one\nPOKéMON."),
    item(ItemId::CARBOS, "Raises the stat SPEED of one\nPOKéMON."),
    item(ItemId::HP_UP, "Raises the HP of one POKéMON."),
    EXIT,
];

const HELD_ITEMS: [ExchangeCornerEntry; 10] = [
    item(
        ItemId::LEFTOVERS,
        "A hold item that gradually restores\nHP in battle.",
    ),
    item(
        ItemId::WHITE_HERB,
        "A hold item that restores any\nlowered stat.",
    ),
    item(
        ItemId::QUICK_CLAW,
        "A hold item that occasionally allows\nthe first strike.",
    ),
    item(
        ItemId::MENTAL_HERB,
        "A hold item that snaps POKéMON out\nof infatuation.",
    ),
    item(
        ItemId::BRIGHT_POWDER,
        "A hold item that casts a glare to\nreduce accuracy.",
    ),
    item(
        ItemId::CHOICE_BAND,
        "Raises a move's power, but permits\nonly that move.",
    ),
    item(
        ItemId::KINGS_ROCK,
        "A hold item that may cause flinching\nwhen the foe is hit.",
    ),
    item(
        ItemId::FOCUS_BAND,
        "A hold item that occasionally\nprevents fainting.",
    ),
    item(
        ItemId::SCOPE_LENS,
        "A hold item that raises the\ncritical-hit rate.",
    ),
    EXIT,
];

#[cfg(test)]
mod tests {
    use super::{
        ExchangeCornerError, ExchangeCornerMenu, ExchangeCornerReward, ExchangeCornerTable,
        EXIT_DESCRIPTION, ITEM_LIST_END,
    };

    // Raw IDs from include/constants/decorations.h and include/constants/items.h,
    // in the order of battle_frontier_exchange_corner.h.
    const SMALL_IDS: [u16; 10] = [75, 103, 87, 79, 84, 85, 86, 80, 81, 82];
    const LARGE_IDS: [u16; 5] = [113, 111, 114, 115, 116];
    const VITAMIN_IDS: [u16; 6] = [64, 67, 65, 70, 66, 63];
    const HELD_IDS: [u16; 9] = [200, 180, 183, 185, 179, 186, 187, 196, 198];

    const SMALL_TEXT: [&str; 10] = [
        "A large poster with a SMOOCHUM print.",
        "A SMOOCHUM cushion.\nPlace it on a mat or a desk.",
        "A SMOOCHUM DOLL.\nPlace it on a mat or a desk.",
        "A TOGEPI DOLL.\nPlace it on a mat or a desk.",
        "A MEOWTH DOLL.\nPlace it on a mat or a desk.",
        "A CLEFAIRY DOLL.\nPlace it on a mat or a desk.",
        "A DITTO DOLL.\nPlace it on a mat or a desk.",
        "A CYNDAQUIL DOLL.\nPlace it on a mat or a desk.",
        "A CHIKORITA DOLL.\nPlace it on a mat or a desk.",
        "A TOTODILE DOLL.\nPlace it on a mat or a desk.",
    ];
    const VITAMIN_TEXT: [&str; 6] = [
        "Raises the stat ATTACK of one\nPOKéMON.",
        "Raises the stat SP. ATK of one\nPOKéMON.",
        "Raises the stat DEFENSE of one\nPOKéMON.",
        "Raises the stat SP. DEF of one\nPOKéMON.",
        "Raises the stat SPEED of one\nPOKéMON.",
        "Raises the HP of one POKéMON.",
    ];
    const HELD_TEXT: [&str; 9] = [
        "A hold item that gradually restores\nHP in battle.",
        "A hold item that restores any\nlowered stat.",
        "A hold item that occasionally allows\nthe first strike.",
        "A hold item that snaps POKéMON out\nof infatuation.",
        "A hold item that casts a glare to\nreduce accuracy.",
        "Raises a move's power, but permits\nonly that move.",
        "A hold item that may cause flinching\nwhen the foe is hit.",
        "A hold item that occasionally\nprevents fainting.",
        "A hold item that raises the\ncritical-hit rate.",
    ];

    fn pairs(menu: ExchangeCornerMenu) -> Vec<(u16, &'static str)> {
        ExchangeCornerTable::new()
            .list(menu)
            .entries()
            .iter()
            .map(|e| (e.reward.raw_id(), e.description))
            .collect()
    }

    fn expect(ids: &[u16], text: &[&'static str]) -> Vec<(u16, &'static str)> {
        let mut rows: Vec<_> = ids.iter().copied().zip(text.iter().copied()).collect();
        rows.push((ITEM_LIST_END, "EXIT"));
        rows
    }

    #[test]
    fn small_decorations_match_source_order() {
        assert_eq!(
            pairs(ExchangeCornerMenu::SmallDecorations),
            expect(&SMALL_IDS, &SMALL_TEXT)
        );
    }

    #[test]
    fn large_decorations_share_one_description() {
        let large = ["A large DOLL.\nPlace it on a mat or a desk."; 5];
        assert_eq!(
            pairs(ExchangeCornerMenu::LargeDecorations),
            expect(&LARGE_IDS, &large)
        );
    }

    #[test]
    fn vitamins_match_source_order() {
        assert_eq!(
            pairs(ExchangeCornerMenu::Vitamins),
            expect(&VITAMIN_IDS, &VITAMIN_TEXT)
        );
    }

    #[test]
    fn held_items_match_source_order() {
        assert_eq!(
            pairs(ExchangeCornerMenu::HeldItems),
            expect(&HELD_IDS, &HELD_TEXT)
        );
    }

    #[test]
    fn rewards_use_decoration_ids_for_dolls_and_item_ids_for_goods() {
        let table = ExchangeCornerTable::new();
        for menu in [
            ExchangeCornerMenu::SmallDecorations,
            ExchangeCornerMenu::LargeDecorations,
        ] {
            for entry in table.list(menu).rewards() {
                assert!(matches!(entry.reward, ExchangeCornerReward::Decoration(_)));
            }
        }
        for menu in [ExchangeCornerMenu::Vitamins, ExchangeCornerMenu::HeldItems] {
            for entry in table.list(menu).rewards() {
                assert!(matches!(entry.reward, ExchangeCornerReward::Item(_)));
            }
        }
    }

    #[test]
    fn every_list_ends_with_exactly_one_exit() {
        let table = ExchangeCornerTable::new();
        let lengths = [11, 6, 7, 10];
        for (menu, length) in ExchangeCornerMenu::ALL.into_iter().zip(lengths) {
            let list = table.list(menu);
            assert_eq!(list.menu(), menu);
            assert_eq!(list.len(), length);
            assert!(!list.is_empty());
            assert_eq!(list.rewards().count(), length - 1);
            assert_eq!(list.exit().reward, ExchangeCornerReward::Exit);
            assert_eq!(list.exit().description, EXIT_DESCRIPTION);
            assert_eq!(list.exit(), list.entries().last().unwrap());
            let exits = list
                .entries()
                .iter()
                .filter(|e| e.reward == ExchangeCornerReward::Exit)
                .count();
            assert_eq!(exits, 1);
        }
        assert_eq!(ITEM_LIST_END, 0xFFFF);
        assert_eq!(ExchangeCornerReward::Exit.raw_id(), 0xFFFF);
    }

    #[test]
    fn menu_ids_match_the_scroll_multi_constants() {
        // include/constants/field_specials.h: DECOR_VENDOR_1 3 .. HOLD_ITEM_VENDOR 6.
        for (raw, menu) in (3..=6).zip(ExchangeCornerMenu::ALL) {
            assert_eq!(ExchangeCornerMenu::from_id(raw), Ok(menu));
            assert_eq!(menu.id(), raw);
        }
        for raw in [0, 2, 7, 0xFFFF] {
            assert_eq!(
                ExchangeCornerMenu::from_id(raw),
                Err(ExchangeCornerError::UnknownMenu(raw))
            );
        }
    }

    #[test]
    fn raw_lookup_selects_by_menu_and_selection() {
        let table = ExchangeCornerTable::new();
        let entry = table.entry(4, 4).unwrap();
        assert_eq!(entry.reward.raw_id(), 116);
        assert_eq!(
            table.entry(4, 5).unwrap().reward,
            ExchangeCornerReward::Exit
        );
        assert_eq!(table.entry(5, 0).unwrap().reward.raw_id(), 64);
        assert_eq!(table.entry(6, 9).unwrap().description, "EXIT");
        assert_eq!(table.entry(3, 10).unwrap().reward.raw_id(), ITEM_LIST_END);
    }

    #[test]
    fn invalid_selections_are_rejected() {
        let table = ExchangeCornerTable::new();
        for (menu, length) in [(3u16, 11u16), (4, 6), (5, 7), (6, 10)] {
            for selection in [length, length + 1, u16::MAX] {
                assert_eq!(
                    table.entry(menu, selection),
                    Err(ExchangeCornerError::SelectionOutOfRange {
                        menu: ExchangeCornerMenu::from_id(menu).unwrap(),
                        selection,
                    })
                );
            }
        }
        assert_eq!(table.entry(7, 0), Err(ExchangeCornerError::UnknownMenu(7)));
        assert_eq!(table.entry(2, 0), Err(ExchangeCornerError::UnknownMenu(2)));
    }

    #[test]
    fn errors_describe_the_rejected_value() {
        assert_eq!(
            ExchangeCornerError::UnknownMenu(9).to_string(),
            "unknown exchange corner menu `9`"
        );
        assert_eq!(
            ExchangeCornerError::SelectionOutOfRange {
                menu: ExchangeCornerMenu::Vitamins,
                selection: 7,
            }
            .to_string(),
            "selection `7` is past the end of Vitamins"
        );
    }
}
