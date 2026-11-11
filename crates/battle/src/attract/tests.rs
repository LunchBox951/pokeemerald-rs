use super::{
    draws_love_immobility, ensure_resolvable, is_attract_effect, resolve_attract_move,
    AttractOutcome, EFFECT_ATTRACT,
};
use crate::battle::BattlerSide;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::script_rng::SequenceRng;
use crate::stat_stage::StatStage;
use assets::{MoveId, SpeciesId};

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

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
        5,
        MAX_IVS,
        personality,
        vec![MoveId::ATTRACT],
    )
    .unwrap()
}

fn resolve(
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rolls: &[u16],
) -> Result<AttractOutcome, BattleError> {
    let mut rng = SequenceRng::new(rolls.iter().copied());
    let outcome = resolve_attract_move(&Dex::new(), MoveId::ATTRACT, attacker, defender, &mut rng);
    rng.assert_exhausted();
    outcome
}

#[test]
fn only_attract_uses_the_pipeline() {
    assert!(is_attract_effect(EFFECT_ATTRACT));
    let dex = Dex::new();
    assert_eq!(ensure_resolvable(&dex, MoveId::ATTRACT), Ok(()));
    assert_eq!(
        ensure_resolvable(&dex, MoveId::TACKLE),
        Err(BattleError::UnsupportedMoveEffect(MoveId::TACKLE))
    );
}

#[test]
fn opposite_genders_infatuate_in_both_directions() {
    let male = mon(MUDKIP, MALE_PERSONALITY);
    let female = mon(MUDKIP, FEMALE_PERSONALITY);
    assert_eq!(resolve(&male, &female, &[0]), Ok(AttractOutcome::Applied));
    assert_eq!(resolve(&female, &male, &[0]), Ok(AttractOutcome::Applied));
}

#[test]
fn the_same_gender_fails_after_the_accuracy_draw() {
    let male = mon(MUDKIP, MALE_PERSONALITY);
    let female = mon(MUDKIP, FEMALE_PERSONALITY);
    assert_eq!(resolve(&male, &male, &[0]), Ok(AttractOutcome::Failed));
    assert_eq!(resolve(&female, &female, &[0]), Ok(AttractOutcome::Failed));
}

#[test]
fn a_genderless_battler_on_either_side_fails() {
    let genderless = mon(BELDUM, MALE_PERSONALITY);
    let male = mon(MUDKIP, MALE_PERSONALITY);
    assert_eq!(
        resolve(&genderless, &male, &[0]),
        Ok(AttractOutcome::Failed)
    );
    assert_eq!(
        resolve(&male, &genderless, &[0]),
        Ok(AttractOutcome::Failed)
    );
    assert_eq!(
        resolve(&genderless, &genderless, &[0]),
        Ok(AttractOutcome::Failed)
    );
}

#[test]
fn an_infatuated_target_fails_and_keeps_its_attractor() {
    let male = mon(MUDKIP, MALE_PERSONALITY);
    let mut female = mon(MUDKIP, FEMALE_PERSONALITY);
    female.volatiles_mut().set_infatuated_by(BattlerSide::Enemy);
    assert_eq!(resolve(&male, &female, &[0]), Ok(AttractOutcome::Failed));
    assert_eq!(
        female.volatiles().infatuated_by,
        Some(BattlerSide::Enemy),
        "resolution mutates nothing"
    );
}

#[test]
fn oblivious_outranks_every_other_guard() {
    let male = mon(MUDKIP, MALE_PERSONALITY);
    let female_numel = mon(NUMEL, FEMALE_PERSONALITY);
    let male_numel = mon(NUMEL, MALE_PERSONALITY);
    assert_eq!(
        resolve(&male, &female_numel, &[0]),
        Ok(AttractOutcome::ObliviousProtected)
    );
    assert_eq!(
        resolve(&male, &male_numel, &[0]),
        Ok(AttractOutcome::ObliviousProtected),
        "protection wins over the same-gender failure"
    );
    let mut infatuated = female_numel;
    infatuated
        .volatiles_mut()
        .set_infatuated_by(BattlerSide::Player);
    assert_eq!(
        resolve(&male, &infatuated, &[0]),
        Ok(AttractOutcome::ObliviousProtected),
        "and over the already-infatuated failure"
    );
}

#[test]
fn a_miss_outranks_oblivious_and_the_gender_guards() {
    let mut male = mon(MUDKIP, MALE_PERSONALITY);
    male.stages_mut().accuracy = StatStage::new(-1).unwrap();
    let female_numel = mon(NUMEL, FEMALE_PERSONALITY);
    // Accuracy 100 at -1 is a threshold of 75: a roll of 74 maps to 75.
    assert_eq!(
        resolve(&male, &female_numel, &[74]),
        Ok(AttractOutcome::ObliviousProtected)
    );
    assert_eq!(
        resolve(&male, &female_numel, &[75]),
        Ok(AttractOutcome::Miss)
    );
}

#[test]
fn the_top_accuracy_roll_still_hits_and_is_drawn() {
    let male = mon(MUDKIP, MALE_PERSONALITY);
    let female = mon(MUDKIP, FEMALE_PERSONALITY);
    assert_eq!(resolve(&male, &female, &[99]), Ok(AttractOutcome::Applied));
}

#[test]
fn a_rejected_move_draws_nothing() {
    let male = mon(MUDKIP, MALE_PERSONALITY);
    let female = mon(MUDKIP, FEMALE_PERSONALITY);
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        resolve_attract_move(&Dex::new(), MoveId::TACKLE, &male, &female, &mut rng),
        Err(BattleError::UnsupportedMoveEffect(MoveId::TACKLE))
    );
    assert_eq!(rng.draws(), 0);
}

#[test]
fn an_even_coin_immobilizes_and_an_odd_coin_continues() {
    for (roll, immobilized) in [(0, true), (2, true), (1, false), (3, false)] {
        let mut rng = SequenceRng::new([roll]);
        assert_eq!(draws_love_immobility(&mut rng), immobilized, "roll {roll}");
        rng.assert_exhausted();
    }
}
