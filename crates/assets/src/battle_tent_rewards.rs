//! The Battle Tent reward pools: the items each tent's challenge prize is drawn
//! from.
//!
//! Source: `src/battle_tent.c` (`sVerdanturfTentRewards` at `:73`,
//! `sFallarborTentRewards` at `:86`, `sSlateportTentRewards` at `:102`; read by
//! the `SetRandom*TentPrize` functions at `:150`, `:200` and `:255`).
//!
//! Each tent keeps its own pool, indexed from `0`, even though every pool holds
//! one item today. Every lookup refuses an index that is out of range instead
//! of clamping it. The random draw, the prize stored in the save, challenge
//! state and delivering the item are not ported here.

use std::error::Error;
use std::fmt;

use crate::items::ItemId;

/// One of the three Battle Tents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleTent {
    Verdanturf,
    Fallarbor,
    Slateport,
}

impl BattleTent {
    /// Every tent, in upstream order.
    pub const ALL: [Self; 3] = [Self::Verdanturf, Self::Fallarbor, Self::Slateport];
}

/// A failed lookup in a Battle Tent reward pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TentRewardError {
    /// The index is not below the length of the tent's pool.
    InvalidIndex { tent: BattleTent, index: usize },
}

impl fmt::Display for TentRewardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIndex { tent, index } => write!(
                f,
                "{tent:?} Battle Tent reward `{index}` is out of range (there are {})",
                tent_rewards(*tent).len()
            ),
        }
    }
}

impl Error for TentRewardError {}

/// `sVerdanturfTentRewards` (`src/battle_tent.c:73`).
const VERDANTURF_REWARDS: [ItemId; 1] = [ItemId::NEST_BALL];

/// `sFallarborTentRewards` (`src/battle_tent.c:86`).
const FALLARBOR_REWARDS: [ItemId; 1] = [ItemId::HYPER_POTION];

/// `sSlateportTentRewards` (`src/battle_tent.c:102`).
const SLATEPORT_REWARDS: [ItemId; 1] = [ItemId::FULL_HEAL];

/// Returns a tent's whole reward pool, in upstream order.
#[must_use]
pub fn tent_rewards(tent: BattleTent) -> &'static [ItemId] {
    match tent {
        BattleTent::Verdanturf => &VERDANTURF_REWARDS,
        BattleTent::Fallarbor => &FALLARBOR_REWARDS,
        BattleTent::Slateport => &SLATEPORT_REWARDS,
    }
}

/// Returns one item of a tent's reward pool.
///
/// # Errors
///
/// Returns [`TentRewardError::InvalidIndex`] when `index` is not below the
/// length of the tent's pool.
pub fn tent_reward(tent: BattleTent, index: usize) -> Result<ItemId, TentRewardError> {
    tent_rewards(tent)
        .get(index)
        .copied()
        .ok_or(TentRewardError::InvalidIndex { tent, index })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each pool as raw `ITEM_*` values from `include/constants/items.h`, read
    /// independently of the tables: Nest Ball 8, Hyper Potion 21, Full Heal 23.
    const EXPECTED: [(BattleTent, &[u16]); 3] = [
        (BattleTent::Verdanturf, &[8]),
        (BattleTent::Fallarbor, &[21]),
        (BattleTent::Slateport, &[23]),
    ];

    #[test]
    fn every_pool_is_pinned_by_raw_item_id_and_length() {
        for (tent, raw) in EXPECTED {
            let pool = tent_rewards(tent);
            assert_eq!(pool.len(), raw.len(), "{tent:?}");
            for (index, &id) in raw.iter().enumerate() {
                assert_eq!(tent_reward(tent, index), Ok(ItemId(id)), "{tent:?} {index}");
            }
        }
        assert_eq!(BattleTent::ALL, EXPECTED.map(|(tent, _)| tent));
    }

    #[test]
    fn pools_are_named_per_tent() {
        assert_eq!(
            tent_reward(BattleTent::Verdanturf, 0),
            Ok(ItemId::NEST_BALL)
        );
        assert_eq!(
            tent_reward(BattleTent::Fallarbor, 0),
            Ok(ItemId::HYPER_POTION)
        );
        assert_eq!(tent_reward(BattleTent::Slateport, 0), Ok(ItemId::FULL_HEAL));
    }

    #[test]
    fn the_first_invalid_index_and_the_largest_input_are_refused() {
        for tent in BattleTent::ALL {
            let len = tent_rewards(tent).len();
            for index in [len, usize::MAX] {
                assert_eq!(
                    tent_reward(tent, index),
                    Err(TentRewardError::InvalidIndex { tent, index }),
                    "{tent:?} {index}"
                );
            }
        }
    }

    #[test]
    fn errors_name_the_tent_and_the_index() {
        let error = TentRewardError::InvalidIndex {
            tent: BattleTent::Fallarbor,
            index: 3,
        };
        assert_eq!(
            error.to_string(),
            "Fallarbor Battle Tent reward `3` is out of range (there are 1)"
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

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_pools() {
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
        let source = upstream_file("src/battle_tent.c");

        for (declaration, tent) in [
            ("sVerdanturfTentRewards[]", BattleTent::Verdanturf),
            ("sFallarborTentRewards[]", BattleTent::Fallarbor),
            ("sSlateportTentRewards[]", BattleTent::Slateport),
        ] {
            let body = &source[source.find(declaration).expect("declaration present")..];
            let open = body.find('{').expect("pool opens") + 1;
            let close = body.find('}').expect("pool closes");
            let upstream: Vec<u16> = body[open..close]
                .split(',')
                .map(|name| items[name.trim()])
                .collect();
            let ours: Vec<u16> = tent_rewards(tent).iter().map(|id| id.index()).collect();
            assert_eq!(ours, upstream, "{declaration}");
        }
    }
}
