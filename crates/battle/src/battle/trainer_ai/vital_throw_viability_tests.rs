//! `AI_CV_VitalThrow` (`data/battle_ai_scripts.s:708`, `:1536`-`:1545`): a
//! user that is not outsped and sits at or below 60% HP may lose one point.
//! Between 40% and 60% a low byte of 180 or more gates the final roll; below
//! 40% that gate is skipped. The final roll penalises from a low byte of 50.

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
const INIT_DRAWS: usize = MAX_MON_MOVES;

/// `if_random_less_than 180` ends the script below 180; 180 reaches the
/// final roll.
const GATE_BELOW_THRESHOLD: u16 = 179;
const GATE_AT_THRESHOLD: u16 = 180;
/// `if_random_less_than 50` ends the script below 50; 50 is the first -1.
const PENALTY_BELOW_THRESHOLD: u16 = 49;
const PENALTY_AT_THRESHOLD: u16 = 50;
const LOW_BYTE_MAX: u16 = 255;
/// A raw draw's high byte never reaches `Random() % 256`.
const HIGH_BYTE: u16 = 0x0100;

/// `if_hp_more_than AI_USER, 60` ends the script above 60%.
const HP_ABOVE_UPPER_CUTOFF: u32 = 61;
const HP_AT_UPPER_CUTOFF: u32 = 60;
/// `if_hp_less_than AI_USER, 40` skips the gate below 40%.
const HP_AT_LOWER_CUTOFF: u32 = 40;
const HP_BELOW_LOWER_CUTOFF: u32 = 39;

/// Bit zero clear and set: neither may change a tie's "target faster" answer.
const EVEN_DRAW: u16 = 0;
const ODD_DRAW: u16 = 1;

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

fn target_faster() -> (BattlePokemon, BattlePokemon) {
    let (user, target) = (
        mon(MUDKIP, &[MoveId::VITAL_THROW]),
        mon(TREECKO, &[MoveId::POUND]),
    );
    assert!(user.speed_for_turn_order() < target.speed_for_turn_order());
    (user, target)
}

fn target_slower() -> (BattlePokemon, BattlePokemon) {
    let (user, target) = (
        mon(TREECKO, &[MoveId::VITAL_THROW]),
        mon(MUDKIP, &[MoveId::POUND]),
    );
    assert!(user.speed_for_turn_order() > target.speed_for_turn_order());
    (user, target)
}

fn speed_tied() -> (BattlePokemon, BattlePokemon) {
    let (user, target) = (
        mon(TREECKO, &[MoveId::VITAL_THROW]),
        mon(TREECKO, &[MoveId::POUND]),
    );
    assert_eq!(user.speed_for_turn_order(), target.speed_for_turn_order());
    (user, target)
}

/// Scores slot 0 under `AI_CheckViability` after the simulated-damage
/// initialisation draws, feeding `tail` to the branch and requiring it to be
/// consumed exactly. Returns the score and the draws the branch alone spent.
fn viability(user: &BattlePokemon, target: &BattlePokemon, tail: &[u16]) -> (i8, usize) {
    let mut script = vec![0; INIT_DRAWS];
    script.extend_from_slice(tail);
    let mut rng = SequenceRng::new(script);
    let mut scores = MoveScores::initialize(user, &mut rng);
    assert_eq!(rng.draws(), INIT_DRAWS);
    score_viability(
        &Dex::new(),
        &mut scores,
        0,
        MoveId::VITAL_THROW,
        user,
        target,
        &mut rng,
    )
    .unwrap();
    rng.assert_exhausted();
    (scores.values[0], rng.draws() - INIT_DRAWS)
}

#[test]
fn a_faster_target_ends_the_script_at_every_hp_with_no_draw() {
    let (user, target) = target_faster();
    for percent in [
        PERCENT_SCALE,
        HP_AT_UPPER_CUTOFF,
        HP_AT_LOWER_CUTOFF,
        HP_BELOW_LOWER_CUTOFF,
        1,
    ] {
        let user = at_hp_percent(user.clone(), percent);
        assert_eq!(
            viability(&user, &target, &[]),
            (UNCHANGED_SCORE, 0),
            "{percent}%"
        );
    }
}

#[test]
fn a_user_above_sixty_percent_ends_the_script_with_no_draw() {
    let (user, target) = target_slower();
    for percent in [PERCENT_SCALE, HP_ABOVE_UPPER_CUTOFF] {
        let user = at_hp_percent(user.clone(), percent);
        assert_eq!(
            viability(&user, &target, &[]),
            (UNCHANGED_SCORE, 0),
            "{percent}%"
        );
    }
}

#[test]
fn from_forty_to_sixty_percent_a_gate_of_180_precedes_the_final_roll() {
    let (user, target) = target_slower();
    for percent in [HP_AT_UPPER_CUTOFF, 50, HP_AT_LOWER_CUTOFF] {
        let user = at_hp_percent(user.clone(), percent);
        for gate in [
            0,
            GATE_BELOW_THRESHOLD,
            HIGH_BYTE | GATE_BELOW_THRESHOLD,
            HIGH_BYTE,
        ] {
            assert_eq!(
                viability(&user, &target, &[gate]),
                (UNCHANGED_SCORE, 1),
                "{percent}%: gate {gate:#x} stays below 180"
            );
        }
        for gate in [
            GATE_AT_THRESHOLD,
            LOW_BYTE_MAX,
            HIGH_BYTE | GATE_AT_THRESHOLD,
        ] {
            for (roll, score) in [
                (0, UNCHANGED_SCORE),
                (PENALTY_BELOW_THRESHOLD, UNCHANGED_SCORE),
                (HIGH_BYTE | PENALTY_BELOW_THRESHOLD, UNCHANGED_SCORE),
                (PENALTY_AT_THRESHOLD, PENALISED_SCORE),
                (LOW_BYTE_MAX, PENALISED_SCORE),
                (HIGH_BYTE | PENALTY_AT_THRESHOLD, PENALISED_SCORE),
            ] {
                assert_eq!(
                    viability(&user, &target, &[gate, roll]),
                    (score, 2),
                    "{percent}%: gate {gate:#x} roll {roll:#x}"
                );
            }
        }
    }
}

#[test]
fn below_forty_percent_only_the_final_roll_is_drawn() {
    let (user, target) = target_slower();
    for percent in [HP_BELOW_LOWER_CUTOFF, 1] {
        let user = at_hp_percent(user.clone(), percent);
        // A first draw of 180 or more would be the gate; it is not consumed
        // here, so these values are the final roll.
        for roll in [
            0,
            PENALTY_BELOW_THRESHOLD,
            HIGH_BYTE | PENALTY_BELOW_THRESHOLD,
        ] {
            assert_eq!(
                viability(&user, &target, &[roll]),
                (UNCHANGED_SCORE, 1),
                "{percent}%: roll {roll:#x}"
            );
        }
        for roll in [
            PENALTY_AT_THRESHOLD,
            GATE_BELOW_THRESHOLD,
            GATE_AT_THRESHOLD,
            LOW_BYTE_MAX,
            HIGH_BYTE | PENALTY_AT_THRESHOLD,
        ] {
            assert_eq!(
                viability(&user, &target, &[roll]),
                (PENALISED_SCORE, 1),
                "{percent}%: roll {roll:#x}"
            );
        }
    }
}

#[test]
fn a_speed_tie_draws_once_and_is_never_target_faster() {
    let (user, target) = speed_tied();
    for tie_break in [EVEN_DRAW, ODD_DRAW, LOW_BYTE_MAX] {
        // Above 60% the tie-break is the only draw.
        let healthy = at_hp_percent(user.clone(), HP_ABOVE_UPPER_CUTOFF);
        assert_eq!(
            viability(&healthy, &target, &[tie_break]),
            (UNCHANGED_SCORE, 1),
            "tie draw {tie_break}"
        );
        // From 40% to 60% the gate and final roll follow it.
        let wounded = at_hp_percent(user.clone(), HP_AT_UPPER_CUTOFF);
        assert_eq!(
            viability(&wounded, &target, &[tie_break, GATE_BELOW_THRESHOLD]),
            (UNCHANGED_SCORE, 2),
            "tie draw {tie_break}"
        );
        assert_eq!(
            viability(
                &wounded,
                &target,
                &[tie_break, GATE_AT_THRESHOLD, PENALTY_AT_THRESHOLD]
            ),
            (PENALISED_SCORE, 3),
            "tie draw {tie_break}"
        );
        // Below 40% only the final roll follows it.
        let critical = at_hp_percent(user.clone(), HP_BELOW_LOWER_CUTOFF);
        assert_eq!(
            viability(&critical, &target, &[tie_break, PENALTY_AT_THRESHOLD]),
            (PENALISED_SCORE, 2),
            "tie draw {tie_break}"
        );
    }
}

#[test]
fn the_moves_own_priority_does_not_change_the_speed_comparison() {
    let wounded = |pokemon| at_hp_percent(pokemon, HP_AT_UPPER_CUTOFF);

    // Vital Throw moves last (-1) and Counter even later (-5), so counting
    // priority would put the slower user first and make the target "not
    // faster". Upstream passes `ignoreChosenMoves`
    // (`src/battle_ai_script_commands.c:1268`-`:1273`), so Speed alone
    // decides and the script ends without a draw.
    let user = wounded(mon(MUDKIP, &[MoveId::VITAL_THROW]));
    let target = mon(TREECKO, &[MoveId::COUNTER]);
    assert!(user.speed_for_turn_order() < target.speed_for_turn_order());
    assert_eq!(viability(&user, &target, &[]), (UNCHANGED_SCORE, 0));

    // A faster user's Vital Throw (-1) would go after a Pound (0) if
    // priority counted, making the target "faster" and skipping the rolls.
    let user = wounded(mon(TREECKO, &[MoveId::VITAL_THROW]));
    let target = mon(MUDKIP, &[MoveId::POUND]);
    assert!(user.speed_for_turn_order() > target.speed_for_turn_order());
    assert_eq!(
        viability(&user, &target, &[GATE_AT_THRESHOLD, PENALTY_AT_THRESHOLD]),
        (PENALISED_SCORE, 2)
    );
}

#[test]
fn stages_and_paralysis_move_the_comparison() {
    let wounded = |pokemon| at_hp_percent(pokemon, HP_AT_UPPER_CUTOFF);

    // Raising the slower user's Speed stage past the target flips the answer.
    let (user, target) = target_faster();
    let mut user = wounded(user);
    user.stages_mut().speed = StatStage::MAX;
    assert!(user.speed_for_turn_order() > target.speed_for_turn_order());
    assert_eq!(
        viability(&user, &target, &[GATE_AT_THRESHOLD, PENALTY_AT_THRESHOLD]),
        (PENALISED_SCORE, 2)
    );

    // Paralysis quarters the faster user's Speed below the target's.
    let (user, target) = target_slower();
    let mut user = wounded(user);
    user.set_status1(Status1::Paralysed);
    assert!(user.speed_for_turn_order() < target.speed_for_turn_order());
    assert_eq!(viability(&user, &target, &[]), (UNCHANGED_SCORE, 0));
}

#[test]
fn admission_widens_for_vital_throw_only() {
    let dex = Dex::new();
    let combined = AiFlags::CHECK_BAD_MOVE
        .union(AiFlags::TRY_TO_FAINT)
        .union(AiFlags::CHECK_VIABILITY)
        .union(AiFlags::SETUP_FIRST_TURN);
    for flags in [
        AiFlags::CHECK_BAD_MOVE,
        AiFlags::CHECK_VIABILITY,
        AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY),
        combined,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, MoveId::VITAL_THROW, flags),
            Ok(()),
            "{flags:?}"
        );
    }
    // The one remaining unported viability row stays refused.
    assert_eq!(
        ensure_scoreable(&dex, MoveId::SLASH, AiFlags::CHECK_VIABILITY),
        Err(BattleError::UnscoreableMoveEffect(MoveId::SLASH)),
        "still-unported HIGH_CRITICAL"
    );
}

#[test]
fn a_combined_script_trainer_scores_and_chooses_around_vital_throw() {
    let dex = Dex::new();
    let flags = AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY);

    // A user at 60% that is not outsped: the gate and final roll cost two
    // draws and the -1 drops Vital Throw (99) below Pound (100), so the pick
    // spends its one tie-break draw on the remaining slot.
    let enemy = at_hp_percent(
        mon(TREECKO, &[MoveId::VITAL_THROW, MoveId::POUND]),
        HP_AT_UPPER_CUTOFF,
    );
    let player = mon(MUDKIP, &[MoveId::POUND]);
    let mut rng = SequenceRng::new([0, 0, 0, 0, GATE_AT_THRESHOLD, PENALTY_AT_THRESHOLD, 0]);
    assert_eq!(
        choose_trainer_action(&dex, &enemy, &player, flags, 0, &mut rng),
        Ok(EnemyAction::Move(1))
    );
    assert_eq!(rng.draws(), INIT_DRAWS + 3);
    rng.assert_exhausted();

    // An outsped user spends no viability draw, so Vital Throw and Pound tie
    // at 100 and the single pick draw chooses between them.
    let enemy = at_hp_percent(
        mon(MUDKIP, &[MoveId::VITAL_THROW, MoveId::POUND]),
        HP_AT_UPPER_CUTOFF,
    );
    let player = mon(TREECKO, &[MoveId::POUND]);
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0]);
    assert_eq!(
        choose_trainer_action(&dex, &enemy, &player, flags, 0, &mut rng),
        Ok(EnemyAction::Move(0))
    );
    assert_eq!(rng.draws(), INIT_DRAWS + 1);
    rng.assert_exhausted();
}
