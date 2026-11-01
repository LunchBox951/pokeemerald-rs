//! Constants shared by the poison turn-engine suites: species, moves, and
//! scripted draws used by more than one seam.

pub(super) use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;

pub(super) const TACKLE: MoveId = MoveId::TACKLE;
/// `MOVE_POISON_STING`: `EFFECT_POISON_HIT`, 100 accuracy, 30% chance.
pub(super) const POISON_STING: MoveId = MoveId::POISON_STING;

/// `SPECIES_RATTATA`: base Speed 72, the fast mover in every fixture below.
pub(super) const RATTATA: u16 = 19;
/// `SPECIES_ZIGZAGOON`: Normal-type, slower than Rattata.
pub(super) const ZIGZAGOON: u16 = 288;
/// `SPECIES_EKANS`: mono Poison-type, so never poisoned.
pub(super) const EKANS: u16 = 23;

/// `SPECIES_RALTS`: carries Synchronize, whose poison reflection runs at move
/// end (`battle::secondary::resolve_synchronize_poison_reflection`).
pub(super) const RALTS: u16 = 392;

/// An effect-chance draw that lands [`POISON_STING`]'s 30% secondary.
pub(super) const POISON_CHANCE_HIT_DRAW: u16 = 29;

/// Scripted draws for a turn where both battlers attack and the player's
/// [`POISON_STING`] effect chance lands: three turn-setup draws (battle start,
/// turn start, enemy selection), then per mover accuracy, crit, damage, and
/// effect chance. Synchronize's reflection draws nothing.
pub(super) const POISON_STING_LANDS: [u16; 11] =
    [0, 0, 0, 0, 1, 0, POISON_CHANCE_HIT_DRAW, 0, 1, 0, 0];

/// `SPECIES_CHARMANDER`: at level 50 it one-shots a level-5 wild mon with
/// Tackle at any damage roll.
pub(super) const CHARMANDER: u16 = 4;
