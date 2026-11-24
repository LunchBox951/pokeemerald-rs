//! `AI_CV_SpDefDown` (`data/battle_ai_scripts.s:699,1186`-`:1196`) scoring for
//! Fake Tears and Metal Sound.

use super::{ensure_scoreable, score_viability, MoveScores};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, Stats, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId, SpeciesId};

const LEVEL: u8 = 5;
const UNCHANGED_SCORE: i8 = 100;
const ONE_DISCOURAGEMENT: i8 = 98;
const TWO_DISCOURAGEMENTS: i8 = 96;
const INIT_DRAWS: usize = MAX_MON_MOVES;

/// Low byte 49 is below the threshold; 50 is the first discouraging value.
const LOW_BYTE_BELOW_THRESHOLD: u16 = 49;
const LOW_BYTE_AT_THRESHOLD: u16 = 50;
/// A raw draw with a high byte set whose low byte is still below the
/// threshold, so only the reduction modulo 256 keeps the score.
const HIGH_BYTE_ABOVE_THRESHOLD_LOW_BYTE_BELOW: u16 = 0xFF00 | LOW_BYTE_BELOW_THRESHOLD;

/// Percentages straddling the 70% boundary of every comparison.
const BELOW_SEVENTY: u32 = 69;
const SEVENTY: u32 = 70;
const ABOVE_SEVENTY: u32 = 71;
const FULL: u32 = 100;

/// Upstream's raw stage 3 is offset -3: not above 3, so a draw is taken.
const RAW_STAGE_THREE: i8 = -3;
/// Raw stage 4 is the first above 3: with a healthy user, no draw is taken.
const RAW_STAGE_FOUR: i8 = -2;

const TREECKO: SpeciesId = SpeciesId(277);

/// A Pokemon with a 100 HP maximum, so its HP and HP percent coincide.
fn mon_at_hp_percent(moves: &[MoveId], percent: u32) -> BattlePokemon {
    let pokemon = BattlePokemon::new(
        &Dex::new(),
        TREECKO,
        LEVEL,
        Ivs::default(),
        0,
        moves.to_vec(),
    )
    .expect("test Pokemon must be dex-resident");
    let stats = Stats {
        max_hp: 100,
        ..pokemon.stats()
    };
    pokemon.with_cached_stats(stats, percent)
}

fn with_sp_defense(mut pokemon: BattlePokemon, offset: i8) -> BattlePokemon {
    pokemon.stages_mut().sp_defense = StatStage::new(offset).unwrap();
    pokemon
}

/// Scores slot 0 under `AI_CheckViability` after the simulated-damage
/// initialisation, feeding `rolls` as the only further draws and asserting the
/// branch consumed exactly them.
fn viability(move_id: MoveId, user_hp: u32, target_hp: u32, target_stage: i8, rolls: &[u16]) -> i8 {
    let user = mon_at_hp_percent(&[move_id], user_hp);
    let target = with_sp_defense(mon_at_hp_percent(&[MoveId::POUND], target_hp), target_stage);
    let script = [0; INIT_DRAWS].into_iter().chain(rolls.iter().copied());
    let mut rng = SequenceRng::new(script);
    let mut scores = MoveScores::initialize(&user, &mut rng);
    assert_eq!(rng.draws(), INIT_DRAWS);
    score_viability(
        &Dex::new(),
        &mut scores,
        0,
        move_id,
        &user,
        &target,
        &mut rng,
    )
    .unwrap();
    assert_eq!(rng.draws(), INIT_DRAWS + rolls.len());
    rng.assert_exhausted();
    scores.values[0]
}

const MOVES: [MoveId; 2] = [MoveId::FAKE_TEARS, MoveId::METAL_SOUND];

#[test]
fn both_moves_share_the_special_defense_down_two_effect() {
    let dex = Dex::new();
    for move_id in MOVES {
        assert_eq!(
            dex.move_data(move_id).unwrap().effect,
            MoveEffect::SPECIAL_DEFENSE_DOWN_2,
            "{move_id:?}"
        );
    }
}

#[test]
fn a_hurt_user_draws_and_the_roll_decides_the_first_discouragement() {
    for move_id in MOVES {
        for (roll, expected) in [
            (0, UNCHANGED_SCORE),
            (LOW_BYTE_BELOW_THRESHOLD, UNCHANGED_SCORE),
            (HIGH_BYTE_ABOVE_THRESHOLD_LOW_BYTE_BELOW, UNCHANGED_SCORE),
            (LOW_BYTE_AT_THRESHOLD, ONE_DISCOURAGEMENT),
            (0xFF, ONE_DISCOURAGEMENT),
        ] {
            // The target is above 70% HP, so only the roll can discourage.
            assert_eq!(
                viability(move_id, BELOW_SEVENTY, FULL, 0, &[roll]),
                expected,
                "{move_id:?} roll {roll:#x}"
            );
        }
    }
}

#[test]
fn a_target_at_or_below_seventy_percent_adds_the_second_discouragement() {
    for move_id in MOVES {
        for (roll, expected) in [
            (LOW_BYTE_BELOW_THRESHOLD, ONE_DISCOURAGEMENT),
            (LOW_BYTE_AT_THRESHOLD, TWO_DISCOURAGEMENTS),
        ] {
            assert_eq!(
                viability(move_id, BELOW_SEVENTY, SEVENTY, 0, &[roll]),
                expected,
                "{move_id:?} roll {roll:#x}"
            );
        }
        assert_eq!(
            viability(
                move_id,
                BELOW_SEVENTY,
                ABOVE_SEVENTY,
                0,
                &[LOW_BYTE_AT_THRESHOLD]
            ),
            ONE_DISCOURAGEMENT,
            "{move_id:?}: 71% target HP is above the threshold"
        );
    }
}

#[test]
fn the_user_hp_boundary_is_seventy_percent_inclusive() {
    for move_id in MOVES {
        // At 69% the draw is taken even though the stage would skip it.
        assert_eq!(
            viability(
                move_id,
                BELOW_SEVENTY,
                FULL,
                RAW_STAGE_FOUR,
                &[LOW_BYTE_AT_THRESHOLD]
            ),
            ONE_DISCOURAGEMENT,
            "{move_id:?} at 69%"
        );
        // At exactly 70% "less than 70" fails, so the stage alone decides.
        assert_eq!(
            viability(move_id, SEVENTY, FULL, RAW_STAGE_FOUR, &[]),
            UNCHANGED_SCORE,
            "{move_id:?} at 70%"
        );
        assert_eq!(
            viability(move_id, ABOVE_SEVENTY, FULL, RAW_STAGE_FOUR, &[]),
            UNCHANGED_SCORE,
            "{move_id:?} at 71%"
        );
    }
}

#[test]
fn the_stage_boundary_is_raw_three_not_above() {
    for move_id in MOVES {
        // Raw 3 is not "more than 3": a healthy user still draws.
        assert_eq!(
            viability(
                move_id,
                FULL,
                FULL,
                RAW_STAGE_THREE,
                &[LOW_BYTE_BELOW_THRESHOLD]
            ),
            UNCHANGED_SCORE,
            "{move_id:?} stage -3, roll 49"
        );
        assert_eq!(
            viability(
                move_id,
                FULL,
                FULL,
                RAW_STAGE_THREE,
                &[LOW_BYTE_AT_THRESHOLD]
            ),
            ONE_DISCOURAGEMENT,
            "{move_id:?} stage -3, roll 50"
        );
        // Raw 4 is above 3: no draw is taken, so no script is needed.
        assert_eq!(
            viability(move_id, FULL, FULL, RAW_STAGE_FOUR, &[]),
            UNCHANGED_SCORE,
            "{move_id:?} stage -2"
        );
    }
}

#[test]
fn the_no_draw_path_still_discourages_a_target_at_seventy_percent() {
    for move_id in MOVES {
        assert_eq!(
            viability(move_id, FULL, SEVENTY, 0, &[]),
            ONE_DISCOURAGEMENT,
            "{move_id:?}"
        );
        assert_eq!(
            viability(move_id, FULL, ABOVE_SEVENTY, StatStage::MAX.offset(), &[]),
            UNCHANGED_SCORE,
            "{move_id:?}"
        );
    }
}

#[test]
fn a_heavily_lowered_stage_with_a_weak_target_reaches_minus_four() {
    for move_id in MOVES {
        assert_eq!(
            viability(
                move_id,
                FULL,
                SEVENTY,
                StatStage::MIN.offset(),
                &[LOW_BYTE_AT_THRESHOLD]
            ),
            TWO_DISCOURAGEMENTS,
            "{move_id:?}"
        );
    }
}

#[test]
fn only_the_special_defense_stage_gates_the_draw() {
    for move_id in MOVES {
        let user = mon_at_hp_percent(&[move_id], FULL);
        let mut target = mon_at_hp_percent(&[MoveId::POUND], FULL);
        target.stages_mut().defense = StatStage::new(RAW_STAGE_THREE).unwrap();
        let mut rng = SequenceRng::new([0; INIT_DRAWS]);
        let mut scores = MoveScores::initialize(&user, &mut rng);
        score_viability(
            &Dex::new(),
            &mut scores,
            0,
            move_id,
            &user,
            &target,
            &mut rng,
        )
        .unwrap();
        assert_eq!(scores.values[0], UNCHANGED_SCORE, "{move_id:?}");
        rng.assert_exhausted();
    }
}

#[test]
fn check_viability_admits_these_moves_and_refuses_unported_effects() {
    let dex = Dex::new();
    let viability_flags = AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY);
    for move_id in MOVES {
        assert_eq!(ensure_scoreable(&dex, move_id, viability_flags), Ok(()));
        // `AI_SetupFirstTurn` has no port for this label.
        assert_eq!(
            ensure_scoreable(
                &dex,
                move_id,
                AiFlags::CHECK_BAD_MOVE.union(AiFlags::SETUP_FIRST_TURN)
            ),
            Err(BattleError::UnscoreableMoveEffect(move_id))
        );
    }
    // Other `AI_CV_*Down` labels stay unported, including the one-stage
    // Special Defense drop that shares this label upstream.
    for move_id in [
        MoveId::SCARY_FACE,
        MoveId::SCREECH,
        MoveId::CHARM,
        MoveId::SAND_ATTACK,
        MoveId::COTTON_SPORE,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability_flags),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}
