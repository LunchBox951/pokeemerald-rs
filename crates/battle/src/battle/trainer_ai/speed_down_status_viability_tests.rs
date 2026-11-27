//! `AI_CV_SpeedDown` reached directly from `EFFECT_SPEED_DOWN` and
//! `EFFECT_SPEED_DOWN_2` (`data/battle_ai_scripts.s:669`, `:697`,
//! `:1142`-`:1151`): String Shot, Scary Face and the other status Speed drops.
//!
//! Treecko (base Speed 70) outspeeds Mudkip (base 40) at equal level and IVs,
//! and two copies of one species tie.

use super::{
    choose_trainer_action, ensure_scoreable, score_viability, MoveScores, ScoreableEffect,
    FIRST_TURN, SPEED_DROP_EFFECTS,
};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use crate::status1::Status1;
use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId, SpeciesId};

const LEVEL: u8 = 5;
const UNCHANGED: i8 = 100;
const PENALTY: i8 = 97;
const BONUS: i8 = 102;
const INIT_DRAWS: usize = MAX_MON_MOVES;

/// `if_random_less_than 70` skips the bonus below a low byte of 70.
const BELOW_BONUS_THRESHOLD: u16 = 69;
const AT_BONUS_THRESHOLD: u16 = 70;
const LOW_BYTE_MAX: u16 = 255;
/// Only the low byte counts: 256 + 69 reads as 69, 256 + 70 as 70.
const HIGH_BITS: u16 = 256;
/// Bit zero clear and set: neither may change a tie's "target faster" answer.
const EVEN_DRAW: u16 = 0;
const ODD_DRAW: u16 = 1;

const TREECKO: SpeciesId = SpeciesId(277);
const MUDKIP: SpeciesId = SpeciesId(283);

/// String Shot is `EFFECT_SPEED_DOWN`; Scary Face and Cotton Spore are
/// `EFFECT_SPEED_DOWN_2`.
const SPEED_DROP_MOVES: [(MoveId, MoveEffect); 3] = [
    (MoveId::STRING_SHOT, MoveEffect::SPEED_DOWN),
    (MoveId::SCARY_FACE, MoveEffect::SPEED_DOWN_2),
    (MoveId::COTTON_SPORE, MoveEffect::SPEED_DOWN_2),
];

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

fn target_faster(move_id: MoveId) -> (BattlePokemon, BattlePokemon) {
    let (user, target) = (mon(MUDKIP, &[move_id]), mon(TREECKO, &[MoveId::POUND]));
    assert!(user.speed_for_turn_order() < target.speed_for_turn_order());
    (user, target)
}

fn target_slower(move_id: MoveId) -> (BattlePokemon, BattlePokemon) {
    let (user, target) = (mon(TREECKO, &[move_id]), mon(MUDKIP, &[MoveId::POUND]));
    assert!(user.speed_for_turn_order() > target.speed_for_turn_order());
    (user, target)
}

fn speed_tied(move_id: MoveId) -> (BattlePokemon, BattlePokemon) {
    let (user, target) = (mon(TREECKO, &[move_id]), mon(TREECKO, &[MoveId::POUND]));
    assert_eq!(user.speed_for_turn_order(), target.speed_for_turn_order());
    (user, target)
}

/// Scores slot 0 under `AI_CheckViability` after the simulated-damage
/// initialisation draws, feeding `tail` to the branch and requiring it to be
/// consumed exactly. Returns the score and the draws the branch alone spent.
fn viability(
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
    tail: &[u16],
) -> (i8, usize) {
    let mut script = vec![0; INIT_DRAWS];
    script.extend_from_slice(tail);
    let mut rng = SequenceRng::new(script);
    let mut scores = MoveScores::initialize(user, &mut rng);
    assert_eq!(rng.draws(), INIT_DRAWS);
    score_viability(&Dex::new(), &mut scores, 0, move_id, user, target, &mut rng).unwrap();
    rng.assert_exhausted();
    (scores.values[0], rng.draws() - INIT_DRAWS)
}

#[test]
fn the_moves_carry_the_effects_the_branch_dispatches() {
    let dex = Dex::new();
    for (move_id, effect) in SPEED_DROP_MOVES {
        assert_eq!(
            dex.move_data(move_id).unwrap().effect,
            effect,
            "{move_id:?}"
        );
        assert!(SPEED_DROP_EFFECTS.contains(&effect), "{move_id:?}");
    }
}

#[test]
fn a_faster_target_rolls_once_and_the_boundary_is_69_then_70() {
    for (move_id, _) in SPEED_DROP_MOVES {
        let (user, target) = target_faster(move_id);
        for roll in [0, BELOW_BONUS_THRESHOLD, HIGH_BITS | BELOW_BONUS_THRESHOLD] {
            assert_eq!(
                viability(move_id, &user, &target, &[roll]),
                (UNCHANGED, 1),
                "{move_id:?}: roll {roll:#x} stays below 70"
            );
        }
        for roll in [
            AT_BONUS_THRESHOLD,
            LOW_BYTE_MAX,
            HIGH_BITS | AT_BONUS_THRESHOLD,
        ] {
            assert_eq!(
                viability(move_id, &user, &target, &[roll]),
                (BONUS, 1),
                "{move_id:?}: roll {roll:#x} reaches 70"
            );
        }
    }
}

#[test]
fn a_slower_target_is_penalised_three_with_no_draw() {
    for (move_id, _) in SPEED_DROP_MOVES {
        let (user, target) = target_slower(move_id);
        assert_eq!(
            viability(move_id, &user, &target, &[]),
            (PENALTY, 0),
            "{move_id:?}"
        );
    }
}

#[test]
fn a_speed_tie_draws_once_and_is_never_target_faster() {
    for (move_id, _) in SPEED_DROP_MOVES {
        let (user, target) = speed_tied(move_id);
        // The tie-break draw is discarded whichever way it falls, and the
        // roll that would add +2 is never reached.
        for tie_break in [EVEN_DRAW, ODD_DRAW] {
            assert_eq!(
                viability(move_id, &user, &target, &[tie_break]),
                (PENALTY, 1),
                "{move_id:?}: tie draw {tie_break}"
            );
        }
    }
}

#[test]
fn the_fixed_penalty_and_the_conditional_bonus_are_separate_paths() {
    for (move_id, _) in SPEED_DROP_MOVES {
        // Exact draw counts: none on the penalty path, one on the bonus path.
        let (user, target) = target_slower(move_id);
        assert_eq!(viability(move_id, &user, &target, &[]).1, 0);
        let (user, target) = target_faster(move_id);
        assert_eq!(viability(move_id, &user, &target, &[LOW_BYTE_MAX]).1, 1);
    }
}

#[test]
fn the_chosen_moves_priority_is_ignored_by_the_speed_query() {
    for (move_id, _) in SPEED_DROP_MOVES {
        // Quick Attack (+1) would put the slower user first if priority
        // counted; the faster-target branch must still roll.
        let user = mon(MUDKIP, &[move_id, MoveId::QUICK_ATTACK]);
        let target = mon(TREECKO, &[MoveId::COUNTER]);
        assert_eq!(
            viability(move_id, &user, &target, &[AT_BONUS_THRESHOLD]),
            (BONUS, 1),
            "{move_id:?}: slower user holding a priority move"
        );

        // Counter (-5) would put the faster user last, and a Quick Attack
        // target first, if priority counted; the penalty path draws nothing.
        let user = mon(TREECKO, &[move_id, MoveId::COUNTER]);
        let target = mon(MUDKIP, &[MoveId::QUICK_ATTACK]);
        assert_eq!(
            viability(move_id, &user, &target, &[]),
            (PENALTY, 0),
            "{move_id:?}: faster user facing a priority move"
        );
    }
}

#[test]
fn stages_and_paralysis_move_the_comparison() {
    for (move_id, _) in SPEED_DROP_MOVES {
        // Raising the slower user's Speed stage past the target flips the answer.
        let (mut user, target) = target_faster(move_id);
        user.stages_mut().speed = StatStage::MAX;
        assert!(user.speed_for_turn_order() > target.speed_for_turn_order());
        assert_eq!(viability(move_id, &user, &target, &[]), (PENALTY, 0));

        // Raising the slower target's Speed stage past the user does the same.
        let (user, mut target) = target_slower(move_id);
        target.stages_mut().speed = StatStage::MAX;
        assert!(user.speed_for_turn_order() < target.speed_for_turn_order());
        assert_eq!(
            viability(move_id, &user, &target, &[AT_BONUS_THRESHOLD]),
            (BONUS, 1)
        );

        // Paralysis quarters the faster user's Speed below the target's.
        let (mut user, target) = target_slower(move_id);
        user.set_status1(Status1::Paralysed);
        assert!(user.speed_for_turn_order() < target.speed_for_turn_order());
        assert_eq!(
            viability(move_id, &user, &target, &[AT_BONUS_THRESHOLD]),
            (BONUS, 1)
        );

        // Paralysing the faster target drops it below the user.
        let (user, mut target) = target_faster(move_id);
        target.set_status1(Status1::Paralysed);
        assert!(user.speed_for_turn_order() > target.speed_for_turn_order());
        assert_eq!(viability(move_id, &user, &target, &[]), (PENALTY, 0));
    }
}

#[test]
fn the_two_speed_drop_effects_are_admitted_under_viability_only() {
    let dex = Dex::new();
    for (move_id, effect) in SPEED_DROP_MOVES {
        let scoreable = ScoreableEffect::from_raw(effect).expect("a speed drop is a stat label");
        assert!(
            scoreable.supported_by(AiFlags::CHECK_VIABILITY),
            "{move_id:?}"
        );
        for flags in [
            AiFlags::CHECK_VIABILITY,
            AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY),
            AiFlags::CHECK_BAD_MOVE
                .union(AiFlags::TRY_TO_FAINT)
                .union(AiFlags::CHECK_VIABILITY),
        ] {
            assert_eq!(
                ensure_scoreable(&dex, move_id, flags),
                Ok(()),
                "{move_id:?}"
            );
        }
        for flags in [
            AiFlags::SETUP_FIRST_TURN,
            AiFlags::CHECK_VIABILITY.union(AiFlags::SETUP_FIRST_TURN),
        ] {
            assert_eq!(
                ensure_scoreable(&dex, move_id, flags),
                Err(BattleError::UnscoreableMoveEffect(move_id)),
                "{move_id:?}"
            );
        }
    }
}

#[test]
fn every_other_stat_label_stays_refused_under_viability() {
    let dex = Dex::new();
    for move_id in [
        MoveId::SWORDS_DANCE,
        MoveId::HARDEN,
        MoveId::GROWTH,
        MoveId::DOUBLE_TEAM,
        MoveId::SCREECH,
        MoveId::SAND_ATTACK,
        MoveId::FLASH,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, AiFlags::CHECK_VIABILITY),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}

/// Chooses between String Shot (slot 0) and Tackle (slot 1, no viability
/// branch) against a faster player; `script` follows the initialisation draws.
fn choose(script: &[u16]) -> EnemyAction {
    let enemy = mon(MUDKIP, &[MoveId::STRING_SHOT, MoveId::TACKLE]);
    let player = mon(TREECKO, &[MoveId::POUND]);
    let mut rng = SequenceRng::new([0; INIT_DRAWS].into_iter().chain(script.iter().copied()));
    let action = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        AiFlags::CHECK_VIABILITY,
        FIRST_TURN,
        &mut rng,
    )
    .unwrap();
    assert_eq!(rng.draws(), INIT_DRAWS + script.len());
    rng.assert_exhausted();
    action
}

#[test]
fn a_viability_trainer_holding_string_shot_scores_it_before_the_final_tie_break() {
    // String Shot's roll, then the final tie-break draw. A bonus lifts it to
    // 102, so a tie-break of 1 still selects slot 0.
    assert_eq!(choose(&[AT_BONUS_THRESHOLD, 1]), EnemyAction::Move(0));
    // Without the bonus both slots stay at 100 and the tie-break picks slot 1.
    assert_eq!(choose(&[BELOW_BONUS_THRESHOLD, 1]), EnemyAction::Move(1));
}
