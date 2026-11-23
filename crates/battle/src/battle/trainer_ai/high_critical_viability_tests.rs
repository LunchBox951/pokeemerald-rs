//! `AI_CV_HighCrit` (`data/battle_ai_scripts.s:1449`-`:1460`) scoring.

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
const BONUS_SCORE: i8 = 101;
const INIT_DRAWS: usize = MAX_MON_MOVES;

/// Upstream's `Random() % 256 < 128` falls through at 128 and above.
const BELOW_THRESHOLD: u16 = 127;
const AT_THRESHOLD: u16 = 128;
/// Only the low byte counts: 256 + 127 reads as 127, 256 + 128 as 128.
const HIGH_BITS: u16 = 256;

const TREECKO: SpeciesId = SpeciesId(277);
const TORCHIC: SpeciesId = SpeciesId(280);
const MUDKIP: SpeciesId = SpeciesId(283);
/// Water/Ground: Grass is quadrupled.
const MARSHTOMP: SpeciesId = SpeciesId(284);
const ZIGZAGOON: SpeciesId = SpeciesId::ZIGZAGOON;
/// Bug/Flying: Grass is quartered.
const NINJASK: SpeciesId = SpeciesId(302);
const DUSKULL: SpeciesId = SpeciesId(361);

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

/// Scores slot 0 under `AI_CheckViability` with `rolls` scripted after the
/// simulated-damage initialisation, returning the score and the draws the
/// viability branch alone spent. A draw past `rolls` panics.
fn viability(
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
    rolls: &[u16],
) -> (i8, usize) {
    let mut rng = SequenceRng::new([0, 0, 0, 0].into_iter().chain(rolls.iter().copied()));
    let mut scores = MoveScores::initialize(user, &mut rng);
    assert_eq!(rng.draws(), INIT_DRAWS);
    score_viability(&Dex::new(), &mut scores, 0, move_id, user, target, &mut rng).unwrap();
    (scores.values[0], rng.draws() - INIT_DRAWS)
}

fn razor_leaf_vs(user: SpeciesId, target: SpeciesId, rolls: &[u16]) -> (i8, usize) {
    viability(
        MoveId::RAZOR_LEAF,
        &mon(user, &[MoveId::RAZOR_LEAF]),
        &mon(target, &[MoveId::POUND]),
        rolls,
    )
}

#[test]
fn resisted_classes_end_at_once_without_a_draw() {
    let cases = [
        ("half", MUDKIP, TREECKO),
        ("half with STAB", TREECKO, TREECKO),
        ("half vs Fire", MUDKIP, TORCHIC),
        ("quarter", MUDKIP, NINJASK),
        ("quarter with STAB", TREECKO, NINJASK),
    ];
    let viability_flags = AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY);
    assert_eq!(
        ensure_scoreable(&Dex::new(), MoveId::RAZOR_LEAF, viability_flags),
        Ok(())
    );
    for (name, user, target) in cases {
        assert_eq!(
            razor_leaf_vs(user, target, &[]),
            (UNCHANGED_SCORE, 0),
            "{name}"
        );
    }
}

#[test]
fn super_effective_classes_spend_one_draw_and_gain_one_from_128() {
    let cases = [
        ("double", MUDKIP, MUDKIP),
        ("double with STAB", TREECKO, MUDKIP),
        ("quadruple", MUDKIP, MARSHTOMP),
        ("quadruple with STAB", TREECKO, MARSHTOMP),
    ];
    for (name, user, target) in cases {
        for roll in [0, BELOW_THRESHOLD, HIGH_BITS | BELOW_THRESHOLD] {
            assert_eq!(
                razor_leaf_vs(user, target, &[roll]),
                (UNCHANGED_SCORE, 1),
                "{name}: roll {roll:#x}"
            );
        }
        for roll in [AT_THRESHOLD, 255, HIGH_BITS | AT_THRESHOLD, 0xFF00 | 255] {
            assert_eq!(
                razor_leaf_vs(user, target, &[roll]),
                (BONUS_SCORE, 1),
                "{name}: roll {roll:#x}"
            );
        }
    }
}

#[test]
fn neutral_classes_draw_a_gate_then_a_second_roll() {
    let cases = [
        ("neutral", MUDKIP, ZIGZAGOON),
        ("neutral with STAB", TREECKO, ZIGZAGOON),
    ];
    for (name, user, target) in cases {
        for gate in [0, BELOW_THRESHOLD, HIGH_BITS | BELOW_THRESHOLD] {
            // A failed gate ends the script: its second roll is never read.
            assert_eq!(
                razor_leaf_vs(user, target, &[gate]),
                (UNCHANGED_SCORE, 1),
                "{name}: gate {gate:#x}"
            );
        }
        for second in [0, BELOW_THRESHOLD, HIGH_BITS | BELOW_THRESHOLD] {
            assert_eq!(
                razor_leaf_vs(user, target, &[AT_THRESHOLD, second]),
                (UNCHANGED_SCORE, 2),
                "{name}: second {second:#x}"
            );
        }
        for second in [AT_THRESHOLD, 255, HIGH_BITS | AT_THRESHOLD] {
            assert_eq!(
                razor_leaf_vs(user, target, &[HIGH_BITS | AT_THRESHOLD, second]),
                (BONUS_SCORE, 2),
                "{name}: second {second:#x}"
            );
        }
    }
}

#[test]
fn an_immune_target_takes_the_neutral_path() {
    // `Cmd_if_type_effectiveness` reports x0 only through a flag the
    // upstream call never sets, and the handler tests no x0 class.
    let user = mon(ZIGZAGOON, &[MoveId::SLASH]);
    let target = mon(DUSKULL, &[MoveId::POUND]);
    assert_eq!(
        viability(MoveId::SLASH, &user, &target, &[BELOW_THRESHOLD]),
        (UNCHANGED_SCORE, 1)
    );
    assert_eq!(
        viability(MoveId::SLASH, &user, &target, &[AT_THRESHOLD, AT_THRESHOLD]),
        (BONUS_SCORE, 2)
    );
}

#[test]
fn every_high_critical_move_takes_the_branch() {
    let dex = Dex::new();
    let user = mon(ZIGZAGOON, &[MoveId::SLASH]);
    let target = mon(ZIGZAGOON, &[MoveId::POUND]);
    for move_id in [
        MoveId::KARATE_CHOP,
        MoveId::RAZOR_LEAF,
        MoveId::CRABHAMMER,
        MoveId::SLASH,
        MoveId::AEROBLAST,
        MoveId::CROSS_CHOP,
        MoveId::AIR_CUTTER,
        MoveId::LEAF_BLADE,
    ] {
        assert_eq!(
            dex.move_data(move_id).unwrap().effect,
            MoveEffect::HIGH_CRITICAL
        );
        let (_, spent) = viability(move_id, &user, &target, &[0]);
        assert_eq!(spent, 1, "{move_id:?}");
    }
}

#[test]
#[should_panic(expected = "SequenceRng exhausted after 5 draws")]
fn a_neutral_target_needs_a_second_draw_the_script_runs_out_of() {
    razor_leaf_vs(MUDKIP, ZIGZAGOON, &[AT_THRESHOLD]);
}

#[test]
#[should_panic(expected = "SequenceRng exhausted after 4 draws")]
fn a_super_effective_target_needs_a_draw_the_script_runs_out_of() {
    razor_leaf_vs(MUDKIP, MUDKIP, &[]);
}

#[test]
fn check_viability_admits_high_critical_moves_and_keeps_the_other_refusals() {
    let dex = Dex::new();
    let viability_flags = AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY);
    for move_id in [MoveId::SLASH, MoveId::RAZOR_LEAF, MoveId::KARATE_CHOP] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability_flags),
            Ok(()),
            "{move_id:?}"
        );
    }
    for move_id in [MoveId::MUD_SHOT, MoveId::VITAL_THROW] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability_flags),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}

#[test]
fn the_chooser_spends_the_viability_draws_between_initialisation_and_the_tie_break() {
    let enemy = mon(MUDKIP, &[MoveId::SLASH, MoveId::POUND]);
    let player = mon(ZIGZAGOON, &[MoveId::POUND]);
    let choose = |rolls: &[u16], tie_break: u16| {
        let script = [0, 0, 0, 0]
            .into_iter()
            .chain(rolls.iter().copied())
            .chain([tie_break]);
        let mut rng = SequenceRng::new(script);
        let action = choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            AiFlags::CHECK_VIABILITY,
            1,
            &mut rng,
        )
        .unwrap();
        rng.assert_exhausted();
        (action, rng.draws())
    };
    // A failed gate leaves Slash tied with Pound, so the tie-break picks.
    assert_eq!(
        choose(&[BELOW_THRESHOLD], 0),
        (EnemyAction::Move(0), INIT_DRAWS + 2)
    );
    assert_eq!(
        choose(&[BELOW_THRESHOLD], 1),
        (EnemyAction::Move(1), INIT_DRAWS + 2)
    );
    // Both rolls passing scores Slash 101, which beats Pound whatever the tie-break says.
    assert_eq!(
        choose(&[AT_THRESHOLD, AT_THRESHOLD], 1),
        (EnemyAction::Move(0), INIT_DRAWS + 3)
    );
}
