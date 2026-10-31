//! Party slot 0's Cute Charm handoff to a wild Pokémon's gender.

use super::super::{slot_zero_cute_charm_wild_gender, to_save_pokemon};
use assets::{MoveId, SpeciesId};
use battle::{BattlePokemon, Dex, Gender, Ivs};
use engine::save::Pokemon;

const CLEFAIRY: SpeciesId = SpeciesId(35);
const MEW: SpeciesId = SpeciesId(151);

/// Clefairy's ratio byte is 191: a personality with a low byte below it is female.
const FEMALE_CLEFAIRY_PERSONALITY: u32 = 0;
const MALE_CLEFAIRY_PERSONALITY: u32 = 200;

const HEADER_FLAGS_BYTE: usize = 19;
const HEADER_BAD_EGG_BIT: u8 = 1 << 0;
const HEADER_EGG_BIT: u8 = 1 << 2;
const FIRST_SECURE_BYTE: usize = 32;

fn saved(species: SpeciesId, personality: u32) -> Pokemon {
    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        species,
        50,
        Ivs::default(),
        personality,
        vec![MoveId::POUND],
    )
    .unwrap();
    to_save_pokemon(&dex, &mon)
}

fn ordinary_party_mate() -> Pokemon {
    to_save_pokemon(&Dex::new(), &crate::new_game::provisional_starter())
}

fn wild_gender(party: &[Pokemon]) -> Option<Gender> {
    slot_zero_cute_charm_wild_gender(&Dex::new(), party)
}

#[test]
fn a_female_slot_zero_hands_the_wild_mon_male_and_a_male_one_female() {
    let female = saved(CLEFAIRY, FEMALE_CLEFAIRY_PERSONALITY);
    let male = saved(CLEFAIRY, MALE_CLEFAIRY_PERSONALITY);
    assert_eq!(wild_gender(&[female]), Some(Gender::Male));
    assert_eq!(wild_gender(&[male]), Some(Gender::Female));
}

#[test]
fn a_fainted_slot_zero_behind_a_healthy_lead_still_hands_over_its_gender() {
    let dex = Dex::new();
    let mut clefairy = BattlePokemon::new(
        &dex,
        CLEFAIRY,
        50,
        Ivs::default(),
        FEMALE_CLEFAIRY_PERSONALITY,
        vec![MoveId::POUND],
    )
    .unwrap();
    clefairy.apply_damage(u32::MAX);
    let fainted = to_save_pokemon(&dex, &clefairy);
    let party = [fainted, ordinary_party_mate()];
    assert_eq!(wild_gender(&party), Some(Gender::Male));
}

#[test]
fn only_slot_zero_counts() {
    let clefairy = saved(CLEFAIRY, FEMALE_CLEFAIRY_PERSONALITY);
    assert_eq!(wild_gender(&[ordinary_party_mate(), clefairy]), None);
    assert_eq!(wild_gender(&[]), None);
}

#[test]
fn a_slot_zero_with_another_ability_hands_over_nothing() {
    assert_eq!(wild_gender(&[saved(MEW, 0)]), None);
}

#[test]
fn a_header_egg_or_bad_egg_hands_over_nothing() {
    for flag in [HEADER_EGG_BIT, HEADER_BAD_EGG_BIT] {
        let mut bytes = saved(CLEFAIRY, FEMALE_CLEFAIRY_PERSONALITY).to_bytes();
        bytes[HEADER_FLAGS_BYTE] |= flag;
        let egg = Pokemon::from_bytes(bytes);
        assert_eq!(wild_gender(&[egg, ordinary_party_mate()]), None);
    }
}

#[test]
fn a_record_failing_its_secure_checksum_hands_over_nothing() {
    let mut bytes = saved(CLEFAIRY, FEMALE_CLEFAIRY_PERSONALITY).to_bytes();
    bytes[FIRST_SECURE_BYTE] ^= 0xFF;
    let corrupted = Pokemon::from_bytes(bytes);
    assert!(corrupted.box_data.substructures().is_err());
    assert_eq!(wild_gender(&[corrupted]), None);
}

#[test]
fn a_malformed_moveset_does_not_suppress_the_handoff() {
    let mut clefairy = saved(CLEFAIRY, FEMALE_CLEFAIRY_PERSONALITY);
    let mut substructures = clefairy.box_data.substructures().unwrap();
    substructures.attacks[0..2].copy_from_slice(&u16::MAX.to_le_bytes());
    clefairy.box_data.set_substructures(&substructures);
    assert_eq!(wild_gender(&[clefairy]), Some(Gender::Male));
}

#[test]
fn a_stored_second_ability_slot_with_no_second_ability_hands_over_nothing() {
    let mut clefairy = saved(CLEFAIRY, FEMALE_CLEFAIRY_PERSONALITY);
    let mut substructures = clefairy.box_data.substructures().unwrap();
    let iv_word = u32::from_le_bytes(
        substructures.misc[crate::party::MISC_IV_WORD]
            .try_into()
            .unwrap(),
    ) | (1 << crate::party::ABILITY_SLOT_SHIFT);
    substructures.misc[crate::party::MISC_IV_WORD].copy_from_slice(&iv_word.to_le_bytes());
    clefairy.box_data.set_substructures(&substructures);
    assert_eq!(wild_gender(&[clefairy]), None);
}

#[test]
fn an_unsaved_live_lead_hands_over_the_opposite_gender_only_with_cute_charm() {
    let dex = Dex::new();
    let live = |species, personality| {
        BattlePokemon::new(
            &dex,
            species,
            50,
            Ivs::default(),
            personality,
            vec![MoveId::POUND],
        )
        .unwrap()
    };
    let hands_over = |lead| super::super::live_lead_cute_charm_wild_gender(&dex, &lead);
    assert_eq!(
        hands_over(live(CLEFAIRY, FEMALE_CLEFAIRY_PERSONALITY)),
        Some(Gender::Male)
    );
    assert_eq!(
        hands_over(live(CLEFAIRY, MALE_CLEFAIRY_PERSONALITY)),
        Some(Gender::Female)
    );
    assert_eq!(hands_over(live(MEW, 0)), None);
}
