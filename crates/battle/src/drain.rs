//! Admission, damage, and HP transfer for draining moves.
//!
//! Resolution consumes an accuracy draw, a critical-hit draw unless critical
//! hits are suppressed, and a damage-variance draw in that order. It omits the
//! ordinary hit pipeline's trailing effect-chance draw. After damage is clamped
//! to the target's remaining HP, the drain magnitude is half of the HP removed,
//! with a minimum of one. The caller clamps healing to missing HP or, for Liquid
//! Ooze, clamps damage to the attacker's current HP.

use assets::species::AbilityId;
use assets::{MoveEffect, MoveId};

use crate::ability::inverts_drain;
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::{accuracy_roll, damage_core, HitOutcome};
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;

/// The move effect handled by draining-move resolution.
pub const EFFECT_ABSORB: MoveEffect = MoveEffect(3);

/// Whether `effect` uses the draining-move pipeline.
#[must_use]
pub fn is_drain_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_ABSORB
}

/// The requested attacker HP transfer after a draining move deals damage.
///
/// The caller clamps and applies this transfer before resolving the target's
/// faint because upstream faints the attacker before the target
/// (`pokeemerald/data/battle_scripts_1.s:358-:359`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DrainOutcome {
    /// HP requested before the caller clamps healing or damage. Always
    /// positive.
    pub amount: u32,
    /// Whether Liquid Ooze turns `amount` into attacker damage instead of
    /// healing.
    pub inverted: bool,
}

/// Returns half the target's actual HP loss, with a minimum of one.
///
/// Zero HP loss returns zero. The caller must pass damage after the target-HP
/// clamp, not the formula result. Upstream halves `gHpDealt` — the HP the
/// target actually lost after `Cmd_datahpupdate`'s clamp
/// (`pokeemerald/src/battle_script_commands.c:1928-1932`) — before applying
/// the one-HP floor (`:6925-6932`).
#[must_use]
pub const fn drain_amount(target_hp_lost: u32) -> u32 {
    if target_hp_lost == 0 {
        return 0;
    }
    let halved_hp_loss = target_hp_lost / 2;
    if halved_hp_loss == 0 {
        1
    } else {
        halved_hp_loss
    }
}

/// Derives the requested attacker HP transfer from the target's actual HP loss.
///
/// Returns `None` when the target lost no HP. Liquid Ooze changes the direction
/// after the one-HP floor is applied, so it preserves the magnitude
/// (`pokeemerald/data/battle_scripts_1.s:343-350`). This stage consumes no RNG
/// draw.
#[must_use]
pub fn resolve_drain(target_hp_lost: u32, target_ability: AbilityId) -> Option<DrainOutcome> {
    let amount = drain_amount(target_hp_lost);
    if amount == 0 {
        return None;
    }
    Some(DrainOutcome {
        amount,
        inverted: inverts_drain(target_ability),
    })
}

/// Validates that `move_id` can use [`resolve_drain_move`].
///
/// Validation completes before any state or RNG is touched.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`],
/// [`BattleError::UnsupportedMoveEffect`], or
/// [`BattleError::UnsupportedMoveType`] for the corresponding unsupported
/// move property.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_drain_effect)
}

/// Resolves the damage half of a draining move against one target.
///
/// A landed move consumes accuracy, critical-hit, and damage-variance draws in
/// order, subject to critical-hit suppression. It omits the effect-chance draw
/// because the upstream drain script exits before that stage
/// (`pokeemerald/data/battle_scripts_1.s:323-360`). The caller clamps the
/// returned damage to the target's remaining HP before passing the HP removed
/// to [`resolve_drain`].
///
/// # Errors
///
/// Returns the errors from [`ensure_resolvable`], [`accuracy_roll`], or
/// [`damage_core`]. Admission completes before any draw.
pub fn resolve_drain_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    critical_hits_suppressed: bool,
    rng: &mut impl BattleRng,
) -> Result<HitOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;
    if !accuracy_roll(dex, move_id, attacker, defender, rng)? {
        return Ok(HitOutcome::Miss);
    }
    damage_core(
        dex,
        move_id,
        attacker,
        defender,
        critical_hits_suppressed,
        rng,
    )
}

#[cfg(test)]
#[path = "drain/tests.rs"]
mod tests;
