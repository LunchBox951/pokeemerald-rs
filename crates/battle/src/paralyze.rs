//! Admission and resolution for `BattleScript_EffectParalyze` (Thunder Wave,
//! Stun Spore, Glare).
//!
//! `data/battle_scripts_1.s:1007`-`:1032`: `attackcanceler`, `attackstring`,
//! `ppreduce`, then `typecalc`'s type-immunity guard, then the two
//! `jumpifstatus` status guards, and only then `accuracycheck`. The guards
//! run before the accuracy draw and consume no randomness of their own; a
//! successful check writes [`crate::status1::Status1::Paralysed`] with
//! `seteffectprimary`, not `seteffectwithchance`, so a landed hit spends no
//! further draw.
//!
//! Not ported: `jumpifability BS_TARGET, ABILITY_LIMBER` (`:1011`, no
//! ability-guard modelled here), `jumpifstatus2 BS_TARGET,
//! STATUS2_SUBSTITUTE` (`:1012`, no Substitute), and `jumpifsideaffecting
//! BS_TARGET, SIDE_STATUS_SAFEGUARD` (`:1018`, no side conditions) — each is
//! simply absent from this module, so none can newly fire before the checks
//! this module does run. The `STATUS1_ANY` guard at `:1016` (any *other*
//! primary status blocks a second one) is likewise never reached: this crate
//! has no primary status besides [`crate::status1::Status1::Paralysed`] to
//! be already carrying.

use assets::{MoveEffect, MoveId, Type};

use crate::accuracy::accuracy_check;
use crate::damage::{apply_dual_type_effectiveness, BattleRng};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;

/// Thunder Wave's, Stun Spore's, and Glare's move effect.
pub const EFFECT_PARALYZE: MoveEffect = MoveEffect(67);

const TYPE_EFFECTIVENESS_PROBE_DAMAGE: u32 = 1;

/// Returns whether `effect` uses [`resolve_paralyze_move`].
#[must_use]
pub fn is_paralyze_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_PARALYZE
}

/// Validates that `move_id` can enter [`resolve_paralyze_move`] without
/// drawing.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`], [`BattleError::UnsupportedMoveEffect`],
/// or [`BattleError::UnsupportedMoveType`] for the corresponding unsupported
/// move property.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_paralyze_effect)
}

fn defender_is_immune(move_type: Type, defender: &BattlePokemon) -> bool {
    apply_dual_type_effectiveness(TYPE_EFFECTIVENESS_PROBE_DAMAGE, move_type, defender.types()) == 0
}

/// The result of resolving an [`EFFECT_PARALYZE`] move, before any mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParalyzeOutcome {
    /// The move failed: the defender's typing is immune to it.
    Failed,
    /// The defender already carries [`Status1::Paralysed`].
    AlreadyParalysed,
    /// The move missed its accuracy check.
    Miss,
    /// The move connected and inflicts [`Status1::Paralysed`].
    Applied,
}

/// Resolves one [`EFFECT_PARALYZE`] move against `defender` without mutating
/// either battler.
///
/// The type-immunity and already-paralysed guards precede the accuracy draw
/// and consume no randomness; a landed hit needs only that one draw, since
/// `seteffectprimary` inflicts the status unconditionally once reached.
///
/// # Errors
///
/// Returns the errors documented by [`ensure_resolvable`], or
/// [`BattleError::UnsupportedMoveType`] if the move has no combat type.
/// Admission completes before any draw.
pub fn resolve_paralyze_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<ParalyzeOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;

    let move_data = dex.move_data(move_id)?;
    let move_type = move_data
        .move_type
        .battle_type()
        .ok_or(BattleError::UnsupportedMoveType(move_id))?;

    if defender_is_immune(move_type, defender) {
        return Ok(ParalyzeOutcome::Failed);
    }
    if defender.status1().is_paralysed() {
        return Ok(ParalyzeOutcome::AlreadyParalysed);
    }

    if !accuracy_check(
        move_data.accuracy,
        move_data.effect,
        attacker.stages().accuracy,
        defender.stages().evasion,
        rng,
    ) {
        return Ok(ParalyzeOutcome::Miss);
    }

    Ok(ParalyzeOutcome::Applied)
}

#[cfg(test)]
#[path = "paralyze/tests.rs"]
mod tests;
