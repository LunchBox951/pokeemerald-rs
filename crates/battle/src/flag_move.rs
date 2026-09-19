//! Resolution for Splash, Focus Energy, and Charge.
//!
//! These self-targeting moves skip accuracy and damage and never consume
//! battle RNG. Splash changes no battle state. Focus Energy fails when already
//! active. Charge always activates or refreshes its timer and does not raise
//! Special Defense in Gen III (`data/battle_scripts_1.s:2297`-`:2306`).

use assets::{MoveEffect, MoveId};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

/// Focus Energy's move effect.
pub const EFFECT_FOCUS_ENERGY: MoveEffect = MoveEffect(47);

/// Splash's move effect.
pub const EFFECT_SPLASH: MoveEffect = MoveEffect(85);

/// Charge's move effect.
pub const EFFECT_CHARGE: MoveEffect = MoveEffect(174);

const FLAG_MOVE_EFFECTS: [MoveEffect; 3] = [EFFECT_FOCUS_ENERGY, EFFECT_SPLASH, EFFECT_CHARGE];

/// Returns whether this module supports `effect`.
#[must_use]
pub fn is_flag_move_effect(effect: MoveEffect) -> bool {
    FLAG_MOVE_EFFECTS.contains(&effect)
}

/// The state change and message produced by a flag-only move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlagMoveOutcome {
    /// Leaves state unchanged and reports "But nothing happened!".
    NothingHappened,
    /// Activates Focus Energy and reports that the user is getting pumped.
    GettingPumped,
    /// Leaves state unchanged and reports "But it failed!".
    Failed,
    /// Activates or refreshes Charge and reports that the user is charging power.
    ChargingPower,
}

/// Validates that `move_id` has a supported flag-only effect.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if the move's effect is unsupported.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    if is_flag_move_effect(dex.move_data(move_id)?.effect) {
        Ok(())
    } else {
        Err(BattleError::UnsupportedMoveEffect(move_id))
    }
}

/// Resolves a flag-only move without mutating the attacker or consuming RNG.
/// The caller applies the state change described by the returned outcome.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`] or [`BattleError::UnsupportedMoveEffect`]
/// under the same conditions as [`ensure_resolvable`].
pub fn resolve_flag_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
) -> Result<FlagMoveOutcome, BattleError> {
    let effect = dex.move_data(move_id)?.effect;
    match effect {
        EFFECT_SPLASH => Ok(FlagMoveOutcome::NothingHappened),
        EFFECT_FOCUS_ENERGY if attacker.volatiles().focus_energy => Ok(FlagMoveOutcome::Failed),
        EFFECT_FOCUS_ENERGY => Ok(FlagMoveOutcome::GettingPumped),
        EFFECT_CHARGE => Ok(FlagMoveOutcome::ChargingPower),
        _ => Err(BattleError::UnsupportedMoveEffect(move_id)),
    }
}

#[cfg(test)]
#[path = "flag_move/tests.rs"]
mod tests;
