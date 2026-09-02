//! Admission and epilogue draws for variable-count multi-hit moves.
//!
//! [`resolve_multi_hit`] validates the move, checks accuracy once, and rolls
//! the hit limit. [`crate::battle::Battle`] owns the
//! interruptible loop so it can read both battlers' live HP before each hit
//! and avoid drawing work for hits that will not run. After that loop,
//! [`spend_multi_hit_effect_chance_draw`] spends one trailing draw for the
//! move, regardless of how many hits ran.

use assets::{MoveEffect, MoveId};

use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::accuracy_roll;
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;
use crate::secondary::spend_effect_chance_draw;

const HIT_COUNT_OFFSET_MASK: u16 = 0b11;
const HIT_COUNT_REDRAW_THRESHOLD: u8 = 2;

/// The effect ID resolved by the variable-count multi-hit pipeline.
pub const EFFECT_MULTI_HIT: MoveEffect = MoveEffect(29);

/// The fewest hits a variable-count multi-hit move can attempt.
pub const MIN_HITS: u8 = 2;

/// The most hits a variable-count multi-hit move can attempt.
pub const MAX_HITS: u8 = 5;

/// Returns whether `effect` uses the variable-count multi-hit pipeline.
#[must_use]
pub fn is_multi_hit_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_MULTI_HIT
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
/// - [`BattleError::UnsupportedMoveEffect`] if it is not a variable-count
///   multi-hit move.
/// - [`BattleError::UnsupportedMoveType`] if its type cannot participate in
///   battle calculations.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_multi_hit_effect)
}

/// Admits a multi-hit move with one accuracy draw, then rolls its hit limit.
///
/// Returns `None` after the accuracy draw on a miss. A successful admission
/// returns the hit limit after one or two additional count draws. The caller
/// must process hits against live battle state before spending the trailing
/// draw with [`spend_multi_hit_effect_chance_draw`].
///
/// # Errors
///
/// Returns any error from [`ensure_resolvable`] without consuming RNG.
pub fn resolve_multi_hit(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<Option<u8>, BattleError> {
    ensure_resolvable(dex, move_id)?;
    let move_lands = accuracy_roll(dex, move_id, attacker, defender, rng)?;
    if !move_lands {
        return Ok(None);
    }
    let hit_limit = roll_hit_count(rng);
    Ok(Some(hit_limit))
}

/// Spends the move's single effect-chance draw after its hit loop finishes.
///
/// # Errors
///
/// Returns any error from [`spend_effect_chance_draw`]. Variable-count
/// multi-hit moves have no ported secondary-effect trampoline, so their draw
/// is discarded.
pub fn spend_multi_hit_effect_chance_draw(
    dex: &Dex,
    move_id: MoveId,
    any_hit_had_effect: bool,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    spend_effect_chance_draw(dex, move_id, any_hit_had_effect, rng)
}

#[cfg(test)]
#[path = "multi_hit/tests.rs"]
mod tests;
