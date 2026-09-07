use super::{
    ensure_resolvable, is_defense_curl_effect, resolve_defense_curl_move, DefenseCurlOutcome,
    EFFECT_DEFENSE_CURL,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, Ivs};
use crate::stat_change::{ChangedStat, StatChangeDirection, StatChangeEffect, StatChangeMagnitude};
use crate::stat_stage::StatStage;
use assets::{MoveId, SpeciesId};

const DEFENSE_CURL: MoveId = MoveId(111);
const TACKLE: MoveId = MoveId(33);
const TEST_SPECIES: SpeciesId = SpeciesId(1);
const TEST_LEVEL: u8 = 5;
const TEST_PERSONALITY: u32 = 0;

const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

fn mon(dex: &Dex, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(
        dex,
        TEST_SPECIES,
        TEST_LEVEL,
        MAX_IVS,
        TEST_PERSONALITY,
        moves,
    )
    .unwrap()
}

#[test]
fn defense_curl_is_resolvable_and_nothing_else_is() {
    let dex = Dex::new();
    assert_eq!(
        dex.move_data(DEFENSE_CURL).unwrap().effect,
        EFFECT_DEFENSE_CURL
    );
    assert!(is_defense_curl_effect(EFFECT_DEFENSE_CURL));
    assert_eq!(ensure_resolvable(&dex, DEFENSE_CURL), Ok(()));

    assert!(!is_defense_curl_effect(
        dex.move_data(TACKLE).unwrap().effect
    ));
    assert_eq!(
        ensure_resolvable(&dex, TACKLE),
        Err(BattleError::UnsupportedMoveEffect(TACKLE))
    );
}

#[test]
fn an_unknown_move_is_reported_as_such() {
    let dex = Dex::new();
    let attacker = mon(&dex, vec![DEFENSE_CURL]);
    let unknown = MoveId(60_000);
    assert_eq!(
        resolve_defense_curl_move(&dex, unknown, &attacker),
        Err(BattleError::UnknownMove(unknown))
    );
    assert_eq!(
        ensure_resolvable(&dex, unknown),
        Err(BattleError::UnknownMove(unknown))
    );
}

#[test]
fn an_unraised_defense_climbs_by_one_stage_and_is_not_capped() {
    let dex = Dex::new();
    let attacker = mon(&dex, vec![DEFENSE_CURL]);
    assert_eq!(attacker.stages().defense, StatStage::NEUTRAL);

    let outcome = resolve_defense_curl_move(&dex, DEFENSE_CURL, &attacker).unwrap();
    assert_eq!(
        outcome,
        DefenseCurlOutcome {
            change: StatChangeEffect {
                stat: ChangedStat::Defense,
                magnitude: StatChangeMagnitude::One,
                direction: StatChangeDirection::Raise,
            },
            new_stage: StatStage::new(1).unwrap(),
            capped: false,
        }
    );
}

#[test]
fn a_defense_already_at_the_maximum_stage_reports_capped_and_unchanged() {
    let dex = Dex::new();
    let mut attacker = mon(&dex, vec![DEFENSE_CURL]);
    attacker.stages_mut().defense = StatStage::MAX;

    let outcome = resolve_defense_curl_move(&dex, DEFENSE_CURL, &attacker).unwrap();
    assert!(outcome.capped);
    assert_eq!(outcome.new_stage, StatStage::MAX);
}

#[test]
fn resolving_defense_curl_does_not_set_its_own_volatile() {
    // `resolve_defense_curl_move` takes `attacker` by shared reference and
    // never touches `Volatiles`: the caller writes
    // `Volatiles::set_defense_curl` itself, before applying this outcome
    // (this module's docs).
    let dex = Dex::new();
    let attacker = mon(&dex, vec![DEFENSE_CURL]);
    let _ = resolve_defense_curl_move(&dex, DEFENSE_CURL, &attacker).unwrap();
    assert!(!attacker.volatiles().defense_curl);
}
