//! Pins the fixtures every seam module below shares: the IV-boundary
//! constants and the one sample battler every non-stat-math test builds
//! from.

use super::super::{BattlePokemon, Ivs};
use crate::dex::Dex;
use assets::{MoveId, SpeciesId};

/// Upstream stores each IV in five bits (`MAX_IV_MASK` = 31,
/// `pokeemerald/include/constants/pokemon.h:201`), so 32 is the first
/// unrepresentable value. Both are pinned here as literals rather than read
/// back from production's [`MAX_IV`](super::super::MAX_IV), so that moving
/// that constant fails these tests instead of moving the fixture, the
/// accepted boundary, and the rejected input together.
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
pub(super) const TACKLE: MoveId = MoveId(33);
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
