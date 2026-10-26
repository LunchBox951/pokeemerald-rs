//! The AI's battle-damage estimate (`AI_CalcDmg`) and the most-powerful-move
//! comparison built on it.

use assets::{MoveEffect, MoveId};

use super::scores::{MoveScores, PERCENT_SCALE};
use super::type_effectiveness::fold_ai_type_effectiveness;
use crate::damage::{apply_stab, base_damage, has_stab, DamageInput, MoveCategory, Weather};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::defender_levitate_blocked;
use crate::pokemon::{BattlePokemon, MAX_MON_MOVES};

const MINIMUM_DAMAGE: u32 = 1;
pub(super) const MINIMUM_DAMAGING_MOVE_POWER: u8 = 2;

/// Effect IDs `AI_TryToFaint` and `AI_CV_Sweeper`'s most-powerful-move
/// comparison treats as powerless (`sIgnoredPowerfulMoveEffects`,
/// `src/battle_ai_script_commands.c:266-280`): Explosion, Dream Eater, Razor
/// Wind, Sky Attack, Recharge, Skull Bash, Solar Beam, Spit Up, Focus Punch,
/// Superpower, Eruption, Overheat. A depleted slot holding one is admitted but
/// still sits in the comparison, so it must be skipped there.
const IGNORED_POWERFUL_MOVE_EFFECTS: [MoveEffect; 12] = [
    MoveEffect::EXPLOSION,
    MoveEffect::DREAM_EATER,
    MoveEffect::RAZOR_WIND,
    MoveEffect::SKY_ATTACK,
    MoveEffect::RECHARGE,
    MoveEffect::SKULL_BASH,
    MoveEffect::SOLAR_BEAM,
    MoveEffect::SPIT_UP,
    MoveEffect::FOCUS_PUNCH,
    MoveEffect::SUPERPOWER,
    MoveEffect::ERUPTION,
    MoveEffect::OVERHEAT,
];

fn is_ignored_powerful_move_effect(effect: MoveEffect) -> bool {
    IGNORED_POWERFUL_MOVE_EFFECTS.contains(&effect)
}

pub(super) fn estimated_damage(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    simulated_damage_percent: u32,
) -> Result<u32, BattleError> {
    let move_data = dex.move_data(move_id)?;
    let Some(move_type) = move_data.move_type.battle_type() else {
        return Ok(MINIMUM_DAMAGE);
    };
    let category = MoveCategory::for_type(move_type);
    let (attack_stat, attack_stage) = attacker.attacking_stat(category);
    let attack_stat = crate::ability::huge_power_attack(attacker.ability(), category, attack_stat);
    // Upstream's `CalculateBaseDamage` halves raw Special Attack for Fire/Ice
    // against Thick Fat before stage scaling (`pokemon.c:3202-3204`); the AI
    // estimates every populated slot, depleted or not.
    let attack_stat =
        crate::ability::thick_fat_special_attack(defender.ability(), move_type, attack_stat);
    // Plus/Minus raise raw Special Attack 150% before stages when the other
    // active mon has the complement (`pokemon.c:3207-3210`).
    let attack_stat = crate::ability::plus_minus_special_attack(
        attacker.ability(),
        defender.ability(),
        category,
        attack_stat,
    );
    let (defense_stat, defense_stage) = defender.defending_stat(category);
    let input = DamageInput {
        attacker_level: attacker.level(),
        power: u32::from(move_data.power),
        move_type,
        attack_stat,
        attack_stage,
        defense_stat,
        defense_stage,
        attacker_burned: false,
        reflect: false,
        light_screen: false,
        weather: Weather::None,
        is_solar_beam: false,
        attacker_pinch_boost: crate::ability::pinch_boosts_power(
            attacker.ability(),
            move_type,
            attacker.current_hp(),
            attacker.stats().max_hp,
        ),
    };
    let damage = base_damage(&input);
    // `TypeCalc` returns for Struggle before STAB and the chart (`battle_script_commands.c:1541`);
    // the caller's percentage and floor still apply (`battle_ai_script_commands.c:1208`-`:1213`).
    if move_id == MoveId::STRUGGLE {
        return Ok((damage * simulated_damage_percent / PERCENT_SCALE).max(MINIMUM_DAMAGE));
    }
    let damage = apply_stab(damage, has_stab(attacker.types(), move_id, move_type));
    let damage = if defender_levitate_blocked(move_type, defender.ability()) {
        damage
    } else {
        fold_ai_type_effectiveness(damage, move_type, defender.types())
    };
    Ok((damage * simulated_damage_percent / PERCENT_SCALE).max(MINIMUM_DAMAGE))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PowerComparison {
    NotComparable,
    WeakerThanAnotherMove,
    Strongest,
}

/// Whether `get_how_powerful_move_is` ranks the considered move rather than
/// answering `MOVE_POWER_OTHER`: power above one and not an ignored effect
/// (`src/battle_ai_script_commands.c:1186-1189`).
pub(super) fn move_is_power_comparable(move_data: &assets::MoveData) -> bool {
    move_data.power >= MINIMUM_DAMAGING_MOVE_POWER
        && !is_ignored_powerful_move_effect(move_data.effect)
}

pub(super) fn compare_move_power(
    dex: &Dex,
    scores: &MoveScores,
    considered_slot: usize,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<PowerComparison, BattleError> {
    if !move_is_power_comparable(dex.move_data(move_id)?) {
        return Ok(PowerComparison::NotComparable);
    }

    let mut damage_by_slot = [0; MAX_MON_MOVES];
    for (slot, damage) in damage_by_slot.iter_mut().enumerate() {
        let Some(candidate_move) = attacker.move_at(slot) else {
            continue;
        };
        let candidate_data = dex.move_data(candidate_move)?;
        if candidate_data.power < MINIMUM_DAMAGING_MOVE_POWER
            || is_ignored_powerful_move_effect(candidate_data.effect)
        {
            continue;
        }
        *damage = estimated_damage(
            dex,
            candidate_move,
            attacker,
            defender,
            scores.simulated_damage_percent[slot],
        )?;
    }

    if damage_by_slot
        .iter()
        .any(|damage| *damage > damage_by_slot[considered_slot])
    {
        Ok(PowerComparison::WeakerThanAnotherMove)
    } else {
        Ok(PowerComparison::Strongest)
    }
}

#[cfg(test)]
mod tests;
