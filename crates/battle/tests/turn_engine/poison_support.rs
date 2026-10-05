//! Constants shared by the poison turn-engine suites: species, moves, and
//! scripted draws used by more than one seam.

pub(super) use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;

/// `MOVE_TACKLE`.
pub(super) const TACKLE: MoveId = MoveId::TACKLE;
/// `MOVE_POISON_STING` (`EFFECT_POISON_HIT`), 100 accuracy, 30% chance,
/// Poison type.
pub(super) const POISON_STING: MoveId = MoveId::POISON_STING;

/// `SPECIES_RATTATA`: base Speed 72, the fast mover in every fixture below.
pub(super) const RATTATA: u16 = 19;
/// `SPECIES_ZIGZAGOON`: an ordinary Normal-type target, slower than Rattata.
pub(super) const ZIGZAGOON: u16 = 288;
/// `SPECIES_EKANS`: mono Poison-type, immune to poison outright.
pub(super) const EKANS: u16 = 23;

/// Level-5 Ralts carries Synchronize, whose poison reflection is modelled
/// at move end (`battle::secondary::resolve_synchronize_poison_reflection`).
pub(super) const RALTS: u16 = 392;

/// A draw that clears [`POISON_STING`]'s 30% secondary chance.
pub(super) const POISON_CHANCE_HIT_DRAW: u16 = 29;

/// [`BOTH_BATTLERS_ATTACK`] with the player's [`POISON_STING`] chance draw
/// clearing.
pub(super) const POISON_STING_LANDS: [u16; 11] =
    [0, 0, 0, 0, 1, 0, POISON_CHANCE_HIT_DRAW, 0, 1, 0, 0];

/// `SPECIES_CHARMANDER`, a level-50 fixture that one-shots the level-5 wild
/// mon below with Tackle at any damage roll.
pub(super) const CHARMANDER: u16 = 4;
