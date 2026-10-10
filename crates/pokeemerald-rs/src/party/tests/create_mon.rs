use super::super::{create_mon_nickname, to_save_pokemon};
use super::common::{
    treecko_fixture, BOX_LANGUAGE, BOX_NICKNAME, BOX_OT_NAME, MISC_MET_DATA, MISC_MET_LOCATION,
    TREECKO,
};
use battle::Dex;

#[test]
fn a_fresh_battler_never_writes_an_in_battle_primary_status_into_a_new_record() {
    let dex = Dex::new();
    let mut lead = treecko_fixture();
    lead.set_status1(battle::Status1::Poisoned);

    let record = to_save_pokemon(&dex, &lead);

    // With no backing record to retain a status from, `to_save_pokemon`
    // falls back to `CreateMon`'s default whatever the in-battle status is.
    let healthy_lead = treecko_fixture();
    let healthy_record = to_save_pokemon(&dex, &healthy_lead);
    assert_eq!(
        record.status, healthy_record.status,
        "a poisoned battler must not write a different status word than a healthy one"
    );
}

/// `CreateBoxMon` (`pokeemerald/src/pokemon.c:2250-2263`) stamps every
/// record it builds with the level it was created at, `gGameVersion`, and
/// `ITEM_POKE_BALL` before any caller sees it.
#[test]
fn a_from_scratch_record_carries_create_mons_met_level_game_and_ball() {
    const VERSION_EMERALD: u16 = 3;
    const ITEM_POKE_BALL: u16 = 4;
    const MET_LEVEL_MASK: u16 = 0x7F;
    const MET_GAME_SHIFT: u16 = 7;
    const POKE_BALL_SHIFT: u16 = 11;
    const NIBBLE: u16 = 0xF;

    let dex = Dex::new();
    let mon = treecko_fixture();
    let built = to_save_pokemon(&dex, &mon);
    let origins = u16::from_le_bytes(
        built.box_data.substructures().unwrap().misc[MISC_MET_DATA]
            .try_into()
            .unwrap(),
    );

    assert_eq!(
        origins & MET_LEVEL_MASK,
        u16::from(mon.level()),
        "the met level is the level the record was built at"
    );
    assert_eq!(
        (origins >> MET_GAME_SHIFT) & NIBBLE,
        VERSION_EMERALD,
        "the met game is this game"
    );
    assert_eq!(
        (origins >> POKE_BALL_SHIFT) & NIBBLE,
        ITEM_POKE_BALL,
        "and the ball is a Poké Ball, not ITEM_NONE"
    );
}

/// Upstream stamps met level exactly once, when `CreateBoxMon` first builds
/// the record (`pokeemerald/src/pokemon.c:2259`); every later save just
/// copies the existing bytes unchanged (`src/load_save.c:160-168`), so a
/// mon that levels up in its first battle, before its first save ever runs
/// (a fresh game's provisional starter has no backing record -- the same
/// scenario `to_save_pokemon_files_ev_aware_stats_after_a_level_up` above
/// exercises), must still file the level it was met at, not the level it
/// happens to be the moment that first save fires.
#[test]
fn a_from_scratch_record_files_the_level_it_was_met_at_not_its_current_level() {
    const MET_LEVEL_MASK: u16 = 0x7F;

    let dex = Dex::new();
    let mut mon = treecko_fixture();
    let created_at_level = mon.created_at_level();
    let species = dex.species(mon.species()).unwrap();
    let next_level_experience =
        assets::experience_for_level(species.growth_rate, created_at_level + 1).unwrap();
    mon.apply_experience(&dex, next_level_experience - mon.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(
        mon.level(),
        created_at_level + 1,
        "fixture sanity: the level moved before this mon's first save"
    );

    let built = to_save_pokemon(&dex, &mon);
    let origins = u16::from_le_bytes(
        built.box_data.substructures().unwrap().misc[MISC_MET_DATA]
            .try_into()
            .unwrap(),
    );

    assert_eq!(
        origins & MET_LEVEL_MASK,
        u16::from(created_at_level),
        "the met level is the level this mon was created at, not the \
         higher level it levelled up to before ever being saved"
    );
}

/// `CreateBoxMon` also stamps the species' own display name into the
/// unencrypted header's nickname field and `gGameLanguage` into the byte
/// beside it (`pokeemerald/src/pokemon.c:2249-2251`), both derivable from
/// `to_save_pokemon`'s own arguments alone.
#[test]
fn a_from_scratch_record_carries_create_mons_nickname_and_language() {
    const LANGUAGE_ENGLISH: u8 = 2;
    // "TREECKO" game-text encoded, `EOS`-terminated, zero-padded to the
    // 10-byte header field.
    const TREECKO_NICKNAME: [u8; 10] = [206, 204, 191, 191, 189, 197, 201, 255, 0, 0];

    let dex = Dex::new();
    let mon = treecko_fixture();
    let built = to_save_pokemon(&dex, &mon);
    let bytes = built.box_data.to_bytes();

    assert_eq!(&bytes[BOX_NICKNAME], &TREECKO_NICKNAME);
    assert_eq!(bytes[BOX_LANGUAGE], LANGUAGE_ENGLISH);
    assert_eq!(create_mon_nickname(TREECKO), TREECKO_NICKNAME);
}

/// `SetBoxMonData(MON_DATA_SPECIES)` raises `hasSpecies` for a nonzero
/// species (`pokeemerald/src/pokemon.c:4220-4227`); occupancy reads that bit, not growth.
#[test]
fn a_from_scratch_record_sets_the_header_has_species_bit() {
    let dex = Dex::new();
    let mon = treecko_fixture();
    assert_ne!(mon.species().0, 0, "the fixture must hold a real species");

    let built = to_save_pokemon(&dex, &mon);

    assert!(
        built.box_data.has_species(),
        "a record carrying a nonzero growth species must raise the header's \
         hasSpecies bit, or Emerald reads the slot as empty"
    );
}

/// A ten-glyph species name (encoded length eleven, with the `EOS`
/// terminator) fills the ten-byte nickname field exactly, matching
/// `SetBoxMonData`'s fixed-width copy (`pokeemerald/src/pokemon.c:4185-4190`,
/// `StringCopyN`) rather than overflowing it or dropping the terminator's
/// slot.
#[test]
fn a_ten_glyph_species_name_uses_every_nickname_byte() {
    const CHARMANDER: assets::SpeciesId = assets::SpeciesId(4);

    let encoded = engine::text::encode_str("CHARMANDER").unwrap();
    assert_eq!(encoded.len(), 11, "fixture sanity: ten glyphs plus EOS");
    assert_eq!(create_mon_nickname(CHARMANDER), encoded[..10]);
}

/// OT name, met location, and OT gender need the current save's player
/// identity and map, which `to_save_pokemon` cannot see from its own
/// arguments -- they stay clear rather than a fabricated value (module
/// docs, issue #869).
#[test]
fn a_from_scratch_record_leaves_unreachable_creation_metadata_clear() {
    const OT_GENDER_BIT: u16 = 1 << 15;

    let dex = Dex::new();
    let mon = treecko_fixture();
    let built = to_save_pokemon(&dex, &mon);
    let bytes = built.box_data.to_bytes();
    let substructures = built.box_data.substructures().unwrap();
    let origins = u16::from_le_bytes(substructures.misc[MISC_MET_DATA].try_into().unwrap());

    assert_eq!(&bytes[BOX_OT_NAME], &[0; 7], "OT name stays clear");
    assert_eq!(
        substructures.misc[MISC_MET_LOCATION], 0,
        "met location stays clear"
    );
    assert_eq!(origins & OT_GENDER_BIT, 0, "OT gender stays clear");
}
