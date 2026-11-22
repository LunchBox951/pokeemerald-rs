//! Admission and epilogue draws for multi-hit moves: the variable-count
//! `EFFECT_MULTI_HIT` and the fixed two-hit `EFFECT_DOUBLE_HIT`.
//!
//! [`resolve_multi_hit`] validates the move, checks accuracy once, and either
//! classifies the failed roll or selects the hit limit: rolled for the
//! variable-count effect, a constant two with no draw for the fixed one
//! (`BattleScript_EffectDoubleHit`, `data/battle_scripts_1.s:839`-`:847`).
//! [`crate::battle::Battle`] owns the
//! interruptible loop so it can read both battlers' live HP before each hit
//! and avoid drawing work for hits that will not run. After that loop,
//! [`spend_multi_hit_effect_chance_draw`] spends one trailing draw for the
//! move, regardless of how many hits ran.

use assets::{MoveEffect, MoveId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::{accuracy_roll, classify_accuracy_failure, HitOutcome};
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;
use crate::secondary::spend_effect_chance_draw;

const HIT_COUNT_OFFSET_MASK: u16 = 0b11;
const HIT_COUNT_REDRAW_THRESHOLD: u8 = 2;

/// The effect ID resolved by the variable-count multi-hit pipeline.
pub const EFFECT_MULTI_HIT: MoveEffect = MoveEffect::MULTI_HIT;

/// The effect ID resolved by the fixed two-hit pipeline (Double Kick,
/// Bonemerang).
pub const EFFECT_DOUBLE_HIT: MoveEffect = MoveEffect::DOUBLE_HIT;

/// The constant hit limit of [`EFFECT_DOUBLE_HIT`] moves, set without a draw
/// by `setmultihitcounter 2` (`src/battle_script_commands.c:7139`-`:7142`).
pub const FIXED_DOUBLE_HIT_COUNT: u8 = 2;

/// The fewest hits a variable-count multi-hit move can attempt.
pub const MIN_HITS: u8 = 2;

/// The most hits a variable-count multi-hit move can attempt.
pub const MAX_HITS: u8 = 5;

/// Returns whether `effect` uses the multi-hit pipeline, whether its hit
/// count is rolled ([`EFFECT_MULTI_HIT`]) or fixed ([`EFFECT_DOUBLE_HIT`]).
#[must_use]
pub fn is_multi_hit_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_MULTI_HIT || effect == EFFECT_DOUBLE_HIT
}

fn draw_hit_count_offset(rng: &mut impl BattleRng) -> u8 {
    (rng.next_u16() & HIT_COUNT_OFFSET_MASK) as u8
}

/// Rolls a hit count from [`MIN_HITS`] through [`MAX_HITS`].
///
/// Emerald's variable-count branch redraws when its first two-bit result is
/// at least two (`src/battle_script_commands.c:7147`-`:7151`). This function
/// preserves that one-or-two-draw ordering instead of sampling the final
/// distribution directly.
#[must_use]
pub fn roll_hit_count(rng: &mut impl BattleRng) -> u8 {
    let first_hit_offset = draw_hit_count_offset(rng);
    let hit_offset = if first_hit_offset < HIT_COUNT_REDRAW_THRESHOLD {
        first_hit_offset
    } else {
        draw_hit_count_offset(rng)
    };
    MIN_HITS + hit_offset
}

/// Validates that `move_id` can enter the multi-hit pipeline without drawing.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if it is neither a variable-count
///   nor a fixed two-hit move.
/// - [`BattleError::UnsupportedMoveType`] if its type cannot participate in
///   battle calculations.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_multi_hit_effect)
}

/// [`resolve_multi_hit`]'s verdict for one multi-hit move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MultiHitAdmission {
    /// The accuracy roll failed, carrying `Cmd_accuracycheck`'s classified
    /// verdict rather than an unconditional [`HitOutcome::Miss`].
    Failed(HitOutcome),
    /// The move landed and may attempt up to `hit_limit` hits.
    Admitted {
        /// The hit limit: rolled from [`MIN_HITS`] through [`MAX_HITS`] for a
        /// variable-count move, [`FIXED_DOUBLE_HIT_COUNT`] for a fixed one.
        hit_limit: u8,
    },
}

/// Admits a multi-hit move with one accuracy draw, then selects its hit limit.
///
/// A failed accuracy roll returns [`MultiHitAdmission::Failed`] after that one
/// draw, classified by [`classify_accuracy_failure`] and so never rolling a
/// hit count. A successful admission of a variable-count move returns
/// [`MultiHitAdmission::Admitted`] after one or two additional count draws; a
/// fixed two-hit move returns it with [`FIXED_DOUBLE_HIT_COUNT`] and no
/// further draw.
/// The caller must process hits against live battle state before spending the
/// trailing draw with [`spend_multi_hit_effect_chance_draw`].
///
/// # Errors
///
/// Returns any error from [`ensure_resolvable`] without consuming RNG, or any
/// error from [`classify_accuracy_failure`] after the accuracy draw.
pub fn resolve_multi_hit(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<MultiHitAdmission, BattleError> {
    ensure_resolvable(dex, move_id)?;
    let move_lands = accuracy_roll(dex, move_id, attacker, defender, rng)?;
    if !move_lands {
        return Ok(MultiHitAdmission::Failed(classify_accuracy_failure(
            dex, move_id, defender,
        )?));
    }
    let hit_limit = if dex.move_data(move_id)?.effect == EFFECT_DOUBLE_HIT {
        FIXED_DOUBLE_HIT_COUNT
    } else {
        roll_hit_count(rng)
    };
    Ok(MultiHitAdmission::Admitted { hit_limit })
}

/// Spends the move's single effect-chance draw after its hit loop finishes.
///
/// # Errors
///
/// Returns any error from [`spend_effect_chance_draw`]. Neither multi-hit
/// effect is `EFFECT_POISON_HIT`, so the poison signal is always `false` and
/// the draw is discarded.
pub fn spend_multi_hit_effect_chance_draw(
    dex: &Dex,
    move_id: MoveId,
    any_hit_had_effect: bool,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    spend_effect_chance_draw(dex, move_id, any_hit_had_effect, defender, rng)?;
    Ok(())
}

#[cfg(test)]
#[path = "multi_hit/tests.rs"]
mod tests;
