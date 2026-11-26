//! `AI_CV_SpeedDownFromChance` -> `AI_CV_SpeedDown`
//! (`data/battle_ai_scripts.s:706`, `:1136`-`:1151`): only Icy Wind, Rock Tomb
//! and Mud Shot reach the Speed branch; every other move with the effect ends
//! at once.

use super::{ensure_scoreable, score_viability, MoveScores};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId, SpeciesId};

const LEVEL: u8 = 5;
const UNCHANGED_SCORE: i8 = 100;
const PENALISED_SCORE: i8 = 97;
const ENCOURAGED_SCORE: i8 = 102;
const INIT_DRAWS: usize = MAX_MON_MOVES;

/// `if_random_less_than 70` falls through to `score +2` from a low byte of 70.
const LOW_BYTE_BELOW_THRESHOLD: u16 = 69;
const LOW_BYTE_AT_THRESHOLD: u16 = 70;
const LOW_BYTE_MAX: u16 = 255;
/// Bit zero clear and set: neither may change a tie's "target faster" answer.
const EVEN_DRAW: u16 = 0;
const ODD_DRAW: u16 = 1;

/// Base Speed 70.
const TREECKO: SpeciesId = SpeciesId(277);
/// Base Speed 40.
const MUDKIP: SpeciesId = SpeciesId(283);

const WHITELISTED: [MoveId; 3] = [MoveId::ICY_WIND, MoveId::ROCK_TOMB, MoveId::MUD_SHOT];

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

/// The user is slower than the target.
fn target_faster(move_id: MoveId) -> (BattlePokemon, BattlePokemon) {
    let (user, target) = (mon(MUDKIP, &[move_id]), mon(TREECKO, &[MoveId::POUND]));
    assert!(user.speed_for_turn_order() < target.speed_for_turn_order());
    (user, target)
}

/// The user is faster than the target.
fn target_slower(move_id: MoveId) -> (BattlePokemon, BattlePokemon) {
    let (user, target) = (mon(TREECKO, &[move_id]), mon(MUDKIP, &[MoveId::POUND]));
    assert!(user.speed_for_turn_order() > target.speed_for_turn_order());
    (user, target)
}

/// Both battlers have exactly the same turn-order Speed.
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
fn a_faster_target_rolls_once_and_the_boundary_is_69_then_70() {
    for move_id in WHITELISTED {
        let (user, target) = target_faster(move_id);
        for roll in [
            0,
            LOW_BYTE_BELOW_THRESHOLD,
            0x0100 | LOW_BYTE_BELOW_THRESHOLD,
        ] {
            assert_eq!(
                viability(move_id, &user, &target, &[roll]),
                (UNCHANGED_SCORE, 1),
                "{move_id:?}: roll {roll:#x} stays below 70"
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
                "{move_id:?}: roll {roll:#x} reaches 70"
            );
        }
    }
}

#[test]
fn a_slower_target_is_penalised_three_with_no_draw() {
    for move_id in WHITELISTED {
        let (user, target) = target_slower(move_id);
        assert_eq!(
            viability(move_id, &user, &target, &[]),
            (PENALISED_SCORE, 0),
            "{move_id:?}"
        );
    }
}

#[test]
fn a_speed_tie_draws_once_and_is_never_target_faster() {
    for move_id in WHITELISTED {
        let (user, target) = speed_tied(move_id);
        // The tie-break draw is discarded whichever way it falls, and the
        // roll that would add +2 is never reached.
        for tie_break in [EVEN_DRAW, ODD_DRAW] {
            assert_eq!(
                viability(move_id, &user, &target, &[tie_break]),
                (PENALISED_SCORE, 1),
                "{move_id:?}: tie draw {tie_break}"
            );
        }
    }
}

#[test]
fn the_chosen_moves_priority_is_ignored_by_the_speed_query() {
    // Quick Attack has priority +1; a priority-aware comparison would put the
    // slower user first. Upstream passes `ignoreChosenMoves`
    // (`src/battle_ai_script_commands.c:1268-1273`).
    let user = mon(MUDKIP, &[MoveId::MUD_SHOT, MoveId::QUICK_ATTACK]);
    let target = mon(TREECKO, &[MoveId::QUICK_ATTACK]);
    assert_eq!(
        viability(MoveId::MUD_SHOT, &user, &target, &[LOW_BYTE_AT_THRESHOLD]),
        (ENCOURAGED_SCORE, 1)
    );
}

#[test]
fn stages_and_paralysis_move_the_comparison() {
    use crate::status1::Status1;
    let move_id = MoveId::MUD_SHOT;

    // Raising the slower user's Speed stage past the target flips the answer.
    let (mut user, target) = target_faster(move_id);
    user.stages_mut().speed = StatStage::MAX;
    assert!(user.speed_for_turn_order() > target.speed_for_turn_order());
    assert_eq!(
        viability(move_id, &user, &target, &[]),
        (PENALISED_SCORE, 0)
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
fn other_speed_down_hit_moves_end_at_once_for_every_speed_relation() {
    for move_id in [MoveId::BUBBLE, MoveId::BUBBLE_BEAM, MoveId::CONSTRICT] {
        assert_eq!(
            Dex::new().move_data(move_id).unwrap().effect,
            MoveEffect::SPEED_DOWN_HIT
        );
        for (user, target) in [
            target_faster(move_id),
            target_slower(move_id),
            speed_tied(move_id),
        ] {
            assert_eq!(
                viability(move_id, &user, &target, &[]),
                (UNCHANGED_SCORE, 0),
                "{move_id:?}"
            );
        }
    }
}

#[test]
fn admission_widens_for_speed_down_hit_only() {
    let dex = Dex::new();
    let flags = AiFlags::CHECK_BAD_MOVE
        .union(AiFlags::TRY_TO_FAINT)
        .union(AiFlags::CHECK_VIABILITY);
    for move_id in [
        MoveId::ICY_WIND,
        MoveId::ROCK_TOMB,
        MoveId::MUD_SHOT,
        MoveId::BUBBLE,
        MoveId::BUBBLE_BEAM,
        MoveId::CONSTRICT,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, flags),
            Ok(()),
            "{move_id:?}"
        );
    }
    // The one remaining unported CHECK_VIABILITY row (HIGH_CRITICAL) stays refused.
    for move_id in [MoveId::SLASH, MoveId::KARATE_CHOP] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, flags),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}
