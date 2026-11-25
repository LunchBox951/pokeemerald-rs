//! `AI_CV_Absorb` (`data/battle_ai_scripts.s:789`-`:798`) scoring.

use super::{choose_trainer_action, ensure_scoreable, score_viability, MoveScores};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId, SpeciesId};

const LEVEL: u8 = 5;
const UNCHANGED_SCORE: i8 = 100;
const PENALISED_SCORE: i8 = 97;
const INIT_DRAWS: usize = MAX_MON_MOVES;

/// Low byte 49 is below the threshold; 50 is the first penalised value.
const LOW_BYTE_BELOW_THRESHOLD: u16 = 49;
const LOW_BYTE_AT_THRESHOLD: u16 = 50;
const LOW_BYTE_MAX: u16 = 255;
/// A raw draw whose low byte is zero, with the high byte set.
const HIGH_BYTE_ONLY: u16 = 0x0100;

const TREECKO: SpeciesId = SpeciesId(277);
const TORCHIC: SpeciesId = SpeciesId(280);
const MUDKIP: SpeciesId = SpeciesId(283);
const ZIGZAGOON: SpeciesId = SpeciesId::ZIGZAGOON;
/// Bug/Flying: Grass is quartered.
const NINJASK: SpeciesId = SpeciesId(302);

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

/// Scores slot 0 under `AI_CheckViability` with `roll` as the one draw after
/// the simulated-damage initialisation, returning the score and the number of
/// draws the viability branch alone spent.
fn viability(
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
    roll: u16,
) -> (i8, usize) {
    let mut rng = SequenceRng::new([0, 0, 0, 0, roll]);
    let mut scores = MoveScores::initialize(user, &mut rng);
    assert_eq!(rng.draws(), INIT_DRAWS);
    score_viability(&Dex::new(), &mut scores, 0, move_id, user, target, &mut rng).unwrap();
    (scores.values[0], rng.draws() - INIT_DRAWS)
}

#[test]
fn resisted_draining_moves_spend_one_draw_and_penalise_from_fifty() {
    // (move, user, target): half or quarter, with and without same-type bonus.
    let cases = [
        ("half", MoveId::ABSORB, MUDKIP, TREECKO),
        ("half with STAB", MoveId::ABSORB, TREECKO, TREECKO),
        ("quarter", MoveId::ABSORB, MUDKIP, NINJASK),
        ("quarter with STAB", MoveId::ABSORB, TREECKO, NINJASK),
        ("Mega Drain half", MoveId::MEGA_DRAIN, MUDKIP, TORCHIC),
        (
            "Giga Drain half with STAB",
            MoveId::GIGA_DRAIN,
            TREECKO,
            TORCHIC,
        ),
        ("Leech Life half", MoveId::LEECH_LIFE, MUDKIP, TORCHIC),
    ];
    for (name, move_id, user, target) in cases {
        let user = mon(user, &[move_id]);
        let target = mon(target, &[MoveId::POUND]);
        for roll in [
            0,
            LOW_BYTE_BELOW_THRESHOLD,
            HIGH_BYTE_ONLY,
            0x0100 | LOW_BYTE_BELOW_THRESHOLD,
        ] {
            assert_eq!(
                viability(move_id, &user, &target, roll),
                (UNCHANGED_SCORE, 1),
                "{name}: roll {roll:#x} is below 50 once reduced to its low byte"
            );
        }
        for roll in [
            LOW_BYTE_AT_THRESHOLD,
            LOW_BYTE_MAX,
            0x0100 | LOW_BYTE_AT_THRESHOLD,
            0xFF00 | LOW_BYTE_MAX,
        ] {
            assert_eq!(
                viability(move_id, &user, &target, roll),
                (PENALISED_SCORE, 1),
                "{name}: roll {roll:#x} reaches 50 once reduced to its low byte"
            );
        }
    }
}

#[test]
fn every_other_effectiveness_class_spends_no_draw_and_keeps_the_score() {
    // A resisted control proves the harness draws when it should.
    let resisted = viability(
        MoveId::ABSORB,
        &mon(MUDKIP, &[MoveId::ABSORB]),
        &mon(TREECKO, &[MoveId::POUND]),
        LOW_BYTE_MAX,
    );
    assert_eq!(resisted, (PENALISED_SCORE, 1));

    let cases = [
        ("neutral", MoveId::ABSORB, TREECKO, ZIGZAGOON),
        ("neutral with STAB", MoveId::ABSORB, MUDKIP, ZIGZAGOON),
        ("super effective", MoveId::ABSORB, MUDKIP, MUDKIP),
        ("super effective with STAB", MoveId::ABSORB, TREECKO, MUDKIP),
    ];
    for (name, move_id, user, target) in cases {
        let user = mon(user, &[move_id]);
        let target = mon(target, &[MoveId::POUND]);
        assert_eq!(
            viability(move_id, &user, &target, LOW_BYTE_MAX),
            (UNCHANGED_SCORE, 0),
            "{name}"
        );
    }
}

#[test]
fn check_viability_admits_draining_moves_and_keeps_the_other_refusals() {
    let dex = Dex::new();
    let viability_flags = AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY);
    for move_id in [
        MoveId::ABSORB,
        MoveId::MEGA_DRAIN,
        MoveId::GIGA_DRAIN,
        MoveId::LEECH_LIFE,
    ] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, MoveEffect::ABSORB);
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability_flags),
            Ok(()),
            "{move_id:?}"
        );
    }
    for move_id in [MoveId::SLASH, MoveId::VITAL_THROW] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability_flags),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}

#[test]
fn the_chooser_spends_the_resisted_draw_between_initialisation_and_the_tie_break() {
    let enemy = mon(MUDKIP, &[MoveId::ABSORB, MoveId::POUND]);
    let player = mon(TREECKO, &[MoveId::POUND]);
    let choose = |roll: u16, tie_break: u16| {
        let mut rng = SequenceRng::new([0, 0, 0, 0, roll, tie_break]);
        let action = choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            AiFlags::CHECK_VIABILITY,
            1,
            &mut rng,
        )
        .unwrap();
        (action, rng.draws())
    };
    let tie_break_draws = INIT_DRAWS + 2;
    // Penalised Absorb loses to Pound whatever the tie-break says.
    assert_eq!(
        choose(LOW_BYTE_AT_THRESHOLD, 0),
        (EnemyAction::Move(1), tie_break_draws)
    );
    // Unpenalised Absorb ties Pound, so the tie-break picks.
    assert_eq!(
        choose(LOW_BYTE_BELOW_THRESHOLD, 0),
        (EnemyAction::Move(0), tie_break_draws)
    );
    assert_eq!(
        choose(LOW_BYTE_BELOW_THRESHOLD, 1),
        (EnemyAction::Move(1), tie_break_draws)
    );
}
