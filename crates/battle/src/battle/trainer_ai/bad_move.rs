//! `AI_CheckBadMove`: the prologue guards and the per-effect penalties.

use assets::{AbilityId, MoveId, Type};

use super::ability_guards::{
    ai_clear_body_or_white_smoke_blocks, ai_get_ability, ai_hyper_cutter_blocks,
    ai_soundproof_discourages, ai_wonder_guard_blocks,
};
use super::admission::{scoreable_effect, ScoreableEffect};
use super::damage_estimate::move_is_power_comparable;
use super::scores::{
    MoveScores, ABSORBED_MOVE_PENALTY, REDUNDANT_CONFUSION_PENALTY, STRONGLY_DISCOURAGE,
};
use super::type_effectiveness::has_no_effect;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::defender_levitate_blocked;
use crate::pokemon::BattlePokemon;
use crate::stat_change::{
    stage_of, ChangedStat, StatChangeDirection, StatChangeEffect, StatChangeMagnitude, KEEN_EYE,
};
use crate::status1::Status1;

/// `AI_CheckBadMove`'s prologue up to its `if_effect` chain
/// (`data/battle_ai_scripts.s:51-103`). Returns whether the script terminated
/// with a penalty. A move whose power the AI does not rank skips straight to
/// Soundproof, except Fissure and Horn Drill, which always run the checks. No
/// step draws: the target's ability is read without a guess.
pub(super) fn score_bad_move_prologue(
    dex: &Dex,
    scores: &mut MoveScores,
    slot: usize,
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
) -> Result<bool, BattleError> {
    let move_data = dex.move_data(move_id)?;
    let runs_negation_checks = move_id == MoveId::FISSURE
        || move_id == MoveId::HORN_DRILL
        || move_is_power_comparable(move_data);

    if runs_negation_checks {
        if has_no_effect(dex, move_id, user, target)? {
            scores.adjust(slot, STRONGLY_DISCOURAGE);
            return Ok(true);
        }
        let ability = ai_get_ability(target)?;
        let move_type = move_data.move_type.battle_type();
        let penalty = if ability == AbilityId::VOLT_ABSORB && move_type == Some(Type::Electric)
            || ability == AbilityId::WATER_ABSORB && move_type == Some(Type::Water)
            || ability == AbilityId::FLASH_FIRE && move_type == Some(Type::Fire)
        {
            Some(ABSORBED_MOVE_PENALTY)
        } else if ability == AbilityId::WONDER_GUARD
            && ai_wonder_guard_blocks(dex, move_id, user, target, ability)?
            || move_type.is_some_and(|move_type| defender_levitate_blocked(move_type, ability))
        {
            Some(STRONGLY_DISCOURAGE)
        } else {
            None
        };
        if let Some(penalty) = penalty {
            scores.adjust(slot, penalty);
            return Ok(true);
        }
    }

    // Soundproof reads the ability at its own call site
    // (`data/battle_ai_scripts.s:92-103`).
    let soundproof_ability = ai_get_ability(target)?;
    if ai_soundproof_discourages(dex, move_id, soundproof_ability)? {
        scores.adjust(slot, STRONGLY_DISCOURAGE);
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn score_bad_move(
    dex: &Dex,
    scores: &mut MoveScores,
    slot: usize,
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
) -> Result<(), BattleError> {
    let effect = scoreable_effect(dex, move_id)?;

    if score_bad_move_prologue(dex, scores, slot, move_id, user, target)? {
        return Ok(());
    }

    let penalty = match effect {
        ScoreableEffect::Hit | ScoreableEffect::UnchangedAfterPrologue(_) => None,
        ScoreableEffect::AttackDown => stat_label_penalty(
            user,
            target,
            StatChangeEffect {
                stat: ChangedStat::Attack,
                magnitude: StatChangeMagnitude::One,
                direction: StatChangeDirection::Lower,
            },
        )?,
        ScoreableEffect::DefenseDown => stat_label_penalty(
            user,
            target,
            StatChangeEffect {
                stat: ChangedStat::Defense,
                magnitude: StatChangeMagnitude::One,
                direction: StatChangeDirection::Lower,
            },
        )?,
        ScoreableEffect::StatLabel(_, label) => stat_label_penalty(user, target, label)?,
        ScoreableEffect::Paralyze => paralyze_penalty(dex, move_id, user, target)?,
        ScoreableEffect::Confuse => confuse_penalty(target)?,
        ScoreableEffect::FocusEnergy => {
            user.volatiles().focus_energy.then_some(STRONGLY_DISCOURAGE)
        }
        ScoreableEffect::HighRiskForDamage => {
            high_risk_for_damage_penalty(dex, move_id, user, target)?
        }
    };
    if let Some(penalty) = penalty {
        scores.adjust(slot, penalty);
    }
    Ok(())
}

/// `AI_CBM_*Up` / `AI_CBM_*Down` (`data/battle_ai_scripts.s:248-312`): the
/// first guard that fires ends the script.
pub(super) fn stat_label_penalty(
    user: &BattlePokemon,
    target: &BattlePokemon,
    label: StatChangeEffect,
) -> Result<Option<i8>, BattleError> {
    let holder = if label.affects_user() { user } else { target };
    if stage_of(holder, label.stat) == label.cap() {
        return Ok(Some(STRONGLY_DISCOURAGE));
    }
    if label.affects_user() {
        return Ok(None);
    }
    let blocked = stat_specific_drop_guard(target, label.stat)?
        || ai_clear_body_or_white_smoke_blocks(ai_get_ability(target)?);
    Ok(blocked.then_some(STRONGLY_DISCOURAGE))
}

/// The guard a drop label runs before `CheckIfAbilityBlocksStatChange`
/// (`data/battle_ai_scripts.s:277-281,287-290,300-304`).
fn stat_specific_drop_guard(
    target: &BattlePokemon,
    stat: ChangedStat,
) -> Result<bool, BattleError> {
    Ok(match stat {
        ChangedStat::Attack => ai_hyper_cutter_blocks(ai_get_ability(target)?),
        ChangedStat::Speed => ai_get_ability(target)? == AbilityId::SPEED_BOOST,
        ChangedStat::Accuracy => ai_get_ability(target)? == KEEN_EYE,
        _ => false,
    })
}

/// `AI_CBM_Paralyze`: each guard ends the script with its own single penalty,
/// so a type-immune target never also takes the Limber or status `-10`
/// (`data/battle_ai_scripts.s:397-403`). Safeguard is not modelled, and no
/// admitted move can set it.
fn paralyze_penalty(
    dex: &Dex,
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
) -> Result<Option<i8>, BattleError> {
    if has_no_effect(dex, move_id, user, target)?
        || ai_get_ability(target)? == AbilityId::LIMBER
        || target.status1() != Status1::Healthy
    {
        Ok(Some(STRONGLY_DISCOURAGE))
    } else {
        Ok(None)
    }
}

/// `AI_CBM_HighRiskForDamage`: a chart-immune target or a Wonder Guard target
/// the move does not hit for exactly double damage ends at `-10`
/// (`data/battle_ai_scripts.s:368-376`). Unlike the prologue, it runs for
/// power-1 moves too, and it draws nothing.
fn high_risk_for_damage_penalty(
    dex: &Dex,
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
) -> Result<Option<i8>, BattleError> {
    let blocked = has_no_effect(dex, move_id, user, target)?
        || ai_wonder_guard_blocks(dex, move_id, user, target, ai_get_ability(target)?)?;
    Ok(blocked.then_some(STRONGLY_DISCOURAGE))
}

/// `AI_CBM_Confuse`: active confusion ends at `-5` before any ability read;
/// Own Tempo is `-10` (`data/battle_ai_scripts.s:386-395`). Safeguard is not
/// modelled.
fn confuse_penalty(target: &BattlePokemon) -> Result<Option<i8>, BattleError> {
    if target.volatiles().confused() {
        Ok(Some(REDUNDANT_CONFUSION_PENALTY))
    } else if ai_get_ability(target)? == AbilityId::OWN_TEMPO {
        Ok(Some(STRONGLY_DISCOURAGE))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod ability_tests;
#[cfg(test)]
mod prologue_tests;
#[cfg(test)]
mod stat_stage_tests;
