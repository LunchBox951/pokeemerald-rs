//! `AI_CV_Confuse` (`data/battle_ai_scripts.s:1467`-`:1477`) scoring.
//!
//! Upstream compares `100 * hp / maxHP` (truncated) with `if_hp_more_than`,
//! which is strict, so a target sitting exactly on 70, 50 or 30 percent takes
//! that branch. `if_random_less_than 128` reads `Random() % 256`.

use super::{choose_trainer_action, ensure_scoreable, score_viability, MoveScores, FIRST_TURN};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId, SpeciesId};

const UNCHANGED: i8 = 100;
const ONE_PENALTY: i8 = 99;
const TWO_PENALTIES: i8 = 98;
const THREE_PENALTIES: i8 = 97;

/// Upstream's `Random() % 256 < 128` skips the penalty at 127 and below.
const BELOW_ROLL_THRESHOLD: u16 = 127;
const AT_ROLL_THRESHOLD: u16 = 128;
/// Only the low byte counts: 256 + 127 reads as 127, 256 + 128 as 128.
const HIGH_BITS: u16 = 256;

/// A level 5 Mudkip has exactly 20 HP, so every multiple of 5 percent is a
/// whole number of HP.
const MUDKIP: SpeciesId = SpeciesId(283);
const SMALL_LEVEL: u8 = 5;
const SMALL_MAX_HP: u32 = 20;
/// A level 100 Mudkip has a max HP that no round percentage divides.
const LARGE_LEVEL: u8 = 100;

fn mon(level: u8, moves: &[MoveId]) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        MUDKIP,
        level,
        Ivs::default(),
        0,
        moves.to_vec(),
    )
    .expect("test Pokemon must be dex-resident")
}

fn user() -> BattlePokemon {
    mon(SMALL_LEVEL, &[MoveId::CONFUSE_RAY])
}

/// A 20 HP target left with `hp` HP.
fn target_with_hp(hp: u32) -> BattlePokemon {
    let mut target = mon(SMALL_LEVEL, &[MoveId::POUND]);
    assert_eq!(target.stats().max_hp, SMALL_MAX_HP);
    target.apply_damage(SMALL_MAX_HP - hp);
    target
}

/// A level 100 target left with `hp` HP.
fn large_target_with_hp(hp: u32) -> BattlePokemon {
    let mut target = mon(LARGE_LEVEL, &[MoveId::POUND]);
    let max_hp = target.stats().max_hp;
    target.apply_damage(max_hp - hp);
    target
}

/// Scores `move_id` under `AI_CheckViability`, feeding `rolls` after the
/// simulated-damage initialisation, and asserts exactly `expected_draws` of
/// them were spent.
fn viability_score(
    move_id: MoveId,
    target: &BattlePokemon,
    rolls: &[u16],
    expected_draws: usize,
) -> i8 {
    let user = user();
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES].into_iter().chain(rolls.iter().copied()));
    let mut scores = MoveScores::initialize(&user, &mut rng);
    score_viability(
        &Dex::new(),
        &mut scores,
        0,
        move_id,
        &user,
        target,
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        rng.draws(),
        MAX_MON_MOVES + expected_draws,
        "confusion viability must spend exactly {expected_draws} draw(s)"
    );
    scores.values[0]
}

fn confuse_ray_score(target: &BattlePokemon, rolls: &[u16], expected_draws: usize) -> i8 {
    viability_score(MoveId::CONFUSE_RAY, target, rolls, expected_draws)
}

#[test]
fn a_target_above_seventy_percent_spends_no_draw_and_keeps_the_score() {
    for hp in [15, 20] {
        assert_eq!(confuse_ray_score(&target_with_hp(hp), &[], 0), UNCHANGED);
    }
}

#[test]
fn a_target_at_exactly_seventy_percent_takes_the_roll() {
    let target = target_with_hp(14);
    assert_eq!(
        confuse_ray_score(&target, &[BELOW_ROLL_THRESHOLD], 1),
        UNCHANGED
    );
    assert_eq!(
        confuse_ray_score(&target, &[AT_ROLL_THRESHOLD], 1),
        ONE_PENALTY
    );
}

#[test]
fn the_roll_penalty_starts_at_128() {
    let target = target_with_hp(12);
    assert_eq!(
        confuse_ray_score(&target, &[BELOW_ROLL_THRESHOLD], 1),
        UNCHANGED
    );
    assert_eq!(
        confuse_ray_score(&target, &[AT_ROLL_THRESHOLD], 1),
        ONE_PENALTY
    );
    assert_eq!(confuse_ray_score(&target, &[u16::MAX], 1), ONE_PENALTY);
}

#[test]
fn the_roll_reads_only_the_low_byte() {
    let target = target_with_hp(12);
    assert_eq!(
        confuse_ray_score(&target, &[HIGH_BITS + BELOW_ROLL_THRESHOLD], 1),
        UNCHANGED
    );
    assert_eq!(
        confuse_ray_score(&target, &[HIGH_BITS + AT_ROLL_THRESHOLD], 1),
        ONE_PENALTY
    );
}

#[test]
fn just_above_fifty_percent_takes_only_the_roll() {
    let target = target_with_hp(11);
    assert_eq!(
        confuse_ray_score(&target, &[BELOW_ROLL_THRESHOLD], 1),
        UNCHANGED
    );
    assert_eq!(
        confuse_ray_score(&target, &[AT_ROLL_THRESHOLD], 1),
        ONE_PENALTY
    );
}

#[test]
fn a_target_at_exactly_fifty_percent_takes_the_second_penalty() {
    let target = target_with_hp(10);
    assert_eq!(
        confuse_ray_score(&target, &[BELOW_ROLL_THRESHOLD], 1),
        ONE_PENALTY
    );
    assert_eq!(
        confuse_ray_score(&target, &[AT_ROLL_THRESHOLD], 1),
        TWO_PENALTIES
    );
}

#[test]
fn just_above_thirty_percent_stops_after_the_second_penalty() {
    let target = target_with_hp(7);
    assert_eq!(
        confuse_ray_score(&target, &[BELOW_ROLL_THRESHOLD], 1),
        ONE_PENALTY
    );
    assert_eq!(
        confuse_ray_score(&target, &[AT_ROLL_THRESHOLD], 1),
        TWO_PENALTIES
    );
}

#[test]
fn a_target_at_exactly_thirty_percent_takes_every_penalty() {
    let target = target_with_hp(6);
    assert_eq!(
        confuse_ray_score(&target, &[BELOW_ROLL_THRESHOLD], 1),
        TWO_PENALTIES
    );
    assert_eq!(
        confuse_ray_score(&target, &[AT_ROLL_THRESHOLD], 1),
        THREE_PENALTIES
    );
}

#[test]
fn a_fainted_target_takes_every_penalty() {
    let target = target_with_hp(0);
    assert_eq!(
        confuse_ray_score(&target, &[BELOW_ROLL_THRESHOLD], 1),
        TWO_PENALTIES
    );
    assert_eq!(
        confuse_ray_score(&target, &[AT_ROLL_THRESHOLD], 1),
        THREE_PENALTIES
    );
}

#[test]
fn the_percentage_truncates_before_the_comparison() {
    // 70.9% truncates to 70 and still rolls; 71.0% does not.
    let max_hp = mon(LARGE_LEVEL, &[MoveId::POUND]).stats().max_hp;
    let at_seventy = (71 * max_hp - 1) / 100;
    let at_seventy_one = (71 * max_hp).div_ceil(100);
    assert_eq!(100 * at_seventy / max_hp, 70);
    assert_eq!(100 * at_seventy_one / max_hp, 71);
    assert_eq!(
        confuse_ray_score(&large_target_with_hp(at_seventy), &[AT_ROLL_THRESHOLD], 1),
        ONE_PENALTY
    );
    assert_eq!(
        confuse_ray_score(&large_target_with_hp(at_seventy_one), &[], 0),
        UNCHANGED
    );
}

#[test]
#[should_panic(expected = "SequenceRng exhausted after 4 draws")]
fn a_target_that_needs_the_roll_exhausts_a_script_without_one() {
    confuse_ray_score(&target_with_hp(14), &[], 1);
}

#[test]
fn every_confusion_move_is_scored_alike() {
    let dex = Dex::new();
    for move_id in [MoveId::CONFUSE_RAY, MoveId::SUPERSONIC, MoveId::SWEET_KISS] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, MoveEffect::CONFUSE);
        assert_eq!(
            viability_score(move_id, &target_with_hp(6), &[AT_ROLL_THRESHOLD], 1),
            THREE_PENALTIES,
            "{move_id:?}"
        );
    }
}

#[test]
fn check_viability_admits_confusion_and_keeps_the_other_refusals() {
    let dex = Dex::new();
    let viability = AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY);
    for move_id in [MoveId::CONFUSE_RAY, MoveId::SUPERSONIC, MoveId::SWEET_KISS] {
        assert_eq!(ensure_scoreable(&dex, move_id, viability), Ok(()));
        assert_eq!(
            ensure_scoreable(&dex, move_id, AiFlags::CHECK_VIABILITY),
            Ok(())
        );
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability.union(AiFlags::SETUP_FIRST_TURN)),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "the first-turn setup script still rolls for confusion: {move_id:?}"
        );
    }
    for move_id in [
        MoveId::THUNDER_WAVE,
        MoveId::STUN_SPORE,
        MoveId::GLARE,
        MoveId::LEECH_SEED,
        MoveId::SWAGGER,
        MoveId::FLATTER,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}

#[test]
fn the_chooser_spends_the_viability_draw_before_the_tie_break() {
    let enemy = mon(SMALL_LEVEL, &[MoveId::CONFUSE_RAY, MoveId::POUND]);
    let player = target_with_hp(14);
    let choose = |roll: u16, tie_break: u16| {
        let mut rng = SequenceRng::new([0, 0, 0, 0, roll, tie_break]);
        let action = choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            AiFlags::CHECK_VIABILITY,
            FIRST_TURN,
            &mut rng,
        )
        .unwrap();
        rng.assert_exhausted();
        action
    };
    // The penalised Confuse Ray loses to Pound whatever the tie-break says.
    assert_eq!(choose(AT_ROLL_THRESHOLD, 0), EnemyAction::Move(1));
    assert_eq!(choose(AT_ROLL_THRESHOLD, 1), EnemyAction::Move(1));
    // The unpenalised one ties Pound, so the tie-break picks.
    assert_eq!(choose(BELOW_ROLL_THRESHOLD, 0), EnemyAction::Move(0));
    assert_eq!(choose(BELOW_ROLL_THRESHOLD, 1), EnemyAction::Move(1));
}

#[test]
fn a_healthy_target_leaves_the_chooser_tied_without_a_viability_draw() {
    let enemy = mon(SMALL_LEVEL, &[MoveId::CONFUSE_RAY, MoveId::POUND]);
    let player = target_with_hp(SMALL_MAX_HP);
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
    rng.assert_exhausted();
}
