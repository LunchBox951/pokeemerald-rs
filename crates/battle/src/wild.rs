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
///
/// # Panics
///
/// Never in practice, for the same reason as [`roll_nature`]: `personality %
/// 25` is always a valid `NATURE_*` id.
#[must_use]
pub fn roll_personality_for_nature(nature: Nature, rng: &mut impl BattleRng) -> u32 {
    loop {
        let personality = rng.next_u32();
        let rolled = Nature::from_id((personality % 25) as u8)
            .expect("value % 25 is always a valid NATURE_* id");
        if rolled == nature {
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
/// # Errors
///
/// Returns [`BattleError::UnknownSpecies`] or [`BattleError::UnknownMove`] if
/// `species`/any of `moves` is not in `dex`.
pub fn build_wild_pokemon(
    dex: &Dex,
    species: SpeciesId,
    level: u8,
    moves: Vec<MoveId>,
    rng: &mut impl BattleRng,
) -> Result<BattlePokemon, BattleError> {
    let nature = roll_nature(rng);
    let personality = roll_personality_for_nature(nature, rng);
    let ivs = roll_ivs(rng);
    BattlePokemon::new(dex, species, level, nature, ivs, personality, moves)
}

#[cfg(test)]
mod tests {
    use super::{build_wild_pokemon, roll_ivs, roll_nature, roll_personality_for_nature};
    use crate::damage::BattleRng;
    use crate::dex::Dex;
    use crate::nature::Nature;
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
        // nature draw: 0 -> Hardy; personality draws: (0,0) -> 0 (matches
        // Hardy on the first try); IV draws: (0, 0) -> all-zero IVs.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 0]);
        let mon = build_wild_pokemon(
            &dex,
            SpeciesId(1), // Bulbasaur
            5,
            vec![MoveId(33)], // Tackle
            &mut rng,
        )
        .unwrap();
        assert_eq!(mon.nature, Nature::Hardy);
        assert_eq!(mon.personality, 0);
        assert_eq!(mon.ivs.hp, 0);
        assert_eq!(rng.draws(), 5);
        assert_eq!(mon.level, 5);
        assert_eq!(mon.species, SpeciesId(1));
        assert!(!mon.is_fainted());
    }
}
