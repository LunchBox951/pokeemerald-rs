//! The flute flags and a Cleanse Tag held by party slot 0 through the production roll
//! (`WildEncounterCheck`, `pokeemerald/src/wild_encounter.c:502-528`; the modifiers
//! `:946-967`): the saved slot 0 decides the Tag even when it is fainted, an egg, or not
//! the battle lead, and every case spends the same draws in the same order.

use super::test_support::{
    held, player_mon, route_101_phase, walk_east_and_land, ENCOUNTER_SEED, FRAMES_PER_STEP,
};
use crate::flow::overworld_phase::{ActiveBattle, OverworldPhase, WildBattle};
use assets::items::ItemId;
use assets::MoveId;
use battle::{BattlePokemon, Dex};
use engine::event_data::{FLAG_SYS_ENC_DOWN_ITEM, FLAG_SYS_ENC_UP_ITEM};
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

const TREECKO: u16 = 277;
const HEADER_FLAGS_BYTE: usize = 19;
const HEADER_EGG_BIT: u8 = 1 << 2;

/// Seeds whose first draw allows the new-metatile check (`% 100 < 60`) and whose second draw
/// modulo 2880 is the second element. Route 101's table rate of 20 scales to 320; a Tag makes
/// that 213, the down flute 160, the up flute 480, up with the Tag 320, down with the Tag 106.
const SEED_RATE_DRAW_138: (u32, u32) = (ENCOUNTER_SEED, 138);
const SEED_RATE_DRAW_170: (u32, u32) = (103, 170);
const SEED_RATE_DRAW_218: (u32, u32) = (23, 218);
const SEED_RATE_DRAW_336: (u32, u32) = (56, 336);

#[derive(Clone, Copy, Debug)]
struct Setup {
    up: bool,
    down: bool,
    tag: bool,
}

const NONE: Setup = Setup {
    up: false,
    down: false,
    tag: false,
};
const UP: Setup = Setup { up: true, ..NONE };
const DOWN: Setup = Setup { down: true, ..NONE };
const TAG: Setup = Setup { tag: true, ..NONE };
const UP_TAG: Setup = Setup { tag: true, ..UP };
const DOWN_TAG: Setup = Setup { tag: true, ..DOWN };
const BOTH: Setup = Setup { down: true, ..UP };
const BOTH_TAG: Setup = Setup { tag: true, ..BOTH };

fn mon(species: u16) -> BattlePokemon {
    player_mon(species, 50, vec![MoveId::POUND])
}

fn saved_holding(mon: &BattlePokemon, item: ItemId) -> engine::save::Pokemon {
    let mut record = crate::party::to_save_pokemon(&Dex::new(), mon);
    let mut substructures = record.box_data.substructures().unwrap();
    substructures.growth[2..4].copy_from_slice(&item.0.to_le_bytes());
    record.box_data.set_substructures(&substructures);
    record
}

fn fainted(species: u16) -> BattlePokemon {
    let mut battler = mon(species);
    battler.apply_damage(u32::MAX);
    battler
}

fn into_egg(mut record: engine::save::Pokemon) -> engine::save::Pokemon {
    let mut bytes = record.to_bytes();
    bytes[HEADER_FLAGS_BYTE] |= HEADER_EGG_BIT;
    record = engine::save::Pokemon::from_bytes(bytes);
    record
}

fn grass_phase(setup: Setup) -> OverworldPhase {
    let mut phase = route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5));
    if setup.up {
        phase
            .save1
            .event_data
            .flag_set(FLAG_SYS_ENC_UP_ITEM)
            .unwrap();
    }
    if setup.down {
        phase
            .save1
            .event_data
            .flag_set(FLAG_SYS_ENC_DOWN_ITEM)
            .unwrap();
    }
    phase
}

/// Files `slot_zero` ahead of a healthy Treecko that leads the battle from slot 1.
fn lead_from_slot_one(phase: &mut OverworldPhase, slot_zero: engine::save::Pokemon) {
    phase.save1.player_party_count = 2;
    phase.save1.player_party[0] = slot_zero;
    let treecko = mon(TREECKO);
    phase.set_backed_lead_for_test(
        1,
        crate::party::to_save_pokemon(&Dex::new(), &treecko),
        treecko,
    );
}

/// Walks onto the grass and through its landing roll from `seed`.
fn roll_on_grass(mut phase: OverworldPhase, seed: u32) -> OverworldPhase {
    phase.rng = Rng::new(seed);
    walk_east_and_land(&mut phase, 4);
    for _ in 0..FRAMES_PER_STEP {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(ButtonState::new());
    phase
}

fn rng_after(seed: u32, draws: usize) -> u32 {
    let mut rng = Rng::new(seed);
    for _ in 0..draws {
        rng.next_u16();
    }
    rng.state()
}

/// The shared stream after a rolled step that did not encounter: the new-metatile draw and
/// the rate draw, nothing else.
fn assert_no_encounter(phase: &OverworldPhase, seed: u32, label: &str) {
    assert!(!phase.is_wild_battle_active(), "{label}: no encounter");
    assert_eq!(
        phase.rng.state(),
        rng_after(seed, 2),
        "{label}: a rejected rate roll spends exactly two draws"
    );
}

fn assert_encounter(phase: &OverworldPhase, label: &str) {
    assert!(
        matches!(phase.active_battle, Some(ActiveBattle::Wild(_))),
        "{label}: the rate roll accepts and a wild battle starts"
    );
}

/// A one-member saved party whose backed lead holds `item`.
fn sole_lead_holding(setup: Setup, item: ItemId) -> OverworldPhase {
    let mut phase = grass_phase(setup);
    let record = saved_holding(&mon(TREECKO), item);
    let lead = crate::party::from_save_pokemon(&Dex::new(), &record).expect("the record decodes");
    phase.save1.player_party_count = 1;
    phase.set_backed_lead_for_test(0, record, lead);
    phase
}

#[test]
fn the_documented_seeds_really_produce_the_documented_rate_draws() {
    for (seed, rate_draw) in [
        SEED_RATE_DRAW_138,
        SEED_RATE_DRAW_170,
        SEED_RATE_DRAW_218,
        SEED_RATE_DRAW_336,
    ] {
        let mut rng = Rng::new(seed);
        assert!(
            rng.next_u16() % 100 < 60,
            "seed {seed}: the check is allowed"
        );
        assert_eq!(
            u32::from(rng.next_u16()) % 2880,
            rate_draw,
            "seed {seed}: the rate draw"
        );
    }
}

/// Each row is `(seed, rate draw, setup, encounters)` against the rate derived in the
/// constants' comment.
#[test]
fn the_flute_flags_and_a_slot_zero_tag_decide_the_production_roll() {
    let cases: &[((u32, u32), Setup, bool)] = &[
        (SEED_RATE_DRAW_218, NONE, true),
        (SEED_RATE_DRAW_218, UP, true),
        (SEED_RATE_DRAW_218, DOWN, false),
        (SEED_RATE_DRAW_218, TAG, false),
        (SEED_RATE_DRAW_218, UP_TAG, true),
        (SEED_RATE_DRAW_218, DOWN_TAG, false),
        (SEED_RATE_DRAW_218, BOTH, true),
        (SEED_RATE_DRAW_218, BOTH_TAG, true),
        (SEED_RATE_DRAW_170, NONE, true),
        (SEED_RATE_DRAW_170, DOWN, false),
        (SEED_RATE_DRAW_170, TAG, true),
        (SEED_RATE_DRAW_170, DOWN_TAG, false),
        (SEED_RATE_DRAW_336, NONE, false),
        (SEED_RATE_DRAW_336, UP, true),
        (SEED_RATE_DRAW_336, TAG, false),
        (SEED_RATE_DRAW_336, UP_TAG, false),
        (SEED_RATE_DRAW_336, BOTH, true),
        (SEED_RATE_DRAW_138, DOWN, true),
        (SEED_RATE_DRAW_138, DOWN_TAG, false),
    ];
    for &((seed, rate_draw), setup, encounters) in cases {
        let mut phase = grass_phase(setup);
        let slot_zero = if setup.tag {
            saved_holding(&fainted(TREECKO), ItemId::CLEANSE_TAG)
        } else {
            crate::party::to_save_pokemon(&Dex::new(), &fainted(TREECKO))
        };
        lead_from_slot_one(&mut phase, slot_zero);
        let phase = roll_on_grass(phase, seed);
        let label = format!("seed {seed} (rate draw {rate_draw}) {setup:?}");
        if encounters {
            assert_encounter(&phase, &label);
        } else {
            assert_no_encounter(&phase, seed, &label);
        }
    }
}

/// An accepted roll spends the same draws in the same order with or without a modifier: the
/// stream after the whole step and the wild Pokemon built from it are identical.
#[test]
fn an_accepted_roll_spends_the_same_draws_whatever_the_modifiers() {
    let (seed, _) = SEED_RATE_DRAW_138;
    let baseline = roll_on_grass(sole_lead_holding(NONE, ItemId::NONE), seed);
    assert_encounter(&baseline, "baseline");
    let Some(ActiveBattle::Wild(WildBattle { battle: base, .. })) = baseline.active_battle.as_ref()
    else {
        unreachable!("asserted above");
    };
    for setup in [UP, DOWN, TAG, UP_TAG, BOTH] {
        let item = if setup.tag {
            ItemId::CLEANSE_TAG
        } else {
            ItemId::NONE
        };
        let phase = roll_on_grass(sole_lead_holding(setup, item), seed);
        assert_encounter(&phase, &format!("{setup:?}"));
        let Some(ActiveBattle::Wild(WildBattle { battle, .. })) = phase.active_battle.as_ref()
        else {
            unreachable!("asserted above");
        };
        assert_eq!(phase.rng.state(), baseline.rng.state(), "{setup:?}");
        assert_eq!(battle.enemy().personality(), base.enemy().personality());
        assert_eq!(battle.enemy().ivs(), base.enemy().ivs());
        assert_eq!(battle.random_turn_number(), base.random_turn_number());
    }
}

/// Seed 23 accepts at the baseline rate (218 < 320) and refuses at the Tag's (213).
#[test]
fn only_the_saved_slot_zero_counts_as_the_tag_holder() {
    let (seed, _) = SEED_RATE_DRAW_218;
    let dex = Dex::new();
    let tag_on = |battler: &BattlePokemon| saved_holding(battler, ItemId::CLEANSE_TAG);

    // Slot 0 is a Tag holder: fainted behind the lead, an egg behind the lead.
    for (label, slot_zero) in [
        ("fainted", tag_on(&fainted(TREECKO))),
        ("egg", into_egg(tag_on(&mon(TREECKO)))),
    ] {
        let mut phase = grass_phase(NONE);
        lead_from_slot_one(&mut phase, slot_zero);
        assert_eq!(phase.lead_slot_for_test(), Some(1), "{label}: not the lead");
        assert_no_encounter(&roll_on_grass(phase, seed), seed, label);
    }

    // The unsaved party's live lead is slot 0.
    let mut phase = grass_phase(NONE);
    let lead = crate::flow::lead_screen::loaded_holding(&mon(TREECKO), ItemId::CLEANSE_TAG);
    phase.set_fresh_lead_for_test(lead);
    assert_eq!(phase.save1.player_party_count, 0, "setup: an unsaved party");
    assert_no_encounter(&roll_on_grass(phase, seed), seed, "unsaved live lead");

    // The Tag on a reserve, an unrelated item on slot 0, and no item change nothing.
    for (label, slot_zero, slot_one) in [
        (
            "tag on slot 1",
            crate::party::to_save_pokemon(&dex, &fainted(TREECKO)),
            Some(tag_on(&mon(TREECKO))),
        ),
        (
            "unrelated item",
            saved_holding(&fainted(TREECKO), ItemId::NUGGET),
            None,
        ),
        (
            "unrelated item on an egg",
            into_egg(saved_holding(&mon(TREECKO), ItemId::NUGGET)),
            None,
        ),
        (
            "no item",
            crate::party::to_save_pokemon(&dex, &fainted(TREECKO)),
            None,
        ),
    ] {
        let mut phase = grass_phase(NONE);
        lead_from_slot_one(&mut phase, slot_zero);
        if let Some(record) = slot_one {
            // The lead itself holds the Tag in slot 1; slot 0 does not.
            let battler = crate::party::from_save_pokemon(&dex, &record).unwrap();
            phase.set_backed_lead_for_test(1, record, battler);
        }
        assert_encounter(&roll_on_grass(phase, seed), label);
    }
}

/// A Cleanse Tag holder is admitted to a battle and keeps the Tag out of it and into its
/// saved slot; every other held-item refusal still applies.
#[test]
fn a_tag_holding_lead_battles_and_keeps_its_item_through_the_save() {
    let (seed, _) = SEED_RATE_DRAW_138;
    let mut phase = sole_lead_holding(NONE, ItemId::CLEANSE_TAG);
    assert_eq!(
        phase.lead_for_test().map(BattlePokemon::held_item),
        Some(ItemId::CLEANSE_TAG)
    );
    assert!(phase.wild_lead_admissible(), "the Tag has no battle effect");
    assert_eq!(phase.wild_lead_refusal, None);

    let mut phase = roll_on_grass(phase, seed);
    let Some(ActiveBattle::Wild(WildBattle { battle, .. })) = phase.active_battle.as_ref() else {
        panic!("the Tag holder is admitted and the 213 rate accepts the draw of 138");
    };
    assert_eq!(battle.player().held_item(), ItemId::CLEANSE_TAG);

    let mut frames = 0;
    while phase.is_wild_battle_active() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
    }
    assert_eq!(
        phase.lead_for_test().map(BattlePokemon::held_item),
        Some(ItemId::CLEANSE_TAG),
        "the live lead keeps its Tag"
    );
    let saved = crate::party::from_save_pokemon(&Dex::new(), &phase.save1.player_party[0])
        .expect("the saved slot decodes");
    assert_eq!(
        saved.held_item(),
        ItemId::CLEANSE_TAG,
        "and so does the save"
    );
}

#[test]
fn another_battle_item_still_disables_the_roll_for_a_lead() {
    let (seed, _) = SEED_RATE_DRAW_138;
    let mut phase = grass_phase(NONE);
    let lead = crate::flow::lead_screen::loaded_holding(&mon(TREECKO), ItemId::SITRUS_BERRY);
    phase.set_fresh_lead_for_test(lead);
    assert!(!phase.wild_lead_admissible());
    let phase = roll_on_grass(phase, seed);
    assert!(!phase.is_wild_battle_active());
    assert_eq!(phase.rng.state(), Rng::new(seed).state(), "no draw at all");
}
