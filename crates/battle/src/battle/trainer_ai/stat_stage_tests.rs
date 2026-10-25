//! Score and draw-budget fixtures for the shared `AI_CBM_*Up` and
//! `AI_CBM_*Down` labels, including Defense Curl and Minimize
//! (`data/battle_ai_scripts.s:248-312`).

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
const FULL: i8 = 100;
const MINUS_10: i8 = 90;

const TREECKO: SpeciesId = SpeciesId(277);
/// Hyper Cutter only.
const PINSIR: SpeciesId = SpeciesId(127);
/// Clear Body only.
const REGIROCK: SpeciesId = SpeciesId(401);
/// White Smoke only.
const TORKOAL: SpeciesId = SpeciesId(321);
/// Speed Boost only.
const NINJASK: SpeciesId = SpeciesId(302);
/// Keen Eye only.
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

/// The `CheckBadMove` score of `move_id`, asserting only the simulated-damage
/// draws are consumed.
fn score(move_id: MoveId, user: &BattlePokemon, target: &BattlePokemon) -> i8 {
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES]);
    let mut scores = MoveScores::initialize(user, &mut rng);
    score_bad_move(&Dex::new(), &mut scores, 0, move_id, user, target).unwrap();
    assert_eq!(rng.draws(), MAX_MON_MOVES, "{move_id:?} drew extra values");
    scores.values[0]
}

/// One real move per label the shipped move data reaches, with the stat and
/// magnitude it must score.
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
        let (cap, near) = if raises { (6, 5) } else { (-6, -5) };
        let user = mon(TREECKO, &[move_id]);
        let target = mon(TREECKO, &[MoveId::POUND]);
        let (capped_user, capped_target, near_user, near_target) = if raises {
            (
                staged(user.clone(), stat, cap),
                target.clone(),
                staged(user.clone(), stat, near),
                target.clone(),
            )
        } else {
            (
                user.clone(),
                staged(target.clone(), stat, cap),
                user.clone(),
                staged(target.clone(), stat, near),
            )
        };
        assert_eq!(score(move_id, &user, &target), FULL, "{move_id:?} neutral");
        assert_eq!(
            score(move_id, &capped_user, &capped_target),
            MINUS_10,
            "{move_id:?} capped"
        );
        // The cap check reads the holder only: a one-stage margin is enough
        // for a two-stage move, so the AI does not model the clamp.
        assert_eq!(
            score(move_id, &near_user, &near_target),
            FULL,
            "{move_id:?} near cap ({magnitude}-stage)"
        );
        // The opposite party's stage never matters.
        let (wrong_user, wrong_target) = if raises {
            (user.clone(), staged(target.clone(), stat, cap))
        } else {
            (staged(user.clone(), stat, -cap), target.clone())
        };
        assert_eq!(
            score(move_id, &wrong_user, &wrong_target),
            FULL,
            "{move_id:?} reads the wrong party"
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
    // Every label the script dispatches: 14 stat-stage labels, each in its
    // one- and two-stage forms, plus Minimize and Defense Curl.
    // Raw sweep of every effect byte: the count is the table-mapping contract.
    let labelled = (0..=255_u8)
        .filter(|raw| ai_stat_label(MoveEffect(*raw)).is_some())
        .count();
    assert_eq!(labelled, 28 + 2);
    for effect in [
        MoveEffect::ALWAYS_HIT,
        MoveEffect::HAZE,
        MoveEffect::TRANSFORM,
        MoveEffect::REFLECT,
    ] {
        assert_eq!(ai_stat_label(effect), None);
    }
}

#[test]
fn every_label_descriptor_scores_its_cap_and_guards_in_script_order() {
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
            Ok(Some(-10)),
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
    // Defense Curl reads Defense, not Evasion, and Minimize the reverse.
    let evasion_capped = staged(user.clone(), ChangedStat::Evasion, 6);
    let defense_capped = staged(user.clone(), ChangedStat::Defense, 6);
    assert_eq!(score(MoveId::DEFENSE_CURL, &evasion_capped, &target), FULL);
    assert_eq!(
        score(MoveId::DEFENSE_CURL, &defense_capped, &target),
        MINUS_10
    );
    assert_eq!(score(MoveId::MINIMIZE, &defense_capped, &target), FULL);
    assert_eq!(score(MoveId::MINIMIZE, &evasion_capped, &target), MINUS_10);
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
        assert_eq!(guarded, MINUS_10, "{move_id:?} vs {ability:?}");
        assert_eq!(control, FULL, "{move_id:?} control");
    }
}

#[test]
fn label_specific_guards_do_not_leak_to_other_labels_or_raises() {
    // Hyper Cutter guards Attack only; Keen Eye Accuracy only; Speed Boost
    // Speed only (`data/battle_ai_scripts.s:277-297`).
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
        assert_eq!(score(move_id, &user, &target), FULL, "{move_id:?}");
    }
    // A raise never reads the target's ability.
    let user = mon(TREECKO, &[MoveId::SWORDS_DANCE]);
    for species in [PINSIR, REGIROCK, TORKOAL, NINJASK, SPEAROW] {
        let target = mon(species, &[MoveId::POUND]);
        assert_eq!(score(MoveId::SWORDS_DANCE, &user, &target), FULL);
    }
}

/// Fall-through order: Attack Down checks the floor, then Hyper Cutter, then
/// the shared Clear Body guard, and the first hit ends the script -- so a
/// capped Attack on a guarded target still takes a single `-10`.
#[test]
fn a_capped_guarded_drop_takes_one_penalty_not_two() {
    let user = mon(TREECKO, &[MoveId::GROWL]);
    for species in [PINSIR, REGIROCK, NINJASK] {
        let target = staged(mon(species, &[MoveId::POUND]), ChangedStat::Attack, -6);
        assert_eq!(score(MoveId::GROWL, &user, &target), MINUS_10);
    }
    let speed_user = mon(TREECKO, &[MoveId::STRING_SHOT]);
    let target = staged(mon(NINJASK, &[MoveId::POUND]), ChangedStat::Speed, -6);
    assert_eq!(score(MoveId::STRING_SHOT, &speed_user, &target), MINUS_10);
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
        // Only Growl and Leer keep the
        // dedicated Attack/Defense drop paths.
        let ported_drop = matches!(move_id, MoveId::GROWL | MoveId::LEER);
        let expected_setup = if ported_drop {
            Ok(())
        } else {
            Err(BattleError::UnscoreableMoveEffect(move_id))
        };
        assert_eq!(setup, expected_setup, "{move_id:?} setup");
        let expected_viability = if ported_drop || move_id == MoveId::DEFENSE_CURL {
            Ok(())
        } else {
            Err(BattleError::UnscoreableMoveEffect(move_id))
        };
        assert_eq!(viability, expected_viability, "{move_id:?} viability");
    }
}
