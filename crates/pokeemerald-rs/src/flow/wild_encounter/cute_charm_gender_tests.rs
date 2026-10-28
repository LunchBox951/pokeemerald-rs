//! A Cute Charm lead's gender handoff through wild battle construction
//! (`CreateWildMon`, `pokeemerald/src/wild_encounter.c:395-412`).
//!
//! The lead handed in is an admissible battler: a Cute Charm battle lead is
//! refused at admission, so the party-slot-0 effect reaches construction as the
//! `cute_charm_gender` argument, as it does from a fainted slot 0.

use super::start_wild_battle_with_lead_effects;
use super::test_support::{player_mon, TEST_PLAYER_TRAINER_ID, WURMPLE};
use assets::{GenderRatio, MoveId, SpeciesId};
use battle::{Battle, Dex, Gender};
use engine::overworld::wild_encounter::WildEncounter;
use engine::rng::Rng;

const NIDORAN_MALE: SpeciesId = SpeciesId(32);
const NIDORAN_FEMALE: SpeciesId = SpeciesId(29);
const WILD_LEVEL: u8 = 2;
const NATURE_COUNT: u16 = 25;

fn ratio_of(species: SpeciesId) -> GenderRatio {
    Dex::new().species(species).unwrap().gender_ratio
}

/// The first personality draw from `probe` with `nature` and, when given, `gender`.
fn matching_personality(
    probe: &mut Rng,
    nature: u32,
    species: SpeciesId,
    gender: Option<Gender>,
) -> u32 {
    loop {
        let personality = probe.next_u32();
        let gender_matches = gender
            .is_none_or(|wanted| Gender::of_personality(ratio_of(species), personality) == wanted);
        if personality % u32::from(NATURE_COUNT) == nature && gender_matches {
            return personality;
        }
    }
}

/// The personality construction yields when no gender is forced: one nature draw
/// from `probe`, then the first matching personality.
fn ordinary_personality(probe: &mut Rng, species: SpeciesId) -> u32 {
    let nature = u32::from(probe.next_u16() % NATURE_COUNT);
    matching_personality(probe, nature, species, None)
}

/// The personality a forcing Cute Charm roll yields: the activation draw, one
/// nature draw, then the first personality of that nature and `gender`.
fn forced_personality(probe: &mut Rng, species: SpeciesId, gender: Gender) -> u32 {
    let _activation = probe.next_u16();
    let nature = u32::from(probe.next_u16() % NATURE_COUNT);
    matching_personality(probe, nature, species, Some(gender))
}

fn cute_charm_roll_wins(seed: u32) -> bool {
    !Rng::new(seed).next_u16().is_multiple_of(3)
}

fn build_with_cute_charm(
    species: SpeciesId,
    cute_charm_gender: Option<Gender>,
    seed: u32,
) -> Battle {
    let encounter = WildEncounter {
        slot: 0,
        species,
        level: WILD_LEVEL,
    };
    let lead = player_mon(SpeciesId::ZIGZAGOON.0, 50, vec![MoveId::TACKLE]);
    start_wild_battle_with_lead_effects(
        lead,
        None,
        cute_charm_gender,
        encounter,
        TEST_PLAYER_TRAINER_ID,
        &mut Rng::new(seed),
    )
    .expect("an admissible lead and a startable wild species")
}

fn enemy_gender(battle: &Battle) -> Gender {
    Gender::of_personality(
        ratio_of(battle.enemy().species()),
        battle.enemy().personality(),
    )
}

/// A seed where the activation draw is `activates` and the roll that follows it
/// would, with no forcing, give a gender other than `forced`.
fn seed_where(activates: bool, forced: Gender) -> u32 {
    (0u32..100_000)
        .find(|&seed| {
            let activation_wins = cute_charm_roll_wins(seed);
            let mut probe = Rng::new(seed);
            let _ = probe.next_u16();
            let unforced = ordinary_personality(&mut probe, WURMPLE);
            activation_wins == activates
                && Gender::of_personality(ratio_of(WURMPLE), unforced) != forced
        })
        .expect("a seed with the wanted activation exists")
}

#[test]
fn a_winning_cute_charm_roll_forces_the_requested_gender() {
    for forced in [Gender::Male, Gender::Female] {
        let seed = seed_where(true, forced);
        let battle = build_with_cute_charm(WURMPLE, Some(forced), seed);
        let expected = forced_personality(&mut Rng::new(seed), WURMPLE, forced);
        assert_eq!(battle.enemy().personality(), expected, "seed {seed}");
        assert_eq!(enemy_gender(&battle), forced, "seed {seed}");
    }
}

#[test]
fn a_zero_cute_charm_roll_falls_through_to_the_ordinary_personality() {
    let forced = Gender::Male;
    let seed = seed_where(false, forced);
    assert!(!cute_charm_roll_wins(seed));
    let battle = build_with_cute_charm(WURMPLE, Some(forced), seed);
    let mut probe = Rng::new(seed);
    let _activation = probe.next_u16();
    let expected = ordinary_personality(&mut probe, WURMPLE);
    assert_eq!(battle.enemy().personality(), expected, "seed {seed}");
    assert_ne!(enemy_gender(&battle), forced, "seed {seed}");
}

#[test]
fn without_a_cute_charm_holder_the_first_draw_is_the_nature() {
    let seed = seed_where(true, Gender::Male);
    let battle = build_with_cute_charm(WURMPLE, None, seed);
    let expected = ordinary_personality(&mut Rng::new(seed), WURMPLE);
    assert_eq!(battle.enemy().personality(), expected, "seed {seed}");
}

#[test]
fn a_gender_locked_wild_species_spends_no_activation_draw() {
    for (species, requested) in [
        (NIDORAN_MALE, Gender::Female),
        (NIDORAN_FEMALE, Gender::Male),
    ] {
        let seed = (0u32..100_000)
            .find(|&seed| cute_charm_roll_wins(seed))
            .unwrap();
        let battle = build_with_cute_charm(species, Some(requested), seed);
        let expected = ordinary_personality(&mut Rng::new(seed), species);
        assert_eq!(battle.enemy().personality(), expected, "{species:?}");
        assert_ne!(enemy_gender(&battle), requested, "{species:?}");
    }
}
