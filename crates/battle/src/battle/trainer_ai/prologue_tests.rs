//! Score-and-draw fixtures for the `AI_CheckBadMove` prologue and the audited
//! no-`if_effect` rows (`data/battle_ai_scripts.s:51-104`).

use super::{
    choose_trainer_action, ensure_scoreable, fold_ai_type_effectiveness, score_bad_move,
    score_bad_move_prologue, MoveScores, ScoreableEffect, AI_EFFECTIVENESS_NEUTRAL,
    UNCHANGED_AFTER_PROLOGUE_EFFECTS,
};
use crate::battle::opponent_ai::EnemyAction;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use assets::trainers::AiFlags;
use assets::{AbilityId, MoveEffect, MoveId, SpeciesId, Type};

const LEVEL: u8 = 5;
const MAX_DAMAGE_DRAW: u16 = 0;
const FULL: i8 = 100;
const MINUS_10: i8 = 90;
const MINUS_12: i8 = 88;

const TREECKO: SpeciesId = SpeciesId(277);
const MUDKIP: SpeciesId = SpeciesId(283);
const VAPOREON: SpeciesId = SpeciesId(134);
const JOLTEON: SpeciesId = SpeciesId(135);
const FLAREON: SpeciesId = SpeciesId(136);
const SHEDINJA: SpeciesId = SpeciesId(303);
const GASTLY: SpeciesId = SpeciesId(92);
const MR_MIME: SpeciesId = SpeciesId(122);
const VOLTORB: SpeciesId = SpeciesId(100);

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

fn draws() -> SequenceRng {
    SequenceRng::new([MAX_DAMAGE_DRAW; MAX_MON_MOVES])
}

/// Runs only `AI_CheckBadMove` on slot 0 and returns its score; the four
/// simulated-damage draws are the only ones consumed.
fn bad_move_score(move_id: MoveId, target: &BattlePokemon) -> i8 {
    let user = mon(TREECKO, &[move_id]);
    let mut rng = draws();
    let mut scores = MoveScores::initialize(&user, &mut rng);
    score_bad_move(&Dex::new(), &mut scores, 0, move_id, &user, target).unwrap();
    assert_eq!(rng.draws(), MAX_MON_MOVES, "the prologue never draws");
    scores.values[0]
}

fn prologue(move_id: MoveId, target: &BattlePokemon) -> (bool, i8) {
    let user = mon(TREECKO, &[move_id]);
    let mut rng = draws();
    let mut scores = MoveScores::initialize(&user, &mut rng);
    let ended =
        score_bad_move_prologue(&Dex::new(), &mut scores, 0, move_id, &user, target).unwrap();
    assert_eq!(rng.draws(), MAX_MON_MOVES);
    (ended, scores.values[0])
}

#[test]
fn chart_immunity_scores_minus_ten_and_ends() {
    let gastly = mon(GASTLY, &[MoveId::POUND]);
    assert_eq!(prologue(MoveId::POUND, &gastly), (true, MINUS_10));
    assert_eq!(bad_move_score(MoveId::POUND, &gastly), MINUS_10);
}

#[test]
fn power_gate_skips_chart_and_ability_checks_for_unranked_moves() {
    let gastly = mon(GASTLY, &[MoveId::POUND]);
    let shedinja = mon(SHEDINJA, &[MoveId::POUND]);
    let jolteon = mon(JOLTEON, &[MoveId::POUND]);
    // Power 0 (Splash, Charge) and power 1 (Dragon Rage) are `MOVE_POWER_OTHER`.
    assert_eq!(bad_move_score(MoveId::SPLASH, &gastly), FULL);
    assert_eq!(bad_move_score(MoveId::SPLASH, &shedinja), FULL);
    assert_eq!(bad_move_score(MoveId::CHARGE, &jolteon), FULL);
    assert_eq!(bad_move_score(MoveId::CHARGE, &shedinja), FULL);
    assert_eq!(bad_move_score(MoveId::DRAGON_RAGE, &shedinja), FULL);
}

#[test]
fn volt_absorb_cancels_electric_with_minus_twelve() {
    let jolteon = mon(JOLTEON, &[MoveId::POUND]);
    assert_eq!(jolteon.ability(), AbilityId::VOLT_ABSORB);
    assert_eq!(prologue(MoveId::SHOCK_WAVE, &jolteon), (true, MINUS_12));
    assert_eq!(bad_move_score(MoveId::SHOCK_WAVE, &jolteon), MINUS_12);
    assert_eq!(prologue(MoveId::WATER_GUN, &jolteon), (false, FULL));
}

#[test]
fn water_absorb_cancels_water_with_minus_twelve() {
    let vaporeon = mon(VAPOREON, &[MoveId::POUND]);
    assert_eq!(vaporeon.ability(), AbilityId::WATER_ABSORB);
    assert_eq!(bad_move_score(MoveId::WATER_GUN, &vaporeon), MINUS_12);
    assert_eq!(prologue(MoveId::SHOCK_WAVE, &vaporeon), (false, FULL));
}

#[test]
fn flash_fire_cancels_fire_with_minus_twelve() {
    let flareon = mon(FLAREON, &[MoveId::POUND]);
    assert_eq!(flareon.ability(), AbilityId::FLASH_FIRE);
    // Ember's BURN_HIT is not an admitted effect, so drive the prologue.
    assert_eq!(prologue(MoveId::EMBER, &flareon), (true, MINUS_12));
    assert_eq!(prologue(MoveId::WATER_GUN, &flareon), (false, FULL));
}

#[test]
fn wonder_guard_cancels_everything_but_an_exactly_super_effective_hit() {
    let shedinja = mon(SHEDINJA, &[MoveId::POUND]);
    assert_eq!(prologue(MoveId::WATER_GUN, &shedinja), (true, MINUS_10));
    assert_eq!(prologue(MoveId::WING_ATTACK, &shedinja), (false, FULL));
}

#[test]
fn levitate_cancels_ground_with_minus_ten() {
    let gastly = mon(GASTLY, &[MoveId::POUND]);
    assert_eq!(gastly.ability(), AbilityId::LEVITATE);
    assert_eq!(prologue(MoveId::MUD_SHOT, &gastly), (true, MINUS_10));
    assert_eq!(bad_move_score(MoveId::MUD_SHOT, &gastly), MINUS_10);
    assert_eq!(prologue(MoveId::WATER_GUN, &gastly), (false, FULL));
}

#[test]
fn soundproof_runs_after_the_checks_and_ignores_the_power_gate() {
    let mut mime = mon(MR_MIME, &[MoveId::POUND]);
    assert_eq!(bad_move_score(MoveId::GROWL, &mime), MINUS_10);
    // At the stage floor the Growl guard would add a second -10; the
    // Soundproof termination comes first.
    mime.stages_mut().attack = StatStage::MIN;
    assert_eq!(bad_move_score(MoveId::GROWL, &mime), MINUS_10);
    assert_eq!(bad_move_score(MoveId::HYPER_VOICE, &mime), FULL);
}

#[test]
fn every_no_row_effect_is_unchanged_after_a_passing_prologue() {
    let target = mon(MUDKIP, &[MoveId::POUND]);
    let moves = [
        MoveId::POUND,
        MoveId::ABSORB,
        MoveId::POISON_STING,
        MoveId::SWIFT,
        MoveId::FURY_SWIPES,
        MoveId::DRAGON_RAGE,
        MoveId::SLASH,
        MoveId::MUD_SHOT,
        MoveId::VITAL_THROW,
        MoveId::SPLASH,
        MoveId::QUICK_ATTACK,
        MoveId::CHARGE,
    ];
    for move_id in moves {
        let effect = Dex::new().move_data(move_id).unwrap().effect;
        assert!(ScoreableEffect::from_raw(effect).is_some(), "{move_id:?}");
        assert_eq!(bad_move_score(move_id, &target), FULL, "{move_id:?}");
    }
}

#[test]
fn the_audited_row_set_is_exact() {
    // ABSORB, POISON_HIT, ALWAYS_HIT, MULTI_HIT, DRAGON_RAGE, HIGH_CRITICAL,
    // SPEED_DOWN_HIT, EVASION_DOWN_HIT, VITAL_THROW, SPLASH, QUICK_ATTACK, CHARGE.
    let effects = [3, 2, 17, 29, 41, 43, 70, 74, 78, 85, 103, 174].map(MoveEffect);
    let raw: Vec<u8> = effects.iter().map(|effect| effect.0).collect();
    let mut expected = UNCHANGED_AFTER_PROLOGUE_EFFECTS.to_vec();
    expected.sort_unstable();
    let mut actual = raw.clone();
    actual.sort_unstable();
    assert_eq!(actual, expected);
    for effect in effects {
        assert_eq!(
            ScoreableEffect::from_raw(effect),
            Some(ScoreableEffect::UnchangedAfterPrologue(effect))
        );
    }
    // An unlisted effect (Sleep, Burn Hit) is never an implicit no-op.
    assert_eq!(ScoreableEffect::from_raw(MoveEffect(1)), None);
    assert_eq!(ScoreableEffect::from_raw(MoveEffect(4)), None);
}

#[test]
fn admission_is_flag_aware() {
    let dex = Dex::new();
    let only_cbm = AiFlags::CHECK_BAD_MOVE;
    for move_id in [
        MoveId::ABSORB,
        MoveId::SWIFT,
        MoveId::SLASH,
        MoveId::MUD_SHOT,
        MoveId::VITAL_THROW,
        MoveId::QUICK_ATTACK,
        MoveId::SPLASH,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, only_cbm),
            Ok(()),
            "{move_id:?}"
        );
    }
    for move_id in [
        MoveId::ABSORB,
        MoveId::SWIFT,
        MoveId::SLASH,
        MoveId::MUD_SHOT,
        MoveId::VITAL_THROW,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, only_cbm.union(AiFlags::CHECK_VIABILITY)),
            Err(BattleError::UnscoreableMoveEffect(move_id))
        );
    }
    assert_eq!(
        ensure_scoreable(
            &dex,
            MoveId::QUICK_ATTACK,
            only_cbm.union(AiFlags::TRY_TO_FAINT)
        ),
        Err(BattleError::UnscoreableMoveEffect(MoveId::QUICK_ATTACK))
    );
    for move_id in [MoveId::SPLASH, MoveId::POISON_STING] {
        assert_eq!(
            ensure_scoreable(
                &dex,
                move_id,
                only_cbm
                    .union(AiFlags::TRY_TO_FAINT)
                    .union(AiFlags::CHECK_VIABILITY)
                    .union(AiFlags::SETUP_FIRST_TURN)
            ),
            Ok(()),
            "{move_id:?}"
        );
    }
    // Charge's doubled damage is not in the simulated-damage estimate, which
    // only TRY_TO_FAINT reads; CHECK_VIABILITY has no Charge branch.
    assert_eq!(
        ensure_scoreable(&dex, MoveId::CHARGE, only_cbm.union(AiFlags::TRY_TO_FAINT)),
        Err(BattleError::UnscoreableMoveEffect(MoveId::CHARGE))
    );
    for flags in [only_cbm, only_cbm.union(AiFlags::CHECK_VIABILITY)] {
        assert_eq!(ensure_scoreable(&dex, MoveId::CHARGE, flags), Ok(()));
    }
    assert_eq!(
        ensure_scoreable(&dex, MoveId::EMBER, only_cbm),
        Err(BattleError::UnscoreableMoveEffect(MoveId::EMBER))
    );
}

#[test]
fn chooser_refuses_an_unported_branch_before_any_draw() {
    let enemy = mon(TREECKO, &[MoveId::SLASH, MoveId::POUND]);
    let player = mon(MUDKIP, &[MoveId::POUND]);
    let mut rng = draws();
    let refused = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        AiFlags::CHECK_BAD_MOVE.union(AiFlags::CHECK_VIABILITY),
        0,
        &mut rng,
    );
    assert_eq!(
        refused,
        Err(BattleError::UnscoreableMoveEffect(MoveId::SLASH))
    );
    assert_eq!(rng.draws(), 0);
}

#[test]
fn chooser_draws_four_damage_rolls_and_the_tie_break_only() {
    let enemy = mon(TREECKO, &[MoveId::SPLASH, MoveId::SLASH]);
    let player = mon(GASTLY, &[MoveId::POUND]);
    // Slash is chart-immune (-10); Splash is unranked and untouched.
    let mut rng = SequenceRng::new([MAX_DAMAGE_DRAW, 0, 0, 0, 1]);
    let action = choose_trainer_action(
        &Dex::new(),
        &enemy,
        &player,
        AiFlags::CHECK_BAD_MOVE,
        0,
        &mut rng,
    )
    .unwrap();
    assert_eq!(action, EnemyAction::Move(0));
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
}

#[test]
fn a_two_ability_target_is_refused_without_a_guess_draw() {
    let enemy = mon(TREECKO, &[MoveId::SLASH]);
    let player = mon(VOLTORB, &[MoveId::POUND]);
    let mut rng = draws();
    assert_eq!(
        choose_trainer_action(
            &Dex::new(),
            &enemy,
            &player,
            AiFlags::CHECK_BAD_MOVE,
            0,
            &mut rng
        ),
        Err(BattleError::AmbiguousPlayerAbility(VOLTORB))
    );
    assert_eq!(rng.draws(), 0);
}

#[test]
fn dual_type_folding_follows_chart_order_not_defender_slot_order() {
    let electric = |defender_types| {
        fold_ai_type_effectiveness(AI_EFFECTIVENESS_NEUTRAL, Type::Electric, defender_types)
    };
    assert_eq!(
        electric([Type::Ground, Type::Flying]),
        electric([Type::Flying, Type::Ground])
    );
    assert_eq!(electric([Type::Ground, Type::Flying]), 1);
    // Immune Ground then non-immune Flying: a later non-immune row lifts the folded zero back to one.
    let ground_first = fold_ai_type_effectiveness(
        AI_EFFECTIVENESS_NEUTRAL,
        Type::Ground,
        [Type::Flying, Type::Water],
    );
    assert_eq!(ground_first, 0);
    // A single type folds once.
    assert_eq!(
        fold_ai_type_effectiveness(
            AI_EFFECTIVENESS_NEUTRAL,
            Type::Normal,
            [Type::Ghost, Type::Ghost]
        ),
        0
    );
}
