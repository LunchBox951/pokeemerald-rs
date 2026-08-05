//! Fixtures shared by every integration test under `crates/battle/tests/`:
//! a scripted RNG and a deterministic max-IV mon builder. Pulled out here
//! (issue #209) so `wild_battle.rs` and `turn_engine.rs` don't each carry
//! their own copy of the same plumbing -- both exercise [`battle::Battle`]
//! through its public surface only, exactly as an external caller would, so
//! both can drive it with the same two building blocks.

use assets::{MoveId, SpeciesId};
use battle::{BattlePokemon, BattleRng, Dex, Ivs};

/// A `BattleRng` fed from a fixed sequence, panicking loudly (never hanging)
/// if a test's script under-provisions draws.
pub struct SequenceRng {
    values: Vec<u16>,
    index: usize,
}

impl SequenceRng {
    pub fn new(values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            values: values.into_iter().collect(),
            index: 0,
        }
    }

    /// How many draws have been taken so far -- one shared counter for a
    /// whole battle, so a test can pin the total against the documented
    /// per-phase draw order.
    pub fn draws(&self) -> usize {
        self.index
    }
}

impl BattleRng for SequenceRng {
    fn next_u16(&mut self) -> u16 {
        let v = self
            .values
            .get(self.index)
            .copied()
            .unwrap_or_else(|| panic!("SequenceRng exhausted after {} draws", self.index));
        self.index += 1;
        v
    }
}

/// The maximum Pokémon Gen-3 **individual values** -- the per-stat genetic
/// rolls that feed the stat formula (`MAX_IV_MASK` = 31,
/// `pokeemerald/include/constants/pokemon.h:201`). Not a cryptographic
/// initialization vector: nothing here is cryptographic.
pub const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

/// A max-IV, neutral-nature mon at `species`/`level`, known moves `moves` --
/// deterministic stats so a scenario is reproducible without depending on
/// the test's own RNG script for stat generation too.
pub fn max_iv_mon(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, SpeciesId(species), level, MAX_IVS, 0, moves).unwrap()
}
