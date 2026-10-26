//! Party slot 0's Cute Charm gender handoff through the production wild-battle
//! entry, where slot 0 need not be the battle lead (`CreateWildMon` reads
//! `gPlayerParty[0]`, `pokeemerald/src/wild_encounter.c:395-398`).

use assets::{MoveId, SpeciesId};
use battle::{BattlePokemon, Dex, Gender, Ivs};
use engine::overworld::wild_encounter::WildEncounter;
use engine::rng::Rng;

use crate::flow::save_continue_support::new_game_phase;
use crate::new_game;

use super::{ActiveBattle, OverworldPhase};

const WURMPLE: SpeciesId = SpeciesId(290);
const CLEFAIRY: SpeciesId = SpeciesId(35);

/// A Clefairy personality with a low byte below its 191 ratio byte: female, so
/// its Cute Charm hands the wild Pokémon male.
const FEMALE_CLEFAIRY_PERSONALITY: u32 = 0;
const STATE_SCAN_LIMIT: u32 = 20_000;

fn cute_charm_clefairy(is_fainted: bool) -> BattlePokemon {
    let mut clefairy = BattlePokemon::new(
        &Dex::new(),
        CLEFAIRY,
        50,
        Ivs::default(),
        FEMALE_CLEFAIRY_PERSONALITY,
        vec![MoveId::POUND],
    )
    .expect("Clefairy is in the dex");
    assert_eq!(clefairy.ability(), assets::AbilityId::CUTE_CHARM);
    if is_fainted {
        clefairy.apply_damage(u32::MAX);
    }
    clefairy
}

/// The gender of the wild Wurmple a phase holding `party` builds from `rng`,
/// and the slot of the battle lead that faces it.
fn wild_gender_and_lead_slot(party: [&BattlePokemon; 2], rng: Rng) -> (Gender, usize) {
    let dex = Dex::new();
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 2;
    for (slot, member) in party.into_iter().enumerate() {
        seed.save1.player_party[slot] = crate::party::to_save_pokemon(&dex, member);
    }
    let mut phase = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(seed.map_id),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    let lead_slot = phase.party_lead_slot;
    phase.rng = rng;
    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));
    let Some(ActiveBattle::Wild(battle)) = phase.active_battle.as_ref() else {
        panic!("a healthy lead must fight");
    };
    let ratio = dex.species(WURMPLE).unwrap().gender_ratio;
    (
        Gender::of_personality(ratio, battle.enemy().personality()),
        lead_slot,
    )
}

/// A state whose first draw does (`activates`) or does not win the Cute Charm
/// roll, and from which the build gives a wild Pokémon of `unforced_gender` when
/// no gender is forced: the first draw picks the nature when no roll is spent, the
/// second when a losing roll spends the first.
fn rng_state_where_activation(activates: bool, unforced_gender: Gender) -> Rng {
    let healthy_starter = new_game::provisional_starter();
    let ordinary_party = [&healthy_starter, &healthy_starter];
    (0..STATE_SCAN_LIMIT)
        .map(Rng::new)
        .find(|candidate| {
            let activation_wins = !candidate.clone().next_u16().is_multiple_of(3);
            let mut unforced_start = candidate.clone();
            if !activates {
                let _spent_roll = unforced_start.next_u16();
            }
            activation_wins == activates
                && wild_gender_and_lead_slot(ordinary_party, unforced_start).0 == unforced_gender
        })
        .expect("a state with the wanted activation exists")
}

#[test]
fn a_fainted_cute_charm_slot_zero_forces_the_wild_mon_to_the_opposite_gender() {
    let rng = rng_state_where_activation(true, Gender::Female);
    let fainted_clefairy = cute_charm_clefairy(true);
    let starter = new_game::provisional_starter();

    let (gender, lead_slot) = wild_gender_and_lead_slot([&fainted_clefairy, &starter], rng);

    assert_eq!(
        lead_slot, 1,
        "setup: the healthy starter is the battle lead"
    );
    assert_eq!(gender, Gender::Male);
}

#[test]
fn a_losing_cute_charm_roll_leaves_the_ordinary_gender() {
    let rng = rng_state_where_activation(false, Gender::Female);
    let fainted_clefairy = cute_charm_clefairy(true);
    let starter = new_game::provisional_starter();

    let (gender, _) = wild_gender_and_lead_slot([&fainted_clefairy, &starter], rng);

    assert_eq!(gender, Gender::Female);
}

#[test]
fn a_cute_charm_holder_behind_slot_zero_forces_nothing() {
    let rng = rng_state_where_activation(true, Gender::Female);
    let healthy_clefairy = cute_charm_clefairy(false);
    let starter = new_game::provisional_starter();

    let (gender, _) = wild_gender_and_lead_slot([&starter, &healthy_clefairy], rng);

    assert_eq!(gender, Gender::Female);
}
