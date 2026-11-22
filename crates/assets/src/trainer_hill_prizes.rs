//! The Trainer Hill prize-list catalog: the two sets of ten prize lists the
//! Trainer Hill reward draws from.
//!
//! Source: `src/trainer_hill.c:144-198` (the `sPrizeList*` arrays,
//! `sPrizeLists1`, `sPrizeLists2` and `sPrizeListSets`; the lists per set is
//! `NUM_TRAINER_HILL_PRIZE_LISTS` in `include/constants/trainer_hill.h`).
//!
//! Sets, lists and entries are indexed from `0`, in upstream order. A list's
//! first entry is its headline prize; the other five are the same in every
//! list. Every lookup refuses an index that is out of range instead of
//! clamping it. Choosing a list from the clear time, the challenge modes and
//! granting the reward are not ported here.

use std::error::Error;
use std::fmt;

use crate::items::ItemId;

/// The number of prize-list sets (`sPrizeListSets`).
pub const PRIZE_SET_COUNT: usize = 2;

/// The number of prize lists in each set (`NUM_TRAINER_HILL_PRIZE_LISTS`).
pub const PRIZE_LISTS_PER_SET: usize = 10;

/// The number of items in each prize list.
pub const PRIZE_LIST_LEN: usize = 6;

/// A failed lookup in the Trainer Hill prize catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrizeListError {
    /// The set is not below [`PRIZE_SET_COUNT`].
    InvalidSet(usize),
    /// The list is not below [`PRIZE_LISTS_PER_SET`].
    InvalidList(usize),
    /// The entry is not below [`PRIZE_LIST_LEN`].
    InvalidEntry(usize),
}

impl fmt::Display for PrizeListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSet(set) => write!(
                f,
                "Trainer Hill prize set `{set}` is out of range (there are {PRIZE_SET_COUNT})"
            ),
            Self::InvalidList(list) => write!(
                f,
                "Trainer Hill prize list `{list}` is out of range (there are \
                 {PRIZE_LISTS_PER_SET} per set)"
            ),
            Self::InvalidEntry(entry) => write!(
                f,
                "Trainer Hill prize entry `{entry}` is out of range (there are {PRIZE_LIST_LEN})"
            ),
        }
    }
}

impl Error for PrizeListError {}

/// `sPrizeListSets`: `sPrizeLists1` then `sPrizeLists2`
/// (`src/trainer_hill.c:144-198`).
#[rustfmt::skip]
const PRIZE_LISTS: [[[ItemId; PRIZE_LIST_LEN]; PRIZE_LISTS_PER_SET]; PRIZE_SET_COUNT] = [
    [
        [ItemId::RARE_CANDY, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::LUXURY_BALL, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::MAX_REVIVE, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::MAX_ETHER, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::ELIXIR, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::TM_ROAR, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::TM_SLUDGE_BOMB, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::TM_TOXIC, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::TM_SUNNY_DAY, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::TM_EARTHQUAKE, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
    ],
    [
        [ItemId::RARE_CANDY, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::LUXURY_BALL, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::MAX_REVIVE, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::MAX_ETHER, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::ELIXIR, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::TM_BRICK_BREAK, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::TM_TORMENT, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::TM_SKILL_SWAP, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::TM_GIGA_DRAIN, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
        [ItemId::TM_ATTRACT, ItemId::ETHER, ItemId::MAX_POTION, ItemId::REVIVE, ItemId::FLUFFY_TAIL, ItemId::GREAT_BALL],
    ],
];

/// Returns the six items of one prize list, in upstream order.
///
/// # Errors
///
/// Returns [`PrizeListError::InvalidSet`] when `set` is not below
/// [`PRIZE_SET_COUNT`], and [`PrizeListError::InvalidList`] when `list` is not
/// below [`PRIZE_LISTS_PER_SET`].
pub fn prize_list(
    set: usize,
    list: usize,
) -> Result<&'static [ItemId; PRIZE_LIST_LEN], PrizeListError> {
    PRIZE_LISTS
        .get(set)
        .ok_or(PrizeListError::InvalidSet(set))?
        .get(list)
        .ok_or(PrizeListError::InvalidList(list))
}

/// Returns one item of one prize list.
///
/// # Errors
///
/// Returns the errors of [`prize_list`], and [`PrizeListError::InvalidEntry`]
/// when `entry` is not below [`PRIZE_LIST_LEN`].
pub fn prize_item(set: usize, list: usize, entry: usize) -> Result<ItemId, PrizeListError> {
    prize_list(set, list)?
        .get(entry)
        .copied()
        .ok_or(PrizeListError::InvalidEntry(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The prize lists as raw `ITEM_*` values from
    /// `include/constants/items.h`, read independently of `PRIZE_LISTS`:
    /// Rare Candy 68, Luxury Ball 11, Max Revive 25, Max Ether 35, Elixir 36,
    /// then the common tail Ether 34, Max Potion 20, Revive 24, Fluffy Tail
    /// 81, Great Ball 3. The headline TMs are `ITEM_TM01` (289) plus the TM
    /// number minus one.
    #[rustfmt::skip]
    const EXPECTED: [[[u16; PRIZE_LIST_LEN]; PRIZE_LISTS_PER_SET]; PRIZE_SET_COUNT] = [
        [
            [68, 34, 20, 24, 81, 3],
            [11, 34, 20, 24, 81, 3],
            [25, 34, 20, 24, 81, 3],
            [35, 34, 20, 24, 81, 3],
            [36, 34, 20, 24, 81, 3],
            [293, 34, 20, 24, 81, 3], // TM05 Roar
            [324, 34, 20, 24, 81, 3], // TM36 Sludge Bomb
            [294, 34, 20, 24, 81, 3], // TM06 Toxic
            [299, 34, 20, 24, 81, 3], // TM11 Sunny Day
            [314, 34, 20, 24, 81, 3], // TM26 Earthquake
        ],
        [
            [68, 34, 20, 24, 81, 3],
            [11, 34, 20, 24, 81, 3],
            [25, 34, 20, 24, 81, 3],
            [35, 34, 20, 24, 81, 3],
            [36, 34, 20, 24, 81, 3],
            [319, 34, 20, 24, 81, 3], // TM31 Brick Break
            [329, 34, 20, 24, 81, 3], // TM41 Torment
            [336, 34, 20, 24, 81, 3], // TM48 Skill Swap
            [307, 34, 20, 24, 81, 3], // TM19 Giga Drain
            [333, 34, 20, 24, 81, 3], // TM45 Attract
        ],
    ];

    #[test]
    fn every_list_is_pinned_entry_by_entry() {
        for (set, lists) in EXPECTED.iter().enumerate() {
            for (list, entries) in lists.iter().enumerate() {
                for (entry, &raw) in entries.iter().enumerate() {
                    assert_eq!(
                        prize_item(set, list, entry),
                        Ok(ItemId(raw)),
                        "set {set} list {list} entry {entry}"
                    );
                }
                let items = prize_list(set, list).unwrap();
                assert_eq!(items.map(ItemId::index), *entries, "set {set} list {list}");
            }
        }
    }

    #[test]
    fn the_catalog_holds_120_items() {
        let total: usize = (0..PRIZE_SET_COUNT)
            .flat_map(|set| (0..PRIZE_LISTS_PER_SET).map(move |list| (set, list)))
            .map(|(set, list)| prize_list(set, list).unwrap().len())
            .sum();
        assert_eq!(total, 120);
        assert_eq!(PRIZE_SET_COUNT * PRIZE_LISTS_PER_SET * PRIZE_LIST_LEN, 120);
    }

    #[test]
    fn headline_prizes_are_named_in_upstream_order() {
        let heads = |set: usize| -> Vec<ItemId> {
            (0..PRIZE_LISTS_PER_SET)
                .map(|list| prize_item(set, list, 0).unwrap())
                .collect()
        };
        assert_eq!(
            heads(0),
            [
                ItemId::RARE_CANDY,
                ItemId::LUXURY_BALL,
                ItemId::MAX_REVIVE,
                ItemId::MAX_ETHER,
                ItemId::ELIXIR,
                ItemId::TM_ROAR,
                ItemId::TM_SLUDGE_BOMB,
                ItemId::TM_TOXIC,
                ItemId::TM_SUNNY_DAY,
                ItemId::TM_EARTHQUAKE,
            ]
        );
        assert_eq!(
            heads(1),
            [
                ItemId::RARE_CANDY,
                ItemId::LUXURY_BALL,
                ItemId::MAX_REVIVE,
                ItemId::MAX_ETHER,
                ItemId::ELIXIR,
                ItemId::TM_BRICK_BREAK,
                ItemId::TM_TORMENT,
                ItemId::TM_SKILL_SWAP,
                ItemId::TM_GIGA_DRAIN,
                ItemId::TM_ATTRACT,
            ]
        );
    }

    #[test]
    fn lists_differ_only_in_their_headline_prize() {
        let tail = &prize_list(0, 0).unwrap()[1..];
        for set in 0..PRIZE_SET_COUNT {
            for list in 0..PRIZE_LISTS_PER_SET {
                assert_eq!(&prize_list(set, list).unwrap()[1..], tail);
            }
        }
        assert_eq!(
            tail,
            [
                ItemId::ETHER,
                ItemId::MAX_POTION,
                ItemId::REVIVE,
                ItemId::FLUFFY_TAIL,
                ItemId::GREAT_BALL,
            ]
        );
    }

    #[test]
    fn the_two_sets_share_their_first_five_lists_and_differ_in_the_last_five() {
        for list in 0..5 {
            assert_eq!(prize_list(0, list), prize_list(1, list), "list {list}");
        }
        for list in 5..PRIZE_LISTS_PER_SET {
            assert_ne!(prize_list(0, list), prize_list(1, list), "list {list}");
        }
    }

    #[test]
    fn out_of_range_indices_are_refused() {
        assert_eq!(prize_list(2, 0), Err(PrizeListError::InvalidSet(2)));
        assert_eq!(prize_list(0, 10), Err(PrizeListError::InvalidList(10)));
        assert_eq!(prize_item(2, 0, 0), Err(PrizeListError::InvalidSet(2)));
        assert_eq!(prize_item(0, 10, 0), Err(PrizeListError::InvalidList(10)));
        assert_eq!(prize_item(1, 9, 6), Err(PrizeListError::InvalidEntry(6)));
        assert_eq!(
            prize_item(usize::MAX, usize::MAX, usize::MAX),
            Err(PrizeListError::InvalidSet(usize::MAX))
        );
    }

    #[test]
    fn errors_name_the_offending_index() {
        assert!(PrizeListError::InvalidSet(7).to_string().contains("`7`"));
        assert!(PrizeListError::InvalidList(11).to_string().contains("`11`"));
        assert!(PrizeListError::InvalidEntry(9).to_string().contains("`9`"));
    }
}
