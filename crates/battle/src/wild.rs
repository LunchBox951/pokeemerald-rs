//! Wild-encounter Pokémon construction (S-6): the `CreateWildMon` RNG path.
//!
//! Upstream builds a wild mon in three RNG-observable steps, in this order
//! (`pokeemerald/src/wild_encounter.c:379`, `src/pokemon.c:2205`/`2298`):
//!
//! 1. [`roll_nature`] — `PickWildMonNature` (`wild_encounter.c:335`): one
//!    `Random()` draw picking a `NATURE_*` id.
//! 2. [`roll_personality_for_nature`] — `CreateMonWithNature`
//!    (`pokemon.c:2305`): a rejection loop, `Random32()` per attempt, until
//!    `GetNatureFromPersonality(personality) == nature`
//!    (`personality % NUM_NATURES`, `pokemon.c:5498`).
//! 3. [`roll_ivs`] — `CreateBoxMon`'s `USE_RANDOM_IVS` branch
//!    (`pokemon.c:2276`): exactly two `Random()` draws, five bits per stat.
//!
//! This crate does not depend on `engine` (`engine::save::pokemon::Pokemon`
//! is a save-file *serialization* boundary — encrypted substructures, no
//! computed stats — not a battle-ready representation) or on `engine::rng`
//! specifically: [`crate::damage::BattleRng`] already matches its shape, so
//! any `engine::rng::Rng` (or test double) plugs in directly
//! `(oop-boundaries, minimal-deps)`.
//!
//! Simplified out of this slice (all ability/mode-gated, `(behavioral-
//! fidelity)`'s "as far as the first-encounter species need"):
//! Safari Zone Pokéblock-weighted natures, the leading party mon's
//! Synchronize/Cute Charm influence on nature/gender, and the OT-id shiny
//! reroll loop (`OT_ID_RANDOM_NO_SHINY`) — a wild mon here always takes the
//! player's OT id with **no** extra `Random32()` draws, matching
//! `CreateMonWithNature`'s `OT_ID_PLAYER_ID` argument. `GiveBoxMonInitialMoveset`
//! (deriving a level-up moveset) is not modelled either: callers supply the
//! wild mon's moves directly.
//!
//! [`roll_personality_for_nature`]'s rejection loop is upstream's own
//! design — a real 32-bit LCG visits every residue class mod
//! [`crate::nature::Nature::ALL`]'s length `25` within its full period, so
//! the loop always terminates for a real generator; it is not artificially
//! bounded here, matching upstream having no bound either.

use assets::{MoveId, SpeciesId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::nature::Nature;
use crate::pokemon::{BattlePokemon, Ivs};

/// `PickWildMonNature`'s v1 path (`pokeemerald/src/wild_encounter.c:335`):
/// `Random() % NUM_NATURES`. The Safari Zone and Synchronize branches ahead
/// of this in upstream are both ability/mode-gated (see the module docs) and
/// not reached for a plain first encounter.
///
/// # Panics
///
/// Never in practice: `value % 25` is always `0..25`, and
/// [`Nature::from_id`] accepts every id in that range.
#[must_use]
pub fn roll_nature(rng: &mut impl BattleRng) -> Nature {
    let id = (rng.next_u16() % 25) as u8;
    Nature::from_id(id).expect("value % 25 is always a valid NATURE_* id")
}

/// `CreateMonWithNature`'s personality loop (`pokeemerald/src/pokemon.c:2305`):
/// draw `Random32()` until its nature (`personality % NUM_NATURES`) matches
/// `nature`.
#[must_use]
pub fn roll_personality_for_nature(nature: Nature, rng: &mut impl BattleRng) -> u32 {
    loop {
        let personality = rng.next_u32();
        if Nature::from_personality(personality) == nature {
            return personality;
        }
    }
}

/// `CreateBoxMon`'s `USE_RANDOM_IVS` branch (`pokeemerald/src/pokemon.c:2276`):
/// two `Random()` draws, each split into three 5-bit fields
/// (`value & MAX_IV_MASK`, `(value & (MAX_IV_MASK << 5)) >> 5`,
/// `(value & (MAX_IV_MASK << 10)) >> 10`) — HP/Attack/Defense from the first
/// draw, Speed/Sp. Attack/Sp. Defense from the second.
#[must_use]
pub fn roll_ivs(rng: &mut impl BattleRng) -> Ivs {
    const MASK: u16 = 0x1F;
    let first = rng.next_u16();
    let second = rng.next_u16();
    Ivs {
        hp: (first & MASK) as u8,
        attack: ((first >> 5) & MASK) as u8,
        defense: ((first >> 10) & MASK) as u8,
        speed: (second & MASK) as u8,
        sp_attack: ((second >> 5) & MASK) as u8,
        sp_defense: ((second >> 10) & MASK) as u8,
    }
}

/// Build a wild [`BattlePokemon`], drawing nature, personality, and IVs from
/// `rng` in upstream's exact order (see the module docs), then the moveset
/// the caller supplies (`GiveBoxMonInitialMoveset` is not modelled — see the
/// module docs).
///
/// Every caller-supplied input is checked **before the first draw**
/// ([`BattlePokemon::validate`]): a rejected request must leave the shared RNG
/// stream exactly as it found it, the same rule
/// [`crate::battle::Battle::new`] follows `(behavioral-fidelity)`. Only the
/// rolled fields are validated afterwards, and [`roll_ivs`] cannot produce an
/// out-of-range IV in the first place (it masks each to five bits).
///
/// # Errors
///
/// [`BattleError::InvalidLevel`] for a `level` outside `MIN_LEVEL..=MAX_LEVEL`
/// (`1..=100`), [`BattleError::InvalidMoveCount`] /
/// [`BattleError::PlaceholderMove`] for a moveset upstream cannot represent,
/// or [`BattleError::UnknownSpecies`] / [`BattleError::UnknownMove`] if
/// `species`/any of `moves` is not in `dex` — none of which draw.
pub fn build_wild_pokemon(
    dex: &Dex,
    species: SpeciesId,
    level: u8,
    moves: Vec<MoveId>,
    rng: &mut impl BattleRng,
) -> Result<BattlePokemon, BattleError> {
    // Before `roll_nature`, not after: an out-of-range level rejected on the
    // way out of `BattlePokemon::new` would already have consumed the five
    // encounter draws.
    BattlePokemon::validate(dex, species, level, &moves)?;
    let nature = roll_nature(rng);
    let personality = roll_personality_for_nature(nature, rng);
    let ivs = roll_ivs(rng);
    // `BattlePokemon::new` re-derives the nature from `personality`; the
    // rejection loop above guarantees it comes out as `nature`.
    BattlePokemon::new(dex, species, level, ivs, personality, moves)
}

#[cfg(test)]
mod tests {
    use super::{build_wild_pokemon, roll_ivs, roll_nature, roll_personality_for_nature};
    use crate::damage::BattleRng;
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::nature::Nature;
    use crate::pokemon::MOVE_NONE;
    use assets::{MoveId, SpeciesId};

    /// A `BattleRng` fed from a fixed sequence, panicking (loudly, not
    /// hanging) if exhausted — for pinning exact draw order/count without
    /// risking an infinite loop in a broken test.
    struct SequenceRng {
        values: Vec<u16>,
        index: usize,
    }
    impl SequenceRng {
        fn new(values: impl IntoIterator<Item = u16>) -> Self {
            Self {
                values: values.into_iter().collect(),
                index: 0,
            }
        }
        fn draws(&self) -> usize {
            self.index
        }
    }
    impl BattleRng for SequenceRng {
        fn next_u16(&mut self) -> u16 {
            let v = self
                .values
                .get(self.index)
                .copied()
                .expect("SequenceRng exhausted");
            self.index += 1;
            v
        }
    }

    #[test]
    fn roll_nature_is_modulo_twenty_five_of_one_draw() {
        let mut rng = SequenceRng::new([0]);
        assert_eq!(roll_nature(&mut rng), Nature::Hardy); // 0 % 25 == 0
        assert_eq!(rng.draws(), 1);

        let mut rng = SequenceRng::new([24]);
        assert_eq!(roll_nature(&mut rng), Nature::Quirky); // 24 % 25 == 24
        assert_eq!(rng.draws(), 1);

        let mut rng = SequenceRng::new([25]); // wraps back to Hardy
        assert_eq!(roll_nature(&mut rng), Nature::Hardy);
    }

    #[test]
    fn roll_personality_for_nature_stops_at_the_first_match() {
        // Personality 0 has nature Hardy (0 % 25 == 0): a target of Hardy
        // matches on the very first Random32() draw.
        let mut rng = SequenceRng::new([0, 0]); // next_u32() composes 2 draws
        let personality = roll_personality_for_nature(Nature::Hardy, &mut rng);
        assert_eq!(personality, 0);
        assert_eq!(rng.draws(), 2);
    }

    #[test]
    fn roll_personality_for_nature_rejects_until_a_match() {
        // First candidate personality = 1 (nature Lonely, id 1) does not
        // match a target of Bold (id 5); second candidate = 5 does.
        // next_u32() draws low-then-high per call: (1, 0) -> 0x0000_0001,
        // (5, 0) -> 0x0000_0005.
        let mut rng = SequenceRng::new([1, 0, 5, 0]);
        let personality = roll_personality_for_nature(Nature::Bold, &mut rng);
        assert_eq!(personality, 5);
        assert_eq!(rng.draws(), 4); // two full Random32() draws, rejected once
    }

    #[test]
    fn roll_ivs_splits_two_draws_into_five_bit_fields() {
        // First draw 0b00_10101_01010_11111 = HP 0b11111=31, Atk
        // 0b01010=10, Def 0b10101=21 (top bit of the 16-bit value unused).
        let first = 0b0_10101_01010_11111u16;
        // Second draw similarly for Speed/SpAtk/SpDef.
        let second = 0b0_00001_00010_00011u16;
        let mut rng = SequenceRng::new([first, second]);
        let ivs = roll_ivs(&mut rng);
        assert_eq!(ivs.hp, 31);
        assert_eq!(ivs.attack, 10);
        assert_eq!(ivs.defense, 21);
        assert_eq!(ivs.speed, 3);
        assert_eq!(ivs.sp_attack, 2);
        assert_eq!(ivs.sp_defense, 1);
        assert_eq!(rng.draws(), 2);
    }

    #[test]
    fn roll_ivs_masks_to_five_bits_even_with_high_bits_set() {
        let mut rng = SequenceRng::new([0xFFFF, 0xFFFF]);
        let ivs = roll_ivs(&mut rng);
        assert_eq!(ivs.hp, 31);
        assert_eq!(ivs.attack, 31);
        assert_eq!(ivs.defense, 31);
        assert_eq!(ivs.speed, 31);
        assert_eq!(ivs.sp_attack, 31);
        assert_eq!(ivs.sp_defense, 31);
    }

    #[test]
    fn build_wild_pokemon_draws_nature_then_personality_then_ivs_in_order() {
        let dex = Dex::new();
        // Distinct, position-revealing values -- an all-zeros script cannot
        // pin the order, because every permutation of the three roll phases
        // would build the same mon from it. Each phase below only reads its
        // own values correctly in the documented order:
        // - nature draw: 24 % 25 -> Quirky;
        // - personality draws (low then high): (24, 0) -> 24, whose 24 % 25
        //   matches Quirky on the first try -- a reordered phase feeds the
        //   wrong values into the rejection loop and exhausts the script;
        // - IV draws: 0x001F -> hp 31 / attack 0 / defense 0, then
        //   0x03E0 -> speed 0 / sp_attack 31 / sp_defense 0.
        let mut rng = SequenceRng::new([24, 24, 0, 0x001F, 0x03E0]);
        let mon = build_wild_pokemon(
            &dex,
            SpeciesId(1), // Bulbasaur
            5,
            vec![MoveId(33)], // Tackle
            &mut rng,
        )
        .unwrap();
        assert_eq!(mon.nature(), Nature::Quirky);
        assert_eq!(mon.personality(), 24);
        assert_eq!(mon.ivs().hp, 31);
        assert_eq!(mon.ivs().attack, 0);
        assert_eq!(mon.ivs().defense, 0);
        assert_eq!(mon.ivs().speed, 0);
        assert_eq!(mon.ivs().sp_attack, 31);
        assert_eq!(mon.ivs().sp_defense, 0);
        assert_eq!(rng.draws(), 5);
        assert_eq!(mon.level(), 5);
        assert_eq!(mon.species(), SpeciesId(1));
        assert!(!mon.is_fainted());
    }

    #[test]
    fn build_wild_pokemon_rejects_bad_inputs_before_drawing_anything() {
        let dex = Dex::new();
        // The level/species/moves are the caller's, not rolled, so they are
        // checked ahead of `PickWildMonNature`'s draw: every script below is
        // *empty*, so a single draw before the rejection would panic on an
        // exhausted SequenceRng rather than quietly pass.
        // (MIN_LEVEL..=MAX_LEVEL is 1..=100, `include/constants/pokemon.h:145`-`:146`.)
        for (level, moves, expected) in [
            (101, vec![MoveId(33)], BattleError::InvalidLevel(101)),
            (0, vec![MoveId(33)], BattleError::InvalidLevel(0)),
            (5, vec![], BattleError::InvalidMoveCount(0)),
            (5, vec![MOVE_NONE], BattleError::PlaceholderMove(0)),
            (
                5,
                vec![MoveId(60_000)],
                BattleError::UnknownMove(MoveId(60_000)),
            ),
        ] {
            let mut rng = SequenceRng::new([]);
            assert_eq!(
                build_wild_pokemon(&dex, SpeciesId(1), level, moves, &mut rng),
                Err(expected)
            );
            assert_eq!(rng.draws(), 0, "a rejected request must not draw");
        }
    }

    #[test]
    fn rolled_ivs_are_always_within_the_upstream_range() {
        // Every 16-bit draw splits into three 5-bit fields, so no roll can
        // produce an out-of-range individual value (Gen-3 stat rolls -- see
        // `Ivs` -- not cryptographic initialization vectors).
        for value in [0u16, 1, 0x5A5A, 0xFFFF] {
            let mut rng = SequenceRng::new([value, !value]);
            assert!(roll_ivs(&mut rng).is_valid());
        }
    }
}
