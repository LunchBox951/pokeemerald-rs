use super::super::{to_save_pokemon, LoadedLead, PartyError};
use super::common::{
    torchic_before_learning_peck, treecko_fixture, EXPECTED_IS_EGG_BIT, EXPECTED_MISC_IV_WORD,
};
use battle::Dex;
use engine::save::Pokemon;

/// Sets the secure-region egg flag on an already-encoded record, the same
/// decode/OR/re-encode shape `the_merge_rewrites_the_iv_word_around_the_egg_bit`
/// uses.
fn as_egg(mut record: Pokemon) -> Pokemon {
    let mut substructures = record.box_data.substructures().unwrap();
    let iv_word = u32::from_le_bytes(
        substructures.misc[EXPECTED_MISC_IV_WORD]
            .try_into()
            .unwrap(),
    );
    substructures.misc[EXPECTED_MISC_IV_WORD]
        .copy_from_slice(&(iv_word | EXPECTED_IS_EGG_BIT).to_le_bytes());
    record.box_data.set_substructures(&substructures);
    record
}

#[test]
fn select_active_battler_skips_a_fainted_slot_0_for_a_healthy_slot_1() {
    let dex = Dex::new();
    let mut fainted = treecko_fixture();
    fainted.apply_damage(u32::MAX);
    let healthy = torchic_before_learning_peck();
    let party = [
        to_save_pokemon(&dex, &fainted),
        to_save_pokemon(&dex, &healthy),
    ];

    let loaded = LoadedLead::load(&dex, &party).expect("slot 1 is usable");
    assert_eq!(loaded.slot(), 1);
    assert_eq!(loaded.battler().species(), healthy.species());
    assert_eq!(loaded.record().to_bytes(), party[1].to_bytes());
}

/// Sets only the unencrypted header's `isBadEgg` bit, leaving the secure
/// egg flag clear and the checksum valid.
fn as_header_bad_egg(record: Pokemon) -> Pokemon {
    let secure = record.box_data.substructures().unwrap();
    let iv_word = u32::from_le_bytes(secure.misc[EXPECTED_MISC_IV_WORD].try_into().unwrap());
    assert_eq!(iv_word & EXPECTED_IS_EGG_BIT, 0);
    let mut bytes = record.to_bytes();
    bytes[19] |= 1;
    let bad_egg = Pokemon::from_bytes(bytes);
    assert_eq!(bad_egg.box_data.checksum(), record.box_data.checksum());
    assert_eq!(bad_egg.box_data.substructures().unwrap(), secure);
    bad_egg
}

#[test]
fn select_active_battler_skips_a_header_bad_egg_slot_0_for_a_healthy_slot_1() {
    let dex = Dex::new();
    let bad_egg = as_header_bad_egg(to_save_pokemon(&dex, &treecko_fixture()));
    let bad_egg_bytes = bad_egg.to_bytes();
    let healthy = torchic_before_learning_peck();
    let party = [bad_egg, to_save_pokemon(&dex, &healthy)];

    let loaded = LoadedLead::load(&dex, &party).expect("slot 1 is usable");
    assert_eq!(loaded.slot(), 1);
    assert_eq!(loaded.battler().species(), healthy.species());
    assert_eq!(loaded.record().to_bytes(), party[1].to_bytes());
    assert_eq!(party[0].to_bytes(), bad_egg_bytes);
}

#[test]
fn select_active_battler_rejects_a_header_bad_egg_only_party() {
    let dex = Dex::new();
    let bad_egg = as_header_bad_egg(to_save_pokemon(&dex, &treecko_fixture()));
    let before = bad_egg.to_bytes();
    let party = [bad_egg];

    let err = LoadedLead::load(&dex, &party).expect_err("a Bad Egg is not a battler");
    assert!(matches!(err, PartyError::Egg), "{err}");
    assert_eq!(party[0].to_bytes(), before);
}

#[test]
fn select_active_battler_skips_an_egg_slot_0_for_a_healthy_slot_1() {
    let dex = Dex::new();
    let egg = as_egg(to_save_pokemon(&dex, &treecko_fixture()));
    let healthy = torchic_before_learning_peck();
    let party = [egg, to_save_pokemon(&dex, &healthy)];

    let loaded = LoadedLead::load(&dex, &party).expect("slot 1 is usable");
    assert_eq!(loaded.slot(), 1);
    assert_eq!(loaded.battler().species(), healthy.species());
    assert_eq!(loaded.record().to_bytes(), party[1].to_bytes());
}

#[test]
fn select_active_battler_falls_back_to_a_fainted_slot_0_when_nothing_is_usable() {
    let dex = Dex::new();
    let mut fainted = treecko_fixture();
    fainted.apply_damage(u32::MAX);
    let party = [to_save_pokemon(&dex, &fainted)];

    let loaded =
        LoadedLead::load(&dex, &party).expect("slot 0's own decode still succeeds, fainted or not");
    assert_eq!(loaded.slot(), 0);
    assert!(loaded.battler().is_fainted());
}

#[test]
fn select_active_battler_surfaces_slot_0s_decode_error_when_nothing_is_usable() {
    let dex = Dex::new();
    let party = [Pokemon::default()];

    let err = LoadedLead::load(&dex, &party).expect_err("SPECIES_NONE is not a fightable mon");
    assert!(matches!(err, PartyError::Battler(_)), "{err}");
}
