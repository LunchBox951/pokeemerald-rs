//! The Battle Pyramid streak-prize pools: the two item lists the end-of-run
//! prize is drawn from.
//!
//! Source: `src/battle_pyramid.c:809-810` (`sShortStreakRewardItems` and
//! `sLongStreakRewardItems`); their reader is `SetBattlePyramidPrize`
//! (`src/battle_pyramid.c:940-946`), which indexes a pool by a random value
//! reduced to the pool's length.
//!
//! The two pools have different lengths: the short-streak pool holds six
//! stat-raising items and the long-streak pool nine held items. Entries are
//! indexed from `0`, in upstream order. Every lookup refuses an index that is
//! out of range instead of clamping it. Classifying a win streak, the random
//! pick, the saved prize and putting the prize in the bag are not ported here.

use std::error::Error;
use std::fmt;

use crate::items::ItemId;

/// The number of items in the short-streak pool (`sShortStreakRewardItems`).
pub const SHORT_STREAK_PRIZE_COUNT: usize = 6;

/// The number of items in the long-streak pool (`sLongStreakRewardItems`).
pub const LONG_STREAK_PRIZE_COUNT: usize = 9;

/// One of the two Battle Pyramid streak-prize pools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyramidPrizePool {
    /// `sShortStreakRewardItems`.
    Short,
    /// `sLongStreakRewardItems`.
    Long,
}

impl PyramidPrizePool {
    /// The number of items in this pool.
    #[must_use]
    pub const fn len(self) -> usize {
        match self {
            Self::Short => SHORT_STREAK_PRIZE_COUNT,
            Self::Long => LONG_STREAK_PRIZE_COUNT,
        }
    }

    /// Whether this pool holds no items. It never does.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }
}

/// A failed lookup in a Battle Pyramid streak-prize pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyramidPrizeError {
    /// The index is not below the length of the pool.
    InvalidIndex {
        /// The pool that was read.
        pool: PyramidPrizePool,
        /// The index that was asked for.
        index: usize,
    },
}

impl fmt::Display for PyramidPrizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIndex { pool, index } => write!(
                f,
                "Battle Pyramid {} prize `{index}` is out of range (there are {})",
                match pool {
                    PyramidPrizePool::Short => "short-streak",
                    PyramidPrizePool::Long => "long-streak",
                },
                pool.len()
            ),
        }
    }
}

impl Error for PyramidPrizeError {}

/// `sShortStreakRewardItems` (`src/battle_pyramid.c:809`).
const SHORT_STREAK_PRIZES: [ItemId; SHORT_STREAK_PRIZE_COUNT] = [
    ItemId::HP_UP,
    ItemId::PROTEIN,
    ItemId::IRON,
    ItemId::CALCIUM,
    ItemId::CARBOS,
    ItemId::ZINC,
];

/// `sLongStreakRewardItems` (`src/battle_pyramid.c:810`).
const LONG_STREAK_PRIZES: [ItemId; LONG_STREAK_PRIZE_COUNT] = [
    ItemId::BRIGHT_POWDER,
    ItemId::WHITE_HERB,
    ItemId::QUICK_CLAW,
    ItemId::LEFTOVERS,
    ItemId::MENTAL_HERB,
    ItemId::KINGS_ROCK,
    ItemId::FOCUS_BAND,
    ItemId::SCOPE_LENS,
    ItemId::CHOICE_BAND,
];

/// Returns every item of one prize pool, in upstream order.
#[must_use]
pub const fn prize_pool(pool: PyramidPrizePool) -> &'static [ItemId] {
    match pool {
        PyramidPrizePool::Short => &SHORT_STREAK_PRIZES,
        PyramidPrizePool::Long => &LONG_STREAK_PRIZES,
    }
}

/// Returns one item of one prize pool.
///
/// # Errors
///
/// Returns [`PyramidPrizeError::InvalidIndex`] when `index` is not below the
/// length of `pool`.
pub fn streak_prize(pool: PyramidPrizePool, index: usize) -> Result<ItemId, PyramidPrizeError> {
    prize_pool(pool)
        .get(index)
        .copied()
        .ok_or(PyramidPrizeError::InvalidIndex { pool, index })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pools as raw `ITEM_*` values from `include/constants/items.h`, read
    /// independently of the tables: HP Up 63, Protein 64, Iron 65, Calcium 67,
    /// Carbos 66, Zinc 70; then Bright Powder 179, White Herb 180, Quick Claw
    /// 183, Leftovers 200, Mental Herb 185, King's Rock 187, Focus Band 196,
    /// Scope Lens 198, Choice Band 186.
    const EXPECTED_SHORT: [u16; 6] = [63, 64, 65, 67, 66, 70];
    const EXPECTED_LONG: [u16; 9] = [179, 180, 183, 200, 185, 187, 196, 198, 186];

    #[test]
    fn short_pool_is_pinned_entry_by_entry() {
        for (index, &raw) in EXPECTED_SHORT.iter().enumerate() {
            assert_eq!(
                streak_prize(PyramidPrizePool::Short, index),
                Ok(ItemId(raw)),
                "short entry {index}"
            );
        }
        assert_eq!(
            prize_pool(PyramidPrizePool::Short)
                .iter()
                .map(|item| item.index())
                .collect::<Vec<_>>(),
            EXPECTED_SHORT
        );
    }

    #[test]
    fn long_pool_is_pinned_entry_by_entry() {
        for (index, &raw) in EXPECTED_LONG.iter().enumerate() {
            assert_eq!(
                streak_prize(PyramidPrizePool::Long, index),
                Ok(ItemId(raw)),
                "long entry {index}"
            );
        }
        assert_eq!(
            prize_pool(PyramidPrizePool::Long)
                .iter()
                .map(|item| item.index())
                .collect::<Vec<_>>(),
            EXPECTED_LONG
        );
    }

    #[test]
    fn pools_are_named_in_upstream_order() {
        assert_eq!(
            prize_pool(PyramidPrizePool::Short),
            [
                ItemId::HP_UP,
                ItemId::PROTEIN,
                ItemId::IRON,
                ItemId::CALCIUM,
                ItemId::CARBOS,
                ItemId::ZINC,
            ]
        );
        assert_eq!(
            prize_pool(PyramidPrizePool::Long),
            [
                ItemId::BRIGHT_POWDER,
                ItemId::WHITE_HERB,
                ItemId::QUICK_CLAW,
                ItemId::LEFTOVERS,
                ItemId::MENTAL_HERB,
                ItemId::KINGS_ROCK,
                ItemId::FOCUS_BAND,
                ItemId::SCOPE_LENS,
                ItemId::CHOICE_BAND,
            ]
        );
    }

    #[test]
    fn each_pool_reports_its_own_length() {
        assert_eq!(PyramidPrizePool::Short.len(), 6);
        assert_eq!(PyramidPrizePool::Long.len(), 9);
        assert_eq!(prize_pool(PyramidPrizePool::Short).len(), 6);
        assert_eq!(prize_pool(PyramidPrizePool::Long).len(), 9);
        assert!(!PyramidPrizePool::Short.is_empty());
        assert!(!PyramidPrizePool::Long.is_empty());
    }

    #[test]
    fn the_first_invalid_index_and_the_largest_input_are_refused() {
        for (pool, len) in [(PyramidPrizePool::Short, 6), (PyramidPrizePool::Long, 9)] {
            assert_eq!(
                streak_prize(pool, len),
                Err(PyramidPrizeError::InvalidIndex { pool, index: len })
            );
            assert_eq!(
                streak_prize(pool, usize::MAX),
                Err(PyramidPrizeError::InvalidIndex {
                    pool,
                    index: usize::MAX
                })
            );
        }
        // An index valid for the long pool is refused by the short pool.
        assert!(streak_prize(PyramidPrizePool::Long, 6).is_ok());
        assert_eq!(
            streak_prize(PyramidPrizePool::Short, 6),
            Err(PyramidPrizeError::InvalidIndex {
                pool: PyramidPrizePool::Short,
                index: 6
            })
        );
    }

    #[test]
    fn errors_name_the_pool_index_and_length() {
        assert_eq!(
            PyramidPrizeError::InvalidIndex {
                pool: PyramidPrizePool::Short,
                index: 6
            }
            .to_string(),
            "Battle Pyramid short-streak prize `6` is out of range (there are 6)"
        );
        assert_eq!(
            PyramidPrizeError::InvalidIndex {
                pool: PyramidPrizePool::Long,
                index: 9
            }
            .to_string(),
            "Battle Pyramid long-streak prize `9` is out of range (there are 9)"
        );
    }

    /// Reads a file of the pinned upstream checkout.
    fn upstream_file(relative: &str) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("crate sits two levels below the workspace root")
            .join("pokeemerald");
        std::fs::read_to_string(root.join(relative)).unwrap_or_else(|error| {
            panic!("cannot read pokeemerald/{relative}: {error}; run ./init.sh")
        })
    }

    /// The item names of the braced list that follows `declaration`.
    fn item_names<'a>(source: &'a str, declaration: &str) -> Vec<&'a str> {
        let start = source
            .find(declaration)
            .unwrap_or_else(|| panic!("`{declaration}` not found"));
        let rest = &source[start..];
        let open = rest.find('{').expect("list opens") + 1;
        let close = rest.find('}').expect("list closes");
        rest[open..close]
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect()
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_tables() {
        let items: std::collections::HashMap<String, u16> =
            upstream_file("include/constants/items.h")
                .lines()
                .filter_map(|line| {
                    let mut words = line.split_whitespace();
                    (words.next()? == "#define").then_some(())?;
                    let name = words.next()?.to_owned();
                    let value = words.next()?.parse().ok()?;
                    Some((name, value))
                })
                .collect();
        let source = upstream_file("src/battle_pyramid.c");

        for (declaration, pool) in [
            ("sShortStreakRewardItems[] =", PyramidPrizePool::Short),
            ("sLongStreakRewardItems[] =", PyramidPrizePool::Long),
        ] {
            let upstream: Vec<u16> = item_names(&source, declaration)
                .into_iter()
                .map(|name| items[name])
                .collect();
            let ours: Vec<u16> = prize_pool(pool).iter().map(|item| item.index()).collect();
            assert_eq!(ours, upstream, "{declaration}");
            assert_eq!(upstream.len(), pool.len(), "{declaration}");
        }
    }
}
