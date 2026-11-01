//! Party codec and merge tests, split by concern.

// The retained-record bodies spell `super::encode_attacks`.
use super::encode_attacks;

mod active_selection;
mod codec;
mod common;
mod create_mon;
mod cute_charm_wild_gender;
mod loaded_lead;
mod merge_hp;
mod merge_stats;
mod retained_record;
mod stats;
mod take_item;

#[test]
fn slot_zero_synchronize_nature_reads_slot_zero_even_when_fainted() {
    let dex = battle::Dex::new();
    let ivs = battle::Ivs {
        hp: 0,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    };
    let mut mew = battle::BattlePokemon::new(
        &dex,
        assets::SpeciesId(151),
        50,
        ivs,
        0,
        vec![assets::MoveId::POUND],
    )
    .unwrap();
    mew.apply_damage(u32::MAX);
    let sync = super::to_save_pokemon(&dex, &mew);
    let plain = super::to_save_pokemon(&dex, &crate::new_game::provisional_starter());
    assert_eq!(
        super::slot_zero_synchronize_nature(&dex, &[sync, plain]),
        Some(battle::Nature::Hardy)
    );
    assert_eq!(
        super::slot_zero_synchronize_nature(&dex, &[plain, sync]),
        None
    );
    assert_eq!(super::slot_zero_synchronize_nature(&dex, &[]), None);
}

#[test]
fn slot_zero_synchronize_nature_ignores_an_interior_empty_move() {
    let dex = battle::Dex::new();
    let mew = battle::BattlePokemon::new(
        &dex,
        assets::SpeciesId(151),
        50,
        battle::Ivs::default(),
        0,
        vec![
            assets::MoveId::POUND,
            assets::MoveId::POUND,
            assets::MoveId::TACKLE,
        ],
    )
    .unwrap();
    let mut sync = super::to_save_pokemon(&dex, &mew);
    let mut subs = sync.box_data.substructures().unwrap();
    subs.attacks[2..4].fill(0);
    sync.box_data.set_substructures(&subs);
    let plain = super::to_save_pokemon(&dex, &crate::new_game::provisional_starter());
    let party = [sync, plain];
    assert_eq!(super::select_active_battler(&dex, &party).unwrap().0, 1);
    assert_eq!(
        super::slot_zero_synchronize_nature(&dex, &party),
        Some(battle::Nature::Hardy)
    );
}

#[test]
fn slot_zero_synchronize_nature_honours_the_header_is_egg_bit() {
    let dex = battle::Dex::new();
    let mew = battle::BattlePokemon::new(
        &dex,
        assets::SpeciesId(151),
        50,
        battle::Ivs::default(),
        0,
        vec![assets::MoveId::POUND],
    )
    .unwrap();
    let sync = super::to_save_pokemon(&dex, &mew);
    let mut bytes = sync.to_bytes();
    // Header `isEgg` (bit 2 of byte 19); the secure egg flag stays clear.
    bytes[19] |= 1 << 2;
    let header_egg = engine::save::Pokemon::from_bytes(bytes);
    assert!(
        header_egg.box_data.substructures().is_ok(),
        "the secure checksum does not cover the header"
    );
    let plain = super::to_save_pokemon(&dex, &crate::new_game::provisional_starter());
    assert_eq!(
        super::slot_zero_synchronize_nature(&dex, &[header_egg, plain]),
        None
    );
}

#[test]
fn slot_zero_synchronize_nature_resolves_a_slot_one_record_to_the_absent_secondary() {
    let dex = battle::Dex::new();
    let mew = battle::BattlePokemon::new(
        &dex,
        assets::SpeciesId(151),
        50,
        battle::Ivs::default(),
        0,
        vec![assets::MoveId::POUND],
    )
    .unwrap();
    let mut sync = super::to_save_pokemon(&dex, &mew);
    let mut subs = sync.box_data.substructures().unwrap();
    let word = u32::from_le_bytes(subs.misc[super::MISC_IV_WORD].try_into().unwrap())
        | (1 << super::ABILITY_SLOT_SHIFT);
    subs.misc[super::MISC_IV_WORD].copy_from_slice(&word.to_le_bytes());
    sync.box_data.set_substructures(&subs);
    let plain = super::to_save_pokemon(&dex, &crate::new_game::provisional_starter());
    assert_eq!(
        super::slot_zero_synchronize_nature(&dex, &[sync, plain]),
        None
    );
}

#[test]
fn slot_zero_synchronize_nature_rejects_a_header_bad_egg() {
    let dex = battle::Dex::new();
    let mew = battle::BattlePokemon::new(
        &dex,
        assets::SpeciesId(151),
        50,
        battle::Ivs::default(),
        0,
        vec![assets::MoveId::POUND],
    )
    .unwrap();
    let sync = super::to_save_pokemon(&dex, &mew);
    let mut bytes = sync.to_bytes();
    // Header `isBadEgg` only (bit 0 of byte 19); `isEgg` stays clear.
    bytes[19] |= 1;
    let bad_egg = engine::save::Pokemon::from_bytes(bytes);
    assert!(bad_egg.box_data.is_bad_egg());
    assert!(!bad_egg.box_data.is_egg_header_flag());
    assert!(bad_egg.box_data.substructures().is_ok());
    let plain = super::to_save_pokemon(&dex, &crate::new_game::provisional_starter());
    assert_eq!(
        super::slot_zero_synchronize_nature(&dex, &[bad_egg, plain]),
        None
    );
}
