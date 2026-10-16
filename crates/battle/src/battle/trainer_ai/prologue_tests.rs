//! Score-and-draw fixtures for the `AI_CheckBadMove` prologue, the audited
//! no-`if_effect` rows (`data/battle_ai_scripts.s:51-104`), and the
//! Paralyze, Confuse and Focus Energy handlers (`:382-403`).

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
use crate::status1::Status1;
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
/// Ground, Sand Veil only: Electric is chart-immune.
const SANDSHREW: SpeciesId = SpeciesId(27);
/// Normal, Limber only.
const DITTO: SpeciesId = SpeciesId(132);
/// Normal, Own Tempo only.
const SPINDA: SpeciesId = SpeciesId(308);
/// Water/Flying, Intimidate only: Electric is quadruple-effective.
const GYARADOS: SpeciesId = SpeciesId(130);

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
        MoveId::CONFUSION,
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
    // SPEED_DOWN_HIT, EVASION_DOWN_HIT, CONFUSE_HIT, VITAL_THROW, SPLASH,
    // QUICK_ATTACK, CHARGE.
    let effects = [3, 2, 17, 29, 41, 43, 70, 74, 76, 78, 85, 103, 174].map(MoveEffect);
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
fn confuse_hit_keeps_the_bad_move_score_and_draw_budget() {
    let dex = Dex::new();
    let target = mon(MUDKIP, &[MoveId::POUND]);
    let all_scripts = AiFlags::CHECK_BAD_MOVE
        .union(AiFlags::TRY_TO_FAINT)
        .union(AiFlags::CHECK_VIABILITY)
        .union(AiFlags::SETUP_FIRST_TURN);
    for move_id in [
        MoveId::CONFUSION,
        MoveId::PSYBEAM,
        MoveId::DIZZY_PUNCH,
        MoveId::DYNAMIC_PUNCH,
        MoveId::SIGNAL_BEAM,
        MoveId::WATER_PULSE,
    ] {
        assert_eq!(dex.move_data(move_id).unwrap().effect, MoveEffect(76));
        assert_eq!(bad_move_score(move_id, &target), FULL, "{move_id:?}");
        let enemy = mon(TREECKO, &[move_id]);
        for flags in [AiFlags::CHECK_BAD_MOVE, all_scripts] {
            assert_eq!(ensure_scoreable(&dex, move_id, flags), Ok(()));
            let mut rng = SequenceRng::new([MAX_DAMAGE_DRAW, 0, 0, 0, 1]);
            let action = choose_trainer_action(&dex, &enemy, &target, flags, 0, &mut rng).unwrap();
            assert_eq!(action, EnemyAction::Move(0));
            assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
        }
    }
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

const MINUS_5: i8 = 95;

fn with_status(species: SpeciesId, status: Status1) -> BattlePokemon {
    let mut target = mon(species, &[MoveId::POUND]);
    target.set_status1(status);
    target
}

fn confused(species: SpeciesId) -> BattlePokemon {
    let mut target = mon(species, &[MoveId::POUND]);
    target.volatiles_mut().set_confusion(3);
    target
}

#[test]
fn paralysis_type_immunity_ends_at_one_minus_ten() {
    let sandshrew = mon(SANDSHREW, &[MoveId::POUND]);
    assert_eq!(bad_move_score(MoveId::THUNDER_WAVE, &sandshrew), MINUS_10);
    // `Score_Minus10` ends the script, so an already-paralysed immune target
    // is not charged again (`data/battle_ai_scripts.s:397-398`).
    let paralysed = with_status(SANDSHREW, Status1::Paralysed);
    assert_eq!(bad_move_score(MoveId::THUNDER_WAVE, &paralysed), MINUS_10);
    let gastly = mon(GASTLY, &[MoveId::POUND]);
    assert_eq!(bad_move_score(MoveId::GLARE, &gastly), MINUS_10);
}

#[test]
fn paralysis_is_discouraged_for_any_existing_primary_status() {
    let moves = [MoveId::THUNDER_WAVE, MoveId::STUN_SPORE, MoveId::GLARE];
    for move_id in moves {
        let healthy = mon(MUDKIP, &[MoveId::POUND]);
        assert_eq!(bad_move_score(move_id, &healthy), FULL, "{move_id:?}");
        for status in [Status1::Paralysed, Status1::Poisoned] {
            let target = with_status(MUDKIP, status);
            assert_eq!(bad_move_score(move_id, &target), MINUS_10, "{move_id:?}");
        }
    }
}

#[test]
fn limber_discourages_paralysis_once() {
    let ditto = mon(DITTO, &[MoveId::POUND]);
    assert_eq!(ditto.ability(), AbilityId::LIMBER);
    assert_eq!(bad_move_score(MoveId::THUNDER_WAVE, &ditto), MINUS_10);
    let paralysed = with_status(DITTO, Status1::Paralysed);
    assert_eq!(bad_move_score(MoveId::THUNDER_WAVE, &paralysed), MINUS_10);
}

#[test]
fn confusion_scores_minus_five_distinct_from_own_tempo_minus_ten() {
    for move_id in [MoveId::CONFUSE_RAY, MoveId::SWEET_KISS] {
        assert_eq!(
            bad_move_score(move_id, &mon(MUDKIP, &[MoveId::POUND])),
            FULL
        );
        assert_eq!(bad_move_score(move_id, &confused(MUDKIP)), MINUS_5);
        // Confusion is tested before the ability read, and Confuse has no
        // type check, so a Ghost move still scores against a Normal target.
        assert_eq!(bad_move_score(move_id, &mon(DITTO, &[MoveId::POUND])), FULL);
    }
}

#[test]
fn own_tempo_discourages_confusion_unless_already_confused() {
    let spinda = mon(SPINDA, &[MoveId::POUND]);
    assert_eq!(spinda.ability(), AbilityId::OWN_TEMPO);
    assert_eq!(bad_move_score(MoveId::CONFUSE_RAY, &spinda), MINUS_10);
    assert_eq!(
        bad_move_score(MoveId::CONFUSE_RAY, &confused(SPINDA)),
        MINUS_5
    );
}

#[test]
fn soundproof_ends_supersonic_before_the_confuse_handler() {
    assert_eq!(
        bad_move_score(MoveId::SUPERSONIC, &confused(MR_MIME)),
        MINUS_10
    );
    assert_eq!(
        bad_move_score(MoveId::CONFUSE_RAY, &confused(MR_MIME)),
        MINUS_5
    );
}

#[test]
fn active_focus_energy_on_the_user_scores_minus_ten() {
    let target = mon(MUDKIP, &[MoveId::POUND]);
    let score = |user: &BattlePokemon| {
        let mut rng = draws();
        let mut scores = MoveScores::initialize(user, &mut rng);
        score_bad_move(
            &Dex::new(),
            &mut scores,
            0,
            MoveId::FOCUS_ENERGY,
            user,
            &target,
        )
        .unwrap();
        assert_eq!(rng.draws(), MAX_MON_MOVES);
        scores.values[0]
    };
    let mut user = mon(TREECKO, &[MoveId::FOCUS_ENERGY]);
    assert_eq!(score(&user), FULL);
    user.volatiles_mut().set_focus_energy();
    assert_eq!(score(&user), MINUS_10);
    // The target's own latch does not matter.
    let mut latched_target = mon(MUDKIP, &[MoveId::POUND]);
    latched_target.volatiles_mut().set_focus_energy();
    assert_eq!(bad_move_score(MoveId::FOCUS_ENERGY, &latched_target), FULL);
}

#[test]
fn status_handlers_pick_the_action_and_draw_budget() {
    let dex = Dex::new();
    let flags = AiFlags::CHECK_BAD_MOVE;
    // Thunder Wave is -10 against Sandshrew, so Pound wins; the draws are the
    // four simulated-damage rolls plus the tie-break.
    let enemy = mon(TREECKO, &[MoveId::THUNDER_WAVE, MoveId::POUND]);
    let mut rng = SequenceRng::new([MAX_DAMAGE_DRAW, 0, 0, 0, 0]);
    let sandshrew = mon(SANDSHREW, &[MoveId::POUND]);
    let action = choose_trainer_action(&dex, &enemy, &sandshrew, flags, 0, &mut rng).unwrap();
    assert_eq!(action, EnemyAction::Move(1));
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);

    // Equal -5 and -5 versus the untouched move: Confuse Ray loses to Pound.
    let enemy = mon(TREECKO, &[MoveId::CONFUSE_RAY, MoveId::POUND]);
    let mut rng = SequenceRng::new([MAX_DAMAGE_DRAW, 0, 0, 0, 0]);
    let action =
        choose_trainer_action(&dex, &enemy, &confused(MUDKIP), flags, 0, &mut rng).unwrap();
    assert_eq!(action, EnemyAction::Move(1));
    assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
}

#[test]
fn try_to_faint_still_rolls_for_a_quadruple_effective_status_move() {
    let dex = Dex::new();
    let flags = AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT);
    let enemy = mon(TREECKO, &[MoveId::THUNDER_WAVE]);
    let gyarados = mon(GYARADOS, &[MoveId::POUND]);
    for (roll, score_delta) in [(79, 0), (80, 2)] {
        let mut rng = SequenceRng::new([MAX_DAMAGE_DRAW, 0, 0, 0, roll, 0]);
        let mut scores = MoveScores::initialize(&enemy, &mut rng);
        score_bad_move(
            &dex,
            &mut scores,
            0,
            MoveId::THUNDER_WAVE,
            &enemy,
            &gyarados,
        )
        .unwrap();
        super::score_try_to_faint(
            &dex,
            &mut scores,
            0,
            MoveId::THUNDER_WAVE,
            &enemy,
            &gyarados,
            &mut rng,
        )
        .unwrap();
        assert_eq!(scores.values[0], FULL + score_delta);
        assert_eq!(rng.draws(), MAX_MON_MOVES + 1);
        let mut chooser_rng = SequenceRng::new([MAX_DAMAGE_DRAW, 0, 0, 0, roll, 0]);
        let action =
            choose_trainer_action(&dex, &enemy, &gyarados, flags, 0, &mut chooser_rng).unwrap();
        assert_eq!(action, EnemyAction::Move(0));
        assert_eq!(chooser_rng.draws(), MAX_MON_MOVES + 2);
    }
}

#[test]
fn status_handler_admission_is_flag_aware_and_exact() {
    let dex = Dex::new();
    let cbm = AiFlags::CHECK_BAD_MOVE;
    for move_id in [
        MoveId::THUNDER_WAVE,
        MoveId::STUN_SPORE,
        MoveId::GLARE,
        MoveId::SUPERSONIC,
        MoveId::CONFUSE_RAY,
        MoveId::SWEET_KISS,
        MoveId::FOCUS_ENERGY,
    ] {
        assert_eq!(ensure_scoreable(&dex, move_id, cbm), Ok(()), "{move_id:?}");
        assert_eq!(
            ensure_scoreable(&dex, move_id, cbm.union(AiFlags::TRY_TO_FAINT)),
            Ok(()),
            "{move_id:?}"
        );
        assert_eq!(
            ensure_scoreable(&dex, move_id, cbm.union(AiFlags::SETUP_FIRST_TURN)),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
    // Only Focus Energy has no `AI_CheckViability` branch.
    let viability = cbm.union(AiFlags::CHECK_VIABILITY);
    assert_eq!(
        ensure_scoreable(&dex, MoveId::FOCUS_ENERGY, viability),
        Ok(())
    );
    for move_id in [MoveId::THUNDER_WAVE, MoveId::CONFUSE_RAY] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, viability),
            Err(BattleError::UnscoreableMoveEffect(move_id))
        );
    }
    // Other handlers, including the Safeguard setter and the other routes into
    // `AI_CBM_Confuse`, stay unscoreable.
    for move_id in [
        MoveId::SAFEGUARD,
        MoveId::TOXIC,
        MoveId::POISON_POWDER,
        MoveId::SWAGGER,
        MoveId::FLATTER,
        MoveId::TEETER_DANCE,
        MoveId::HYPNOSIS,
        MoveId::WILL_O_WISP,
        MoveId::SUBSTITUTE,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, move_id, cbm),
            Err(BattleError::UnscoreableMoveEffect(move_id)),
            "{move_id:?}"
        );
    }
}
