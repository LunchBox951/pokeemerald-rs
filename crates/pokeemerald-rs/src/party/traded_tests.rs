//! The save-boundary traded classification: `IsTradedMon` through
//! `IsOtherTrainer` (`pokeemerald/src/pokemon.c:6602-6626`).

use super::{is_traded_record, to_save_pokemon};
use battle::{BattlePokemon, Dex, Ivs};
use engine::save::Pokemon;

const OWNER_ID: u32 = 0x1122_3344;
/// `RUSTY` in the game's text encoding, terminated.
const OWNER_NAME: [u8; 8] = [0xCC, 0xCF, 0xC9, 0xCD, 0xD3, 0xFF, 0, 0];

fn record(ot_id: u32, ot_name: [u8; 7]) -> Pokemon {
    let mut record = to_save_pokemon(
        &Dex::new(),
        &BattlePokemon::new(
            &Dex::new(),
            assets::SpeciesId(277),
            5,
            Ivs::default(),
            0x1234_ABCD,
            vec![assets::MoveId::POUND],
        )
        .expect("treecko")
        .with_original_trainer_id(ot_id),
    );
    record.box_data.set_ot_name(ot_name);
    record
}

#[test]
fn a_record_with_the_owners_id_and_name_is_not_traded() {
    let own = record(OWNER_ID, [0xCC, 0xCF, 0xC9, 0xCD, 0xD3, 0xFF, 0xFF]);
    assert!(!is_traded_record(&own, OWNER_ID, &OWNER_NAME));
}

#[test]
fn a_different_ot_id_is_traded_whatever_the_name() {
    let foreign = record(OWNER_ID ^ 1, [0xCC, 0xCF, 0xC9, 0xCD, 0xD3, 0xFF, 0xFF]);
    assert!(is_traded_record(&foreign, OWNER_ID, &OWNER_NAME));
}

#[test]
fn the_same_id_with_a_different_name_is_traded() {
    let other = record(OWNER_ID, [0xCC, 0xCF, 0xC9, 0xCE, 0xD3, 0xFF, 0xFF]);
    assert!(is_traded_record(&other, OWNER_ID, &OWNER_NAME));
}

#[test]
fn a_record_name_longer_than_the_owners_is_traded() {
    let longer = record(OWNER_ID, [0xCC, 0xCF, 0xC9, 0xCD, 0xD3, 0xD4, 0xFF]);
    assert!(is_traded_record(&longer, OWNER_ID, &OWNER_NAME));
}

/// Upstream stops at the record's own terminator (`pokemon.c:6618-6621`), so
/// an owner whose name merely extends the record's is not traded.
#[test]
fn an_owner_name_that_extends_the_records_is_not_traded() {
    let shorter = record(OWNER_ID, [0xCC, 0xCF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    assert!(!is_traded_record(&shorter, OWNER_ID, &OWNER_NAME));
}
