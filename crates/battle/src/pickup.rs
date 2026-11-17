//! The Pickup ability's after-battle item roll.
//!
//! Upstream runs `Cmd_pickup` (`pokeemerald/src/battle_script_commands.c:9654`)
//! once after a won battle, from `BattleScript_PayDayMoneyAndPickUpItems`
//! (`pokeemerald/data/battle_scripts_1.s:2940-2943`). For every party slot, in
//! order, a Pokémon whose ability is Pickup, that is not an egg and holds no
//! item draws `Random() % 10`; only a zero draws a second `Random() % 100`,
//! which selects the item from the level tier
//! (`pokeemerald/src/battle_script_commands.c:9699-9726`).
//!
//! This module is the rule alone. It reads each slot's facts and the random
//! source and names the slots that found an item; it changes no party, so the
//! caller owns when the roll runs (a won battle) and how a found item reaches
//! the saved party. The ordinary distribution only: the Battle Pike skip and
//! the Battle Pyramid distribution are not modelled.

use assets::items::ItemId;
use assets::pickup::{PickupTables, LEVEL_TIER_COUNT};
use assets::AbilityId;

use crate::damage::BattleRng;

/// Upstream rolls one in this many Pickup holders per battle
/// (`(Random() % 10) == 0`, `pokeemerald/src/battle_script_commands.c:9701`).
const FIND_ONE_IN: u16 = 10;

/// The percentile range the level tier's item lookup is drawn over
/// (`Random() % 100`, `pokeemerald/src/battle_script_commands.c:9706`).
const PERCENTILE_MODULUS: u16 = 100;

/// Levels per Pickup tier (`(level - 1) / 10`,
/// `pokeemerald/src/battle_script_commands.c:9707`).
const LEVELS_PER_TIER: u32 = 10;

/// The last level tier (`if (lvlDivBy10 > 9) lvlDivBy10 = 9`,
/// `pokeemerald/src/battle_script_commands.c:9708-9709`).
const LAST_TIER: u8 = 9;

const _: () = assert!(LAST_TIER as usize + 1 == LEVEL_TIER_COUNT);

/// What the roll reads of one party slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickupSlotFacts {
    /// The ability the saved record names: the species' ability at the
    /// record's ability-slot bit, not a battle-time copy of another ability.
    pub ability: AbilityId,

    /// Whether the slot is an egg.
    pub is_egg: bool,

    /// The slot's held item, [`ItemId::NONE`] when it holds nothing.
    pub held_item: ItemId,

    /// The slot's level.
    pub level: u8,
}

/// An item a party slot found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickupFind {
    /// The party slot, counted over the whole slice handed to [`roll_pickup`].
    pub slot: usize,

    /// The item the slot now holds.
    pub item: ItemId,
}

/// Rolls Pickup over `party`, slot by slot.
///
/// `None` is a slot with no Pokémon (`SPECIES_NONE`), which draws nothing but
/// keeps its place in the slot numbering. A slot draws only if its ability is
/// Pickup, it is not an egg and it holds no item. Of those, each draws one
/// value from `rng`, `% 10`, and a second, `% 100`, only on zero, so a party
/// with no eligible slot leaves `rng` untouched.
///
/// A level outside `1..=100` is not rejected: the tier is upstream's
/// arithmetic, `(level - 1) / 10` clamped to the last tier. That arithmetic is
/// unsigned (`GetMonData` returns `u32`), so level `0` wraps and lands in the
/// last tier.
///
/// # Panics
///
/// Never in practice: the tier is clamped below [`LEVEL_TIER_COUNT`] and the
/// percentile is below `100`, the only inputs the item lookup rejects.
#[must_use]
pub fn roll_pickup(party: &[Option<PickupSlotFacts>], rng: &mut impl BattleRng) -> Vec<PickupFind> {
    let tables = PickupTables::new();
    let mut finds = Vec::new();
    for (slot, facts) in party.iter().enumerate() {
        let Some(facts) = facts else {
            continue;
        };
        if facts.ability != AbilityId::PICKUP
            || facts.is_egg
            || facts.held_item != ItemId::NONE
            || !rng.next_u16().is_multiple_of(FIND_ONE_IN)
        {
            continue;
        }
        let percentile = rng.next_u16() % PERCENTILE_MODULUS;
        let outcome = tables
            .lookup(
                level_tier(facts.level),
                u8::try_from(percentile).expect("a percentile below 100 fits a byte"),
            )
            .expect("the tier is clamped and the percentile is below 100");
        finds.push(PickupFind {
            slot,
            item: outcome.item(),
        });
    }
    finds
}

/// `(level - 1) / 10` in `GetMonData`'s `u32` arithmetic, stored in a `u8`
/// and clamped to the last tier (`u8 lvlDivBy10 = (GetMonData(..) - 1) / 10`,
/// `pokeemerald/src/battle_script_commands.c:9707-9709`).
///
/// Level `0` wraps to `0xFFFF_FFFF`, whose tenth, `0x1999_9999`, keeps its low
/// byte `0x99` in the `u8` and so clamps to the last tier.
fn level_tier(level: u8) -> u8 {
    let tenths = u32::from(level).wrapping_sub(1) / LEVELS_PER_TIER;
    let stored = u8::try_from(tenths & u32::from(u8::MAX)).expect("masked to a byte");
    stored.min(LAST_TIER)
}

#[cfg(test)]
mod tests;
