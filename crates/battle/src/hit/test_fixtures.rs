//! Battlers, move and species constants, and RNG scripts shared by more than
//! one hit test family; single-family fixtures stay in their own module.

use crate::dex::Dex;
use crate::pokemon::{BattlePokemon, Ivs};
use assets::{MoveId, SpeciesId};

pub(super) const BULBASAUR: SpeciesId = SpeciesId(1);

pub(super) const SQUIRTLE: SpeciesId = SpeciesId(7);

pub(super) const RATTATA: SpeciesId = SpeciesId(19);

pub(super) const GASTLY: SpeciesId = SpeciesId(92);

pub(super) const TACKLE: MoveId = MoveId::TACKLE;

pub(super) const GROWL: MoveId = MoveId::GROWL;

pub(super) const WATER_GUN: MoveId = MoveId::WATER_GUN;

pub(super) const SWIFT: MoveId = MoveId::SWIFT;

pub(super) const SLASH: MoveId = MoveId::SLASH;

pub(super) const UNKNOWN_MOVE: MoveId = MoveId(60_000);

/// Normal-type (physical), 75 accuracy.
pub(super) const SLAM: MoveId = MoveId::SLAM;

/// `SPECIES_SHEDINJA`: Wonder Guard in its primary (and only) ability slot.
pub(super) const SHEDINJA: SpeciesId = SpeciesId(303);

pub(super) const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

pub(super) const ACCURACY_HIT_DRAW: u16 = 0;

pub(super) const TACKLE_MISS_DRAW: u16 = 95;

pub(super) const ORDINARY_NO_CRIT_DRAW: u16 = 1;

pub(super) const BEST_DAMAGE_DRAW: u16 = 0;

pub(super) const DISCARDED_EFFECT_DRAW: u16 = 0;

pub(super) const ORDINARY_NON_CRITICAL_DRAWS: [u16; 4] = [
    ACCURACY_HIT_DRAW,
    ORDINARY_NO_CRIT_DRAW,
    BEST_DAMAGE_DRAW,
    DISCARDED_EFFECT_DRAW,
];

pub(super) const ALWAYS_HIT_NON_CRITICAL_DRAWS: [u16; 3] = [
    ORDINARY_NO_CRIT_DRAW,
    BEST_DAMAGE_DRAW,
    DISCARDED_EFFECT_DRAW,
];

pub(super) const CRITICAL_SUPPRESSED_DRAWS: [u16; 3] =
    [ACCURACY_HIT_DRAW, BEST_DAMAGE_DRAW, DISCARDED_EFFECT_DRAW];

pub(super) const STRUGGLE_NON_CRITICAL_DRAWS: [u16; 3] =
    [ACCURACY_HIT_DRAW, ORDINARY_NO_CRIT_DRAW, BEST_DAMAGE_DRAW];

pub(super) fn mon(dex: &Dex, species: SpeciesId, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, species, level, MAX_IVS, 0, moves).unwrap()
}

/// `MOVE_POISON_STING`: `EFFECT_POISON_HIT`.
pub(super) const POISON_STING: MoveId = MoveId::POISON_STING;

/// `MOVE_PSYBEAM`: `EFFECT_CONFUSE_HIT`, 100 accuracy, 10% chance.
pub(super) const PSYBEAM: MoveId = MoveId::PSYBEAM;
