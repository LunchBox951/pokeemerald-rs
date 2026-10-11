//! Fixtures shared by the sibling test modules: IV boundaries and a sample battler.

use super::super::{BattlePokemon, Ivs};
use crate::dex::Dex;
use assets::{MoveId, SpeciesId};

/// Upstream `MAX_IV_MASK` (`include/constants/pokemon.h:201`) is 31, so 32 is
/// the first unrepresentable IV. Literal, not [`MAX_IV`](super::super::MAX_IV),
/// so a drifting production constant fails the tests.
pub(super) const PINNED_MAX_IV: u8 = 31;
pub(super) const FIRST_UNREPRESENTABLE_IV: u8 = 32;
pub(super) const MAX_IVS: Ivs = Ivs {
    hp: PINNED_MAX_IV,
    attack: PINNED_MAX_IV,
    defense: PINNED_MAX_IV,
    speed: PINNED_MAX_IV,
    sp_attack: PINNED_MAX_IV,
    sp_defense: PINNED_MAX_IV,
};

pub(super) const BULBASAUR: SpeciesId = SpeciesId(1);
pub(super) const TACKLE: MoveId = MoveId::TACKLE;
pub(super) const HARDY_PERSONALITY: u32 = 0x1234_5663;

pub(super) fn sample_mon(dex: &Dex) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        BULBASAUR,
        5,
        Ivs::default(),
        HARDY_PERSONALITY,
        vec![TACKLE],
    )
    .unwrap()
}
