//! Validation and resolution for Attract, and the infatuation coin.
//!
//! `BattleScript_EffectAttract` spends PP, runs the accuracy check, then
//! `Cmd_tryinfatuating` decides the result (`data/battle_scripts_1.s:1645`-
//! `:1655`, `src/battle_script_commands.c:7654`-`:7697`). An Oblivious target
//! is protected first; otherwise a pair of the same gender, a genderless
//! battler on either side, or an already-infatuated target fails; and any
//! other target is infatuated with the attacker. The accuracy draw is
//! therefore spent in every outcome, and nothing else is drawn.
//!
//! An infatuated battler draws one coin at its action's `CANCELER_IN_LOVE`
//! ([`draws_love_immobility`]). Cute Charm's contact trigger, the Oblivious
//! cure of an existing infatuation and the cure-attract held items are not
//! modelled.

use assets::{AbilityId, MoveEffect, MoveId};

use crate::accuracy::accuracy_check;
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::gender::Gender;
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;

/// Move effect of Attract.
pub const EFFECT_ATTRACT: MoveEffect = MoveEffect::ATTRACT;

/// Returns whether `effect` uses [`resolve_attract_move`].
#[must_use]
pub fn is_attract_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_ATTRACT
}

/// Validates move lookup, Attract effect, and combat type before resolution.
/// Validation consumes no RNG.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`], [`BattleError::UnsupportedMoveEffect`],
/// or [`BattleError::UnsupportedMoveType`] for the corresponding unsupported
/// move property.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_attract_effect)
}

/// The result of resolving one Attract move, before any mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttractOutcome {
    /// The accuracy check missed, so `MOVE_RESULT_MISSED` is set before the
    /// script's failure jump and its miss message wins
    /// (`src/battle_script_commands.c:1176`-`:1187`).
    Miss,
    /// The target's Oblivious prevents the attraction
    /// (`BattleScript_ObliviousPreventsAttraction`,
    /// `src/battle_script_commands.c:7676`-`:7680`). It outranks every gender
    /// and reapplication failure.
    ObliviousProtected,
    /// Both battlers are the same gender, either is genderless, or the target
    /// is already infatuated: the script jumps to `BattleScript_ButItFailed`
    /// (`src/battle_script_commands.c:7682`-`:7688`).
    Failed,
    /// The move connected; the caller must record the attacker's slot on the
    /// target's [`crate::volatile::Volatiles::set_infatuated_by`].
    Applied,
}

/// Whether Attract can infatuate `target` on behalf of `attacker` once its
/// accuracy has passed: opposite genders, neither genderless, target not
/// already infatuated.
fn can_infatuate(attacker: &BattlePokemon, target: &BattlePokemon) -> bool {
    let attacker_gender = attacker.gender();
    let target_gender = target.gender();
    attacker_gender != target_gender
        && attacker_gender != Gender::Genderless
        && target_gender != Gender::Genderless
        && !target.volatiles().infatuated()
}

/// Resolves one Attract move against `defender` without mutating either
/// battler.
///
/// Draws exactly one accuracy roll in every outcome; the guards run after it.
///
/// # Errors
///
/// Returns the errors from [`ensure_resolvable`].
pub fn resolve_attract_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<AttractOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;

    let move_data = dex.move_data(move_id)?;
    if !accuracy_check(
        move_data.accuracy,
        move_data.effect,
        move_data.move_type,
        attacker.ability(),
        attacker.stages().accuracy,
        defender.stages().evasion,
        rng,
    ) {
        return Ok(AttractOutcome::Miss);
    }
    if defender.ability() == AbilityId::OBLIVIOUS {
        return Ok(AttractOutcome::ObliviousProtected);
    }
    if can_infatuate(attacker, defender) {
        Ok(AttractOutcome::Applied)
    } else {
        Ok(AttractOutcome::Failed)
    }
}

/// `CANCELER_IN_LOVE`'s coin for an infatuated attacker: reports whether the
/// chosen move is cancelled. An even `Random() & 1` immobilizes the battler;
/// an odd one lets the move continue (`src/battle_util.c:2204`-`:2214`).
///
/// Draws exactly once. Callers must not call this for a battler that is not
/// infatuated, which draws nothing.
#[must_use]
pub fn draws_love_immobility(rng: &mut impl BattleRng) -> bool {
    rng.next_u16().is_multiple_of(2)
}

#[cfg(test)]
#[path = "attract/tests.rs"]
mod tests;
