//! The Keen Eye and Intimidate turn-away through the production roll
//! (`IsAbilityAllowingEncounter`, `pokeemerald/src/wild_encounter.c:897-913`). The seeded
//! Route 101 landing selects a level-2 Wurmple, so a slot 0 of level 7 qualifies (7 - 5 = 2)
//! and one of level 6 does not. The saved slot 0 supplies the ability and level, even when it
//! is fainted behind another battle lead.

use super::test_support::{
    held, player_mon, route_101_phase, walk_east_and_land, ENCOUNTER_SEED, FRAMES_PER_STEP, WURMPLE,
};
use super::SharedRng;
use crate::flow::overworld_phase::{ActiveBattle, OverworldPhase, WildBattle};
use assets::{AbilityId, MoveId};
use battle::{BattlePokemon, Dex};
use engine::event_data::VAR_REPEL_STEP_COUNT;
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

const TREECKO: u16 = 277;
const SPEAROW: u16 = 21;
const MIGHTYENA: u16 = 287;
const HEADER_FLAGS_BYTE: usize = 19;
const HEADER_EGG_BIT: u8 = 1 << 2;
/// Permission, rate, slot and level: the draws the seeded landing spends selecting.
const SELECTION_DRAWS: usize = 4;
/// A seed whose selection draws are the same four, with an odd fifth draw (27253).
const ODD_COIN_SEED: u32 = 140;

fn battler(species: u16, level: u8) -> BattlePokemon {
    player_mon(species, level, vec![MoveId::POUND])
}

fn fainted(species: u16, level: u8) -> BattlePokemon {
    let mut mon = battler(species, level);
    mon.apply_damage(u32::MAX);
    mon
}

fn record(mon: &BattlePokemon) -> engine::save::Pokemon {
    crate::party::to_save_pokemon(&Dex::new(), mon)
}

fn grass_phase() -> OverworldPhase {
    route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5))
}

fn roll_on_seeded_grass(mut phase: OverworldPhase, seed: u32) -> OverworldPhase {
    phase.rng = Rng::new(seed);
    walk_east_and_land(&mut phase, 4);
    for _ in 0..FRAMES_PER_STEP {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(ButtonState::new());
    phase
}

/// `slot_zero` saved ahead of a healthy level-50 Treecko that leads the battle from slot 1.
fn encounter_behind(slot_zero: engine::save::Pokemon, seed: u32) -> OverworldPhase {
    let mut phase = grass_phase();
    phase.save1.player_party_count = 2;
    phase.save1.player_party[0] = slot_zero;
    let treecko = battler(TREECKO, 50);
    phase.set_backed_lead_for_test(1, record(&treecko), treecko);
    assert_eq!(phase.lead_slot_for_test(), Some(1));
    roll_on_seeded_grass(phase, seed)
}

fn rng_state_after(seed: u32, draws: usize) -> u32 {
    let mut rng = Rng::new(seed);
    for _ in 0..draws {
        rng.next_u16();
    }
    rng.state()
}

fn assert_rejected(phase: &OverworldPhase, seed: u32, draws: usize) {
    assert!(
        phase.active_battle.is_none(),
        "a rejected encounter starts no battle"
    );
    assert_eq!(
        phase.rng.state(),
        rng_state_after(seed, draws),
        "no nature, personality or IV draw follows the rejection"
    );
}

/// Asserts the encounter was accepted after `selection_draws` draws and built from the stream
/// after them.
fn assert_accepted(phase: &OverworldPhase, seed: u32, selection_draws: usize) {
    let Some(ActiveBattle::Wild(WildBattle { battle, .. })) = phase.active_battle.as_ref() else {
        panic!("the seeded roll fires an encounter");
    };
    assert_eq!(battle.enemy().species(), WURMPLE);
    assert_eq!(battle.enemy().level(), 2);
    let mut reference = Rng::new(seed);
    for _ in 0..selection_draws {
        reference.next_u16();
    }
    let nature = battle::wild::roll_nature(&mut SharedRng::new(&mut reference));
    let personality =
        battle::wild::roll_personality_for_nature(nature, &mut SharedRng::new(&mut reference));
    assert_eq!(battle.enemy().personality(), personality);
}

#[test]
fn the_seeds_draw_the_assumed_streams() {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    assert_eq!(rng.next_u16() % 100, 7);
    let draws: Vec<u16> = (0..4).map(|_| rng.next_u16()).collect();
    assert_eq!(draws[3] % 2, 0, "the even coin that rejects");
    let mut odd = Rng::new(ODD_COIN_SEED);
    let fifth = (0..5).map(|_| odd.next_u16()).last().unwrap();
    assert_eq!(fifth % 2, 1);
}

#[test]
fn a_fainted_intimidate_slot_zero_rejects_behind_a_healthy_lead() {
    let holder = fainted(MIGHTYENA, 7);
    assert_eq!(holder.ability(), AbilityId::INTIMIDATE, "setup");
    let phase = encounter_behind(record(&holder), ENCOUNTER_SEED);
    assert_rejected(&phase, ENCOUNTER_SEED, SELECTION_DRAWS + 1);
}

#[test]
fn an_odd_coin_lets_the_encounter_through_after_the_same_draws() {
    let phase = encounter_behind(record(&fainted(MIGHTYENA, 7)), ODD_COIN_SEED);
    assert_accepted(&phase, ODD_COIN_SEED, SELECTION_DRAWS + 1);
}

#[test]
fn the_saved_slot_zero_level_not_the_lead_level_decides() {
    // Level 6 is only four above the wild level 2: no coin, though the even seed would reject.
    let phase = encounter_behind(record(&fainted(MIGHTYENA, 6)), ENCOUNTER_SEED);
    assert_accepted(&phase, ENCOUNTER_SEED, SELECTION_DRAWS);
}

#[test]
fn an_unrelated_slot_zero_ability_or_a_header_egg_spends_no_coin() {
    let treecko = fainted(TREECKO, 7);
    assert_eq!(treecko.ability(), AbilityId::OVERGROW, "setup");
    let phase = encounter_behind(record(&treecko), ENCOUNTER_SEED);
    assert_accepted(&phase, ENCOUNTER_SEED, SELECTION_DRAWS);

    let mut egg_bytes = record(&fainted(MIGHTYENA, 7)).to_bytes();
    egg_bytes[HEADER_FLAGS_BYTE] |= HEADER_EGG_BIT;
    let egg = engine::save::Pokemon::from_bytes(egg_bytes);
    let phase = encounter_behind(egg, ENCOUNTER_SEED);
    assert_accepted(&phase, ENCOUNTER_SEED, SELECTION_DRAWS);
}

#[test]
fn a_repel_rejection_comes_first_and_draws_no_coin() {
    let mut phase = grass_phase();
    phase.save1.player_party_count = 2;
    phase.save1.player_party[0] = record(&fainted(MIGHTYENA, 7));
    let treecko = battler(TREECKO, 50);
    phase.set_backed_lead_for_test(1, record(&treecko), treecko);
    phase
        .save1
        .event_data
        .var_set(VAR_REPEL_STEP_COUNT, 37)
        .unwrap();
    let phase = roll_on_seeded_grass(phase, ENCOUNTER_SEED);
    assert_rejected(&phase, ENCOUNTER_SEED, SELECTION_DRAWS);
}

/// A live Keen Eye lead in slot 0 over a saved record of `saved_level`.
fn keen_eye_lead(saved_level: u8, live_level: u8) -> OverworldPhase {
    let mut phase = grass_phase();
    phase.save1.player_party_count = 1;
    let live = battler(SPEAROW, live_level);
    assert_eq!(live.ability(), AbilityId::KEEN_EYE, "setup");
    phase.set_backed_lead_for_test(0, record(&battler(SPEAROW, saved_level)), live);
    phase
}

#[test]
fn the_live_slot_zero_lead_level_decides_not_its_stale_saved_record() {
    let rejecting = roll_on_seeded_grass(keen_eye_lead(6, 7), ENCOUNTER_SEED);
    assert_rejected(&rejecting, ENCOUNTER_SEED, SELECTION_DRAWS + 1);
    let accepting = roll_on_seeded_grass(keen_eye_lead(7, 6), ENCOUNTER_SEED);
    assert_accepted(&accepting, ENCOUNTER_SEED, SELECTION_DRAWS);
}

#[test]
fn an_unsaved_party_uses_its_live_lead_as_slot_zero() {
    let mut phase = grass_phase();
    assert_eq!(phase.save1.player_party_count, 0, "setup: an unsaved party");
    // Saved leftovers a stored count of zero must not read.
    phase.save1.player_party[0] = record(&battler(SPEAROW, 6));
    phase.set_fresh_lead_for_test(battler(SPEAROW, 7));
    let phase = roll_on_seeded_grass(phase, ENCOUNTER_SEED);
    assert_rejected(&phase, ENCOUNTER_SEED, SELECTION_DRAWS + 1);
}
