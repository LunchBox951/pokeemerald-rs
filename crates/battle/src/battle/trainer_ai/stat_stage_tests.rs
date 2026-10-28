use super::{
    ai_stat_label, ensure_scoreable, score_bad_move, stat_label_penalty, MoveScores,
    AI_ONLY_STAT_LABELS,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use crate::stat_change::{
    set_stage, stage_of, ChangedStat, StatChangeMagnitude, STAT_CHANGE_EFFECTS,
};
use crate::stat_stage::StatStage;
use assets::trainers::AiFlags;
use assets::{AbilityId, MoveEffect, MoveId, SpeciesId};

const LEVEL: u8 = 5;
const UNPENALIZED_SCORE: i8 = 100;
const STAT_LABEL_PENALTY: i8 = -10;
const SCORE_AFTER_ONE_PENALTY: i8 = UNPENALIZED_SCORE + STAT_LABEL_PENALTY;

const STAT_DIRECTION_LABELS: usize = 14;
const MAGNITUDES_PER_LABEL: usize = 2;
const SPECIAL_STAT_LABEL_EFFECTS: usize = 2;

const TREECKO: SpeciesId = SpeciesId(277);
const PINSIR: SpeciesId = SpeciesId(127);
const REGIROCK: SpeciesId = SpeciesId(401);
const TORKOAL: SpeciesId = SpeciesId(321);
const NINJASK: SpeciesId = SpeciesId(302);
const SPEAROW: SpeciesId = SpeciesId(21);

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

fn staged(mut pokemon: BattlePokemon, stat: ChangedStat, offset: i8) -> BattlePokemon {
    set_stage(&mut pokemon, stat, StatStage::new(offset).unwrap());
    pokemon
}

fn score(move_id: MoveId, user: &BattlePokemon, target: &BattlePokemon) -> i8 {
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES]);
    let mut scores = MoveScores::initialize(user, &mut rng);
    score_bad_move(&Dex::new(), &mut scores, 0, move_id, user, target).unwrap();
    assert_eq!(
        rng.draws(),
        MAX_MON_MOVES,
        "{move_id:?}: CheckBadMove must consume no draws beyond simulated-damage initialization"
    );
    scores.values[0]
}

const MOVE_LABELS: [(MoveId, ChangedStat, i8, bool); 22] = [
    (MoveId::MEDITATE, ChangedStat::Attack, 1, true),
    (MoveId::SWORDS_DANCE, ChangedStat::Attack, 2, true),
    (MoveId::HARDEN, ChangedStat::Defense, 1, true),
    (MoveId::BARRIER, ChangedStat::Defense, 2, true),
    (MoveId::GROWTH, ChangedStat::SpAttack, 1, true),
    (MoveId::TAIL_GLOW, ChangedStat::SpAttack, 2, true),
    (MoveId::AMNESIA, ChangedStat::SpDefense, 2, true),
    (MoveId::AGILITY, ChangedStat::Speed, 2, true),
    (MoveId::DOUBLE_TEAM, ChangedStat::Evasion, 1, true),
    (MoveId::GROWL, ChangedStat::Attack, 1, false),
    (MoveId::CHARM, ChangedStat::Attack, 2, false),
    (MoveId::LEER, ChangedStat::Defense, 1, false),
    (MoveId::SCREECH, ChangedStat::Defense, 2, false),
    (MoveId::STRING_SHOT, ChangedStat::Speed, 1, false),
    (MoveId::SCARY_FACE, ChangedStat::Speed, 2, false),
    (MoveId::FAKE_TEARS, ChangedStat::SpDefense, 2, false),
    (MoveId::SAND_ATTACK, ChangedStat::Accuracy, 1, false),
    (MoveId::SWEET_SCENT, ChangedStat::Evasion, 1, false),
    (MoveId::MINIMIZE, ChangedStat::Evasion, 1, true),
    (MoveId::DEFENSE_CURL, ChangedStat::Defense, 1, true),
    (MoveId::HOWL, ChangedStat::Attack, 1, true),
    (MoveId::IRON_DEFENSE, ChangedStat::Defense, 2, true),
];

#[test]
fn every_move_label_scores_minus_ten_at_its_cap_and_not_one_stage_short() {
    for (move_id, stat, magnitude, raises) in MOVE_LABELS {
        let (cap, one_stage_from_cap) = if raises {
            (StatStage::MAX.offset(), StatStage::MAX.offset() - 1)
        } else {
            (StatStage::MIN.offset(), StatStage::MIN.offset() + 1)
        };
        let user = mon(TREECKO, &[move_id]);
        let target = mon(TREECKO, &[MoveId::POUND]);
        let (capped_user, capped_target, near_user, near_target) = if raises {
            (
                staged(user.clone(), stat, cap),
                target.clone(),
                staged(user.clone(), stat, one_stage_from_cap),
                target.clone(),
            )
        } else {
            (
                user.clone(),
                staged(target.clone(), stat, cap),
                user.clone(),
                staged(target.clone(), stat, one_stage_from_cap),
            )
        };
        assert_eq!(
            score(move_id, &user, &target),
            UNPENALIZED_SCORE,
            "{move_id:?} neutral"
        );
        assert_eq!(
            score(move_id, &capped_user, &capped_target),
            SCORE_AFTER_ONE_PENALTY,
            "{move_id:?} capped"
        );
        assert_eq!(
            score(move_id, &near_user, &near_target),
            UNPENALIZED_SCORE,
            "{move_id:?}: one stage of room avoids the cap penalty even for a {magnitude}-stage change"
        );
        let (opposite_holder_capped_user, opposite_holder_capped_target) = if raises {
            (user.clone(), staged(target.clone(), stat, cap))
        } else {
            (staged(user.clone(), stat, -cap), target.clone())
        };
        assert_eq!(
            score(
                move_id,
                &opposite_holder_capped_user,
                &opposite_holder_capped_target
            ),
            UNPENALIZED_SCORE,
            "{move_id:?}: capping the opposite holder must not change the score"
        );
    }
}

#[test]
fn the_label_table_covers_every_effect_through_the_stat_change_contract() {
    for (effect, change) in STAT_CHANGE_EFFECTS {
        assert_eq!(ai_stat_label(effect), Some(change));
    }
    for (effect, stat, magnitude, direction) in AI_ONLY_STAT_LABELS {
        assert!(
            STAT_CHANGE_EFFECTS.iter().all(|(id, _)| *id != effect),
            "{effect:?} is already in the execution table"
        );
        let label = ai_stat_label(effect).unwrap();
        assert_eq!(
            (label.stat, label.magnitude, label.direction),
            (stat, magnitude, direction)
        );
    }
    let labelled = (0..=255_u8)
        .filter(|raw| ai_stat_label(MoveEffect(*raw)).is_some())
        .count();
    assert_eq!(
        labelled,
        STAT_DIRECTION_LABELS * MAGNITUDES_PER_LABEL + SPECIAL_STAT_LABEL_EFFECTS
    );
    for effect in [
        MoveEffect::ALWAYS_HIT,
        MoveEffect::HAZE,
        MoveEffect::TRANSFORM,
        MoveEffect::REFLECT,
    ] {
        assert_eq!(
            ai_stat_label(effect),
            None,
            "{effect:?} is not a stat label"
        );
    }
}

#[test]
fn every_label_descriptor_has_a_supported_magnitude_and_scores_its_cap() {
    let user = mon(TREECKO, &[MoveId::POUND]);
    let target = mon(TREECKO, &[MoveId::POUND]);
    // Raw sweep of every effect byte, so no label escapes the descriptor check.
    for raw in 0..=255_u8 {
        let Some(label) = ai_stat_label(MoveEffect(raw)) else {
            continue;
        };
        assert_eq!(stat_label_penalty(&user, &target, label), Ok(None), "{raw}");
        let cap = label.cap().offset();
        let (capped_user, capped_target) = if label.affects_user() {
            (staged(user.clone(), label.stat, cap), target.clone())
        } else {
            (user.clone(), staged(target.clone(), label.stat, cap))
        };
        assert_eq!(
            stat_label_penalty(&capped_user, &capped_target, label),
            Ok(Some(STAT_LABEL_PENALTY)),
            "{raw}"
        );
        assert!(matches!(
            label.magnitude,
            StatChangeMagnitude::One | StatChangeMagnitude::Two
        ));
        assert_eq!(
            stage_of(
                if label.affects_user() {
                    &capped_user
                } else {
                    &capped_target
                },
                label.stat
            ),
            label.cap()
        );
    }
}

#[test]
fn defense_curl_and_minimize_score_defense_and_evasion_not_their_flags() {
    let user = mon(TREECKO, &[MoveId::DEFENSE_CURL]);
    let target = mon(TREECKO, &[MoveId::POUND]);
    let evasion_capped = staged(user.clone(), ChangedStat::Evasion, StatStage::MAX.offset());
    let defense_capped = staged(user.clone(), ChangedStat::Defense, StatStage::MAX.offset());
    assert_eq!(
        score(MoveId::DEFENSE_CURL, &evasion_capped, &target),
        UNPENALIZED_SCORE
    );
    assert_eq!(
        score(MoveId::DEFENSE_CURL, &defense_capped, &target),
        SCORE_AFTER_ONE_PENALTY
    );
    assert_eq!(
        score(MoveId::MINIMIZE, &defense_capped, &target),
        UNPENALIZED_SCORE
    );
    assert_eq!(
        score(MoveId::MINIMIZE, &evasion_capped, &target),
        SCORE_AFTER_ONE_PENALTY
    );
}

fn guard_case(
    move_id: MoveId,
    species: SpeciesId,
    ability: AbilityId,
    stat: ChangedStat,
) -> (i8, i8) {
    let user = mon(TREECKO, &[move_id]);
    let target = mon(species, &[MoveId::POUND]);
    assert_eq!(target.ability(), ability);
    assert_eq!(
        target.ability_slots()[1].0,
        0,
        "ability must be unambiguous"
    );
    let control = mon(TREECKO, &[MoveId::POUND]);
    assert_eq!(stage_of(&target, stat), StatStage::NEUTRAL);
    (
        score(move_id, &user, &target),
        score(move_id, &user, &control),
    )
}

#[test]
fn each_ability_guard_discourages_its_stat_drop_at_a_neutral_stage() {
    use AbilityId as A;
    use ChangedStat as S;
    for (move_id, species, ability, stat) in [
        (MoveId::GROWL, PINSIR, A::HYPER_CUTTER, S::Attack),
        (MoveId::CHARM, PINSIR, A::HYPER_CUTTER, S::Attack),
        (MoveId::STRING_SHOT, NINJASK, A::SPEED_BOOST, S::Speed),
        (MoveId::SCARY_FACE, NINJASK, A::SPEED_BOOST, S::Speed),
        (MoveId::SAND_ATTACK, SPEAROW, A::KEEN_EYE, S::Accuracy),
        (MoveId::GROWL, REGIROCK, A::CLEAR_BODY, S::Attack),
        (MoveId::LEER, REGIROCK, A::CLEAR_BODY, S::Defense),
        (MoveId::SCREECH, TORKOAL, A::WHITE_SMOKE, S::Defense),
        (MoveId::STRING_SHOT, TORKOAL, A::WHITE_SMOKE, S::Speed),
        (MoveId::SAND_ATTACK, REGIROCK, A::CLEAR_BODY, S::Accuracy),
        (MoveId::SWEET_SCENT, TORKOAL, A::WHITE_SMOKE, S::Evasion),
        (MoveId::FAKE_TEARS, REGIROCK, A::CLEAR_BODY, S::SpDefense),
    ] {
        let (guarded, control) = guard_case(move_id, species, ability, stat);
        assert_eq!(
            guarded, SCORE_AFTER_ONE_PENALTY,
            "{move_id:?} vs {ability:?}"
        );
        assert_eq!(control, UNPENALIZED_SCORE, "{move_id:?} control");
    }
}

#[test]
fn label_specific_guards_do_not_leak_to_other_labels_or_raises() {
    for (move_id, species) in [
        (MoveId::LEER, PINSIR),
        (MoveId::SAND_ATTACK, PINSIR),
        (MoveId::STRING_SHOT, PINSIR),
        (MoveId::GROWL, SPEAROW),
        (MoveId::LEER, SPEAROW),
        (MoveId::GROWL, NINJASK),
        (MoveId::SAND_ATTACK, NINJASK),
    ] {
        let user = mon(TREECKO, &[move_id]);
        let target = mon(species, &[MoveId::POUND]);
        assert_eq!(
            score(move_id, &user, &target),
            UNPENALIZED_SCORE,
            "{move_id:?}: {species:?}'s ability must not guard this stat label"
        );
    }
    let user = mon(TREECKO, &[MoveId::SWORDS_DANCE]);
    for species in [PINSIR, REGIROCK, TORKOAL, NINJASK, SPEAROW] {
        let target = mon(species, &[MoveId::POUND]);
        assert_eq!(
            score(MoveId::SWORDS_DANCE, &user, &target),
            UNPENALIZED_SCORE,
            "a user-side raise must ignore {species:?}'s ability"
        );
    }
}

// Attack Down checks the floor before Hyper Cutter and Clear Body/White Smoke;
// the first match exits (`data/battle_ai_scripts.s:277-281,308-312,620-622`).
#[test]
fn a_capped_guarded_drop_takes_one_penalty_not_two() {
    let user = mon(TREECKO, &[MoveId::GROWL]);
    for species in [PINSIR, REGIROCK, NINJASK] {
        let target = staged(
            mon(species, &[MoveId::POUND]),
            ChangedStat::Attack,
            StatStage::MIN.offset(),
        );
        assert_eq!(
            score(MoveId::GROWL, &user, &target),
            SCORE_AFTER_ONE_PENALTY,
            "a capped, guarded drop must receive exactly one stat-label penalty"
        );
    }
    let speed_user = mon(TREECKO, &[MoveId::STRING_SHOT]);
    let target = staged(
        mon(NINJASK, &[MoveId::POUND]),
        ChangedStat::Speed,
        StatStage::MIN.offset(),
    );
    assert_eq!(
        score(MoveId::STRING_SHOT, &speed_user, &target),
        SCORE_AFTER_ONE_PENALTY,
        "a capped, guarded drop must receive exactly one stat-label penalty"
    );
}

#[test]
fn stat_labels_are_admitted_only_where_the_other_scripts_are_ported() {
    let dex = Dex::new();
    let cbm = AiFlags::CHECK_BAD_MOVE;
    for (move_id, _, _, _) in MOVE_LABELS {
        assert_eq!(ensure_scoreable(&dex, move_id, cbm), Ok(()), "{move_id:?}");
        assert_eq!(
            ensure_scoreable(&dex, move_id, cbm.union(AiFlags::TRY_TO_FAINT)),
            Ok(()),
            "{move_id:?}"
        );
        let setup = ensure_scoreable(&dex, move_id, cbm.union(AiFlags::SETUP_FIRST_TURN));
        let viability = ensure_scoreable(&dex, move_id, cbm.union(AiFlags::CHECK_VIABILITY));
        let has_dedicated_stat_drop_handler = matches!(
            dex.move_data(move_id).unwrap().effect,
            MoveEffect::ATTACK_DOWN | MoveEffect::DEFENSE_DOWN
        );
        let expected_setup = if has_dedicated_stat_drop_handler {
            Ok(())
        } else {
            Err(BattleError::UnscoreableMoveEffect(move_id))
        };
        assert_eq!(setup, expected_setup, "{move_id:?} setup");
        let expected_viability =
            if has_dedicated_stat_drop_handler || move_id == MoveId::DEFENSE_CURL {
                Ok(())
            } else {
                Err(BattleError::UnscoreableMoveEffect(move_id))
            };
        assert_eq!(viability, expected_viability, "{move_id:?} viability");
    }
}
