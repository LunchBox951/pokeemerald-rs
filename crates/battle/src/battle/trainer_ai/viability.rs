//! `AI_CheckViability`: the stat-drop branches the modelled effects reach.

use assets::{MoveEffect, MoveId, Type};

use super::admission::{scoreable_effect, ScoreableEffect};
use super::scores::{
    current_hp_percent, random_roll_is_at_least, MoveScores, DISCOURAGE, SLIGHTLY_DISCOURAGE,
};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::stat_stage::StatStage;

pub(super) const STAT_DROP_DISCOURAGEMENT_THRESHOLD: u16 = 50;
const ATTACK_DROP_USER_HP_THRESHOLD: u32 = 90;
const DEFENSE_DROP_USER_HP_THRESHOLD: u32 = 70;
const STAT_DROP_TARGET_HP_THRESHOLD: u32 = 70;
const HEAVILY_LOWERED_STAT_STAGE: i8 = -3;

pub(super) fn score_viability(
    dex: &Dex,
    scores: &mut MoveScores,
    slot: usize,
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    match scoreable_effect(dex, move_id)? {
        ScoreableEffect::Hit
        | ScoreableEffect::UnchangedAfterPrologue(_)
        | ScoreableEffect::FocusEnergy => {}
        ScoreableEffect::HighRiskForDamage => {
            return Err(BattleError::UnscoreableMoveEffect(move_id))
        }
        // Defense Curl is absent from `AI_CheckViability`'s dispatch
        // (`data/battle_ai_scripts.s:652-776`); every other label is unported.
        ScoreableEffect::StatLabel(effect, _) if effect == MoveEffect::DEFENSE_CURL => {}
        ScoreableEffect::Paralyze | ScoreableEffect::Confuse | ScoreableEffect::StatLabel(..) => {
            return Err(BattleError::UnscoreableMoveEffect(move_id));
        }
        ScoreableEffect::AttackDown => {
            score_attack_drop_viability(scores, slot, user, target, rng);
        }
        ScoreableEffect::DefenseDown => {
            score_defense_drop_viability(scores, slot, user, target, rng);
        }
    }
    Ok(())
}

/// Exact six-entry `AI_CV_AttackDown_PhysicalTypeList`. Emerald omits Flying,
/// Poison, and Ghost (`data/battle_ai_scripts.s:1115`-`:1121`).
const PHYSICAL_TYPE_LIST: [Type; 6] = [
    Type::Normal,
    Type::Fighting,
    Type::Ground,
    Type::Rock,
    Type::Bug,
    Type::Steel,
];

fn has_physical_type(pokemon: &BattlePokemon) -> bool {
    pokemon
        .types()
        .iter()
        .any(|pokemon_type| PHYSICAL_TYPE_LIST.contains(pokemon_type))
}

fn score_attack_drop_viability(
    scores: &mut MoveScores,
    slot: usize,
    user: &BattlePokemon,
    target: &BattlePokemon,
    rng: &mut impl BattleRng,
) {
    let attack_stage = target.stages().attack;
    if attack_stage != StatStage::NEUTRAL {
        scores.adjust(slot, SLIGHTLY_DISCOURAGE);
        if current_hp_percent(user) <= ATTACK_DROP_USER_HP_THRESHOLD {
            scores.adjust(slot, SLIGHTLY_DISCOURAGE);
        }
        if attack_stage.offset() <= HEAVILY_LOWERED_STAT_STAGE
            && random_roll_is_at_least(rng, STAT_DROP_DISCOURAGEMENT_THRESHOLD)
        {
            scores.adjust(slot, DISCOURAGE);
        }
    }

    if current_hp_percent(target) <= STAT_DROP_TARGET_HP_THRESHOLD {
        scores.adjust(slot, DISCOURAGE);
    }
    if !has_physical_type(target)
        && random_roll_is_at_least(rng, STAT_DROP_DISCOURAGEMENT_THRESHOLD)
    {
        scores.adjust(slot, DISCOURAGE);
    }
}

fn score_defense_drop_viability(
    scores: &mut MoveScores,
    slot: usize,
    user: &BattlePokemon,
    target: &BattlePokemon,
    rng: &mut impl BattleRng,
) {
    let user_is_healthy = current_hp_percent(user) >= DEFENSE_DROP_USER_HP_THRESHOLD;
    let defense_is_not_heavily_lowered =
        target.stages().defense.offset() > HEAVILY_LOWERED_STAT_STAGE;
    let skip_discouragement_roll = user_is_healthy && defense_is_not_heavily_lowered;

    if !skip_discouragement_roll && random_roll_is_at_least(rng, STAT_DROP_DISCOURAGEMENT_THRESHOLD)
    {
        scores.adjust(slot, DISCOURAGE);
    }
    if current_hp_percent(target) <= STAT_DROP_TARGET_HP_THRESHOLD {
        scores.adjust(slot, DISCOURAGE);
    }
}
