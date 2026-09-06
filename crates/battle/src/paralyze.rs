//! Admission and resolution for `BattleScript_EffectParalyze` (Thunder Wave,
//! Stun Spore, Glare).
//!
//! The Limber, type-immunity, and already-paralysed guards
//! (`data/battle_scripts_1.s:1007`-`:1032`) run ahead of `accuracycheck` and
//! draw nothing of their own; a landed hit writes
//! [`crate::status1::Status1::Paralysed`] with `seteffectprimary`, not
//! `seteffectwithchance`, so it spends no further draw.
//!
//! [`ParalyzeOutcome::Immune`] maps to [`crate::battle::BattleEvent::NoEffect`]
//! because the type-immunity exit still resolves to the ordinary "doesn't
//! affect" message (`battle_script_commands.c:2090`-`:2093`). Limber exits
//! through a separate script that names the ability in its own message
//! (`BattleScript_LimberProtected`, `data/battle_scripts_1.s:1034`-`:1038`;
//! `gPRLZPreventionStringIds[B_MSG_ABILITY_PREVENTS_MOVE_STATUS]`,
//! `src/battle_message.c:1223`), so [`ParalyzeOutcome::LimberProtected`]
//! keeps its own outcome instead of collapsing into `Immune`.
//!
//! Not ported: `jumpifstatus2 BS_TARGET, STATUS2_SUBSTITUTE`
//! (`data/battle_scripts_1.s:1012`, no Substitute),
//! `jumpifsideaffecting BS_TARGET, SIDE_STATUS_SAFEGUARD` (`:1018`, no side
//! conditions), and the `STATUS1_ANY` guard at `:1016` (this crate has no
//! primary status besides [`crate::status1::Status1::Paralysed`] to already
//! be carrying).

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

/// Refuses an [`EFFECT_PARALYZE`] move that would newly paralyse a defender
/// whose ability this slice cannot follow past the infliction; see
/// [`resolve_paralyze_move`] for where this sits among the module's other
/// guards.
///
/// * Synchronize reflects the status onto the attacker
///   (`MOVEEND_SYNCHRONIZE_TARGET`, `src/battle_script_commands.c:4275`-`:4277`
///   via `src/battle_util.c:2971`-`:2985`); an attacker already paralysed is
///   admitted, since the reflection's `SetMoveEffect` re-entry then leaves
///   `statusChanged` false (`:2422`-`:2423`).
/// * Shed Skin rolls a one-in-three end-of-turn cure while its holder is
///   statused (`ABILITYEFFECT_ENDTURN`, `src/battle_util.c:2620`-`:2621`), a
///   draw [`crate::battle::Battle`]'s residual pass does not make.
/// * Guts and Marvel Scale read their own holder's `status1` inside
///   `CalculateBaseDamage` (`src/pokemon.c`) to raise a statused holder's
///   physical Attack or Defense, which
///   [`crate::pokemon::BattlePokemon::attacking_stat`] and
///   [`crate::pokemon::BattlePokemon::defending_stat`] do not model.
///
/// # Errors
///
/// [`BattleError::UnportedAbilityInteraction`], carrying the offending
/// ability, when the move would reach `seteffectprimary` against one.
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
        || defender.status1().is_paralysed()
    {
        return Ok(());
    }
    match defender.ability() {
        ability @ (AbilityId::SHED_SKIN | AbilityId::GUTS | AbilityId::MARVEL_SCALE) => {
            Err(BattleError::UnportedAbilityInteraction(ability))
        }
        AbilityId::SYNCHRONIZE if !attacker.status1().is_paralysed() => Err(
            BattleError::UnportedAbilityInteraction(AbilityId::SYNCHRONIZE),
        ),
        _ => Ok(()),
    }
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
/// The Limber, type-immunity and already-paralysed guards, and
/// [`ensure_admissible`]'s refusals behind them, precede the accuracy draw
/// and consume no randomness; a landed hit needs only that one draw, since
/// `seteffectprimary` inflicts the status unconditionally once reached.
///
/// # Errors
///
/// Returns the errors documented by [`ensure_resolvable`] and
/// [`ensure_admissible`], or [`BattleError::UnsupportedMoveType`] if the move
/// has no combat type. Admission completes before any draw.
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
    // The last line of defence behind the pre-turn screens, at the script's
    // own position: every earlier exit is modelled, `accuracycheck` is not yet
    // paid for.
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
