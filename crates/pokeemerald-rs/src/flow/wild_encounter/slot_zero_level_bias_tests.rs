//! Party slot 0's Hustle, Vital Spirit and Pressure coin through the production roll
//! (`ChooseWildMonLevel`, `pokeemerald/src/wild_encounter.c:289-300`): the saved slot 0
//! reaches the chooser even when it is fainted behind another battle lead, and the shared
//! stream advances by the upstream draw count before the wild Pokémon is built.

use super::test_support::{
    held, route_101_phase, walk_east_and_land, ENCOUNTER_SEED, FRAMES_PER_STEP, WURMPLE,
};
use super::SharedRng;
use crate::flow::overworld_phase::{ActiveBattle, OverworldPhase, WildBattle};
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{BattlePokemon, Dex, Ivs};
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

const TREECKO: SpeciesId = SpeciesId(277);
const TOGEPI: SpeciesId = SpeciesId(175);
const MANKEY: SpeciesId = SpeciesId(56);
const ABSOL: SpeciesId = SpeciesId(376);
const HEADER_FLAGS_BYTE: usize = 19;
const HEADER_EGG_BIT: u8 = 1 << 2;

fn mon(species: SpeciesId) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        species,
        50,
        Ivs::default(),
        0,
        vec![MoveId::POUND],
    )
    .expect("the species is in the dex")
}

fn fainted(species: SpeciesId) -> BattlePokemon {
    let mut battler = mon(species);
    battler.apply_damage(u32::MAX);
    battler
}

/// Walks `phase` onto the seeded Route 101 grass tile and through its landing roll.
fn roll_on_seeded_grass(mut phase: OverworldPhase) -> OverworldPhase {
    phase.rng = Rng::new(ENCOUNTER_SEED);
    walk_east_and_land(&mut phase, 4);
    for _ in 0..FRAMES_PER_STEP {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(ButtonState::new());
    phase
}

fn grass_phase() -> OverworldPhase {
    route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5))
}

/// The seeded landing with `slot_zero` saved ahead of a healthy Treecko that leads the battle.
fn encounter_behind(slot_zero: engine::save::Pokemon) -> OverworldPhase {
    let mut phase = grass_phase();
    phase.save1.player_party_count = 2;
    phase.save1.player_party[0] = slot_zero;
    let treecko = mon(TREECKO);
    phase.set_backed_lead_for_test(
        1,
        crate::party::to_save_pokemon(&Dex::new(), &treecko),
        treecko,
    );
    roll_on_seeded_grass(phase)
}

/// The wild Pokémon's personality and IVs, and the stream after its turn number, for a
/// stream that spent `selection_draws` choosing the encounter.
fn expected_after_selection(selection_draws: usize) -> (u32, battle::Ivs, u16, u32) {
    let mut reference = Rng::new(ENCOUNTER_SEED);
    for _ in 0..selection_draws {
        reference.next_u16();
    }
    let nature = battle::wild::roll_nature(&mut SharedRng::new(&mut reference));
    let personality =
        battle::wild::roll_personality_for_nature(nature, &mut SharedRng::new(&mut reference));
    let ivs = battle::wild::roll_ivs(&mut SharedRng::new(&mut reference));
    let _held_item = reference.next_u16();
    let turn_number = reference.next_u16();
    (personality, ivs, turn_number, reference.state())
}

fn assert_selection_spent(phase: &OverworldPhase, selection_draws: usize) {
    let Some(ActiveBattle::Wild(WildBattle { battle, .. })) = phase.active_battle.as_ref() else {
        panic!("the seeded roll fires an encounter");
    };
    assert_eq!(battle.enemy().species(), WURMPLE);
    assert_eq!(battle.enemy().level(), 2);
    let (personality, ivs, turn_number, state) = expected_after_selection(selection_draws);
    assert_eq!(battle.enemy().personality(), personality);
    assert_eq!(battle.enemy().ivs(), ivs);
    assert_eq!(battle.random_turn_number(), turn_number);
    assert_eq!(phase.rng.state(), state);
}

#[test]
fn the_two_selection_streams_build_different_wild_mons() {
    assert_ne!(expected_after_selection(4), expected_after_selection(5));
}

#[test]
fn a_fainted_slot_zero_holder_spends_the_coin_behind_a_healthy_lead() {
    let dex = Dex::new();
    for (species, ability) in [
        (TOGEPI, AbilityId::HUSTLE),
        (MANKEY, AbilityId::VITAL_SPIRIT),
        (ABSOL, AbilityId::PRESSURE),
    ] {
        let holder = fainted(species);
        assert_eq!(holder.ability(), ability, "setup: {species:?}");
        let phase = encounter_behind(crate::party::to_save_pokemon(&dex, &holder));
        assert_eq!(phase.lead_slot_for_test(), Some(1), "{species:?}");
        assert_selection_spent(&phase, 5);
    }
}

#[test]
fn a_slot_zero_egg_or_unrelated_ability_spends_no_coin() {
    let dex = Dex::new();
    let mut egg_bytes = crate::party::to_save_pokemon(&dex, &mon(TOGEPI)).to_bytes();
    egg_bytes[HEADER_FLAGS_BYTE] |= HEADER_EGG_BIT;
    let egg = engine::save::Pokemon::from_bytes(egg_bytes);
    let unrelated = crate::party::to_save_pokemon(&dex, &fainted(TREECKO));
    for slot_zero in [egg, unrelated] {
        assert_selection_spent(&encounter_behind(slot_zero), 4);
    }
}

#[test]
fn an_unsaved_party_uses_its_live_lead_as_slot_zero() {
    for (species, selection_draws) in [(ABSOL, 5), (TREECKO, 4)] {
        let mut phase = grass_phase();
        assert_eq!(phase.save1.player_party_count, 0, "setup: an unsaved party");
        phase.set_fresh_lead_for_test(mon(species));
        assert_selection_spent(&roll_on_seeded_grass(phase), selection_draws);
    }
}
