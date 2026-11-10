//! Level-tiered Pickup item distributions (`sPickupItems`, `sRarePickupItems`,
//! `sPickupProbabilities`).
//!
//! Upstream `Cmd_pickup` (`pokeemerald/src/battle_script_commands.c:9654`) draws
//! two numbers for a Pokémon that already qualified for the ability: a
//! percentile `Random() % 100` and a level tier
//! `(level - 1) / 10` clamped to `9` (`:9707-9709`). A tier and a percentile
//! then select one entry from the two item lists (`:9711-9723`):
//!
//! - Walking `sPickupProbabilities` in order, the first threshold `j` that is
//!   *strictly greater* than the percentile selects the normal item
//!   `sPickupItems[tier + j]`.
//! - A percentile no threshold exceeds is `98` or `99` (the last threshold is
//!   `98`) and selects the rare item `sRarePickupItems[tier + (99 - percentile)]`,
//!   so `99` reads one slot *before* `98`.
//!
//! This module is data only: the eligibility checks, RNG draws, party
//! mutation, the Battle Pike skip and the Battle Pyramid distribution of
//! `Cmd_pickup` are not modelled here.

use std::error::Error;
use std::fmt;

use crate::experience::MAX_LEVEL;
use crate::items::ItemId;

/// The number of level tiers: `(level - 1) / 10` for levels `1..=100`
/// (`pokeemerald/src/battle_script_commands.c:9707-9709`).
pub const LEVEL_TIER_COUNT: usize = 10;

/// The number of Pickup percentiles, `Random() % 100`
/// (`pokeemerald/src/battle_script_commands.c:9706`).
pub const PERCENTILE_COUNT: usize = 100;

/// First percentile that selects a rare item (`98`); equal to the last
/// threshold in `sPickupProbabilities`, which no percentile exceeds.
const FIRST_RARE_PERCENTILE: u8 = 98;

/// Highest percentile (`99`).
const LAST_PERCENTILE: u8 = 99;

/// `sPickupItems` (`pokeemerald/src/battle_script_commands.c:784-803`).
const COMMON_ITEMS: [ItemId; 18] = [
    ItemId::POTION,
    ItemId::ANTIDOTE,
    ItemId::SUPER_POTION,
    ItemId::GREAT_BALL,
    ItemId::REPEL,
    ItemId::ESCAPE_ROPE,
    ItemId::X_ATTACK,
    ItemId::FULL_HEAL,
    ItemId::ULTRA_BALL,
    ItemId::HYPER_POTION,
    ItemId::RARE_CANDY,
    ItemId::PROTEIN,
    ItemId::REVIVE,
    ItemId::HP_UP,
    ItemId::FULL_RESTORE,
    ItemId::MAX_REVIVE,
    ItemId::PP_UP,
    ItemId::MAX_ELIXIR,
];

/// `sRarePickupItems` (`pokeemerald/src/battle_script_commands.c:806-818`).
const RARE_ITEMS: [ItemId; 11] = [
    ItemId::HYPER_POTION,
    ItemId::NUGGET,
    ItemId::KINGS_ROCK,
    ItemId::FULL_RESTORE,
    ItemId::ETHER,
    ItemId::WHITE_HERB,
    ItemId::TM_REST,
    ItemId::ELIXIR,
    ItemId::TM_FOCUS_PUNCH,
    ItemId::LEFTOVERS,
    ItemId::TM_EARTHQUAKE,
];

/// `sPickupProbabilities` (`pokeemerald/src/battle_script_commands.c:821-824`).
const THRESHOLDS: [u8; 9] = [30, 40, 50, 60, 70, 80, 90, 94, 98];

// The lookup indexes `COMMON_ITEMS[tier + j]` and `RARE_ITEMS[tier + (99 - p)]`
// without further checks; these hold the arithmetic that keeps that in range.
const _: () = {
    assert!(COMMON_ITEMS.len() == LEVEL_TIER_COUNT - 1 + THRESHOLDS.len());
    assert!(RARE_ITEMS.len() == LEVEL_TIER_COUNT - 1 + 2);
    assert!(THRESHOLDS[THRESHOLDS.len() - 1] == FIRST_RARE_PERCENTILE);
    assert!(LAST_PERCENTILE as usize == PERCENTILE_COUNT - 1);
};

/// A Pickup lookup rejected before reading any table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickupError {
    /// The level is `0` or exceeds [`MAX_LEVEL`].
    InvalidLevel(u8),

    /// The level tier is not below [`LEVEL_TIER_COUNT`].
    InvalidLevelTier(u8),

    /// The percentile is not below [`PERCENTILE_COUNT`].
    InvalidPercentile(u8),
}

impl fmt::Display for PickupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLevel(level) => {
                write!(f, "pickup level `{level}` is outside 1..={MAX_LEVEL}")
            }
            Self::InvalidLevelTier(tier) => {
                write!(
                    f,
                    "pickup level tier `{tier}` is outside 0..{LEVEL_TIER_COUNT}"
                )
            }
            Self::InvalidPercentile(percentile) => {
                write!(
                    f,
                    "pickup percentile `{percentile}` is outside 0..{PERCENTILE_COUNT}"
                )
            }
        }
    }
}

impl Error for PickupError {}

/// The item a successful Pickup roll grants, and which list it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PickupOutcome {
    /// An entry of `sPickupItems`.
    Common(ItemId),

    /// An entry of `sRarePickupItems`.
    Rare(ItemId),
}

impl PickupOutcome {
    /// Returns the granted item.
    #[must_use]
    pub const fn item(self) -> ItemId {
        match self {
            Self::Common(item) | Self::Rare(item) => item,
        }
    }

    /// Returns whether the item came from `sRarePickupItems`.
    #[must_use]
    pub const fn is_rare(self) -> bool {
        matches!(self, Self::Rare(_))
    }
}

/// Typed access to the Pickup item lists and percentile thresholds.
#[derive(Debug, Clone, Copy)]
pub struct PickupTables {
    common: &'static [ItemId; 18],
    rare: &'static [ItemId; 11],
    thresholds: &'static [u8; 9],
}

impl PickupTables {
    /// Creates the canonical Pickup tables.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            common: &COMMON_ITEMS,
            rare: &RARE_ITEMS,
            thresholds: &THRESHOLDS,
        }
    }

    /// Returns `sPickupItems`.
    #[must_use]
    pub const fn common_items(&self) -> &'static [ItemId] {
        self.common
    }

    /// Returns `sRarePickupItems`.
    #[must_use]
    pub const fn rare_items(&self) -> &'static [ItemId] {
        self.rare
    }

    /// Returns `sPickupProbabilities`, the exclusive upper percentile bound of
    /// each normal-item step.
    #[must_use]
    pub const fn thresholds(&self) -> &'static [u8] {
        self.thresholds
    }

    /// Returns the level tier for `level`: `(level - 1) / 10`
    /// (`pokeemerald/src/battle_script_commands.c:9707-9709`).
    ///
    /// # Errors
    ///
    /// Returns [`PickupError::InvalidLevel`] if `level` is `0` or exceeds
    /// [`MAX_LEVEL`]. Upstream's `> 9` clamp is unreachable for valid levels.
    pub const fn level_tier(level: u8) -> Result<u8, PickupError> {
        if level == 0 || level > MAX_LEVEL {
            Err(PickupError::InvalidLevel(level))
        } else {
            Ok((level - 1) / 10)
        }
    }

    /// Returns the outcome for a Pokémon of `level` and a percentile roll.
    ///
    /// # Errors
    ///
    /// Returns [`PickupError::InvalidLevel`] or
    /// [`PickupError::InvalidPercentile`] for an input outside its range.
    pub fn lookup_level(&self, level: u8, percentile: u8) -> Result<PickupOutcome, PickupError> {
        self.lookup(Self::level_tier(level)?, percentile)
    }

    /// Returns the outcome for a level tier (`0..10`) and percentile (`0..100`).
    ///
    /// # Errors
    ///
    /// Returns [`PickupError::InvalidLevelTier`] if `tier` is not below
    /// [`LEVEL_TIER_COUNT`], and [`PickupError::InvalidPercentile`] if
    /// `percentile` is not below [`PERCENTILE_COUNT`].
    pub fn lookup(&self, tier: u8, percentile: u8) -> Result<PickupOutcome, PickupError> {
        let tier_index = usize::from(tier);
        if tier_index >= LEVEL_TIER_COUNT {
            return Err(PickupError::InvalidLevelTier(tier));
        }
        if usize::from(percentile) >= PERCENTILE_COUNT {
            return Err(PickupError::InvalidPercentile(percentile));
        }
        Ok(
            match self
                .thresholds
                .iter()
                .position(|&threshold| threshold > percentile)
            {
                Some(step) => PickupOutcome::Common(self.common[tier_index + step]),
                None => PickupOutcome::Rare(
                    self.rare[tier_index + usize::from(LAST_PERCENTILE - percentile)],
                ),
            },
        )
    }
}

impl Default for PickupTables {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemTable;

    /// FNV-1a 64-bit hash, matching the change locator in `xtask::record_snapshot`.
    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    fn items_le(items: &[ItemId]) -> Vec<u8> {
        items
            .iter()
            .flat_map(|item| item.index().to_le_bytes())
            .collect()
    }

    /// The `ITEM_*` numbers hand-copied from `include/constants/items.h`
    /// (`ITEM_TM_*` are `ITEM_TM01 + index` in `include/constants/tms_hms.h`
    /// `FOREACH_TM` order, `include/item.h:80-97`), in `sPickupItems` order.
    const COMMON_IDS: [u16; 18] = [
        13, 14, 22, 3, 86, 85, 75, 23, 2, 21, 68, 64, 24, 63, 19, 25, 69, 37,
    ];

    /// The same numbers for `sRarePickupItems`.
    const RARE_IDS: [u16; 11] = [21, 110, 187, 19, 34, 180, 332, 36, 289, 200, 314];

    #[test]
    fn the_three_arrays_match_upstream() {
        let tables = PickupTables::new();
        let common: Vec<u16> = tables.common_items().iter().map(|i| i.index()).collect();
        let rare: Vec<u16> = tables.rare_items().iter().map(|i| i.index()).collect();
        assert_eq!(common, COMMON_IDS);
        assert_eq!(rare, RARE_IDS);
        assert_eq!(tables.thresholds(), [30, 40, 50, 60, 70, 80, 90, 94, 98]);
    }

    /// Digests computed independently by parsing the three arrays out of
    /// `src/battle_script_commands.c:784-824` and resolving the `ITEM_*`
    /// names against `include/constants/items.h`; items as little-endian
    /// `u16`, thresholds as bytes.
    #[test]
    fn the_arrays_match_the_canonical_digests() {
        let tables = PickupTables::new();
        assert_eq!(
            format!("{:016x}", fnv1a64(&items_le(tables.common_items()))),
            "8fe221ba01bdb3aa",
        );
        assert_eq!(
            format!("{:016x}", fnv1a64(&items_le(tables.rare_items()))),
            "04617dda6a4d817c",
        );
        assert_eq!(
            format!("{:016x}", fnv1a64(tables.thresholds())),
            "d61b700ea273919f",
        );
    }

    #[test]
    fn every_listed_item_exists_in_the_item_table() {
        let items = ItemTable::new();
        let tables = PickupTables::new();
        for &item in tables.common_items().iter().chain(tables.rare_items()) {
            assert_ne!(item, ItemId::NONE);
            assert!(items.get(item).is_ok(), "{item:?} missing from item table");
        }
    }

    #[test]
    fn dimensions_match_the_selection_arithmetic() {
        let tables = PickupTables::new();
        assert_eq!(LEVEL_TIER_COUNT, 10);
        assert_eq!(PERCENTILE_COUNT, 100);
        assert_eq!(tables.common_items().len(), 18);
        assert_eq!(tables.rare_items().len(), 11);
        assert_eq!(tables.thresholds().len(), 9);
        assert!(tables.thresholds().windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// A direct transcription of the selection loop at
    /// `pokeemerald/src/battle_script_commands.c:9711-9723`, kept separate
    /// from the production lookup.
    fn reference(tier: usize, rand: usize) -> PickupOutcome {
        for (j, &threshold) in THRESHOLDS.iter().enumerate() {
            if usize::from(threshold) > rand {
                return PickupOutcome::Common(COMMON_ITEMS[tier + j]);
            } else if rand == 99 || rand == 98 {
                return PickupOutcome::Rare(RARE_ITEMS[tier + (99 - rand)]);
            }
        }
        panic!("upstream loop selected nothing for tier {tier}, percentile {rand}");
    }

    #[test]
    fn all_one_thousand_outcomes_match_the_upstream_loop() {
        let tables = PickupTables::new();
        for tier in 0..10_u8 {
            for percentile in 0..100_u8 {
                assert_eq!(
                    tables.lookup(tier, percentile),
                    Ok(reference(usize::from(tier), usize::from(percentile))),
                    "tier {tier}, percentile {percentile}",
                );
            }
        }
    }

    /// Digest of all 1000 outcomes (tier-major, percentile-minor), each
    /// rendered as a rarity byte (`0` normal, `1` rare) and the little-endian
    /// `u16` item id, computed independently by executing the `:9711-9723`
    /// loop over the parsed upstream arrays.
    #[test]
    fn the_outcome_grid_matches_the_canonical_digest() {
        let tables = PickupTables::new();
        let mut bytes = Vec::with_capacity(LEVEL_TIER_COUNT * PERCENTILE_COUNT * 3);
        for tier in 0..10_u8 {
            for percentile in 0..100_u8 {
                let outcome = tables.lookup(tier, percentile).unwrap();
                bytes.push(u8::from(outcome.is_rare()));
                bytes.extend_from_slice(&outcome.item().index().to_le_bytes());
            }
        }
        assert_eq!(bytes.len(), 3000);
        assert_eq!(format!("{:016x}", fnv1a64(&bytes)), "5a8eb1217f503785");
    }

    #[test]
    fn spot_checked_rows_are_pinned_by_hand() {
        let tables = PickupTables::new();
        let common = |item| Ok(PickupOutcome::Common(item));
        let rare = |item| Ok(PickupOutcome::Rare(item));
        assert_eq!(tables.lookup(0, 0), common(ItemId::POTION));
        assert_eq!(tables.lookup(0, 30), common(ItemId::ANTIDOTE));
        assert_eq!(tables.lookup(4, 55), common(ItemId::FULL_HEAL));
        assert_eq!(tables.lookup(9, 0), common(ItemId::HYPER_POTION));
        assert_eq!(tables.lookup(9, 97), common(ItemId::MAX_ELIXIR));
        assert_eq!(tables.lookup(0, 98), rare(ItemId::NUGGET));
        assert_eq!(tables.lookup(0, 99), rare(ItemId::HYPER_POTION));
        assert_eq!(tables.lookup(9, 98), rare(ItemId::TM_EARTHQUAKE));
        assert_eq!(tables.lookup(9, 99), rare(ItemId::LEFTOVERS));
    }

    #[test]
    fn thresholds_are_strict_at_every_boundary() {
        let tables = PickupTables::new();
        for tier in 0..10_u8 {
            for (step, &threshold) in tables.thresholds().iter().enumerate() {
                let below = tables.lookup(tier, threshold - 1).unwrap();
                assert_eq!(
                    below,
                    PickupOutcome::Common(tables.common_items()[usize::from(tier) + step]),
                    "tier {tier}: percentile {} stays in step {step}",
                    threshold - 1,
                );
                if threshold < FIRST_RARE_PERCENTILE {
                    assert_eq!(
                        tables.lookup(tier, threshold).unwrap(),
                        PickupOutcome::Common(tables.common_items()[usize::from(tier) + step + 1]),
                        "tier {tier}: percentile {threshold} moves to step {}",
                        step + 1,
                    );
                } else {
                    assert!(tables.lookup(tier, threshold).unwrap().is_rare());
                }
            }
        }
    }

    #[test]
    fn rare_items_read_nine_nine_before_nine_eight() {
        let tables = PickupTables::new();
        for tier in 0..10_u8 {
            let at_99 = tables.lookup(tier, 99).unwrap();
            let at_98 = tables.lookup(tier, 98).unwrap();
            assert_eq!(
                at_99,
                PickupOutcome::Rare(tables.rare_items()[usize::from(tier)])
            );
            assert_eq!(
                at_98,
                PickupOutcome::Rare(tables.rare_items()[usize::from(tier) + 1])
            );
            assert_ne!(at_99, at_98);
        }
        // Only these two percentiles are rare.
        let rare_count = (0..100_u8)
            .filter(|&p| tables.lookup(3, p).unwrap().is_rare())
            .count();
        assert_eq!(rare_count, 2);
    }

    #[test]
    fn level_tier_is_level_minus_one_over_ten() {
        for (level, tier) in [
            (1, 0),
            (10, 0),
            (11, 1),
            (50, 4),
            (51, 5),
            (91, 9),
            (100, 9),
        ] {
            assert_eq!(PickupTables::level_tier(level), Ok(tier), "level {level}");
        }
        let tables = PickupTables::new();
        assert_eq!(
            tables.lookup_level(55, 98),
            tables.lookup(5, 98),
            "lookup_level resolves the tier first",
        );
    }

    #[test]
    fn invalid_inputs_are_rejected_explicitly() {
        let tables = PickupTables::new();
        assert_eq!(tables.lookup(10, 0), Err(PickupError::InvalidLevelTier(10)));
        assert_eq!(
            tables.lookup(255, 0),
            Err(PickupError::InvalidLevelTier(255))
        );
        assert_eq!(
            tables.lookup(0, 100),
            Err(PickupError::InvalidPercentile(100))
        );
        assert_eq!(
            tables.lookup(0, 255),
            Err(PickupError::InvalidPercentile(255))
        );
        assert_eq!(
            PickupTables::level_tier(0),
            Err(PickupError::InvalidLevel(0))
        );
        assert_eq!(
            PickupTables::level_tier(101),
            Err(PickupError::InvalidLevel(101))
        );
        assert_eq!(tables.lookup_level(0, 0), Err(PickupError::InvalidLevel(0)));
        assert_eq!(
            tables.lookup_level(101, 0),
            Err(PickupError::InvalidLevel(101))
        );
        assert_eq!(
            tables.lookup_level(50, 100),
            Err(PickupError::InvalidPercentile(100))
        );
        // Both bad: the level tier is validated first.
        assert_eq!(
            tables.lookup(10, 100),
            Err(PickupError::InvalidLevelTier(10))
        );
    }

    #[test]
    fn errors_describe_the_rejected_input() {
        assert_eq!(
            PickupError::InvalidLevel(0).to_string(),
            "pickup level `0` is outside 1..=100"
        );
        assert_eq!(
            PickupError::InvalidLevelTier(10).to_string(),
            "pickup level tier `10` is outside 0..10"
        );
        assert_eq!(
            PickupError::InvalidPercentile(100).to_string(),
            "pickup percentile `100` is outside 0..100"
        );
    }
}
