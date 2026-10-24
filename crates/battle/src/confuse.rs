//! Validation and resolution for moves that directly inflict confusion.
//!
//! Own Tempo and an already-active confusion are checked before accuracy and
//! consume no RNG; only an accuracy success draws the duration
//! (`pokeemerald/data/battle_scripts_1.s:907`-`:915`,
//! `pokeemerald/src/battle_script_commands.c:2533`-`:2544`).
//!
//! Confusion is a `status2` volatile, so there is no type-immunity check and
//! no Synchronize reflection (`pokeemerald/src/battle_util.c:2971`-`:2986`).
//! Substitute and Safeguard are not modeled.

use assets::{AbilityId, MoveEffect, MoveId};

use crate::accuracy::accuracy_check;
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;

/// Move effect shared by Supersonic, Confuse Ray, and Sweet Kiss.
pub const EFFECT_CONFUSE: MoveEffect = MoveEffect(49);

const CONFUSION_DURATION_OFFSET_MASK: u16 = 0b11;

const MIN_CONFUSION_TURNS: u8 = 2;

/// Returns whether `effect` uses [`resolve_confuse_move`].
#[must_use]
pub fn is_confuse_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_CONFUSE
}

/// Validates move lookup, confuse effect, and combat type before resolution.
/// Validation consumes no RNG.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`], [`BattleError::UnsupportedMoveEffect`],
/// or [`BattleError::UnsupportedMoveType`] for the corresponding unsupported
/// move property.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_confuse_effect)
}

/// Draws a confusion duration of 2..=5 actions, matching `(Random() % 4) + 2`
/// (`pokeemerald/src/battle_script_commands.c:2541`).
pub(crate) fn draw_confusion_duration(rng: &mut impl BattleRng) -> u8 {
    (rng.next_u16() & CONFUSION_DURATION_OFFSET_MASK) as u8 + MIN_CONFUSION_TURNS
}

/// The result of resolving one [`EFFECT_CONFUSE`] move, before any mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfuseOutcome {
    /// The defender's Own Tempo blocked the move
    /// (`BattleScript_OwnTempoPrevents`, `data/battle_scripts_1.s:4152`-`:4156`).
    OwnTempoProtected,
    /// The defender already carries an active confusion volatile
    /// (`BattleScript_AlreadyConfused`, `data/battle_scripts_1.s:918`-`:922`).
    AlreadyConfused,
    /// The accuracy check missed.
    Miss,
    /// The move connected; the caller must write `turns` to the defender's
    /// [`crate::volatile::Volatiles::set_confusion`].
    Applied {
        /// The rolled duration, 2..=5 actions.
        turns: u8,
    },
}

/// Resolves one [`EFFECT_CONFUSE`] move against `defender` without mutating
/// either battler.
///
/// Own Tempo and an already-active confusion are resolved before accuracy.
/// RNG is consumed by the accuracy check and, on success, the duration draw.
///
/// # Errors
///
/// Returns the errors from [`ensure_resolvable`].
pub fn resolve_confuse_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<ConfuseOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;

    if defender.ability() == AbilityId::OWN_TEMPO {
        return Ok(ConfuseOutcome::OwnTempoProtected);
    }
    if defender.volatiles().confused() {
        return Ok(ConfuseOutcome::AlreadyConfused);
    }

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
        return Ok(ConfuseOutcome::Miss);
    }

    Ok(ConfuseOutcome::Applied {
        turns: draw_confusion_duration(rng),
    })
}

#[cfg(test)]
#[path = "confuse/tests.rs"]
mod tests;
