//! `AI_CV_AlwaysHit` (`data/battle_ai_scripts.s:1075`-`:1088`) scoring.
//!
//! Upstream compares raw stages (`0..=12`, neutral `6`); the crate stores the
//! signed offset, so raw `r` is offset `r - 6`. Each boundary below names both.

use super::{choose_trainer_action, ensure_scoreable, score_viability, MoveScores, FIRST_TURN};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use crate::stat_change::{set_stage, ChangedStat};
use crate::stat_stage::StatStage;
use assets::trainers::AiFlags;
use assets::{MoveId, SpeciesId};

const LEVEL: u8 = 5;
const UNCHANGED: i8 = 100;
const ONE_BONUS: i8 = 101;
const TWO_BONUSES: i8 = 102;

/// Upstream's `Random() % 256 < 100` falls through at 100 and above.
const BELOW_ROLL_THRESHOLD: u16 = 99;
const AT_ROLL_THRESHOLD: u16 = 100;
/// Only the low byte counts: 256 + 99 reads as 99, 256 + 100 as 100.
const HIGH_BITS: u16 = 256;

const MUDKIP: SpeciesId = SpeciesId(283);

fn mon(moves: &[MoveId]) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        MUDKIP,
        LEVEL,
        Ivs::default(),
        0,
        moves.to_vec(),
    )
    .expect("test Pokemon must be dex-resident")
}

/// A user whose accuracy sits at raw stage `raw`.
fn user_with_raw_accuracy(raw: i8) -> BattlePokemon {
    let mut user = mon(&[MoveId::SWIFT]);
    set_stage(
        &mut user,
        ChangedStat::Accuracy,
        StatStage::new(raw - 6).unwrap(),
    );
    user
}

/// A target whose evasion sits at raw stage `raw`.
fn target_with_raw_evasion(raw: i8) -> BattlePokemon {
    let mut target = mon(&[MoveId::TACKLE]);
    set_stage(
        &mut target,
        ChangedStat::Evasion,
        StatStage::new(raw - 6).unwrap(),
    );
    target
}

/// Scores Swift through `AI_CheckViability` once admitted, feeding `rolls` after the
/// simulated-damage initialisation, and asserts exactly `expected_draws`
/// of them were spent.
fn viability_score(
    user: &BattlePokemon,
    target: &BattlePokemon,
    rolls: &[u16],
    expected_draws: usize,
) -> i8 {
    assert_eq!(
        ensure_scoreable(&Dex::new(), MoveId::SWIFT, AiFlags::CHECK_VIABILITY),
        Ok(()),
        "the script under test must be admitted"
    );
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES].into_iter().chain(rolls.iter().copied()));
    let mut scores = MoveScores::initialize(user, &mut rng);
    score_viability(
        &Dex::new(),
        &mut scores,
        0,
        MoveId::SWIFT,
        user,
        target,
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        rng.draws(),
        MAX_MON_MOVES + expected_draws,
        "always-hit viability must spend exactly {expected_draws} draw(s)"
    );
    scores.values[0]
}

#[test]
fn ordinary_stages_change_nothing_and_spend_no_draw() {
    let user = user_with_raw_accuracy(6);
    let target = target_with_raw_evasion(6);
    assert_eq!(viability_score(&user, &target, &[], 0), UNCHANGED);
}

#[test]
fn evasion_at_raw_8_is_not_above_the_random_branch_threshold() {
    // Raw 8 is offset +2.
    let user = user_with_raw_accuracy(6);
    let target = target_with_raw_evasion(8);
    assert_eq!(viability_score(&user, &target, &[], 0), UNCHANGED);
}

#[test]
fn accuracy_at_raw_4_is_not_below_the_random_branch_threshold() {
    // Raw 4 is offset -2.
    let user = user_with_raw_accuracy(4);
    let target = target_with_raw_evasion(6);
    assert_eq!(viability_score(&user, &target, &[], 0), UNCHANGED);
}

#[test]
fn evasion_at_raw_9_takes_only_the_random_branch() {
    // Raw 9 is offset +3.
    let user = user_with_raw_accuracy(6);
    let target = target_with_raw_evasion(9);
    assert_eq!(
        viability_score(&user, &target, &[BELOW_ROLL_THRESHOLD], 1),
        UNCHANGED
    );
    assert_eq!(
        viability_score(&user, &target, &[AT_ROLL_THRESHOLD], 1),
        ONE_BONUS
    );
}

#[test]
fn accuracy_at_raw_3_takes_only_the_random_branch() {
    // Raw 3 is offset -3.
    let user = user_with_raw_accuracy(3);
    let target = target_with_raw_evasion(6);
    assert_eq!(
        viability_score(&user, &target, &[BELOW_ROLL_THRESHOLD], 1),
        UNCHANGED
    );
    assert_eq!(
        viability_score(&user, &target, &[AT_ROLL_THRESHOLD], 1),
        ONE_BONUS
    );
}

#[test]
fn evasion_at_raw_10_is_not_above_the_immediate_bonus_threshold() {
    // Raw 10 is offset +4: random branch only.
    let user = user_with_raw_accuracy(6);
    let target = target_with_raw_evasion(10);
    assert_eq!(
        viability_score(&user, &target, &[BELOW_ROLL_THRESHOLD], 1),
        UNCHANGED
    );
    assert_eq!(
        viability_score(&user, &target, &[AT_ROLL_THRESHOLD], 1),
        ONE_BONUS
    );
}

#[test]
fn accuracy_at_raw_2_is_not_below_the_immediate_bonus_threshold() {
    // Raw 2 is offset -4: random branch only.
    let user = user_with_raw_accuracy(2);
    let target = target_with_raw_evasion(6);
    assert_eq!(
        viability_score(&user, &target, &[BELOW_ROLL_THRESHOLD], 1),
        UNCHANGED
    );
    assert_eq!(
        viability_score(&user, &target, &[AT_ROLL_THRESHOLD], 1),
        ONE_BONUS
    );
}

#[test]
fn evasion_at_raw_11_scores_at_once_then_rolls() {
    // Raw 11 is offset +5.
    let user = user_with_raw_accuracy(6);
    let target = target_with_raw_evasion(11);
    assert_eq!(
        viability_score(&user, &target, &[BELOW_ROLL_THRESHOLD], 1),
        ONE_BONUS
    );
    assert_eq!(
        viability_score(&user, &target, &[AT_ROLL_THRESHOLD], 1),
        TWO_BONUSES
    );
}

#[test]
fn accuracy_at_raw_1_scores_at_once_then_rolls() {
    // Raw 1 is offset -5.
    let user = user_with_raw_accuracy(1);
    let target = target_with_raw_evasion(6);
    assert_eq!(
        viability_score(&user, &target, &[BELOW_ROLL_THRESHOLD], 1),
        ONE_BONUS
    );
    assert_eq!(
        viability_score(&user, &target, &[AT_ROLL_THRESHOLD], 1),
        TWO_BONUSES
    );
}

#[test]
fn the_extremes_score_at_once_then_roll() {
    for (user, target) in [
        (user_with_raw_accuracy(0), target_with_raw_evasion(6)),
        (user_with_raw_accuracy(6), target_with_raw_evasion(12)),
    ] {
        assert_eq!(
            viability_score(&user, &target, &[BELOW_ROLL_THRESHOLD], 1),
            ONE_BONUS
        );
        assert_eq!(
            viability_score(&user, &target, &[AT_ROLL_THRESHOLD], 1),
            TWO_BONUSES
        );
    }
}

#[test]
fn both_conditions_still_award_the_immediate_bonus_once() {
    let user = user_with_raw_accuracy(0);
    let target = target_with_raw_evasion(12);
    assert_eq!(
        viability_score(&user, &target, &[BELOW_ROLL_THRESHOLD], 1),
        ONE_BONUS
    );
    assert_eq!(
        viability_score(&user, &target, &[AT_ROLL_THRESHOLD], 1),
        TWO_BONUSES
    );
}

#[test]
fn the_roll_reads_only_the_low_byte() {
    let user = user_with_raw_accuracy(6);
    let target = target_with_raw_evasion(9);
    assert_eq!(
        viability_score(&user, &target, &[HIGH_BITS + BELOW_ROLL_THRESHOLD], 1),
        UNCHANGED
    );
    assert_eq!(
        viability_score(&user, &target, &[HIGH_BITS + AT_ROLL_THRESHOLD], 1),
        ONE_BONUS
    );
}

#[test]
fn the_opposite_sides_stages_are_not_read() {
    // The user's evasion and the target's accuracy are irrelevant.
    let mut user = user_with_raw_accuracy(6);
    let mut target = target_with_raw_evasion(6);
    set_stage(&mut user, ChangedStat::Evasion, StatStage::MAX);
    set_stage(&mut target, ChangedStat::Accuracy, StatStage::MIN);
    assert_eq!(viability_score(&user, &target, &[], 0), UNCHANGED);
}

#[test]
fn check_viability_admits_the_effect_and_keeps_other_refusals() {
    let dex = Dex::new();
    let viability = AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY);
    assert_eq!(ensure_scoreable(&dex, MoveId::SWIFT, viability), Ok(()));
    for move_id in [MoveId::MUD_SHOT, MoveId::VITAL_THROW] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}

#[test]
fn the_chooser_spends_the_viability_draws_before_selection() {
    let enemy = mon(&[MoveId::TACKLE, MoveId::SWIFT]);
    let mut player = mon(&[MoveId::POUND]);
    set_stage(&mut player, ChangedStat::Evasion, StatStage::MAX);
    // Initialisation, Swift's roll, then the tie-break draw.
    let mut rng = SequenceRng::new([0, 0, 0, 0, AT_ROLL_THRESHOLD, 0]);
    let action = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        AiFlags::CHECK_VIABILITY,
        FIRST_TURN,
        &mut rng,
    );
    assert_eq!(action, Ok(EnemyAction::Move(1)));
    assert_eq!(rng.draws(), MAX_MON_MOVES + 2);
    rng.assert_exhausted();
}

#[test]
fn ordinary_stages_leave_the_chooser_tied_without_a_viability_draw() {
    let enemy = mon(&[MoveId::TACKLE, MoveId::SWIFT]);
    let player = mon(&[MoveId::POUND]);
    // Initialisation and the tie-break draw only.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0]);
    let action = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        AiFlags::CHECK_VIABILITY,
        FIRST_TURN,
        &mut rng,
    );
    assert_eq!(action, Ok(EnemyAction::Move(0)));
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
    rng.assert_exhausted();
}
