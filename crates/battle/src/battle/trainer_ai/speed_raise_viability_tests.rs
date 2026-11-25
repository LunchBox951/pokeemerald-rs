//! `AI_CV_SpeedUp` (`data/battle_ai_scripts.s:956`-`:965`), reached from the
//! one-stage and two-stage Speed raises (`:661`, `:690`).
//!
//! Treecko (base Speed 70) outspeeds Mudkip (base 40) at equal level and IVs,
//! and two copies of one species tie.

use super::{
    choose_trainer_action, ensure_scoreable, score_speed_up_viability, score_viability, MoveScores,
    ScoreableEffect, FIRST_TURN, SPEED_RAISE_EFFECTS,
};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use crate::stat_change::{set_stage, ChangedStat};
use crate::stat_stage::StatStage;
use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId, SpeciesId};

const LEVEL: u8 = 5;
const UNCHANGED: i8 = 100;
const BONUS: i8 = 103;
const PENALTY: i8 = 97;

/// `Random() % 256 < 70` skips the bonus.
const BELOW_BONUS_THRESHOLD: u16 = 69;
const AT_BONUS_THRESHOLD: u16 = 70;
/// Only the low byte counts: 256 + 69 reads as 69, 256 + 70 as 70.
const HIGH_BITS: u16 = 256;

const TREECKO: SpeciesId = SpeciesId(277);
const MUDKIP: SpeciesId = SpeciesId(283);

fn mon(species: SpeciesId, moves: &[MoveId]) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        species,
        LEVEL,
        Ivs::default(),
        0,
        moves.to_vec(),
    )
    .expect("test Pokemon must be dex-resident")
}

fn user_and_target(
    user_species: SpeciesId,
    user_moves: &[MoveId],
    target_species: SpeciesId,
    target_moves: &[MoveId],
) -> (BattlePokemon, BattlePokemon) {
    (
        mon(user_species, user_moves),
        mon(target_species, target_moves),
    )
}

fn slower_user() -> (BattlePokemon, BattlePokemon) {
    user_and_target(MUDKIP, &[MoveId::AGILITY], TREECKO, &[MoveId::POUND])
}

fn faster_user() -> (BattlePokemon, BattlePokemon) {
    user_and_target(TREECKO, &[MoveId::AGILITY], MUDKIP, &[MoveId::POUND])
}

/// Equal level, IVs, species and nature give equal Speed.
fn tied_user() -> (BattlePokemon, BattlePokemon) {
    let pair = user_and_target(TREECKO, &[MoveId::AGILITY], TREECKO, &[MoveId::POUND]);
    assert_eq!(pair.0.speed_for_turn_order(), pair.1.speed_for_turn_order());
    pair
}

/// Runs Agility through `score_viability` after the simulated-damage
/// initialisation draws, asserting it drew exactly `rolls` and no more.
fn agility_score(user: &BattlePokemon, target: &BattlePokemon, rolls: &[u16]) -> i8 {
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES].into_iter().chain(rolls.iter().copied()));
    let mut scores = MoveScores::initialize(user, &mut rng);
    score_viability(
        &Dex::new(),
        &mut scores,
        0,
        MoveId::AGILITY,
        user,
        target,
        &mut rng,
    )
    .unwrap();
    assert_eq!(rng.draws(), MAX_MON_MOVES + rolls.len());
    rng.assert_exhausted();
    scores.values[0]
}

/// The same scoring body without a dex move, for the one-stage effect no
/// shipped move carries.
fn speed_up_score(user: &BattlePokemon, target: &BattlePokemon, rolls: &[u16]) -> i8 {
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES].into_iter().chain(rolls.iter().copied()));
    let mut scores = MoveScores::initialize(user, &mut rng);
    score_speed_up_viability(&mut scores, 0, user, target, &mut rng);
    assert_eq!(rng.draws(), MAX_MON_MOVES + rolls.len());
    rng.assert_exhausted();
    scores.values[0]
}

#[test]
fn a_faster_target_rolls_once_and_gains_three_from_a_low_byte_of_70() {
    let (user, target) = slower_user();
    assert_eq!(
        agility_score(&user, &target, &[BELOW_BONUS_THRESHOLD]),
        UNCHANGED
    );
    assert_eq!(agility_score(&user, &target, &[AT_BONUS_THRESHOLD]), BONUS);
    assert_eq!(
        agility_score(&user, &target, &[HIGH_BITS + BELOW_BONUS_THRESHOLD]),
        UNCHANGED
    );
    assert_eq!(
        agility_score(&user, &target, &[HIGH_BITS + AT_BONUS_THRESHOLD]),
        BONUS
    );
}

#[test]
fn a_slower_target_costs_three_with_no_roll() {
    let (user, target) = faster_user();
    assert_eq!(agility_score(&user, &target, &[]), PENALTY);
}

#[test]
fn a_tied_target_draws_the_tie_break_once_and_costs_three_whatever_it_returns() {
    let (user, target) = tied_user();
    for tie_draw in [0, 1, 2, 3] {
        assert_eq!(
            agility_score(&user, &target, &[tie_draw]),
            PENALTY,
            "tie draw {tie_draw}"
        );
    }
}

#[test]
fn stat_stages_decide_the_comparison_not_the_base_stats() {
    let (mut user, target) = slower_user();
    set_stage(&mut user, ChangedStat::Speed, StatStage::MAX);
    assert_eq!(agility_score(&user, &target, &[]), PENALTY);

    let (user, mut target) = faster_user();
    set_stage(&mut target, ChangedStat::Speed, StatStage::MAX);
    assert_eq!(agility_score(&user, &target, &[AT_BONUS_THRESHOLD]), BONUS);
}

#[test]
fn move_priority_never_decides_the_comparison() {
    // Quick Attack would put the slower user first and Counter would put the
    // faster target last, so a priority-aware query would call the target
    // slower and score -3; the speed-only query rolls.
    let (user, target) = user_and_target(
        MUDKIP,
        &[MoveId::AGILITY, MoveId::QUICK_ATTACK],
        TREECKO,
        &[MoveId::COUNTER],
    );
    assert_eq!(agility_score(&user, &target, &[AT_BONUS_THRESHOLD]), BONUS);

    // The reverse holding: a priority-aware query would call the target
    // faster and roll; the speed-only query scores -3 with no draw.
    let (user, target) = user_and_target(
        TREECKO,
        &[MoveId::AGILITY, MoveId::COUNTER],
        MUDKIP,
        &[MoveId::QUICK_ATTACK],
    );
    assert_eq!(agility_score(&user, &target, &[]), PENALTY);
}

#[test]
fn the_one_stage_raise_scores_through_the_same_label() {
    let (user, target) = slower_user();
    assert_eq!(
        speed_up_score(&user, &target, &[BELOW_BONUS_THRESHOLD]),
        UNCHANGED
    );
    assert_eq!(speed_up_score(&user, &target, &[AT_BONUS_THRESHOLD]), BONUS);
    let (user, target) = faster_user();
    assert_eq!(speed_up_score(&user, &target, &[]), PENALTY);
    let (user, target) = tied_user();
    assert_eq!(speed_up_score(&user, &target, &[1]), PENALTY);
}

#[test]
fn only_the_two_speed_raise_effects_take_the_speed_up_branch() {
    assert_eq!(
        SPEED_RAISE_EFFECTS,
        [MoveEffect::SPEED_UP, MoveEffect::SPEED_UP_2]
    );
    for effect in SPEED_RAISE_EFFECTS {
        let scoreable = ScoreableEffect::from_raw(effect).expect("a speed raise is a stat label");
        assert!(
            scoreable.supported_by(AiFlags::CHECK_VIABILITY),
            "{effect:?}"
        );
        assert!(
            scoreable.supported_by(AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY)),
            "{effect:?}"
        );
        assert!(
            !scoreable.supported_by(AiFlags::SETUP_FIRST_TURN),
            "{effect:?}"
        );
        assert!(
            !scoreable.supported_by(AiFlags::CHECK_VIABILITY.union(AiFlags::SETUP_FIRST_TURN)),
            "{effect:?}"
        );
    }
    for effect in [
        MoveEffect::SPEED_DOWN,
        MoveEffect::SPEED_DOWN_2,
        MoveEffect::ATTACK_UP,
        MoveEffect::DEFENSE_UP,
        MoveEffect::SPECIAL_ATTACK_UP,
        MoveEffect::SPECIAL_DEFENSE_UP,
        MoveEffect::ACCURACY_UP,
        MoveEffect::EVASION_UP,
    ] {
        let scoreable = ScoreableEffect::from_raw(effect).expect("a stat label");
        assert!(
            !scoreable.supported_by(AiFlags::CHECK_VIABILITY),
            "{effect:?}"
        );
    }
}

#[test]
fn agility_is_admitted_under_viability_but_still_refused_under_setup_first_turn() {
    let dex = Dex::new();
    for flags in [
        AiFlags::CHECK_VIABILITY,
        AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY),
        AiFlags::CHECK_BAD_MOVE
            .union(AiFlags::TRY_TO_FAINT)
            .union(AiFlags::CHECK_VIABILITY),
    ] {
        assert_eq!(ensure_scoreable(&dex, MoveId::AGILITY, flags), Ok(()));
    }
    for flags in [
        AiFlags::SETUP_FIRST_TURN,
        AiFlags::CHECK_VIABILITY.union(AiFlags::SETUP_FIRST_TURN),
    ] {
        assert_eq!(
            ensure_scoreable(&dex, MoveId::AGILITY, flags),
            Err(BattleError::UnscoreableMoveEffect(MoveId::AGILITY))
        );
    }
}

/// Chooses between Agility (slot 0) and Tackle (slot 1, no viability branch)
/// against a faster player; `script` follows the four initialisation draws.
fn choose(script: &[u16]) -> EnemyAction {
    let enemy = mon(MUDKIP, &[MoveId::AGILITY, MoveId::TACKLE]);
    let player = mon(TREECKO, &[MoveId::POUND]);
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES].into_iter().chain(script.iter().copied()));
    let action = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        AiFlags::CHECK_VIABILITY,
        FIRST_TURN,
        &mut rng,
    )
    .unwrap();
    assert_eq!(rng.draws(), MAX_MON_MOVES + script.len());
    rng.assert_exhausted();
    action
}

#[test]
fn a_viability_trainer_holding_agility_scores_it_before_the_final_tie_break() {
    // Agility's roll, then the final tie-break draw. A bonus leaves Agility
    // alone at 103, so a tie-break of 1 still selects slot 0.
    assert_eq!(choose(&[AT_BONUS_THRESHOLD, 1]), EnemyAction::Move(0));
    // Without the bonus both slots stay at 100 and the tie-break picks slot 1.
    assert_eq!(choose(&[BELOW_BONUS_THRESHOLD, 1]), EnemyAction::Move(1));
}
