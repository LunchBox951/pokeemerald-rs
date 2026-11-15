//! `AI_CBM_Attract` (`data/battle_ai_scripts.s:456`-`:478`) scoring.

use super::{ensure_scoreable, score_bad_move, MoveScores};
use crate::battle::BattlerSide;
use crate::dex::Dex;
use crate::pokemon::{BattlePokemon, Ivs, MAX_MON_MOVES};
use crate::script_rng::SequenceRng;
use assets::trainers::AiFlags;
use assets::{MoveId, SpeciesId};

const LEVEL: u8 = 5;
const UNPENALIZED_SCORE: i8 = 100;
const SCORE_AFTER_ONE_PENALTY: i8 = 90;

/// Low byte below every mixed ratio's threshold, and a neutral nature.
const FEMALE_PERSONALITY: u32 = 0;
/// Low byte above every mixed ratio's threshold, and a neutral nature.
const MALE_PERSONALITY: u32 = 200;

const MUDKIP: SpeciesId = SpeciesId::MUDKIP;
const BELDUM: SpeciesId = SpeciesId::BELDUM;
const NUMEL: SpeciesId = SpeciesId::NUMEL;

fn mon(species: SpeciesId, personality: u32) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        species,
        LEVEL,
        Ivs::default(),
        personality,
        vec![MoveId::ATTRACT],
    )
    .expect("test Pokemon must be dex-resident")
}

fn male(species: SpeciesId) -> BattlePokemon {
    mon(species, MALE_PERSONALITY)
}

fn female(species: SpeciesId) -> BattlePokemon {
    mon(species, FEMALE_PERSONALITY)
}

fn infatuated(mut pokemon: BattlePokemon) -> BattlePokemon {
    pokemon
        .volatiles_mut()
        .set_infatuated_by(BattlerSide::Enemy);
    pokemon
}

fn score(user: &BattlePokemon, target: &BattlePokemon) -> i8 {
    let mut rng = SequenceRng::new([0; MAX_MON_MOVES]);
    let mut scores = MoveScores::initialize(user, &mut rng);
    score_bad_move(&Dex::new(), &mut scores, 0, MoveId::ATTRACT, user, target).unwrap();
    assert_eq!(
        rng.draws(),
        MAX_MON_MOVES,
        "scoring Attract must draw nothing beyond the simulated-damage initialisation"
    );
    scores.values[0]
}

#[test]
fn a_male_user_keeps_the_score_against_a_female_target() {
    assert_eq!(score(&male(MUDKIP), &female(MUDKIP)), UNPENALIZED_SCORE);
}

#[test]
fn a_female_user_keeps_the_score_against_a_male_target() {
    assert_eq!(score(&female(MUDKIP), &male(MUDKIP)), UNPENALIZED_SCORE);
}

#[test]
fn the_same_gender_scores_minus_ten() {
    assert_eq!(score(&male(MUDKIP), &male(MUDKIP)), SCORE_AFTER_ONE_PENALTY);
    assert_eq!(
        score(&female(MUDKIP), &female(MUDKIP)),
        SCORE_AFTER_ONE_PENALTY
    );
}

#[test]
fn a_genderless_user_or_target_scores_minus_ten() {
    assert_eq!(
        score(&male(BELDUM), &female(MUDKIP)),
        SCORE_AFTER_ONE_PENALTY
    );
    assert_eq!(
        score(&male(MUDKIP), &female(BELDUM)),
        SCORE_AFTER_ONE_PENALTY
    );
    assert_eq!(
        score(&male(BELDUM), &female(BELDUM)),
        SCORE_AFTER_ONE_PENALTY
    );
}

#[test]
fn an_already_infatuated_target_scores_minus_ten() {
    assert_eq!(
        score(&male(MUDKIP), &infatuated(female(MUDKIP))),
        SCORE_AFTER_ONE_PENALTY
    );
}

#[test]
fn an_oblivious_target_scores_minus_ten() {
    assert_eq!(
        score(&male(MUDKIP), &female(NUMEL)),
        SCORE_AFTER_ONE_PENALTY
    );
}

#[test]
fn several_reasons_take_the_penalty_once() {
    assert_eq!(
        score(&male(BELDUM), &infatuated(male(NUMEL))),
        SCORE_AFTER_ONE_PENALTY,
        "the script ends at its first penalty"
    );
}

#[test]
fn every_supported_script_admits_attract_and_only_check_bad_move_scores_it() {
    let dex = Dex::new();
    for flags in [
        AiFlags::CHECK_BAD_MOVE,
        AiFlags::CHECK_BAD_MOVE | AiFlags::CHECK_VIABILITY,
        AiFlags::CHECK_BAD_MOVE | AiFlags::SETUP_FIRST_TURN,
        AiFlags::CHECK_BAD_MOVE | AiFlags::TRY_TO_FAINT,
    ] {
        assert_eq!(
            ensure_scoreable(&dex, MoveId::ATTRACT, flags),
            Ok(()),
            "{flags:?}"
        );
    }
}
