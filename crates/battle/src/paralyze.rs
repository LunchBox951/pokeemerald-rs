//! Validation and resolution for moves that inflict paralysis.
//!
//! Limber, type immunity, and existing paralysis are checked before accuracy,
//! so those outcomes consume no RNG. A successful accuracy check applies
//! [`crate::status1::Status1::Paralysed`] without another draw.
//!
//! [`ParalyzeOutcome::Immune`] maps to [`crate::battle::BattleEvent::NoEffect`]
//! while Limber has a distinct event that names the ability. This ordering and
//! split follow `BattleScript_EffectParalyze`
//! (`pokeemerald/data/battle_scripts_1.s:1007-1038`).
//!
//! A target already carrying a primary status other than paralysis takes the
//! generic failure exit, not [`ParalyzeOutcome::AlreadyParalysed`].
//!
//! Substitute and Safeguard are outside this battle model.
//!
//! Guts and Marvel Scale are modelled as raw-stat modifiers in
//! [`crate::pokemon::BattlePokemon::attacking_stat`] and
//! [`crate::pokemon::BattlePokemon::defending_stat`], so admission never
//! refuses either.

use assets::{AbilityId, MoveEffect, MoveId, Type};

use crate::accuracy::accuracy_check;
use crate::damage::{apply_dual_type_effectiveness, BattleRng};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;

/// Move effect shared by Thunder Wave, Stun Spore, and Glare.
pub const EFFECT_PARALYZE: MoveEffect = MoveEffect(67);

const TYPE_EFFECTIVENESS_PROBE_DAMAGE: u32 = 1;

/// Returns whether `effect` uses [`resolve_paralyze_move`].
#[must_use]
pub fn is_paralyze_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_PARALYZE
}

/// Validates move lookup, paralysis effect, and combat type before resolution.
/// Validation consumes no RNG.
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

/// Rejects a paralysis move when inflicting the status would activate an
/// unsupported ability interaction.
///
/// This function does not report move-data errors; callers use
/// [`ensure_resolvable`] for those. Attempts stopped by Limber, type immunity,
/// or an existing primary status are accepted because they cannot reach an
/// unsupported interaction. Synchronize is also accepted when the attacker
/// already carries a primary status and therefore cannot receive the
/// reflected one. Guts and Marvel Scale are modelled by
/// [`BattlePokemon::attacking_stat`] and [`BattlePokemon::defending_stat`], so
/// newly paralysing either holder is admitted.
///
/// # Errors
///
/// Returns [`BattleError::UnportedAbilityInteraction`] for Synchronize or
/// Shed Skin when the move would newly paralyse the defender.
pub fn ensure_admissible(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<(), BattleError> {
    if ensure_resolvable(dex, move_id).is_err() {
        return Ok(());
    }
    let Some(move_type) = dex.move_data(move_id)?.move_type.battle_type() else {
        return Ok(());
    };
    if defender.ability() == AbilityId::LIMBER
        || defender_is_immune(move_type, defender)
        || !defender.status1().is_healthy()
    {
        return Ok(());
    }
    match defender.ability() {
        AbilityId::SHED_SKIN => Err(BattleError::UnportedAbilityInteraction(
            AbilityId::SHED_SKIN,
        )),
        AbilityId::SYNCHRONIZE if attacker.status1().is_healthy() => Err(
            BattleError::UnportedAbilityInteraction(AbilityId::SYNCHRONIZE),
        ),
        _ => Ok(()),
    }
}

/// The result of resolving an [`EFFECT_PARALYZE`] move, before any mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParalyzeOutcome {
    /// Limber protected the defender.
    LimberProtected,
    /// The defender's type is immune to the move.
    Immune,
    /// The defender is already [`crate::status1::Status1::Paralysed`].
    AlreadyParalysed,
    /// The defender already carries a primary status other than paralysis.
    AlreadyStatused,
    /// The accuracy check missed.
    Miss,
    /// The move connected; the caller must inflict
    /// [`crate::status1::Status1::Paralysed`].
    Applied,
}

/// Resolves one [`EFFECT_PARALYZE`] move against `defender` without mutating
/// either battler.
///
/// Limber, type immunity, an existing primary status, and unsupported ability
/// interactions are resolved before accuracy. Only the accuracy check can
/// consume RNG.
///
/// # Errors
///
/// Returns the errors from [`ensure_resolvable`] or [`ensure_admissible`].
/// Admission completes before any draw.
pub fn resolve_paralyze_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<ParalyzeOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;

    // Upstream checks Limber before type calculation, so preserve that order
    // even though the outcome does not need the move's type.
    if defender.ability() == AbilityId::LIMBER {
        return Ok(ParalyzeOutcome::LimberProtected);
    }

    let move_data = dex.move_data(move_id)?;
    let move_type = move_data
        .move_type
        .battle_type()
        .ok_or(BattleError::UnsupportedMoveType(move_id))?;

    if defender_is_immune(move_type, defender) {
        return Ok(ParalyzeOutcome::Immune);
    }
    if defender.status1().is_paralysed() {
        return Ok(ParalyzeOutcome::AlreadyParalysed);
    }
    if !defender.status1().is_healthy() {
        return Ok(ParalyzeOutcome::AlreadyStatused);
    }
    ensure_admissible(dex, move_id, attacker, defender)?;

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
