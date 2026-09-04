//! Admission and resolution for `BattleScript_EffectParalyze` (Thunder Wave,
//! Stun Spore, Glare).
//!
//! `data/battle_scripts_1.s:1007`-`:1032`: `attackcanceler`, `attackstring`,
//! `ppreduce`, then the `jumpifability BS_TARGET, ABILITY_LIMBER` guard
//! (`:1011`), then `typecalc`'s type-immunity guard, then the two
//! `jumpifstatus` status guards, and only then `accuracycheck`. The guards
//! run before the accuracy draw and consume no randomness of their own; a
//! successful check writes [`crate::status1::Status1::Paralysed`] with
//! `seteffectprimary`, not `seteffectwithchance`, so a landed hit spends no
//! further draw.
//!
//! The type-immunity guard's `jumpifmovehadnoeffect` lands on
//! `BattleScript_ButItFailed` (`:1014`), but that script only *adds*
//! `MOVE_RESULT_FAILED` to the `MOVE_RESULT_DOESNT_AFFECT_FOE` bit
//! `typecalc` already set (`battle_script_commands.c:1327`,
//! `:2058`-`:2061`); with both bits set and `MOVE_RESULT_MISSED` clear,
//! `Cmd_resultmessage` still reports the "doesn't affect" string
//! (`:2090`-`:2093`), the same as an ordinary immune hit. [`ParalyzeOutcome::Immune`]
//! is named, and mapped to [`crate::battle::BattleEvent::NoEffect`], to
//! match that resolved message rather than the script label it exits
//! through.
//!
//! `ABILITY_LIMBER` exits through `BattleScript_LimberProtected` (`:1034`-
//! `:1038`) instead, a distinct script that never reaches `typecalc` or
//! `accuracycheck`; unlike the immune exit above, its own printed message
//! (`gPRLZPreventionStringIds[B_MSG_ABILITY_PREVENTS_MOVE_STATUS]`,
//! `src/battle_message.c:1223`) names the ability, so it keeps its own
//! [`ParalyzeOutcome::LimberProtected`] outcome rather than collapsing into
//! [`ParalyzeOutcome::Immune`].
//!
//! Not ported: `jumpifstatus2 BS_TARGET, STATUS2_SUBSTITUTE` (`:1012`, no
//! Substitute), and `jumpifsideaffecting BS_TARGET, SIDE_STATUS_SAFEGUARD`
//! (`:1018`, no side conditions) — each is simply absent from this module,
//! so neither can newly fire before the checks this module does run. The
//! `STATUS1_ANY` guard at `:1016` (any *other* primary status blocks a
//! second one) is likewise never reached: this crate has no primary status
//! besides [`crate::status1::Status1::Paralysed`] to be already carrying.

use assets::{AbilityId, MoveEffect, MoveId, Type};

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
    /// The defender's ability is [`AbilityId::LIMBER`].
    LimberProtected,
    /// The defender's typing is immune to the move.
    Immune,
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
/// The Limber, type-immunity, and already-paralysed guards precede the
/// accuracy draw and consume no randomness; a landed hit needs only that one
/// draw, since `seteffectprimary` inflicts the status unconditionally once
/// reached.
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

    // `jumpifability BS_TARGET, ABILITY_LIMBER` (`data/battle_scripts_1.s:1011`)
    // exits before `typecalc` runs, so this precedes even the move-type lookup
    // below.
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
