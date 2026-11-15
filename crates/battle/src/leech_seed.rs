//! Validation, resolution, and the end-of-turn drain amount for Leech Seed.
//!
//! `BattleScript_EffectLeechSeed` spends PP and runs the accuracy check, then
//! `Cmd_setseeded` decides the result: an accuracy miss, an already-seeded
//! target, or a Grass-type target fails and nothing is recorded; otherwise the
//! target is seeded by the attacker's slot (`data/battle_scripts_1.s:1157`-
//! `:1170`, `src/battle_script_commands.c:6718`-`:6738`). The accuracy draw is
//! therefore spent even when the target is already seeded or Grass. A seeded
//! battler loses an eighth of its maximum HP at `ENDTURN_LEECH_SEED`
//! (`src/battle_util.c:1509`-`:1523`). Substitute and Liquid Ooze are not
//! modelled.

use assets::{MoveEffect, MoveId, Type};

use crate::accuracy::accuracy_check;
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;

/// Move effect of Leech Seed.
pub const EFFECT_LEECH_SEED: MoveEffect = MoveEffect::LEECH_SEED;

const RESIDUAL_DAMAGE_DENOMINATOR: u32 = 8;

/// Returns whether `effect` uses [`resolve_leech_seed_move`].
#[must_use]
pub fn is_leech_seed_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_LEECH_SEED
}

/// Validates move lookup, Leech Seed effect, and combat type before
/// resolution. Validation consumes no RNG.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`], [`BattleError::UnsupportedMoveEffect`],
/// or [`BattleError::UnsupportedMoveType`] for the corresponding unsupported
/// move property.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_leech_seed_effect)
}

/// The result of resolving one Leech Seed move, before any mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeechSeedOutcome {
    /// The accuracy check missed, so `Cmd_setseeded` saw `MOVE_RESULT_MISSED`
    /// (`src/battle_script_commands.c:6720`-`:6724`).
    Miss,
    /// The target already carried a seed: the move is marked missed and keeps
    /// the original seeder (`src/battle_script_commands.c:6720`-`:6724`).
    AlreadySeeded,
    /// The target is a Grass type: the move is marked missed with the
    /// "doesn't affect" message (`src/battle_script_commands.c:6725`-`:6729`).
    GrassImmune,
    /// The move connected; the caller must record the attacker as the
    /// target's seeder.
    Applied,
}

/// Resolves one Leech Seed move against `defender` without mutating either
/// battler.
///
/// Draws exactly one accuracy roll in every outcome; the failure guards run
/// after it, and nothing else is drawn.
///
/// # Errors
///
/// Returns the errors from [`ensure_resolvable`].
pub fn resolve_leech_seed_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<LeechSeedOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;

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
        return Ok(LeechSeedOutcome::Miss);
    }
    if defender.volatiles().seeded() {
        return Ok(LeechSeedOutcome::AlreadySeeded);
    }
    if defender.types().contains(&Type::Grass) {
        return Ok(LeechSeedOutcome::GrassImmune);
    }
    Ok(LeechSeedOutcome::Applied)
}

/// `ENDTURN_LEECH_SEED`'s damage for a seeded battler with `max_hp`: an
/// eighth of it, never less than one, with no RNG draw
/// (`src/battle_util.c:1516`-`:1518`).
#[must_use]
pub const fn leech_seed_residual_damage(max_hp: u32) -> u32 {
    let damage = max_hp / RESIDUAL_DAMAGE_DENOMINATOR;
    if damage == 0 {
        1
    } else {
        damage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residual_damage_is_an_eighth_of_max_hp_floored_to_one() {
        assert_eq!(leech_seed_residual_damage(120), 15);
        assert_eq!(
            leech_seed_residual_damage(105),
            13,
            "integer division truncates"
        );
        assert_eq!(leech_seed_residual_damage(8), 1);
        assert_eq!(leech_seed_residual_damage(7), 1);
        assert_eq!(leech_seed_residual_damage(1), 1);
    }

    #[test]
    fn only_leech_seed_uses_the_pipeline() {
        let dex = Dex::new();
        assert!(ensure_resolvable(&dex, MoveId::LEECH_SEED).is_ok());
        assert_eq!(
            ensure_resolvable(&dex, MoveId::SPLASH),
            Err(BattleError::UnsupportedMoveEffect(MoveId::SPLASH))
        );
    }
}
