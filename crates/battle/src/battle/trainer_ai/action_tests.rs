//! `choose_trainer_action`: draw budgets, Struggle fallback, and slot selection
//! across the enabled scripts.

use crate::battle::opponent_ai::EnemyAction;
use crate::battle::trainer_ai::choose_trainer_action;
use crate::battle::trainer_ai::damage_estimate::estimated_damage;
use crate::battle::trainer_ai::scores::PERCENT_SCALE;
use crate::battle::trainer_ai::setup_first_turn::FIRST_TURN;
use crate::battle::trainer_ai::setup_first_turn::FIRST_TURN_SETUP_BONUS_THRESHOLD;
use crate::battle::trainer_ai::test_support::first_turn_setup_flags;
use crate::battle::trainer_ai::test_support::pokemon;
use crate::battle::trainer_ai::test_support::rng_with_maximum_simulated_damage;
use crate::battle::trainer_ai::test_support::route_103_flags;
use crate::battle::trainer_ai::test_support::spend_move;
use crate::battle::trainer_ai::test_support::FLAGLESS_DRAWS;
use crate::battle::trainer_ai::test_support::GROWL;
use crate::battle::trainer_ai::test_support::LEER;
use crate::battle::trainer_ai::test_support::MUDKIP;
use crate::battle::trainer_ai::test_support::POUND;
use crate::battle::trainer_ai::test_support::SCRATCH;
use crate::battle::trainer_ai::test_support::SECOND_TURN;
use crate::battle::trainer_ai::test_support::SEISMIC_TOSS;
use crate::battle::trainer_ai::test_support::SELECT_FIRST_TIED_MOVE;
use crate::battle::trainer_ai::test_support::SELECT_SECOND_TIED_MOVE;
use crate::battle::trainer_ai::test_support::SONIC_BOOM;
use crate::battle::trainer_ai::test_support::TACKLE;
use crate::battle::trainer_ai::test_support::TORCHIC;
use crate::battle::trainer_ai::test_support::TREECKO;
use crate::battle::trainer_ai::test_support::VOLTORB;
use crate::battle::trainer_ai::viability::STAT_DROP_DISCOURAGEMENT_THRESHOLD;
use crate::dex::Dex;
use crate::pokemon::MAX_MON_MOVES;
use crate::script_rng::SequenceRng;
use assets::trainers::AiFlags;

#[test]
fn a_flagless_trainer_chooses_among_executable_fixed_damage_moves() {
    let dex = Dex::new();
    let mut enemy = pokemon(TREECKO, vec![SEISMIC_TOSS, SONIC_BOOM, POUND]);
    spend_move(&mut enemy, 2);
    let player = pokemon(VOLTORB, vec![POUND]);
    for (tie, slot) in [(0, 0), (1, 1), (2, 0)] {
        let mut rng = SequenceRng::new([0, 1, 2, 3, tie]);
        assert_eq!(
            choose_trainer_action(&dex, &enemy, &player, AiFlags::NONE, FIRST_TURN, &mut rng),
            Ok(EnemyAction::Move(slot))
        );
        assert_eq!(rng.draws(), FLAGLESS_DRAWS);
        rng.assert_exhausted();
    }
}

#[test]
fn every_spent_move_forces_struggle_without_drawing() {
    let mut enemy = pokemon(TORCHIC, vec![SCRATCH, GROWL]);
    for slot in 0..enemy.moves().len() {
        spend_move(&mut enemy, slot);
    }
    let player = pokemon(TREECKO, vec![POUND]);
    let mut rng = SequenceRng::new([]);

    assert_eq!(
        choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            route_103_flags(),
            FIRST_TURN,
            &mut rng,
        )
        .unwrap(),
        EnemyAction::Struggle
    );
    assert_eq!(rng.draws(), 0);
    rng.assert_exhausted();
}

#[test]
fn opening_tackle_and_growl_cost_setup_viability_and_tie_break_draws() {
    let enemy = pokemon(MUDKIP, vec![TACKLE, GROWL]);
    let player = pokemon(TORCHIC, vec![SCRATCH]);
    let mut rng = rng_with_maximum_simulated_damage([
        STAT_DROP_DISCOURAGEMENT_THRESHOLD,
        SELECT_FIRST_TIED_MOVE,
    ]);

    let action = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        route_103_flags(),
        FIRST_TURN,
        &mut rng,
    )
    .unwrap();

    assert_eq!(action, EnemyAction::Move(0));
    assert_eq!(rng.draws(), MAX_MON_MOVES + 2);
}

#[test]
fn opening_leer_skips_the_viability_roll_and_ties_with_pound() {
    let enemy = pokemon(TREECKO, vec![POUND, LEER]);
    let player = pokemon(MUDKIP, vec![TACKLE]);
    let mut choose_pound = rng_with_maximum_simulated_damage([SELECT_FIRST_TIED_MOVE]);
    let mut choose_leer = rng_with_maximum_simulated_damage([SELECT_SECOND_TIED_MOVE]);

    assert_eq!(
        choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            route_103_flags(),
            FIRST_TURN,
            &mut choose_pound,
        )
        .unwrap(),
        EnemyAction::Move(0)
    );
    assert_eq!(choose_pound.draws(), MAX_MON_MOVES + 1);
    assert_eq!(
        choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            route_103_flags(),
            FIRST_TURN,
            &mut choose_leer,
        )
        .unwrap(),
        EnemyAction::Move(1)
    );
    assert_eq!(choose_leer.draws(), MAX_MON_MOVES + 1);
}

#[test]
fn a_finishable_target_makes_tackle_outscore_growl() {
    let dex = Dex::new();
    let enemy = pokemon(MUDKIP, vec![TACKLE, GROWL]);
    let mut player = pokemon(TORCHIC, vec![SCRATCH]);
    let estimate = estimated_damage(&dex, TACKLE, &enemy, &player, PERCENT_SCALE).unwrap();
    player.apply_damage(player.stats().max_hp - estimate);
    assert_eq!(player.current_hp(), estimate);

    let mut rng = rng_with_maximum_simulated_damage([
        STAT_DROP_DISCOURAGEMENT_THRESHOLD,
        SELECT_FIRST_TIED_MOVE,
    ]);

    assert_eq!(
        choose_trainer_action(
            &dex,
            &enemy,
            &player,
            route_103_flags(),
            FIRST_TURN,
            &mut rng,
        )
        .unwrap(),
        EnemyAction::Move(0)
    );
}

#[test]
fn setup_first_turn_scores_stat_moves_only_on_turn_one() {
    let enemy = pokemon(TREECKO, vec![POUND, LEER]);
    let player = pokemon(MUDKIP, vec![TACKLE]);
    let mut first_turn_rng = rng_with_maximum_simulated_damage([
        FIRST_TURN_SETUP_BONUS_THRESHOLD,
        SELECT_FIRST_TIED_MOVE,
    ]);

    assert_eq!(
        choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            first_turn_setup_flags(),
            FIRST_TURN,
            &mut first_turn_rng,
        )
        .unwrap(),
        EnemyAction::Move(1)
    );
    assert_eq!(first_turn_rng.draws(), MAX_MON_MOVES + 2);

    let mut second_turn_rng = rng_with_maximum_simulated_damage([SELECT_FIRST_TIED_MOVE]);
    assert_eq!(
        choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            first_turn_setup_flags(),
            SECOND_TURN,
            &mut second_turn_rng,
        )
        .unwrap(),
        EnemyAction::Move(0)
    );
    assert_eq!(second_turn_rng.draws(), MAX_MON_MOVES + 1);
}

#[test]
fn a_spent_move_is_neither_scored_nor_selected() {
    let mut enemy = pokemon(MUDKIP, vec![TACKLE, GROWL]);
    spend_move(&mut enemy, 0);
    let player = pokemon(TORCHIC, vec![SCRATCH]);
    let mut rng = rng_with_maximum_simulated_damage([
        STAT_DROP_DISCOURAGEMENT_THRESHOLD,
        SELECT_FIRST_TIED_MOVE,
    ]);

    assert_eq!(
        choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            route_103_flags(),
            FIRST_TURN,
            &mut rng,
        )
        .unwrap(),
        EnemyAction::Move(1)
    );
    assert_eq!(rng.draws(), MAX_MON_MOVES + 2);
    rng.assert_exhausted();
}
