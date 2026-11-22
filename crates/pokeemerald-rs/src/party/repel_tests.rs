//! The party scan behind `IsWildLevelAllowedByRepel`
//! (`pokeemerald/src/wild_encounter.c:874-895`).

use super::{
    first_repel_qualifying_level, read_u32, to_save_pokemon, LoadedLead, IS_EGG_BIT, MISC_IV_WORD,
};
use battle::{BattlePokemon, Dex, Ivs};
use engine::save::Pokemon;

const PARTY_SIZE: usize = 6;

const HEADER_FLAGS_BYTE: usize = 19;
const HEADER_EGG_BIT: u8 = 1 << 2;

fn battler(level: u8, moves: Vec<assets::MoveId>) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        assets::SpeciesId(277),
        level,
        Ivs::default(),
        0x1234_ABCD,
        moves,
    )
    .expect("treecko")
}

fn healthy(level: u8) -> Pokemon {
    to_save_pokemon(&Dex::new(), &battler(level, vec![assets::MoveId::POUND]))
}

fn fainted(level: u8) -> Pokemon {
    let mut record = healthy(level);
    record.hp = 0;
    record
}

/// Sets the secure-region `MON_DATA_IS_EGG` bit and leaves the header flag alone.
fn secure_egg(level: u8) -> Pokemon {
    let mut record = healthy(level);
    let mut substructures = record.box_data.substructures().unwrap();
    let word = read_u32(&substructures.misc, MISC_IV_WORD) | IS_EGG_BIT;
    substructures.misc[MISC_IV_WORD].copy_from_slice(&word.to_le_bytes());
    record.box_data.set_substructures(&substructures);
    assert!(!record.box_data.is_egg_header_flag());
    record
}

/// Sets only the header's `isEgg` flag, which `MON_DATA_IS_EGG` does not read.
fn header_only_egg(level: u8) -> Pokemon {
    let mut bytes = healthy(level).to_bytes();
    bytes[HEADER_FLAGS_BYTE] |= HEADER_EGG_BIT;
    Pokemon::from_bytes(bytes)
}

fn party(members: &[Pokemon]) -> [Pokemon; PARTY_SIZE] {
    let mut party = [Pokemon::default(); PARTY_SIZE];
    party[..members.len()].copy_from_slice(members);
    party
}

#[test]
fn the_first_member_with_hp_that_is_not_an_egg_supplies_the_level() {
    assert_eq!(
        first_repel_qualifying_level(&party(&[healthy(10), healthy(50)]), None),
        Some(10)
    );
    assert_eq!(
        first_repel_qualifying_level(&party(&[healthy(50), healthy(5)]), None),
        Some(50),
        "the first member wins, not the strongest or weakest"
    );
}

#[test]
fn a_fainted_slot_zero_is_skipped() {
    assert_eq!(
        first_repel_qualifying_level(&party(&[fainted(50), healthy(10)]), None),
        Some(10)
    );
}

#[test]
fn eggs_ahead_of_a_usable_member_are_skipped() {
    assert_eq!(
        first_repel_qualifying_level(&party(&[secure_egg(40), secure_egg(41), healthy(10)]), None),
        Some(10)
    );
}

#[test]
fn only_the_secure_egg_field_marks_an_egg() {
    assert_eq!(
        first_repel_qualifying_level(&party(&[header_only_egg(30), healthy(10)]), None),
        Some(30),
        "the header flag is MON_DATA_SANITY_IS_EGG, which Repel does not read"
    );
}

#[test]
fn no_usable_member_yields_none() {
    assert_eq!(first_repel_qualifying_level(&party(&[]), None), None);
    assert_eq!(
        first_repel_qualifying_level(&party(&[fainted(20), secure_egg(20)]), None),
        None
    );
}

#[test]
fn a_record_that_fails_its_checksum_counts_as_an_egg() {
    let mut bytes = healthy(40).to_bytes();
    bytes[28] ^= 0xFF; // the stored checksum word
    let corrupt = Pokemon::from_bytes(bytes);
    assert!(corrupt.box_data.substructures().is_err());
    assert_eq!(
        first_repel_qualifying_level(&party(&[corrupt, healthy(10)]), None),
        Some(10)
    );
}

#[test]
fn the_scan_covers_all_six_slots() {
    let mut members = [fainted(50); PARTY_SIZE];
    members[PARTY_SIZE - 1] = healthy(12);
    assert_eq!(first_repel_qualifying_level(&members, None), Some(12));
}

#[test]
fn a_moveset_the_battle_engine_cannot_load_still_qualifies() {
    let mut record = healthy(33);
    let mut substructures = record.box_data.substructures().unwrap();
    // A move in the second slot with the first left empty is an interior gap.
    substructures.attacks[0..2].copy_from_slice(&0u16.to_le_bytes());
    substructures.attacks[2..4].copy_from_slice(&1u16.to_le_bytes());
    record.box_data.set_substructures(&substructures);
    assert!(
        super::from_save_pokemon(&Dex::new(), &record).is_err(),
        "the fixture must be a record the battler decoder refuses"
    );
    assert_eq!(
        first_repel_qualifying_level(&party(&[record]), None),
        Some(33)
    );
}

#[test]
fn the_live_lead_overrides_its_saved_slot() {
    let stale = healthy(10);
    let lead =
        LoadedLead::backed_for_test(1, battler(20, vec![assets::MoveId::POUND]), healthy(10));
    let members = party(&[fainted(50), stale]);
    assert_eq!(
        first_repel_qualifying_level(&members, Some(&lead)),
        Some(20),
        "the live battler's level replaces the stale record's"
    );
    let mut fainted_lead =
        LoadedLead::backed_for_test(1, battler(20, vec![assets::MoveId::POUND]), healthy(10));
    fainted_lead.battler_mut().apply_damage(u32::MAX);
    assert_eq!(
        first_repel_qualifying_level(&members, Some(&fainted_lead)),
        None
    );
}
