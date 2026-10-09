use std::ops::Range;

use super::super::to_save_pokemon;
use battle::{BattlePokemon, Dex, Ivs};
use engine::save::{BoxPokemon, Pokemon};

pub(super) const TREECKO: assets::SpeciesId = assets::SpeciesId(277);

const TORCHIC: assets::SpeciesId = assets::SpeciesId(280);

pub(super) const POUND: assets::MoveId = assets::MoveId::POUND;

pub(super) const SCRATCH: assets::MoveId = assets::MoveId::SCRATCH;

pub(super) const GROWL: assets::MoveId = assets::MoveId::GROWL;

pub(super) const FIXTURE_PERSONALITY: u32 = 0x1234_ABCD;

pub(super) const FIXTURE_ORIGINAL_TRAINER_ID: u32 = 0x89AB_CDEF;

pub(super) const EXPECTED_GROWTH_HELD_ITEM: Range<usize> = 2..4;

pub(super) const EXPECTED_GROWTH_EXPERIENCE: Range<usize> = 4..8;

pub(super) const EXPECTED_GROWTH_FRIENDSHIP: usize = 9;

pub(super) const EXPECTED_ATTACK_PP_OFFSET: usize = 8;

pub(super) const EXPECTED_MISC_IV_WORD: Range<usize> = 4..8;

pub(super) const EXPECTED_IS_EGG_BIT: u32 = 1 << 30;

const MISC_POKERUS: usize = 0;

pub(super) const MISC_MET_LOCATION: usize = 1;

pub(super) const MISC_MET_DATA: Range<usize> = 2..4;

pub(super) const MISC_RIBBONS: Range<usize> = 8..12;

pub(super) const BOX_NICKNAME: Range<usize> = 8..18;

pub(super) const BOX_LANGUAGE: usize = 18;

pub(super) const BOX_OT_NAME: Range<usize> = 20..27;

const BOX_MARKINGS: usize = 27;

const RETAINED_HELD_ITEM: u16 = 200;

pub(super) const RETAINED_STATUS: u32 = 1 << 6;

pub(super) const RETAINED_MAIL: u8 = 2;

pub(super) const RETAINED_FRIENDSHIP: u8 = 213;

pub(super) const RETAINED_EVS_AND_CONDITION: [u8; engine::save::SUBSTRUCTURE_LEN] =
    [252, 6, 0, 2, 4, 246, 11, 22, 33, 44, 55, 66];
const RETAINED_STAT_BONUS: [u16; 6] = [7, 15, 1, 15, 1, 2];

const RETAINED_POKERUS: u8 = 0x24;

const RETAINED_MET_LOCATION: u8 = 0x59;

const RETAINED_MET_DATA: u16 = 0xB2C5;

const RETAINED_RIBBONS: u32 = 0x1234_5678;

const RETAINED_NICKNAME: [u8; 10] = [0xBB; 10];

const RETAINED_LANGUAGE: u8 = 5;

const RETAINED_OT_NAME: [u8; 7] = [0xCC; 7];

const RETAINED_MARKINGS: u8 = 0b0000_1010;

pub(super) fn treecko_fixture() -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        TREECKO,
        12,
        Ivs {
            hp: 1,
            attack: 2,
            defense: 3,
            speed: 4,
            sp_attack: 5,
            sp_defense: 6,
        },
        FIXTURE_PERSONALITY,
        battle::initial_moveset(TREECKO, 12),
    )
    .expect("Treecko with its level-12 learnset is in the dex")
    .with_original_trainer_id(FIXTURE_ORIGINAL_TRAINER_ID)
}

pub(super) fn torchic_before_learning_peck() -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        TORCHIC,
        15,
        Ivs {
            hp: 1,
            attack: 2,
            defense: 3,
            speed: 4,
            sp_attack: 5,
            sp_defense: 6,
        },
        FIXTURE_PERSONALITY,
        vec![SCRATCH, GROWL],
    )
    .expect("Torchic with a two-move starting set is in the dex")
    .with_original_trainer_id(FIXTURE_ORIGINAL_TRAINER_ID)
}

pub(super) fn retained_evs() -> battle::Evs {
    let [hp, attack, defense, speed, sp_attack, sp_defense, ..] = RETAINED_EVS_AND_CONDITION;
    battle::Evs {
        hp,
        attack,
        defense,
        speed,
        sp_attack,
        sp_defense,
    }
}

pub(super) fn stored_record_with_retained_fields() -> Pokemon {
    let mut record = to_save_pokemon(&Dex::new(), &treecko_fixture());

    let mut substructures = record.box_data.substructures().unwrap();
    substructures.growth[EXPECTED_GROWTH_HELD_ITEM]
        .copy_from_slice(&RETAINED_HELD_ITEM.to_le_bytes());
    substructures.growth[EXPECTED_GROWTH_FRIENDSHIP] = RETAINED_FRIENDSHIP;
    substructures.evs_and_condition = RETAINED_EVS_AND_CONDITION;
    substructures.misc[MISC_POKERUS] = RETAINED_POKERUS;
    substructures.misc[MISC_MET_LOCATION] = RETAINED_MET_LOCATION;
    substructures.misc[MISC_MET_DATA].copy_from_slice(&RETAINED_MET_DATA.to_le_bytes());
    substructures.misc[MISC_RIBBONS].copy_from_slice(&RETAINED_RIBBONS.to_le_bytes());
    record.box_data.set_substructures(&substructures);

    let mut bytes = record.box_data.to_bytes();
    bytes[BOX_NICKNAME].copy_from_slice(&RETAINED_NICKNAME);
    bytes[BOX_LANGUAGE] = RETAINED_LANGUAGE;
    bytes[BOX_OT_NAME].copy_from_slice(&RETAINED_OT_NAME);
    bytes[BOX_MARKINGS] = RETAINED_MARKINGS;
    record.box_data = BoxPokemon::from_bytes(bytes);

    record.status = RETAINED_STATUS;
    record.mail = RETAINED_MAIL;

    let [max_hp, attack, defense, speed, special_attack, special_defense] = RETAINED_STAT_BONUS;
    record.max_hp += max_hp;
    record.attack += attack;
    record.defense += defense;
    record.speed += speed;
    record.special_attack += special_attack;
    record.special_defense += special_defense;
    record
}
