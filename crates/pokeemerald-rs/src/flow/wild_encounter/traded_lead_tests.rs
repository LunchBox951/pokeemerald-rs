//! A lead whose OT id differs from the save owner's is a traded recipient:
//! `Cmd_getexp` boosts its award by a truncating 150%
//! (`src/battle_script_commands.c:3381`-`:3390`; `IsTradedMon`,
//! `pokemon.c:6602`).

use super::test_support::{player_mon, ENCOUNTER_SEED, TEST_PLAYER_TRAINER_ID, WURMPLE};
use super::{start_wild_battle, SharedRng};
use crate::flow::first_battle::start_first_battle;
use assets::MoveId;
use battle::{BattleEvent, Dex, PlayerAction};
use engine::overworld::wild_encounter::WildEncounter;
use engine::rng::Rng;

const TRADED_OT_ID: u32 = 0xAABB_CCDD;
const LEAD_SPECIES: u16 = 277;

/// The wild Wurmple's award and the lead's level after the knockout.
fn wild_knockout(lead: battle::BattlePokemon, lead_ot_id: u32) -> (u32, u8) {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    let encounter = WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    };
    let lead = lead.with_original_trainer_id(lead_ot_id);
    let mut battle =
        start_wild_battle(lead, encounter, TEST_PLAYER_TRAINER_ID, &mut rng).expect("fightable");
    let mut gained = None;
    for _ in 0..100 {
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut SharedRng::new(&mut rng))
            .expect("turn");
        for event in events {
            if let BattleEvent::ExpGained(exp) = event {
                gained = Some(exp);
            }
        }
        if battle.outcome().is_some() {
            break;
        }
    }
    (
        gained.expect("the wild mon faints"),
        battle.player().level(),
    )
}

fn lead_at(level: u8) -> battle::BattlePokemon {
    player_mon(LEAD_SPECIES, level, vec![MoveId::POUND])
}

fn wurmple_award() -> u32 {
    battle::wild_faint_exp(Dex::new().species(WURMPLE).expect("wurmple").base_exp, 2)
}

#[test]
fn a_domestic_lead_earns_the_plain_wild_award() {
    assert_eq!(
        wild_knockout(lead_at(50), TEST_PLAYER_TRAINER_ID).0,
        wurmple_award()
    );
}

#[test]
fn a_traded_lead_earns_the_boosted_wild_award() {
    assert_ne!(TRADED_OT_ID, TEST_PLAYER_TRAINER_ID);
    let plain = wurmple_award();
    assert_eq!(
        wild_knockout(lead_at(50), TRADED_OT_ID).0,
        plain * 150 / 100
    );
}

/// A lead exactly `boosted` experience short of level 51 crosses with the
/// traded bonus but not without it.
#[test]
fn the_boosted_wild_award_can_cross_a_level_the_plain_award_does_not() {
    let plain = wurmple_award();
    let boosted = plain * 150 / 100;
    assert!(boosted > plain);
    let base = lead_at(50);
    let mut probe = base.clone();
    let mut threshold = base.experience();
    while probe.level() == 50 {
        threshold += 1;
        probe.reconcile_saved_experience(threshold);
    }
    let mut near = base;
    near.reconcile_saved_experience(threshold - boosted);
    assert_eq!(near.level(), 50);

    assert_eq!(
        wild_knockout(near.clone(), TEST_PLAYER_TRAINER_ID),
        (plain, 50)
    );
    assert_eq!(wild_knockout(near, TRADED_OT_ID), (boosted, 51));
}

#[test]
fn the_first_battle_classifies_a_foreign_lead_as_traded() {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    let domestic = player_mon(LEAD_SPECIES, 5, vec![MoveId::POUND])
        .with_original_trainer_id(TEST_PLAYER_TRAINER_ID);
    let battle = start_first_battle(domestic, TEST_PLAYER_TRAINER_ID, &mut rng).expect("starts");
    assert!(!battle.player().is_traded_recipient());

    let mut rng = Rng::new(ENCOUNTER_SEED);
    let foreign =
        player_mon(LEAD_SPECIES, 5, vec![MoveId::POUND]).with_original_trainer_id(TRADED_OT_ID);
    let battle = start_first_battle(foreign, TEST_PLAYER_TRAINER_ID, &mut rng).expect("starts");
    assert!(battle.player().is_traded_recipient());
}
