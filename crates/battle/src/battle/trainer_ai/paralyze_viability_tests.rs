//! `AI_CV_Paralyze` (`data/battle_ai_scripts.s:704`, `:1524`-`:1534`): a target
//! that goes first rolls once for +3; otherwise a user at or below 70% HP
//! loses one, with no roll on that branch.

use super::{choose_trainer_action, current_hp_percent, ensure_scoreable, score_viability};
use super::{MoveScores, PERCENT_SCALE};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use crate::status1::Status1;
use assets::trainers::AiFlags;
use assets::{MoveId, SpeciesId};

const LEVEL: u8 = 50;
const UNCHANGED_SCORE: i8 = 100;
const PENALISED_SCORE: i8 = 99;
const ENCOURAGED_SCORE: i8 = 103;
const INIT_DRAWS: usize = MAX_MON_MOVES;

/// `if_random_less_than 20` ends the script below 20; 20 is the first +3.
const LOW_BYTE_BELOW_THRESHOLD: u16 = 19;
const LOW_BYTE_AT_THRESHOLD: u16 = 20;
const LOW_BYTE_MAX: u16 = 255;
/// `if_hp_more_than AI_USER, 70` skips the penalty above 70%.
const HP_AT_THRESHOLD: u32 = 70;
const HP_ABOVE_THRESHOLD: u32 = 71;
/// Bit zero clear and set: neither may change a tie's "target faster" answer.
const EVEN_DRAW: u16 = 0;
const ODD_DRAW: u16 = 1;

const PARALYSING_MOVES: [MoveId; 3] = [MoveId::THUNDER_WAVE, MoveId::STUN_SPORE, MoveId::GLARE];

/// Base Speed 70.
const TREECKO: SpeciesId = SpeciesId(277);
/// Base Speed 40.
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

/// Leaves `pokemon` at the largest HP whose truncated percentage is `percent`
/// (`100 * hp / maxHP`, `src/battle_ai_script_commands.c:737`).
fn at_hp_percent(mut pokemon: BattlePokemon, percent: u32) -> BattlePokemon {
    let max = pokemon.stats().max_hp;
    let hp = (1..=max)
        .rev()
        .find(|hp| PERCENT_SCALE * hp / max == percent)
        .expect("some HP truncates to the percentage");
    pokemon.apply_damage(pokemon.current_hp() - hp);
    assert_eq!(current_hp_percent(&pokemon), percent);
    pokemon
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
fn a_faster_target_rolls_once_and_the_boundary_is_19_then_20() {
    for move_id in PARALYSING_MOVES {
        let (user, target) = target_faster(move_id);
        // The user's HP does not matter on this branch.
        for percent in [PERCENT_SCALE, HP_AT_THRESHOLD, 1] {
            let user = at_hp_percent(user.clone(), percent);
            for roll in [
                0,
                LOW_BYTE_BELOW_THRESHOLD,
                0x0100 | LOW_BYTE_BELOW_THRESHOLD,
            ] {
                assert_eq!(
                    viability(move_id, &user, &target, &[roll]),
                    (UNCHANGED_SCORE, 1),
                    "{move_id:?}: {percent}% roll {roll:#x} stays below 20"
                );
            }
            for roll in [
                LOW_BYTE_AT_THRESHOLD,
                LOW_BYTE_MAX,
                0x0100 | LOW_BYTE_AT_THRESHOLD,
            ] {
                assert_eq!(
                    viability(move_id, &user, &target, &[roll]),
                    (ENCOURAGED_SCORE, 1),
                    "{move_id:?}: {percent}% roll {roll:#x} reaches 20"
                );
            }
        }
    }
}

#[test]
fn a_slower_target_penalises_a_user_at_or_below_seventy_percent_with_no_draw() {
    for move_id in PARALYSING_MOVES {
        let (user, target) = target_slower(move_id);
        for (percent, score) in [
            (HP_AT_THRESHOLD, PENALISED_SCORE),
            (HP_ABOVE_THRESHOLD, UNCHANGED_SCORE),
            (PERCENT_SCALE, UNCHANGED_SCORE),
            (1, PENALISED_SCORE),
        ] {
            let user = at_hp_percent(user.clone(), percent);
            assert_eq!(
                viability(move_id, &user, &target, &[]),
                (score, 0),
                "{move_id:?}: {percent}%"
            );
        }
    }
}

#[test]
fn a_speed_tie_draws_once_and_is_never_target_faster() {
    for move_id in PARALYSING_MOVES {
        let (user, target) = speed_tied(move_id);
        // The tie-break draw is discarded whichever way it falls, and the
        // roll that would add +3 is never reached.
        for tie_break in [EVEN_DRAW, ODD_DRAW, LOW_BYTE_MAX] {
            for (percent, score) in [
                (HP_AT_THRESHOLD, PENALISED_SCORE),
                (HP_ABOVE_THRESHOLD, UNCHANGED_SCORE),
            ] {
                let user = at_hp_percent(user.clone(), percent);
                assert_eq!(
                    viability(move_id, &user, &target, &[tie_break]),
                    (score, 1),
                    "{move_id:?}: {percent}% tie draw {tie_break}"
                );
            }
        }
    }
}

#[test]
fn move_priority_does_not_change_the_speed_comparison() {
    // Quick Attack (+1) against Counter (-5) would put the slower user first
    // if priority counted; upstream passes `ignoreChosenMoves`
    // (`src/battle_ai_script_commands.c:1268-1273`), so the target is still
    // the faster battler.
    let user = mon(MUDKIP, &[MoveId::THUNDER_WAVE, MoveId::QUICK_ATTACK]);
    let target = mon(TREECKO, &[MoveId::COUNTER]);
    assert!(user.speed_for_turn_order() < target.speed_for_turn_order());
    assert_eq!(
        viability(
            MoveId::THUNDER_WAVE,
            &user,
            &target,
            &[LOW_BYTE_AT_THRESHOLD]
        ),
        (ENCOURAGED_SCORE, 1)
    );
}

#[test]
fn stages_and_paralysis_move_the_comparison() {
    let move_id = MoveId::THUNDER_WAVE;

    // Raising the slower user's Speed stage past the target flips the answer.
    let (mut user, target) = target_faster(move_id);
    user.stages_mut().speed = StatStage::MAX;
    assert!(user.speed_for_turn_order() > target.speed_for_turn_order());
    assert_eq!(
        viability(move_id, &user, &target, &[]),
        (UNCHANGED_SCORE, 0)
    );

    // Paralysis quarters the faster user's Speed below the target's.
    let (mut user, target) = target_slower(move_id);
    user.set_status1(Status1::Paralysed);
    assert!(user.speed_for_turn_order() < target.speed_for_turn_order());
    assert_eq!(
        viability(move_id, &user, &target, &[LOW_BYTE_AT_THRESHOLD]),
        (ENCOURAGED_SCORE, 1)
    );
}

#[test]
fn admission_widens_for_paralysis_only() {
    let dex = Dex::new();
    let combined = AiFlags::CHECK_BAD_MOVE
        .union(AiFlags::TRY_TO_FAINT)
        .union(AiFlags::CHECK_VIABILITY);
    for move_id in PARALYSING_MOVES {
        assert_eq!(
            ensure_scoreable(&dex, move_id, combined),
            Ok(()),
            "{move_id:?}"
        );
        assert_eq!(
            ensure_scoreable(&dex, move_id, AiFlags::CHECK_VIABILITY),
            Ok(()),
            "{move_id:?}"
        );
        // `AI_SetupFirstTurn` still rolls for the paralysing moves.
        assert_eq!(
            ensure_scoreable(&dex, move_id, combined.union(AiFlags::SETUP_FIRST_TURN)),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
    // The other status handlers' viability branches stay unported.
    for move_id in [
        MoveId::CONFUSE_RAY,
        MoveId::SUPERSONIC,
        MoveId::SWEET_KISS,
        MoveId::TOXIC,
        MoveId::SWAGGER,
        MoveId::HYPNOSIS,
        MoveId::WILL_O_WISP,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, combined),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}

#[test]
fn a_combined_script_trainer_scores_and_picks_the_paralysing_move() {
    let dex = Dex::new();
    let flags = AiFlags::CHECK_BAD_MOVE
        .union(AiFlags::TRY_TO_FAINT)
        .union(AiFlags::CHECK_VIABILITY);

    // A faster target: one viability roll, and +3 from 20 puts Thunder Wave
    // (103) ahead of Pound (100); the final pick spends the last draw.
    let enemy = mon(MUDKIP, &[MoveId::THUNDER_WAVE, MoveId::POUND]);
    let player = mon(TREECKO, &[MoveId::POUND]);
    let mut rng = SequenceRng::new([0, 0, 0, 0, LOW_BYTE_AT_THRESHOLD, 0]);
    assert_eq!(
        choose_trainer_action(&dex, &enemy, &player, flags, 0, &mut rng),
        Ok(EnemyAction::Move(0))
    );
    assert_eq!(rng.draws(), INIT_DRAWS + 2);
    rng.assert_exhausted();

    // A slower target and a user at 70%: -1 with no roll, so Pound wins
    // after only the final pick's draw.
    let enemy = at_hp_percent(mon(TREECKO, &[MoveId::THUNDER_WAVE, MoveId::POUND]), 70);
    let player = mon(MUDKIP, &[MoveId::POUND]);
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0]);
    assert_eq!(
        choose_trainer_action(&dex, &enemy, &player, flags, 0, &mut rng),
        Ok(EnemyAction::Move(1))
    );
    assert_eq!(rng.draws(), INIT_DRAWS + 1);
    rng.assert_exhausted();
}
