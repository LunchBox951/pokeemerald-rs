//! `AI_CV_AttackUp` (`data/battle_ai_scripts.s:896`-`:913`) scoring.
//!
//! Upstream compares the user's raw Attack stage (`0..=12`, neutral `6`); the
//! crate stores the signed offset, so raw `r` is offset `r - 6`.

use super::{choose_trainer_action, ensure_scoreable, score_viability, MoveScores, FIRST_TURN};
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
const FULL_HP_BONUS: i8 = 102;
const ONE_PENALTY: i8 = 99;
const TWO_PENALTIES: i8 = 98;
const THREE_PENALTIES: i8 = 97;

/// Raw stage 8 is offset +2; raw stage 9 is offset +3.
const RAW_8_OFFSET: i8 = 2;
const RAW_9_OFFSET: i8 = 3;
const RAW_12_OFFSET: i8 = 6;
const RAW_0_OFFSET: i8 = -6;

/// `Random() % 256 < 128` skips the full-HP bonus.
const BELOW_BONUS_THRESHOLD: u16 = 127;
const AT_BONUS_THRESHOLD: u16 = 128;
/// `Random() % 256 < 100` skips the high-stage penalty.
const BELOW_STAGE_THRESHOLD: u16 = 99;
const AT_STAGE_THRESHOLD: u16 = 100;
/// `Random() % 256 < 40` skips the low-HP penalty.
const BELOW_HP_THRESHOLD: u16 = 39;
const AT_HP_THRESHOLD: u16 = 40;
/// Only the low byte counts.
const HIGH_BITS: u16 = 256;

const MUDKIP: SpeciesId = SpeciesId(283);

/// One-stage moves and the two-stage Swords Dance share the label.
const ATTACK_UP_MOVES: [MoveId; 4] = [
    MoveId::MEDITATE,
    MoveId::SHARPEN,
    MoveId::HOWL,
    MoveId::SWORDS_DANCE,
];

fn user_at(move_id: MoveId, attack_offset: i8, hp: u32) -> BattlePokemon {
    let dex = Dex::new();
    let mut user = BattlePokemon::new(&dex, MUDKIP, LEVEL, Ivs::default(), 0, vec![move_id])
        .expect("test Pokemon must be dex-resident");
    let stats = Stats {
        max_hp: MAX_HP,
        ..user.stats()
    };
    user = user.with_cached_stats(stats, hp);
    set_stage(
        &mut user,
        ChangedStat::Attack,
        StatStage::new(attack_offset).unwrap(),
    );
    user
}

fn target() -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        MUDKIP,
        LEVEL,
        Ivs::default(),
        0,
        vec![MoveId::TACKLE],
    )
    .expect("test Pokemon must be dex-resident")
}

/// Scores `move_id` through `AI_CheckViability`, feeding `rolls` after the
/// simulated-damage initialisation, and asserts the script drew all of them.
fn viability_score(move_id: MoveId, attack_offset: i8, hp: u32, rolls: &[u16]) -> i8 {
    let user = user_at(move_id, attack_offset, hp);
    assert_eq!(
        ensure_scoreable(&Dex::new(), move_id, AiFlags::CHECK_VIABILITY),
        Ok(()),
        "{move_id:?} must be admitted"
    );
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES].into_iter().chain(rolls.iter().copied()));
    let mut scores = MoveScores::initialize(&user, &mut rng);
    score_viability(
        &Dex::new(),
        &mut scores,
        0,
        move_id,
        &user,
        &target(),
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        rng.draws(),
        MAX_MON_MOVES + rolls.len(),
        "{move_id:?} at offset {attack_offset}, {hp} HP must draw exactly {} roll(s)",
        rolls.len()
    );
    rng.assert_exhausted();
    scores.values[0]
}

fn for_each_attack_up_move(check: impl Fn(MoveId)) {
    let dex = Dex::new();
    let mut effects = Vec::new();
    for move_id in ATTACK_UP_MOVES {
        let effect = dex.move_data(move_id).unwrap().effect;
        assert!(
            matches!(effect, MoveEffect::ATTACK_UP | MoveEffect::ATTACK_UP_2),
            "{move_id:?}"
        );
        effects.push(effect);
        check(move_id);
    }
    assert!(effects.contains(&MoveEffect::ATTACK_UP));
    assert!(effects.contains(&MoveEffect::ATTACK_UP_2));
}

#[test]
fn below_raw_9_at_full_hp_rolls_for_the_bonus_at_128() {
    for_each_attack_up_move(|move_id| {
        for offset in [RAW_0_OFFSET, 0, RAW_8_OFFSET] {
            assert_eq!(
                viability_score(move_id, offset, MAX_HP, &[BELOW_BONUS_THRESHOLD]),
                UNCHANGED,
                "{move_id:?} offset {offset}"
            );
            assert_eq!(
                viability_score(move_id, offset, MAX_HP, &[AT_BONUS_THRESHOLD]),
                FULL_HP_BONUS,
                "{move_id:?} offset {offset}"
            );
        }
    });
}

#[test]
fn a_roll_of_100_does_not_earn_the_full_hp_bonus() {
    for_each_attack_up_move(|move_id| {
        assert_eq!(
            viability_score(move_id, RAW_8_OFFSET, MAX_HP, &[AT_STAGE_THRESHOLD]),
            UNCHANGED
        );
    });
}

#[test]
fn below_raw_9_one_hp_short_of_full_takes_no_bonus_roll() {
    for_each_attack_up_move(|move_id| {
        assert_eq!(
            viability_score(move_id, RAW_8_OFFSET, MAX_HP - 1, &[]),
            UNCHANGED
        );
    });
}

#[test]
fn at_raw_9_and_above_a_roll_of_100_costs_one_point_at_full_hp() {
    for_each_attack_up_move(|move_id| {
        for offset in [RAW_9_OFFSET, RAW_12_OFFSET] {
            assert_eq!(
                viability_score(move_id, offset, MAX_HP, &[BELOW_STAGE_THRESHOLD]),
                UNCHANGED,
                "{move_id:?} offset {offset}"
            );
            assert_eq!(
                viability_score(move_id, offset, MAX_HP, &[AT_STAGE_THRESHOLD]),
                ONE_PENALTY,
                "{move_id:?} offset {offset}"
            );
        }
    });
}

#[test]
fn at_raw_9_the_stage_roll_does_not_wait_for_full_hp() {
    for_each_attack_up_move(|move_id| {
        assert_eq!(
            viability_score(move_id, RAW_9_OFFSET, MAX_HP - 1, &[AT_STAGE_THRESHOLD]),
            ONE_PENALTY
        );
    });
}

#[test]
fn a_roll_of_128_does_not_change_the_high_stage_threshold() {
    for_each_attack_up_move(|move_id| {
        assert_eq!(
            viability_score(move_id, RAW_9_OFFSET, MAX_HP, &[AT_BONUS_THRESHOLD]),
            ONE_PENALTY
        );
    });
}

#[test]
fn above_70_percent_hp_ends_without_a_third_roll() {
    for_each_attack_up_move(|move_id| {
        assert_eq!(viability_score(move_id, 0, 71, &[]), UNCHANGED);
        assert_eq!(
            viability_score(move_id, RAW_9_OFFSET, 71, &[BELOW_STAGE_THRESHOLD]),
            UNCHANGED
        );
    });
}

#[test]
fn at_exactly_70_percent_hp_a_roll_of_40_costs_two_points() {
    for_each_attack_up_move(|move_id| {
        assert_eq!(
            viability_score(move_id, 0, 70, &[BELOW_HP_THRESHOLD]),
            UNCHANGED
        );
        assert_eq!(
            viability_score(move_id, 0, 70, &[AT_HP_THRESHOLD]),
            TWO_PENALTIES
        );
    });
}

#[test]
fn at_exactly_40_percent_hp_a_roll_of_40_costs_two_points() {
    for_each_attack_up_move(|move_id| {
        assert_eq!(
            viability_score(move_id, 0, 40, &[BELOW_HP_THRESHOLD]),
            UNCHANGED
        );
        assert_eq!(
            viability_score(move_id, 0, 40, &[AT_HP_THRESHOLD]),
            TWO_PENALTIES
        );
    });
}

#[test]
fn below_40_percent_hp_costs_two_points_without_a_roll() {
    for_each_attack_up_move(|move_id| {
        assert_eq!(viability_score(move_id, 0, 39, &[]), TWO_PENALTIES);
        assert_eq!(viability_score(move_id, 0, 1, &[]), TWO_PENALTIES);
    });
}

#[test]
fn hp_percent_is_floored_before_the_comparison() {
    let base = user_at(MoveId::MEDITATE, 0, MAX_HP);
    let stats = Stats {
        max_hp: 200,
        ..base.stats()
    };
    let user = base.with_cached_stats(stats, 141);
    // 141 / 200 floors to 70, which is not above 70.
    assert_eq!(super::current_hp_percent(&user), 70);
    let mut rng = SequenceRng::new([0, 0, 0, 0, AT_HP_THRESHOLD]);
    let mut scores = MoveScores::initialize(&user, &mut rng);
    score_viability(
        &Dex::new(),
        &mut scores,
        0,
        MoveId::MEDITATE,
        &user,
        &target(),
        &mut rng,
    )
    .unwrap();
    rng.assert_exhausted();
    assert_eq!(scores.values[0], TWO_PENALTIES);
}

#[test]
fn a_high_stage_at_exactly_70_percent_hp_can_take_both_rolls() {
    for_each_attack_up_move(|move_id| {
        for (rolls, expected) in [
            ([BELOW_STAGE_THRESHOLD, BELOW_HP_THRESHOLD], UNCHANGED),
            ([AT_STAGE_THRESHOLD, BELOW_HP_THRESHOLD], ONE_PENALTY),
            ([BELOW_STAGE_THRESHOLD, AT_HP_THRESHOLD], TWO_PENALTIES),
            ([AT_STAGE_THRESHOLD, AT_HP_THRESHOLD], THREE_PENALTIES),
        ] {
            assert_eq!(
                viability_score(move_id, RAW_9_OFFSET, 70, &rolls),
                expected,
                "{move_id:?} {rolls:?}"
            );
        }
    });
}

#[test]
fn a_high_stage_below_40_percent_hp_rolls_once_then_takes_the_low_hp_penalty() {
    for_each_attack_up_move(|move_id| {
        assert_eq!(
            viability_score(move_id, RAW_9_OFFSET, 39, &[BELOW_STAGE_THRESHOLD]),
            TWO_PENALTIES
        );
        assert_eq!(
            viability_score(move_id, RAW_9_OFFSET, 39, &[AT_STAGE_THRESHOLD]),
            THREE_PENALTIES
        );
    });
}

#[test]
fn the_rolls_read_only_the_low_byte() {
    for_each_attack_up_move(|move_id| {
        assert_eq!(
            viability_score(move_id, 0, MAX_HP, &[HIGH_BITS + BELOW_BONUS_THRESHOLD]),
            UNCHANGED
        );
        assert_eq!(
            viability_score(move_id, 0, MAX_HP, &[HIGH_BITS + AT_BONUS_THRESHOLD]),
            FULL_HP_BONUS
        );
        assert_eq!(
            viability_score(move_id, 0, 70, &[HIGH_BITS + AT_HP_THRESHOLD]),
            TWO_PENALTIES
        );
    });
}

#[test]
fn only_the_users_attack_stage_is_read() {
    let mut user = user_at(MoveId::MEDITATE, 0, MAX_HP);
    set_stage(&mut user, ChangedStat::SpAttack, StatStage::MAX);
    let mut foe = target();
    set_stage(&mut foe, ChangedStat::Attack, StatStage::MAX);
    let mut rng = SequenceRng::new([0, 0, 0, 0, AT_BONUS_THRESHOLD]);
    let mut scores = MoveScores::initialize(&user, &mut rng);
    score_viability(
        &Dex::new(),
        &mut scores,
        0,
        MoveId::MEDITATE,
        &user,
        &foe,
        &mut rng,
    )
    .unwrap();
    rng.assert_exhausted();
    assert_eq!(scores.values[0], FULL_HP_BONUS);
}

#[test]
fn check_viability_admits_attack_up_but_not_first_turn_setup_or_other_stat_effects() {
    let dex = Dex::new();
    let viability = AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY);
    for move_id in ATTACK_UP_MOVES {
        assert_eq!(ensure_scoreable(&dex, move_id, viability), Ok(()));
        assert_eq!(
            ensure_scoreable(&dex, move_id, AiFlags::CHECK_VIABILITY),
            Ok(())
        );
        for setup in [
            AiFlags::SETUP_FIRST_TURN,
            AiFlags::CHECK_VIABILITY.union(AiFlags::SETUP_FIRST_TURN),
        ] {
            assert_eq!(
                ensure_scoreable(&dex, move_id, setup),
                Err(BattleError::UnscoreableMoveEffect(move_id)),
                "{move_id:?} {setup:?}"
            );
        }
    }
    for move_id in [
        MoveId::HARDEN,
        MoveId::GROWTH,
        MoveId::DOUBLE_TEAM,
        MoveId::AGILITY,
        MoveId::SCREECH,
        MoveId::CHARM,
        MoveId::SAND_ATTACK,
        MoveId::STRING_SHOT,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}

#[test]
fn the_chooser_spends_the_bonus_draw_before_the_tie_break() {
    let enemy = BattlePokemon::new(
        &Dex::new(),
        MUDKIP,
        LEVEL,
        Ivs::default(),
        0,
        vec![MoveId::SWORDS_DANCE, MoveId::TACKLE],
    )
    .unwrap();
    let choose = |roll: u16, tie_break: u16| {
        let mut rng = SequenceRng::new([0, 0, 0, 0, roll, tie_break]);
        let action = choose_trainer_action(
            &Dex::new(),
            &enemy,
            &target(),
            AiFlags::CHECK_VIABILITY,
            FIRST_TURN,
            &mut rng,
        )
        .unwrap();
        rng.assert_exhausted();
        action
    };
    // The bonus makes Swords Dance the unique best move, whatever the tie-break.
    assert_eq!(choose(AT_BONUS_THRESHOLD, 0), EnemyAction::Move(0));
    assert_eq!(choose(AT_BONUS_THRESHOLD, 1), EnemyAction::Move(0));
    // Without it the two moves tie and the tie-break picks.
    assert_eq!(choose(BELOW_BONUS_THRESHOLD, 0), EnemyAction::Move(0));
    assert_eq!(choose(BELOW_BONUS_THRESHOLD, 1), EnemyAction::Move(1));
}
