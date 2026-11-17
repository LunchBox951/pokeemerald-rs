//! The flute flags, a Cleanse Tag held by party slot 0 and slot 0's Stench, Illuminate,
//! White Smoke or Arena Trap through the production roll (`WildEncounterCheck`,
//! `pokeemerald/src/wild_encounter.c:502-528`; the item modifiers `:946-967`): the saved
//! slot 0 decides the Tag even when it is fainted, an egg, or not the battle lead, the
//! ability only when slot 0 is not an egg, and every case spends the same draws in the
//! same order.

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

const GRIMER: u16 = 88;
const VOLBEAT: u16 = 386;
const TORKOAL: u16 = 321;
const TRAPINCH: u16 = 332;

/// `(species, ability slot, the ability it names)`: one real holder per scaling ability.
const HOLDERS: [(u16, u8, assets::AbilityId, bool); 4] = [
    (GRIMER, 0, assets::AbilityId::STENCH, false),
    (VOLBEAT, 0, assets::AbilityId::ILLUMINATE, true),
    (TORKOAL, 0, assets::AbilityId::WHITE_SMOKE, false),
    (TRAPINCH, 1, assets::AbilityId::ARENA_TRAP, true),
];

fn holder(species: u16, ability_slot: u8) -> BattlePokemon {
    mon(species).with_ability_slot(ability_slot)
}

fn saved(battler: &BattlePokemon) -> engine::save::Pokemon {
    crate::party::to_save_pokemon(&Dex::new(), battler)
}

fn saved_ability(record: &engine::save::Pokemon) -> Option<assets::AbilityId> {
    crate::party::slot_zero_wild_level_ability(&Dex::new(), std::slice::from_ref(record))
}

#[test]
fn the_holders_really_carry_the_abilities_that_scale_the_rate() {
    for (species, slot, ability, _) in HOLDERS {
        assert_eq!(
            saved_ability(&saved(&holder(species, slot))),
            Some(ability),
            "species {species} slot {slot}"
        );
    }
}

/// Table rate 20 scales to 320. Seed 103 draws 170 (below 320 but not below the halved 160)
/// and seed 56 draws 336 (not below 320 but below the doubled 640). A fainted holder in saved
/// slot 0 behind a healthy Treecko decides the rate: halvers turn
/// seed 103's acceptance (170 < 320) into a refusal (170 >= 160), doublers turn seed 56's
/// refusal (336 >= 320) into an acceptance (336 < 640).
#[test]
fn a_slot_zero_ability_scales_the_production_roll() {
    let (halve_seed, _) = SEED_RATE_DRAW_170;
    let (double_seed, _) = SEED_RATE_DRAW_336;
    for (species, slot, ability, doubles) in HOLDERS {
        let build = |seed: u32| {
            let mut phase = grass_phase(NONE);
            lead_from_slot_one(&mut phase, saved(&fainted_holder(species, slot)));
            assert_eq!(phase.lead_slot_for_test(), Some(1), "{ability:?}: not the lead");
            roll_on_grass(phase, seed)
        };
        let (seed, label) = if doubles {
            (double_seed, "doubles: 336 < 640")
        } else {
            (halve_seed, "halves: 170 >= 160")
        };
        let phase = build(seed);
        if doubles {
            assert_encounter(&phase, &format!("{ability:?} {label}"));
        } else {
            assert_no_encounter(&phase, seed, &format!("{ability:?} {label}"));
        }
        // The other seed keeps its baseline outcome: a halver still refuses 336 (>= 160) and a
        // doubler still accepts 170 (< 640).
        let (other_seed, other_accepts) = if doubles {
            (halve_seed, true)
        } else {
            (double_seed, false)
        };
        let other = build(other_seed);
        if other_accepts {
            assert_encounter(&other, &format!("{ability:?} other seed"));
        } else {
            assert_no_encounter(&other, other_seed, &format!("{ability:?} other seed"));
        }
    }
}

fn fainted_holder(species: u16, slot: u8) -> BattlePokemon {
    let mut battler = holder(species, slot);
    battler.apply_damage(u32::MAX);
    battler
}

/// An egg in slot 0 has no ability (`:509`), so the baseline outcome returns, while any
/// other ability (Treecko's Overgrow) never scaled it in the first place.
#[test]
fn an_egg_or_an_unrelated_ability_in_slot_zero_leaves_the_rate_unchanged() {
    let (halve_seed, _) = SEED_RATE_DRAW_170;
    let (double_seed, _) = SEED_RATE_DRAW_336;
    for (species, slot, ability, doubles) in HOLDERS {
        let seed = if doubles { double_seed } else { halve_seed };
        let egg = into_egg(saved(&holder(species, slot)));
        assert_eq!(saved_ability(&egg), None, "{ability:?}: an egg has no ability");
        let mut phase = grass_phase(NONE);
        lead_from_slot_one(&mut phase, egg);
        let phase = roll_on_grass(phase, seed);
        if doubles {
            assert_no_encounter(&phase, seed, &format!("{ability:?} egg, baseline refuses 336"));
        } else {
            assert_encounter(&phase, &format!("{ability:?} egg, baseline accepts 170"));
        }
    }
    for (seed, accepts) in [(halve_seed, true), (double_seed, false)] {
        let mut phase = grass_phase(NONE);
        lead_from_slot_one(&mut phase, saved(&fainted(TREECKO)));
        let phase = roll_on_grass(phase, seed);
        if accepts {
            assert_encounter(&phase, "Overgrow");
        } else {
            assert_no_encounter(&phase, seed, "Overgrow");
        }
    }
}

/// Only saved slot 0 counts: a holder that leads from slot 1 (behind a plain slot 0), or sits
/// in a reserve slot, changes nothing; the unsaved party's live lead is slot 0.
#[test]
fn only_slot_zero_supplies_the_rate_ability() {
    let (halve_seed, _) = SEED_RATE_DRAW_170;
    let (double_seed, _) = SEED_RATE_DRAW_336;

    // Grimer (Stench) leads from slot 1 behind a fainted Treecko: seed 103 still accepts.
    let mut phase = grass_phase(NONE);
    phase.save1.player_party_count = 2;
    phase.save1.player_party[0] = saved(&fainted(TREECKO));
    let grimer = holder(GRIMER, 0);
    phase.set_backed_lead_for_test(1, saved(&grimer), grimer);
    assert_encounter(&roll_on_grass(phase, halve_seed), "Stench on slot 1");

    // Volbeat (Illuminate) leads from slot 1 behind a fainted Treecko: seed 56 still refuses.
    let mut phase = grass_phase(NONE);
    phase.save1.player_party_count = 2;
    phase.save1.player_party[0] = saved(&fainted(TREECKO));
    let volbeat = holder(VOLBEAT, 0);
    phase.set_backed_lead_for_test(1, saved(&volbeat), volbeat);
    assert_no_encounter(
        &roll_on_grass(phase, double_seed),
        double_seed,
        "Illuminate on slot 1",
    );

    // An unsaved party's live lead is slot 0.
    let mut phase = grass_phase(NONE);
    phase.set_fresh_lead_for_test(holder(GRIMER, 0));
    assert_eq!(phase.save1.player_party_count, 0, "setup: an unsaved party");
    assert_no_encounter(&roll_on_grass(phase, halve_seed), halve_seed, "unsaved Stench lead");
    let mut phase = grass_phase(NONE);
    phase.set_fresh_lead_for_test(holder(VOLBEAT, 0));
    assert_encounter(&roll_on_grass(phase, double_seed), "unsaved Illuminate lead");
}

/// Seed 138 accepts under every ability (138 < 160), so the whole encounter -- slot, level and
/// opponent construction -- spends the same draws as a Treecko lead.
#[test]
fn an_accepted_roll_spends_the_same_draws_whatever_the_slot_zero_ability() {
    let (seed, _) = SEED_RATE_DRAW_138;
    let baseline = roll_on_grass(sole_lead_holding(NONE, ItemId::NONE), seed);
    assert_encounter(&baseline, "baseline");
    let Some(ActiveBattle::Wild(WildBattle { battle: base, .. })) = baseline.active_battle.as_ref()
    else {
        unreachable!("asserted above");
    };
    for (species, slot, ability, _) in HOLDERS {
        let mut phase = grass_phase(NONE);
        lead_from_slot_one(&mut phase, saved(&fainted_holder(species, slot)));
        let phase = roll_on_grass(phase, seed);
        assert_encounter(&phase, &format!("{ability:?}"));
        let Some(ActiveBattle::Wild(WildBattle { battle, .. })) = phase.active_battle.as_ref()
        else {
            unreachable!("asserted above");
        };
        assert_eq!(phase.rng.state(), baseline.rng.state(), "{ability:?}");
        assert_eq!(battle.enemy().personality(), base.enemy().personality());
        assert_eq!(battle.enemy().ivs(), base.enemy().ivs());
        assert_eq!(battle.random_turn_number(), base.random_turn_number());
    }
}
