//! `AI_CV_EvasionDown` (`data/battle_ai_scripts.s:1235`-`:1245`) scoring.
//!
//! Upstream compares the target's raw evasion stage (`0..=12`, neutral `6`); the
//! crate stores the signed offset, so raw `r` is offset `r - 6`.

use super::{choose_trainer_action, ensure_scoreable, score_viability, MoveScores, FIRST_TURN};
use super::{current_hp_percent, ScoreableEffect};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, Stats, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use crate::stat_change::{set_stage, ChangedStat};
use crate::stat_stage::StatStage;
use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId, SpeciesId};

const LEVEL: u8 = 5;
const MAX_HP: u32 = 100;
const UNCHANGED: i8 = 100;
const ONE_PENALTY: i8 = 98;
const TWO_PENALTIES: i8 = 96;

/// Raw stage 4 is offset -2 (above 3, skips the roll); raw 3 is offset -3.
const RAW_4_OFFSET: i8 = -2;
const RAW_3_OFFSET: i8 = -3;
const RAW_0_OFFSET: i8 = -6;
const RAW_12_OFFSET: i8 = 6;

/// `Random() % 256 < 50` skips the penalty.
const BELOW_THRESHOLD: u16 = 49;
const AT_THRESHOLD: u16 = 50;
/// Only the low byte counts.
const HIGH_BITS: u16 = 256;

const MUDKIP: SpeciesId = SpeciesId(283);

fn mon(moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(&Dex::new(), MUDKIP, LEVEL, Ivs::default(), 0, moves)
        .expect("test Pokemon must be dex-resident")
}

fn with_hp(pokemon: BattlePokemon, max_hp: u32, hp: u32) -> BattlePokemon {
    let stats = Stats {
        max_hp,
        ..pokemon.stats()
    };
    pokemon.with_cached_stats(stats, hp)
}

fn user_with_hp(hp: u32) -> BattlePokemon {
    with_hp(mon(vec![MoveId::SWEET_SCENT]), MAX_HP, hp)
}

fn target_at(evasion_offset: i8, hp: u32) -> BattlePokemon {
    let mut target = with_hp(mon(vec![MoveId::TACKLE]), MAX_HP, hp);
    set_stage(
        &mut target,
        ChangedStat::Evasion,
        StatStage::new(evasion_offset).unwrap(),
    );
    target
}

fn score_pair(user: &BattlePokemon, target: &BattlePokemon, rolls: &[u16]) -> i8 {
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES].into_iter().chain(rolls.iter().copied()));
    let mut scores = MoveScores::initialize(user, &mut rng);
    score_viability(
        &Dex::new(),
        &mut scores,
        0,
        MoveId::SWEET_SCENT,
        user,
        target,
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        rng.draws(),
        MAX_MON_MOVES + rolls.len(),
        "must draw exactly {} roll(s)",
        rolls.len()
    );
    rng.assert_exhausted();
    scores.values[0]
}

fn viability_score(user_hp: u32, evasion_offset: i8, target_hp: u32, rolls: &[u16]) -> i8 {
    score_pair(
        &user_with_hp(user_hp),
        &target_at(evasion_offset, target_hp),
        rolls,
    )
}

#[test]
fn a_healthy_user_against_raw_4_or_higher_evasion_takes_no_roll() {
    for offset in [RAW_4_OFFSET, 0, RAW_12_OFFSET] {
        assert_eq!(
            viability_score(MAX_HP, offset, MAX_HP, &[]),
            UNCHANGED,
            "offset {offset}"
        );
    }
}

#[test]
fn a_healthy_user_against_raw_3_or_lower_evasion_rolls_from_50() {
    for offset in [RAW_3_OFFSET, RAW_0_OFFSET] {
        assert_eq!(
            viability_score(MAX_HP, offset, MAX_HP, &[BELOW_THRESHOLD]),
            UNCHANGED,
            "offset {offset}"
        );
        assert_eq!(
            viability_score(MAX_HP, offset, MAX_HP, &[AT_THRESHOLD]),
            ONE_PENALTY,
            "offset {offset}"
        );
    }
}

#[test]
fn a_user_below_70_percent_hp_rolls_whatever_the_evasion_stage() {
    for offset in [RAW_4_OFFSET, 0, RAW_12_OFFSET] {
        assert_eq!(
            viability_score(69, offset, MAX_HP, &[BELOW_THRESHOLD]),
            UNCHANGED,
            "offset {offset}"
        );
        assert_eq!(
            viability_score(69, offset, MAX_HP, &[AT_THRESHOLD]),
            ONE_PENALTY,
            "offset {offset}"
        );
    }
}

#[test]
fn a_user_at_exactly_70_percent_hp_is_healthy_and_skips_the_roll() {
    assert_eq!(viability_score(70, 0, MAX_HP, &[]), UNCHANGED);
    assert_eq!(
        viability_score(70, RAW_3_OFFSET, MAX_HP, &[AT_THRESHOLD]),
        ONE_PENALTY
    );
}

#[test]
fn a_target_above_70_percent_hp_adds_no_penalty() {
    assert_eq!(viability_score(MAX_HP, 0, 71, &[]), UNCHANGED);
    assert_eq!(viability_score(69, 0, 71, &[BELOW_THRESHOLD]), UNCHANGED);
}

#[test]
fn a_target_at_exactly_70_percent_hp_costs_two_points_without_a_roll() {
    assert_eq!(viability_score(MAX_HP, 0, 70, &[]), ONE_PENALTY);
    assert_eq!(viability_score(MAX_HP, 0, 1, &[]), ONE_PENALTY);
}

#[test]
fn both_penalties_stack_to_four_against_two_for_either_alone() {
    assert_eq!(viability_score(69, 0, 70, &[AT_THRESHOLD]), UNCHANGED - 4);
    assert_eq!(viability_score(69, 0, 70, &[BELOW_THRESHOLD]), ONE_PENALTY);
    assert_eq!(viability_score(69, 0, 71, &[AT_THRESHOLD]), ONE_PENALTY);
    assert_eq!(
        viability_score(MAX_HP, RAW_3_OFFSET, 70, &[AT_THRESHOLD]),
        UNCHANGED - 4
    );
    assert_eq!(UNCHANGED - 4, TWO_PENALTIES);
}

#[test]
fn hp_percents_are_floored_before_the_comparison() {
    let user = with_hp(mon(vec![MoveId::SWEET_SCENT]), 200, 141);
    let target = with_hp(mon(vec![MoveId::TACKLE]), 200, 141);
    assert_eq!(current_hp_percent(&user), 70);
    assert_eq!(score_pair(&user, &target, &[]), ONE_PENALTY);

    let weak_user = with_hp(mon(vec![MoveId::SWEET_SCENT]), 200, 139);
    assert_eq!(current_hp_percent(&weak_user), 69);
    assert_eq!(
        score_pair(&weak_user, &target, &[BELOW_THRESHOLD]),
        ONE_PENALTY
    );
    assert_eq!(
        score_pair(&weak_user, &target, &[AT_THRESHOLD]),
        TWO_PENALTIES
    );
}

#[test]
fn the_roll_reads_only_the_low_byte() {
    assert_eq!(
        viability_score(69, 0, MAX_HP, &[HIGH_BITS + BELOW_THRESHOLD]),
        UNCHANGED
    );
    assert_eq!(
        viability_score(69, 0, MAX_HP, &[HIGH_BITS + AT_THRESHOLD]),
        ONE_PENALTY
    );
}

#[test]
fn only_the_targets_evasion_stage_is_read() {
    let user = user_with_hp(MAX_HP);
    let mut target = target_at(0, MAX_HP);
    set_stage(&mut target, ChangedStat::Accuracy, StatStage::MIN);
    assert_eq!(score_pair(&user, &target, &[]), UNCHANGED);

    let mut evasive_user = user_with_hp(MAX_HP);
    set_stage(&mut evasive_user, ChangedStat::Evasion, StatStage::MIN);
    assert_eq!(
        score_pair(&evasive_user, &target_at(0, MAX_HP), &[]),
        UNCHANGED
    );
}

#[test]
fn check_viability_admits_sweet_scent_but_not_first_turn_setup_or_other_stat_effects() {
    let dex = Dex::new();
    let viability = AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY);
    assert_eq!(
        ensure_scoreable(&dex, MoveId::SWEET_SCENT, viability),
        Ok(())
    );
    assert_eq!(
        ensure_scoreable(&dex, MoveId::SWEET_SCENT, AiFlags::CHECK_VIABILITY),
        Ok(())
    );
    for setup in [
        AiFlags::SETUP_FIRST_TURN,
        AiFlags::CHECK_VIABILITY.union(AiFlags::SETUP_FIRST_TURN),
    ] {
        assert_eq!(
            ensure_scoreable(&dex, MoveId::SWEET_SCENT, setup),
            Err(BattleError::UnscoreableMoveEffect(MoveId::SWEET_SCENT)),
            "{setup:?}"
        );
    }
    for move_id in [
        MoveId::SAND_ATTACK,
        MoveId::DOUBLE_TEAM,
        MoveId::MINIMIZE,
        MoveId::HARDEN,
        MoveId::AGILITY,
        MoveId::STRING_SHOT,
        MoveId::FAKE_TEARS,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}

#[test]
fn the_two_stage_evasion_drop_row_shares_the_branch_admission() {
    let flags = AiFlags::CHECK_VIABILITY;
    for effect in [MoveEffect::EVASION_DOWN, MoveEffect::EVASION_DOWN_2] {
        let scoreable = ScoreableEffect::from_raw(effect).unwrap();
        assert!(scoreable.supported_by(flags), "{effect:?}");
        assert!(
            !scoreable.supported_by(flags.union(AiFlags::SETUP_FIRST_TURN)),
            "{effect:?}"
        );
    }
    assert!(!ScoreableEffect::from_raw(MoveEffect::ACCURACY_DOWN_2)
        .unwrap()
        .supported_by(flags));
}

#[test]
fn the_chooser_spends_the_roll_before_the_tie_break() {
    let enemy = mon(vec![MoveId::SWEET_SCENT, MoveId::TACKLE]);
    let target = target_at(RAW_3_OFFSET, MAX_HP);
    let choose = |roll: u16, tie_break: u16| {
        let mut rng = SequenceRng::new([0, 0, 0, 0, roll, tie_break]);
        let action = choose_trainer_action(
            &Dex::new(),
            &enemy,
            &target,
            AiFlags::CHECK_VIABILITY,
            FIRST_TURN,
            &mut rng,
        )
        .unwrap();
        rng.assert_exhausted();
        action
    };
    // The penalty makes Tackle the unique best move, whatever the tie-break.
    assert_eq!(choose(AT_THRESHOLD, 0), EnemyAction::Move(1));
    assert_eq!(choose(AT_THRESHOLD, 1), EnemyAction::Move(1));
    // Without it the two moves tie and the tie-break picks.
    assert_eq!(choose(BELOW_THRESHOLD, 0), EnemyAction::Move(0));
    assert_eq!(choose(BELOW_THRESHOLD, 1), EnemyAction::Move(1));
}
