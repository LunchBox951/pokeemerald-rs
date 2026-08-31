//! Admission and resolution for ordinary single-target damaging moves.
//!
//! Resolution consumes accuracy, critical-hit, damage-variance, and trailing
//! effect-chance draws in that order. Accuracy bypasses and critical-hit
//! suppression omit their respective draws. A miss stops immediately, while a
//! type immunity still consumes the damage and effect-chance draws. Struggle
//! skips the trailing effect-chance draw.

use assets::{MoveEffect, MoveId, Type};

use crate::ability::{huge_power_attack, pinch_boosts_power, suppresses_critical_hits};
use crate::accuracy::accuracy_check;
use crate::critical::{crit_adjusted_stages, crit_roll, crit_stage};
use crate::damage::{
    apply_damage_roll, apply_dual_type_effectiveness, apply_stab, base_damage, has_stab, BattleRng,
    DamageInput, MoveCategory, Weather, STRUGGLE,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::secondary::spend_effect_chance_draw;

const EFFECT_HIT: MoveEffect = MoveEffect(0);
const EFFECT_SPEED_UP: MoveEffect = MoveEffect(12);
const EFFECT_SPECIAL_DEFENSE_UP: MoveEffect = MoveEffect(14);
const EFFECT_ACCURACY_UP: MoveEffect = MoveEffect(15);
const EFFECT_ALWAYS_HIT: MoveEffect = MoveEffect(17);
const EFFECT_SPECIAL_ATTACK_DOWN: MoveEffect = MoveEffect(21);
const EFFECT_SPECIAL_DEFENSE_DOWN: MoveEffect = MoveEffect(22);
const EFFECT_HIGH_CRITICAL: MoveEffect = MoveEffect(43);
const EFFECT_ACCURACY_UP_2: MoveEffect = MoveEffect(55);
const EFFECT_EVASION_UP_2: MoveEffect = MoveEffect(56);
const EFFECT_SPECIAL_ATTACK_DOWN_2: MoveEffect = MoveEffect(61);
const EFFECT_ACCURACY_DOWN_2: MoveEffect = MoveEffect(63);
const EFFECT_EVASION_DOWN_2: MoveEffect = MoveEffect(64);
const EFFECT_EVASION_DOWN_HIT: MoveEffect = MoveEffect(74);
const EFFECT_VITAL_THROW: MoveEffect = MoveEffect(78);
const EFFECT_UNUSED_60: MoveEffect = MoveEffect(96);
const EFFECT_QUICK_ATTACK: MoveEffect = MoveEffect(103);
const EFFECT_UNUSED_6E: MoveEffect = MoveEffect(110);
const EFFECT_UNUSED_83: MoveEffect = MoveEffect(131);
const EFFECT_UNUSED_8D: MoveEffect = MoveEffect(141);
const EFFECT_UNUSED_A3: MoveEffect = MoveEffect(163);

const ORDINARY_HIT_EFFECTS: [MoveEffect; 21] = [
    EFFECT_HIT,
    EFFECT_SPEED_UP,
    EFFECT_SPECIAL_DEFENSE_UP,
    EFFECT_ACCURACY_UP,
    EFFECT_ALWAYS_HIT,
    EFFECT_SPECIAL_ATTACK_DOWN,
    EFFECT_SPECIAL_DEFENSE_DOWN,
    EFFECT_HIGH_CRITICAL,
    EFFECT_ACCURACY_UP_2,
    EFFECT_EVASION_UP_2,
    EFFECT_SPECIAL_ATTACK_DOWN_2,
    EFFECT_ACCURACY_DOWN_2,
    EFFECT_EVASION_DOWN_2,
    EFFECT_EVASION_DOWN_HIT,
    EFFECT_VITAL_THROW,
    EFFECT_UNUSED_60,
    EFFECT_QUICK_ATTACK,
    EFFECT_UNUSED_6E,
    EFFECT_UNUSED_83,
    EFFECT_UNUSED_8D,
    EFFECT_UNUSED_A3,
];

const CRITICAL_DAMAGE_MULTIPLIER: u32 = 2;
const CHARGE_DAMAGE_MULTIPLIER: u32 = 2;

/// The result of resolving one move against one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HitOutcome {
    /// The accuracy check failed.
    Miss,
    /// The move connected, but the target was immune.
    NoEffect,
    /// The move connected and dealt damage.
    Hit {
        /// HP of damage dealt.
        damage: u32,
        /// Whether the hit was critical.
        is_critical: bool,
    },
}

/// Returns whether `effect` uses the ordinary-hit script without an engine
/// special case.
///
/// False Swipe and Pursuit share the script but are excluded because upstream
/// clamps or redirects them outside it (`battle_script_commands.c:1683`,
/// `:8745`, `:9854`).
#[must_use]
pub fn is_ordinary_hit_effect(effect: MoveEffect) -> bool {
    ORDINARY_HIT_EFFECTS.contains(&effect)
}

/// Validates that `move_id` can use [`resolve_hit`].
///
/// Validation completes before any state or RNG is touched.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`], [`BattleError::NonDamagingMove`],
/// [`BattleError::UnsupportedMoveType`], or
/// [`BattleError::UnsupportedMoveEffect`] for the corresponding unsupported
/// move property.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    let move_data = dex.move_data(move_id)?;
    if move_data.power == 0 {
        return Err(BattleError::NonDamagingMove(move_id));
    }
    if move_data.move_type.battle_type().is_none() {
        return Err(BattleError::UnsupportedMoveType(move_id));
    }
    if !is_ordinary_hit_effect(move_data.effect) && move_id != STRUGGLE {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    Ok(())
}

/// Runs the accuracy stage of a damaging move.
///
/// Always-hit effects consume no RNG value. Other effects consume one.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`] before drawing when `move_id` is not
/// in `dex`.
pub fn accuracy_roll(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<bool, BattleError> {
    let move_data = dex.move_data(move_id)?;
    Ok(accuracy_check(
        move_data.accuracy,
        move_data.effect,
        attacker.stages().accuracy,
        defender.stages().evasion,
        rng,
    ))
}

/// Damage after critical-hit, STAB, and type rules but before damage variance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawDamage {
    /// Damage before variance, or zero when the target is immune.
    pub damage: u32,
    /// Whether the hit was critical.
    pub is_critical: bool,
}

fn roll_critical_hit(
    move_effect: MoveEffect,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    critical_hits_suppressed: bool,
    rng: &mut impl BattleRng,
) -> bool {
    let defender_suppresses_critical_hits = suppresses_critical_hits(defender.ability());
    if critical_hits_suppressed || defender_suppresses_critical_hits {
        return false;
    }

    let critical_hit_stage = crit_stage(move_effect, attacker.volatiles().focus_energy);
    crit_roll(critical_hit_stage, rng)
}

fn damage_input(
    move_power: u8,
    move_type: Type,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    is_critical: bool,
) -> DamageInput {
    let move_category = MoveCategory::for_type(move_type);
    let (raw_attack, attack_stage) = attacker.attacking_stat(move_category);
    let ability_adjusted_attack = huge_power_attack(attacker.ability(), move_category, raw_attack);
    let (defense, defense_stage) = defender.defending_stat(move_category);
    let (attack_stage, defense_stage) =
        crit_adjusted_stages(attack_stage, defense_stage, is_critical);

    DamageInput {
        attacker_level: attacker.level(),
        power: u32::from(move_power),
        move_type,
        attack_stat: ability_adjusted_attack,
        attack_stage,
        defense_stat: defense,
        defense_stage,
        attacker_burned: false,
        reflect: false,
        light_screen: false,
        weather: Weather::None,
        is_solar_beam: false,
        attacker_pinch_boost: pinch_boosts_power(
            attacker.ability(),
            move_type,
            attacker.current_hp(),
            attacker.stats().max_hp,
        ),
    }
}

/// Calculates damage through type effectiveness, stopping before damage
/// variance.
///
/// The critical-hit draw is skipped when either the caller or defender's
/// ability suppresses critical hits.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`] or
/// [`BattleError::UnsupportedMoveType`] before drawing.
pub fn damage_before_roll(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    critical_hits_suppressed: bool,
    rng: &mut impl BattleRng,
) -> Result<RawDamage, BattleError> {
    let move_data = dex.move_data(move_id)?;
    let move_type = move_data
        .move_type
        .battle_type()
        .ok_or(BattleError::UnsupportedMoveType(move_id))?;

    let is_critical = roll_critical_hit(
        move_data.effect,
        attacker,
        defender,
        critical_hits_suppressed,
        rng,
    );
    let input = damage_input(move_data.power, move_type, attacker, defender, is_critical);

    let base_damage = base_damage(&input);
    let damage_after_critical = if is_critical {
        base_damage * CRITICAL_DAMAGE_MULTIPLIER
    } else {
        base_damage
    };
    let damage_after_charge = if attacker.volatiles().charged_up() && move_type == Type::Electric {
        damage_after_critical * CHARGE_DAMAGE_MULTIPLIER
    } else {
        damage_after_critical
    };
    let damage = if move_id == STRUGGLE {
        damage_after_charge
    } else {
        let damage_after_stab = apply_stab(
            damage_after_charge,
            has_stab(attacker.types(), move_id, move_type),
        );
        apply_dual_type_effectiveness(damage_after_stab, move_type, defender.types())
    };

    Ok(RawDamage {
        damage,
        is_critical,
    })
}

/// Calculates a landed move's outcome through damage variance.
///
/// This always consumes a damage-variance draw, including for an immune
/// target. It consumes a preceding critical-hit draw unless critical hits are
/// suppressed.
///
/// # Errors
///
/// Returns the errors from [`damage_before_roll`] before the damage-variance
/// draw.
pub fn damage_core(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    critical_hits_suppressed: bool,
    rng: &mut impl BattleRng,
) -> Result<HitOutcome, BattleError> {
    let raw_damage = damage_before_roll(
        dex,
        move_id,
        attacker,
        defender,
        critical_hits_suppressed,
        rng,
    )?;
    let damage = apply_damage_roll(raw_damage.damage, rng);

    if damage == 0 {
        Ok(HitOutcome::NoEffect)
    } else {
        Ok(HitOutcome::Hit {
            damage,
            is_critical: raw_damage.is_critical,
        })
    }
}

/// Resolves an ordinary hit against one target.
///
/// A landed ordinary move consumes accuracy, critical-hit, damage-variance,
/// and effect-chance draws in order, subject to the documented bypasses.
/// Struggle omits the effect-chance stage because its recoil effect is outside
/// this function's damage-only contract.
///
/// # Errors
///
/// Returns the errors from [`ensure_resolvable`], [`accuracy_roll`],
/// [`damage_core`], or [`spend_effect_chance_draw`]. Admission completes
/// before any draw.
pub fn resolve_hit(
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

    let outcome = damage_core(
        dex,
        move_id,
        attacker,
        defender,
        critical_hits_suppressed,
        rng,
    )?;

    if move_id != STRUGGLE {
        let hit_had_effect = outcome != HitOutcome::NoEffect;
        spend_effect_chance_draw(dex, move_id, hit_had_effect, rng)?;
    }

    Ok(outcome)
}

#[cfg(test)]
#[path = "hit/tests.rs"]
mod tests;
