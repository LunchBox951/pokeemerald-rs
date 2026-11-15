//! Admission and the power series for Rollout and Ice Ball.
//!
//! `EFFECT_ROLLOUT` runs `BattleScript_EffectRollout`: accuracy, then
//! `typecalc2`, then `Cmd_rolloutdamagecalculation`
//! (`data/battle_scripts_1.s:1596`-`:1606`). A failed accuracy roll and a
//! type-chart block both reach that command as `MOVE_RESULT_NO_EFFECT`,
//! which cancels the series. A landed attempt starts a five-attempt series on
//! the first use, counts it down, and releases it on the fifth
//! (`src/battle_script_commands.c:8536`-`:8571`). While the series runs the
//! battler's selection is forced to the move
//! ([`crate::battle::BattlerState::locked_move`]).

use assets::{MoveEffect, MoveId};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::move_gate::ensure_resolvable_effect;

/// Move effect of Rollout and Ice Ball.
pub const EFFECT_ROLLOUT: MoveEffect = MoveEffect::ROLLOUT;

/// The attempts one series lasts (`rolloutTimer = 5`,
/// `src/battle_script_commands.c:8548`).
pub const SERIES_ATTEMPTS: u8 = 5;

/// Returns whether `effect` uses the Rollout pipeline.
#[must_use]
pub fn is_rollout_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_ROLLOUT
}

/// Validates move lookup, Rollout effect, and combat type before resolution.
/// Validation consumes no RNG.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`], [`BattleError::UnsupportedMoveEffect`],
/// or [`BattleError::UnsupportedMoveType`] for the corresponding unsupported
/// move property.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_rollout_effect)
}

/// One landed Rollout attempt's power and the series state it leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RolloutAttempt {
    /// The dynamic base power (`gDynamicBasePower`): the move's power doubled
    /// once per earlier landed attempt, and once more under Defense Curl.
    /// A `u32` because the fifth attempt reaches 480, or 960 curled.
    pub power: u32,
    /// Attempts left after this one; zero means this attempt released the
    /// series.
    pub attempts_left: u8,
}

/// Advances a series by one landed attempt (`Cmd_rolloutdamagecalculation`,
/// `src/battle_script_commands.c:8547`-`:8565`).
///
/// `attempts_left_before` is the lock's remaining attempts, or `None` for the
/// first attempt of a series, which starts the timer at five before
/// decrementing it.
#[must_use]
pub fn next_attempt(
    base_power: u8,
    attempts_left_before: Option<u8>,
    defense_curl: bool,
) -> RolloutAttempt {
    let attempts_left = attempts_left_before
        .unwrap_or(SERIES_ATTEMPTS)
        .saturating_sub(1);
    let earlier_attempts = SERIES_ATTEMPTS - 1 - attempts_left;
    let series_power = u32::from(base_power) << earlier_attempts;
    RolloutAttempt {
        power: if defense_curl {
            series_power * 2
        } else {
            series_power
        },
        attempts_left,
    }
}

#[cfg(test)]
#[path = "rollout/tests.rs"]
mod tests;
