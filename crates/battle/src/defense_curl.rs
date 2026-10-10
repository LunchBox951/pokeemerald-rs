//! Admission and resolution for Defense Curl.

use assets::{MoveEffect, MoveId};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::stat_change::{
    stage_of, ChangedStat, StatChangeDirection, StatChangeEffect, StatChangeMagnitude,
};
use crate::stat_stage::StatStage;

/// Defense Curl's move effect.
pub const EFFECT_DEFENSE_CURL: MoveEffect = MoveEffect(156);

/// Returns whether this module supports `effect`.
#[must_use]
pub fn is_defense_curl_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_DEFENSE_CURL
}

/// Validates that `move_id` is Defense Curl.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if the move's effect is not
///   [`EFFECT_DEFENSE_CURL`].
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    if is_defense_curl_effect(dex.move_data(move_id)?.effect) {
        Ok(())
    } else {
        Err(BattleError::UnsupportedMoveEffect(move_id))
    }
}

/// Defense Curl's capped one-stage Defense raise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefenseCurlOutcome {
    /// The one-stage Defense raise.
    pub change: StatChangeEffect,
    /// The clamped Defense stage after applying the raise.
    pub new_stage: StatStage,
    /// Whether Defense was already at its maximum stage and did not move.
    pub capped: bool,
}

/// Resolves Defense Curl's stat raise without mutating `attacker` or
/// consuming RNG. The caller sets
/// [`crate::volatile::Volatiles::set_defense_curl`] before applying the raise,
/// even when `capped`
/// (`pokeemerald/data/battle_scripts_1.s:2018`-`:2020`,
/// `pokeemerald/src/battle_script_commands.c:8858`-`:8862`).
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`] or [`BattleError::UnsupportedMoveEffect`]
/// under the same conditions as [`ensure_resolvable`].
pub fn resolve_defense_curl_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
) -> Result<DefenseCurlOutcome, BattleError> {
    if !is_defense_curl_effect(dex.move_data(move_id)?.effect) {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    let change = StatChangeEffect {
        stat: ChangedStat::Defense,
        magnitude: StatChangeMagnitude::One,
        direction: StatChangeDirection::Raise,
    };
    let current_stage = stage_of(attacker, change.stat);
    let capped = current_stage == change.cap();
    let new_stage = current_stage.saturating_add(change.delta());
    Ok(DefenseCurlOutcome {
        change,
        new_stage,
        capped,
    })
}

#[cfg(test)]
#[path = "defense_curl/tests.rs"]
mod tests;
