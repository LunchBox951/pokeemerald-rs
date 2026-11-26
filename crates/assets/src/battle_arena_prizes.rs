//! The Battle Arena prize-item pools: the two ordered item lists the Arena's
//! end-of-challenge prize is drawn from.
//!
//! Source: `sShortStreakPrizeItems` and `sLongStreakPrizeItems`
//! (`src/battle_arena.c:355-376`). `SetArenaPrize` (`:744-752`) draws from the
//! long pool when the win streak is above 41 and from the short pool
//! otherwise.
//!
//! Data only: that streak test, the random draw, the award into the bag and
//! the saved prize are not ported here.

use crate::items::ItemId;

/// Which of the two Arena prize pools to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArenaPrizePool {
    /// `sShortStreakPrizeItems`.
    ShortStreak,
    /// `sLongStreakPrizeItems`.
    LongStreak,
}

/// `sShortStreakPrizeItems` (`src/battle_arena.c:355-363`).
const SHORT_STREAK_PRIZE_ITEMS: [ItemId; 6] = [
    ItemId::HP_UP,
    ItemId::PROTEIN,
    ItemId::IRON,
    ItemId::CALCIUM,
    ItemId::CARBOS,
    ItemId::ZINC,
];

/// `sLongStreakPrizeItems` (`src/battle_arena.c:365-376`).
const LONG_STREAK_PRIZE_ITEMS: [ItemId; 9] = [
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

/// Returns the items of one prize pool, in upstream order.
#[must_use]
pub const fn prize_items(pool: ArenaPrizePool) -> &'static [ItemId] {
    match pool {
        ArenaPrizePool::ShortStreak => &SHORT_STREAK_PRIZE_ITEMS,
        ArenaPrizePool::LongStreak => &LONG_STREAK_PRIZE_ITEMS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The short pool as raw `ITEM_*` values from `include/constants/items.h`,
    /// read independently of the table: HP Up 63, Protein 64, Iron 65,
    /// Calcium 67, Carbos 66, Zinc 70.
    const SHORT_EXPECTED: [u16; 6] = [63, 64, 65, 67, 66, 70];

    /// The long pool as raw `ITEM_*` values: Bright Powder 179, White Herb
    /// 180, Quick Claw 183, Leftovers 200, Mental Herb 185, King's Rock 187,
    /// Focus Band 196, Scope Lens 198, Choice Band 186.
    const LONG_EXPECTED: [u16; 9] = [179, 180, 183, 200, 185, 187, 196, 198, 186];

    #[test]
    fn the_short_pool_is_pinned_slot_by_slot() {
        let items = prize_items(ArenaPrizePool::ShortStreak);
        assert_eq!(items.len(), 6);
        for (slot, (&item, &raw)) in items.iter().zip(&SHORT_EXPECTED).enumerate() {
            assert_eq!(item, ItemId(raw), "short pool slot {slot}");
        }
        assert_eq!(
            items,
            [
                ItemId::HP_UP,
                ItemId::PROTEIN,
                ItemId::IRON,
                ItemId::CALCIUM,
                ItemId::CARBOS,
                ItemId::ZINC,
            ]
        );
    }

    #[test]
    fn the_long_pool_is_pinned_slot_by_slot() {
        let items = prize_items(ArenaPrizePool::LongStreak);
        assert_eq!(items.len(), 9);
        for (slot, (&item, &raw)) in items.iter().zip(&LONG_EXPECTED).enumerate() {
            assert_eq!(item, ItemId(raw), "long pool slot {slot}");
        }
        assert_eq!(
            items,
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
    fn the_pools_differ_in_length_and_in_every_slot() {
        let short = prize_items(ArenaPrizePool::ShortStreak);
        let long = prize_items(ArenaPrizePool::LongStreak);
        assert_ne!(short.len(), long.len());
        for slot in 0..short.len() {
            assert_ne!(short[slot], long[slot], "slot {slot}");
        }
        for item in short {
            assert!(!long.contains(item), "{item:?} is in both pools");
        }
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

    /// Parses the `ITEM_*` names of one upstream array, in order.
    fn parse_pool<'a>(source: &'a str, name: &str) -> Vec<&'a str> {
        let declaration = format!("{name}[] =\n{{");
        let start = source
            .find(&declaration)
            .unwrap_or_else(|| panic!("`{declaration}` not found"))
            + declaration.len();
        let body = &source[start..];
        let body = &body[..body.find("\n};").expect("initializer closes")];
        body.split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .collect()
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn both_upstream_pools_match_the_tables() {
        let header = upstream_file("include/constants/items.h");
        let value_of = |name: &str| -> u16 {
            header
                .lines()
                .find_map(|line| {
                    let mut words = line.split_whitespace();
                    (words.next()? == "#define" && words.next()? == name)
                        .then(|| words.next()?.parse().ok())?
                })
                .unwrap_or_else(|| panic!("`{name}` not found in items.h"))
        };
        let source = upstream_file("src/battle_arena.c");
        for (name, pool) in [
            ("sShortStreakPrizeItems", ArenaPrizePool::ShortStreak),
            ("sLongStreakPrizeItems", ArenaPrizePool::LongStreak),
        ] {
            let upstream: Vec<u16> = parse_pool(&source, &format!("static const u16 {name}"))
                .into_iter()
                .map(value_of)
                .collect();
            let table: Vec<u16> = prize_items(pool).iter().map(|item| item.index()).collect();
            assert_eq!(table, upstream, "{name}");
        }
    }
}
