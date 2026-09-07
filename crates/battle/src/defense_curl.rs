//! Admission and resolution for Defense Curl.
//!
//! `BattleScript_EffectDefenseCurl` (`data/battle_scripts_1.s:2014`-`:2025`)
//! inlines its own copy of the Defense+1 raise that
//! `BattleScript_EffectDefenseUp` instead reaches through
//! `goto BattleScript_EffectStatUp` (`:479`-`:481`), because
//! `setdefensecurlbit` (`src/battle_script_commands.c:8858`-`:8862`) must run
//! before the raise -- the only reason this effect gets its own pipeline
//! instead of a [`crate::stat_change::STAT_CHANGE_EFFECTS`] row. The raise
//! itself draws no RNG, matching every other raising effect
//! ([`crate::stat_change`]'s module docs): no accuracy check, no
//! critical-hit roll, no damage roll, no secondary-effect roll.

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

/// The Defense raise Defense Curl applies, after the volatile write the
/// caller must perform first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefenseCurlOutcome {
    /// The Defense+1 raise, in [`crate::stat_change::StatChangeEffect`]'s own
    /// shape so a caller can push the same event fields
    /// `execute_stat_change_move` does.
    pub change: StatChangeEffect,
    /// The clamped Defense stage after applying the raise.
    pub new_stage: StatStage,
    /// Whether Defense was already at its maximum stage and did not move.
    pub capped: bool,
}

/// Resolves Defense Curl's stat raise without mutating `attacker` or
/// consuming RNG. The caller writes
/// [`crate::volatile::Volatiles::set_defense_curl`] **before** applying this
/// outcome, even when `capped` is `true` -- `setdefensecurlbit` precedes
/// `statbuffchange` in the script regardless of its result
/// (`data/battle_scripts_1.s:2017`-`:2019`).
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
