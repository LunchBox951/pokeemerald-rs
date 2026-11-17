use assets::items::ItemId;
use assets::AbilityId;

use super::{roll_pickup, PickupFind, PickupSlotFacts};
use crate::script_rng::SequenceRng;

/// A hit draw: a multiple of ten, above a byte so a truncating `% 10` fails.
const HIT: u16 = 65_530;

/// A miss draw: `65_535 % 10 == 5`.
const MISS: u16 = 65_535;

/// A draw whose `% 100` is `percentile`, above a byte so a truncating read of
/// the draw fails.
fn percentile(percentile: u16) -> u16 {
    assert!(percentile < 100);
    65_400 + percentile
}

fn holder_facts(level: u8) -> PickupSlotFacts {
    PickupSlotFacts {
        ability: AbilityId::PICKUP,
        is_egg: false,
        held_item: ItemId::NONE,
        level,
    }
}

/// What a single level-`level` holder finds on a hit with `percentile`.
fn found(level: u8, rolled_percentile: u16) -> Option<ItemId> {
    let mut rng = SequenceRng::new([HIT, percentile(rolled_percentile)]);
    let finds = roll_pickup(&[Some(holder_facts(level))], &mut rng);
    rng.assert_exhausted();
    finds.first().map(|find| find.item)
}

#[test]
fn an_empty_party_draws_nothing() {
    let mut rng = SequenceRng::new([]);
    assert_eq!(roll_pickup(&[], &mut rng), []);
    assert_eq!(roll_pickup(&[None; 6], &mut rng), []);
    assert_eq!(rng.draws(), 0);
}

#[test]
fn an_ineligible_slot_draws_nothing() {
    let other_ability = PickupSlotFacts {
        ability: AbilityId::RUN_AWAY,
        ..holder_facts(5)
    };
    let no_ability = PickupSlotFacts {
        ability: AbilityId::NONE,
        ..holder_facts(5)
    };
    let egg = PickupSlotFacts {
        is_egg: true,
        ..holder_facts(5)
    };
    let holding = PickupSlotFacts {
        held_item: ItemId::POTION,
        ..holder_facts(5)
    };
    let mut rng = SequenceRng::new([]);
    let party = [
        Some(other_ability),
        Some(no_ability),
        Some(egg),
        Some(holding),
        None,
        None,
    ];
    assert_eq!(roll_pickup(&party, &mut rng), []);
    assert_eq!(rng.draws(), 0);
}

#[test]
fn an_eligible_slot_that_misses_draws_once() {
    for miss in [1, 9, 11, MISS] {
        let mut rng = SequenceRng::new([miss]);
        assert_eq!(roll_pickup(&[Some(holder_facts(5))], &mut rng), []);
        assert_eq!(rng.draws(), 1, "draw {miss}");
    }
}

#[test]
fn an_eligible_slot_that_hits_draws_a_second_value() {
    for hit in [0, 10, 100, HIT] {
        let mut rng = SequenceRng::new([hit, percentile(0)]);
        assert_eq!(
            roll_pickup(&[Some(holder_facts(5))], &mut rng),
            [PickupFind {
                slot: 0,
                item: ItemId::POTION
            }],
            "draw {hit}",
        );
        assert_eq!(rng.draws(), 2);
    }
}

#[test]
fn slots_draw_in_order_and_keep_their_numbering() {
    let egg = PickupSlotFacts {
        is_egg: true,
        ..holder_facts(5)
    };
    let party = [
        Some(holder_facts(5)),   // draws 1: miss
        None,                    // no Pokemon
        Some(egg),               // egg
        Some(holder_facts(5)),   // draws 2, 3: hit, percentile 98 -> NUGGET
        Some(holder_facts(100)), // draws 4, 5: hit, percentile 99 -> LEFTOVERS
        Some(holder_facts(50)),  // draws 6: miss
    ];
    let mut rng = SequenceRng::new([MISS, HIT, percentile(98), HIT, percentile(99), 3]);
    assert_eq!(
        roll_pickup(&party, &mut rng),
        [
            PickupFind {
                slot: 3,
                item: ItemId::NUGGET
            },
            PickupFind {
                slot: 4,
                item: ItemId::LEFTOVERS
            },
        ],
    );
    rng.assert_exhausted();
}

#[test]
fn the_level_tier_changes_at_each_multiple_of_ten_plus_one() {
    // Percentile 0 is the first common item of the tier: sPickupItems[tier].
    let first_of_tier = [
        (1, ItemId::POTION),
        (10, ItemId::POTION),
        (11, ItemId::ANTIDOTE),
        (20, ItemId::ANTIDOTE),
        (21, ItemId::SUPER_POTION),
        (30, ItemId::SUPER_POTION),
        (31, ItemId::GREAT_BALL),
        (40, ItemId::GREAT_BALL),
        (41, ItemId::REPEL),
        (50, ItemId::REPEL),
        (51, ItemId::ESCAPE_ROPE),
        (60, ItemId::ESCAPE_ROPE),
        (61, ItemId::X_ATTACK),
        (70, ItemId::X_ATTACK),
        (71, ItemId::FULL_HEAL),
        (80, ItemId::FULL_HEAL),
        (81, ItemId::ULTRA_BALL),
        (90, ItemId::ULTRA_BALL),
        (91, ItemId::HYPER_POTION),
        (100, ItemId::HYPER_POTION),
    ];
    for (level, item) in first_of_tier {
        assert_eq!(found(level, 0), Some(item), "level {level}");
    }
}

#[test]
fn a_level_outside_the_party_range_follows_upstreams_tier_arithmetic() {
    // `GetMonData` is unsigned, so `(0 - 1) / 10` is `0x1999_9999`; its low
    // byte, `0x99`, is past tier 9 and is clamped to it, as is any level past 100.
    assert_eq!(found(0, 0), Some(ItemId::HYPER_POTION));
    assert_eq!(found(101, 0), Some(ItemId::HYPER_POTION));
    assert_eq!(found(255, 0), Some(ItemId::HYPER_POTION));
}

#[test]
fn the_percentile_steps_change_at_each_threshold() {
    // Level 5, tier 0: sPickupProbabilities is [30, 40, 50, 60, 70, 80, 90,
    // 94, 98], and a percentile equal to a threshold moves to the next item.
    let steps = [
        (0, ItemId::POTION),
        (29, ItemId::POTION),
        (30, ItemId::ANTIDOTE),
        (39, ItemId::ANTIDOTE),
        (40, ItemId::SUPER_POTION),
        (49, ItemId::SUPER_POTION),
        (50, ItemId::GREAT_BALL),
        (59, ItemId::GREAT_BALL),
        (60, ItemId::REPEL),
        (69, ItemId::REPEL),
        (70, ItemId::ESCAPE_ROPE),
        (79, ItemId::ESCAPE_ROPE),
        (80, ItemId::X_ATTACK),
        (89, ItemId::X_ATTACK),
        (90, ItemId::FULL_HEAL),
        (93, ItemId::FULL_HEAL),
        (94, ItemId::ULTRA_BALL),
        (97, ItemId::ULTRA_BALL),
        (98, ItemId::NUGGET),
        (99, ItemId::HYPER_POTION),
    ];
    for (rolled, item) in steps {
        assert_eq!(found(5, rolled), Some(item), "percentile {rolled}");
    }
}

#[test]
fn the_rare_items_follow_the_tier_and_read_backwards_from_99() {
    // Percentile 98 reads sRarePickupItems[tier + 1], 99 reads [tier].
    assert_eq!(found(100, 97), Some(ItemId::MAX_ELIXIR));
    assert_eq!(found(100, 98), Some(ItemId::TM_EARTHQUAKE));
    assert_eq!(found(100, 99), Some(ItemId::LEFTOVERS));
    assert_eq!(found(45, 98), Some(ItemId::WHITE_HERB));
    assert_eq!(found(45, 99), Some(ItemId::ETHER));
}

#[test]
fn a_tier_and_a_percentile_pick_the_matching_common_entry() {
    // sPickupItems[tier + step]: level 45 is tier 4, percentile 55 step 3.
    assert_eq!(found(45, 55), Some(ItemId::FULL_HEAL));
    // Level 91, tier 9, percentile 97 is the last step: sPickupItems[17].
    assert_eq!(found(91, 97), Some(ItemId::MAX_ELIXIR));
}
