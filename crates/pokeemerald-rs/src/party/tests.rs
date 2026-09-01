use std::ops::Range;

use super::{
    compute_levelled_up_stats, evs_from_substruct2, from_save_pokemon, hp_hidden_by_load,
    merge_into_save_pokemon, pack_ivs, to_save_pokemon, unpack_ivs, zero_ev_max_hp, PartyError,
    MAIL_NONE,
};
use battle::{BattlePokemon, Dex, Ivs};
use engine::save::{BoxPokemon, Pokemon};

const TREECKO: assets::SpeciesId = assets::SpeciesId(277);
const TORCHIC: assets::SpeciesId = assets::SpeciesId(280);
const TENTACOOL: assets::SpeciesId = assets::SpeciesId(72);

const POUND: assets::MoveId = assets::MoveId(1);
const SCRATCH: assets::MoveId = assets::MoveId(10);
const TACKLE: assets::MoveId = assets::MoveId(33);
const GROWL: assets::MoveId = assets::MoveId(45);
const PECK: assets::MoveId = assets::MoveId(64);

const FIXTURE_PERSONALITY: u32 = 0x1234_ABCD;
const FIXTURE_ORIGINAL_TRAINER_ID: u32 = 0x89AB_CDEF;

const EXPECTED_GROWTH_SPECIES: Range<usize> = 0..2;
const EXPECTED_GROWTH_HELD_ITEM: Range<usize> = 2..4;
const EXPECTED_GROWTH_EXPERIENCE: Range<usize> = 4..8;
const EXPECTED_GROWTH_PP_BONUSES: usize = 8;
const EXPECTED_GROWTH_FRIENDSHIP: usize = 9;
const EXPECTED_ATTACK_PP_OFFSET: usize = 8;
const EXPECTED_MISC_IV_WORD: Range<usize> = 4..8;

const EXPECTED_IV_FIELD_WIDTH: usize = 5;
const EXPECTED_IV_FIELD_MASK: u32 = 0x1F;
const EXPECTED_IS_EGG_BIT: u32 = 1 << 30;
const EXPECTED_ABILITY_SLOT_SHIFT: usize = 31;

const HP_EV_INDEX: usize = 0;
const ATTACK_EV_INDEX: usize = 1;
const DEFENSE_EV_INDEX: usize = 2;
const MAX_EFFECTIVE_EV: u8 = 252;
const MAX_TOTAL_EVS: u16 = 510;
const MISC_POKERUS: usize = 0;
const MISC_MET_LOCATION: usize = 1;
const MISC_MET_DATA: Range<usize> = 2..4;
const MISC_ENCOUNTER_DATA: Range<usize> = 0..4;
const MISC_RIBBONS: Range<usize> = 8..12;
const BOX_NICKNAME: Range<usize> = 8..18;
const BOX_LANGUAGE: usize = 18;
const BOX_OT_NAME: Range<usize> = 20..27;
const BOX_MARKINGS: usize = 27;
const BOX_RETAINED_HEADER: Range<usize> = 8..28;

fn treecko_fixture() -> BattlePokemon {
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

#[test]
fn ivs_pack_into_five_bit_fields_in_declaration_order() {
    let ivs = Ivs {
        hp: 1,
        attack: 2,
        defense: 3,
        speed: 4,
        sp_attack: 5,
        sp_defense: 6,
    };
    let word = pack_ivs(ivs);
    assert_eq!(word & EXPECTED_IV_FIELD_MASK, 1);
    assert_eq!(
        (word >> EXPECTED_IV_FIELD_WIDTH) & EXPECTED_IV_FIELD_MASK,
        2
    );
    assert_eq!(
        (word >> (2 * EXPECTED_IV_FIELD_WIDTH)) & EXPECTED_IV_FIELD_MASK,
        3
    );
    assert_eq!(
        (word >> (3 * EXPECTED_IV_FIELD_WIDTH)) & EXPECTED_IV_FIELD_MASK,
        4
    );
    assert_eq!(
        (word >> (4 * EXPECTED_IV_FIELD_WIDTH)) & EXPECTED_IV_FIELD_MASK,
        5
    );
    assert_eq!(
        (word >> (5 * EXPECTED_IV_FIELD_WIDTH)) & EXPECTED_IV_FIELD_MASK,
        6
    );
    assert_eq!(word >> (6 * EXPECTED_IV_FIELD_WIDTH), 0);
    assert_eq!(unpack_ivs(word), ivs);
}

#[test]
fn evs_from_substruct2_maps_each_byte_to_its_named_field() {
    let evs_and_condition: [u8; engine::save::SUBSTRUCTURE_LEN] =
        [10, 20, 30, 40, 50, 60, 0, 0, 0, 0, 0, 0];
    assert_eq!(
        evs_from_substruct2(&evs_and_condition),
        battle::Evs {
            hp: 10,
            attack: 20,
            defense: 30,
            speed: 40,
            sp_attack: 50,
            sp_defense: 60,
        }
    );
}

fn shedinja_fixture(level: u8) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        battle::SPECIES_SHEDINJA,
        level,
        Ivs {
            hp: 31,
            attack: 1,
            defense: 2,
            speed: 3,
            sp_attack: 4,
            sp_defense: 5,
        },
        FIXTURE_PERSONALITY,
        battle::initial_moveset(battle::SPECIES_SHEDINJA, level),
    )
    .expect("Shedinja with its own learnset is representable")
}

#[test]
fn compute_levelled_up_stats_forces_shedinja_to_one_max_hp() {
    let dex = Dex::new();
    let mon = shedinja_fixture(50);
    let evs_and_condition: [u8; engine::save::SUBSTRUCTURE_LEN] =
        [252, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let stats = compute_levelled_up_stats(&dex, &mon, &evs_and_condition);
    assert_eq!(stats.max_hp, 1);
}

#[test]
fn zero_ev_max_hp_is_one_for_shedinja() {
    let dex = Dex::new();
    let mon = shedinja_fixture(50);
    assert_eq!(
        zero_ev_max_hp(&dex, battle::SPECIES_SHEDINJA.0, 50, &mon),
        1
    );
}

#[test]
fn a_levelled_up_shedinja_lead_saves_at_one_max_hp() {
    let dex = Dex::new();
    let lead = shedinja_fixture(20);
    let stored = to_save_pokemon(&dex, &lead);

    let mut levelled = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let next_level_experience = assets::experience_for_level(
        dex.species(battle::SPECIES_SHEDINJA).unwrap().growth_rate,
        21,
    )
    .unwrap();
    levelled
        .apply_experience(&dex, next_level_experience - levelled.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(levelled.level(), 21, "fixture sanity: the level moved");
    assert_eq!(levelled.stats().max_hp, 1);
    assert_eq!(levelled.current_hp(), 1);

    let mut offset = hp_hidden_by_load(&dex, &stored, &levelled);
    let merged = merge_into_save_pokemon(&dex, &levelled, &stored, &mut offset);
    assert_eq!(merged.max_hp, 1, "the merge recomputed a level-21 block");
    assert_eq!(
        merged.hp, 1,
        "a Shedinja lead is never filed above its one point"
    );
}

#[test]
fn an_unchanged_shedinja_lead_normalizes_a_stale_stored_maximum() {
    let dex = Dex::new();
    let lead = shedinja_fixture(20);
    let mut stored = to_save_pokemon(&dex, &lead);
    stored.max_hp = 40;
    stored.hp = 40;

    let reloaded = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert_eq!(
        reloaded.stats().max_hp,
        1,
        "fixture sanity: the live model is already correct regardless of \
         the stale stored bytes"
    );

    let mut offset = hp_hidden_by_load(&dex, &stored, &reloaded);
    let merged = merge_into_save_pokemon(&dex, &reloaded, &stored, &mut offset);
    assert_eq!(
        merged.max_hp, 1,
        "an unchanged-level Shedinja still normalizes a stale stored \
         maximum rather than carrying it forward unchanged"
    );
    assert_eq!(merged.hp, 1);
    assert_eq!(
        offset, 0,
        "the points the normalization removed leave the offset with them; \
         they are not real hidden HP under a maximum of 1"
    );

    let resaved = merge_into_save_pokemon(&dex, &reloaded, &merged, &mut offset);
    assert_eq!(resaved.max_hp, 1);
    assert_eq!(resaved.hp, 1);
    assert_eq!(offset, 0);
}

#[test]
fn an_unchanged_shedinja_keeps_the_five_cached_stats_its_evs_have_outrun() {
    let dex = Dex::new();
    let lead = shedinja_fixture(20);
    let mut stored = to_save_pokemon(&dex, &lead);

    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[ATTACK_EV_INDEX] = MAX_EFFECTIVE_EV;
    substructures.evs_and_condition[DEFENSE_EV_INDEX] = MAX_EFFECTIVE_EV;
    stored.box_data.set_substructures(&substructures);
    stored.max_hp = 40;
    stored.hp = 40;

    let reloaded = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let ev_aware = compute_levelled_up_stats(&dex, &reloaded, &substructures.evs_and_condition);
    assert!(
        ev_aware.attack > u32::from(stored.attack),
        "fixture sanity: a fresh EV-aware recompute really would move the \
         cached Attack, so retaining it is an observable choice"
    );

    let mut offset = hp_hidden_by_load(&dex, &stored, &reloaded);
    let merged = merge_into_save_pokemon(&dex, &reloaded, &stored, &mut offset);

    assert_eq!(merged.max_hp, 1, "the invariant entry is normalized");
    assert_eq!(merged.hp, 1);
    for (stat, filed, retained) in [
        ("Attack", merged.attack, stored.attack),
        ("Defense", merged.defense, stored.defense),
        ("Speed", merged.speed, stored.speed),
        ("Sp. Attack", merged.special_attack, stored.special_attack),
        (
            "Sp. Defense",
            merged.special_defense,
            stored.special_defense,
        ),
    ] {
        assert_eq!(
            filed, retained,
            "{stat} remains cached until a stat-recomputation event"
        );
    }
}

#[test]
fn the_egg_and_ability_bits_do_not_leak_into_the_ivs() {
    let ivs = Ivs {
        hp: 31,
        attack: 31,
        defense: 31,
        speed: 31,
        sp_attack: 31,
        sp_defense: 31,
    };
    let egg_and_ability_bits = EXPECTED_IS_EGG_BIT | (1 << EXPECTED_ABILITY_SLOT_SHIFT);
    assert_eq!(unpack_ivs(pack_ivs(ivs) | egg_and_ability_bits), ivs);
}

#[test]
fn a_battler_round_trips_through_the_save_layout() {
    let dex = Dex::new();
    let mut mon = treecko_fixture();
    mon.apply_damage(7);
    mon.deduct_pp(0).unwrap();
    mon.deduct_pp(0).unwrap();

    let saved = to_save_pokemon(&dex, &mon);
    let restored = from_save_pokemon(&dex, &saved).expect("what we just wrote must decode");

    assert_eq!(restored.species(), mon.species());
    assert_eq!(restored.level(), mon.level());
    assert_eq!(restored.personality(), mon.personality());
    assert_eq!(restored.original_trainer_id(), mon.original_trainer_id());
    assert_eq!(restored.nature(), mon.nature());
    assert_eq!(restored.ivs(), mon.ivs());
    assert_eq!(restored.stats(), mon.stats());
    assert_eq!(
        restored.current_hp(),
        mon.current_hp(),
        "damage taken before saving must survive the save"
    );
    assert_ne!(
        restored.current_hp(),
        restored.stats().max_hp,
        "the fixture must save a damaged mon, or full-HP restore would pass"
    );
    assert_eq!(restored.moves(), mon.moves(), "moves and PP, slot for slot");
}

#[test]
fn sub_level_experience_survives_the_round_trip() {
    let dex = Dex::new();
    let mut mon = treecko_fixture();
    assert!(mon.apply_experience(&dex, 10).unwrap().is_none());
    let treecko = dex.species(mon.species()).unwrap();
    assert_eq!(
        mon.experience(),
        assets::experience_for_level(treecko.growth_rate, 12).unwrap() + 10,
        "the fixture must sit strictly between two thresholds, or a \
         level-derived re-encode would pass"
    );
    assert_eq!(mon.level(), 12);

    let restored = from_save_pokemon(&dex, &to_save_pokemon(&dex, &mon))
        .expect("what we just wrote must decode");
    assert_eq!(restored.experience(), mon.experience());
    assert_eq!(restored.level(), mon.level());
    assert_eq!(restored.stats(), mon.stats());
}

fn torchic_before_learning_peck() -> BattlePokemon {
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

#[test]
fn a_move_learned_by_levelling_up_survives_the_round_trip() {
    let dex = Dex::new();
    let mut mon = torchic_before_learning_peck();

    let torchic = dex.species(mon.species()).unwrap();
    let level_16 = assets::experience_for_level(torchic.growth_rate, 16).unwrap();
    assert!(
        mon.apply_experience(&dex, level_16 - mon.experience())
            .unwrap()
            .is_none(),
        "two of the four slots are free, so Peck is learned without asking"
    );
    assert_eq!(
        mon.moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        vec![SCRATCH, GROWL, PECK],
        "fixture sanity: the level-up must actually have taught Peck \
         (MOVE_PECK, Torchic's level-16 learnset entry) before the round \
         trip can prove anything about it"
    );

    let restored = from_save_pokemon(&dex, &to_save_pokemon(&dex, &mon))
        .expect("what we just wrote must decode");
    assert_eq!(
        restored.moves(),
        mon.moves(),
        "the taught move -- and its freshly rolled PP -- survives the save \
         round trip like any other moveset slot"
    );
    assert_eq!(restored.level(), mon.level());
}

#[test]
fn an_experience_total_past_the_next_threshold_levels_the_decoded_mon_up() {
    let dex = Dex::new();
    let mon = treecko_fixture();
    let mut saved = to_save_pokemon(&dex, &mon);

    let treecko = dex.species(mon.species()).unwrap();
    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    let mut substructures = saved.box_data.substructures().unwrap();
    substructures.growth[EXPECTED_GROWTH_EXPERIENCE].copy_from_slice(&level_13.to_le_bytes());
    saved.box_data.set_substructures(&substructures);

    let restored = from_save_pokemon(&dex, &saved).expect("valid bytes must decode");
    assert_eq!(restored.level(), 13, "the level follows the experience");
    assert_eq!(restored.experience(), level_13);
}

#[test]
fn decoding_an_inconsistent_save_levels_up_without_teaching_moves() {
    let dex = Dex::new();
    let mon = torchic_before_learning_peck();
    let mut saved = to_save_pokemon(&dex, &mon);

    let torchic = dex.species(mon.species()).unwrap();
    let level_16 = assets::experience_for_level(torchic.growth_rate, 16).unwrap();
    let mut substructures = saved.box_data.substructures().unwrap();
    substructures.growth[EXPECTED_GROWTH_EXPERIENCE].copy_from_slice(&level_16.to_le_bytes());
    saved.box_data.set_substructures(&substructures);

    let restored = from_save_pokemon(&dex, &saved).expect("valid bytes must decode");
    assert_eq!(
        restored.level(),
        16,
        "the level still follows the experience"
    );
    assert_eq!(
        restored
            .moves()
            .iter()
            .map(|slot| slot.move_id)
            .collect::<Vec<_>>(),
        vec![SCRATCH, GROWL],
        "but the moveset stays exactly the saved attacks substructure -- \
         no Peck: load is not a level-up"
    );
}

#[test]
fn save_fields_are_encoded_at_their_layout_offsets() {
    let dex = Dex::new();
    let mon = treecko_fixture();
    let saved = to_save_pokemon(&dex, &mon);

    assert_eq!(saved.box_data.ot_id(), mon.original_trainer_id());
    assert_eq!(saved.level, 12);
    assert_eq!(saved.mail, MAIL_NONE);
    assert_eq!(saved.max_hp, u16::try_from(mon.stats().max_hp).unwrap());

    let substructures = saved.box_data.substructures().unwrap();
    assert_eq!(
        u16::from_le_bytes(
            substructures.growth[EXPECTED_GROWTH_SPECIES]
                .try_into()
                .unwrap()
        ),
        mon.species().0
    );
    let treecko = dex.species(mon.species()).unwrap();
    assert_eq!(
        u32::from_le_bytes(
            substructures.growth[EXPECTED_GROWTH_EXPERIENCE]
                .try_into()
                .unwrap()
        ),
        mon.experience(),
        "the growth word holds the mon's own accumulated experience"
    );
    assert_eq!(
        mon.experience(),
        assets::experience_for_level(treecko.growth_rate, 12).unwrap(),
        "a freshly built mon begins at its level's growth threshold"
    );
    assert_eq!(
        substructures.growth[EXPECTED_GROWTH_FRIENDSHIP],
        treecko.base_friendship
    );
    assert_eq!(
        substructures.evs_and_condition,
        [0; engine::save::SUBSTRUCTURE_LEN],
        "no EVs are modelled, so the EV substructure is written all-zero"
    );
    assert_eq!(
        u16::from_le_bytes(
            substructures.attacks[..std::mem::size_of::<u16>()]
                .try_into()
                .unwrap()
        ),
        mon.moves()[0].move_id.0
    );
    assert_eq!(
        substructures.attacks[EXPECTED_ATTACK_PP_OFFSET],
        mon.moves()[0].pp
    );
}

#[test]
fn empty_move_slots_are_dropped_rather_than_decoded_as_moves() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(&dex, TREECKO, 5, Ivs::default(), 0, vec![POUND]).unwrap();
    let saved = to_save_pokemon(&dex, &mon);
    let restored = from_save_pokemon(&dex, &saved).expect("a one-move mon must decode");
    assert_eq!(restored.moves().len(), 1);
}

#[test]
fn a_corrupt_secure_region_is_reported_not_guessed_at() {
    const SECURE_REGION_BYTE: usize = 40;
    const CORRUPTION_MASK: u8 = 0x80;

    let dex = Dex::new();
    let mut saved = to_save_pokemon(&dex, &treecko_fixture());
    let mut bytes = saved.box_data.to_bytes();
    bytes[SECURE_REGION_BYTE] ^= CORRUPTION_MASK;
    saved.box_data = BoxPokemon::from_bytes(bytes);

    assert!(matches!(
        from_save_pokemon(&dex, &saved),
        Err(PartyError::Substructures(_))
    ));
}

#[test]
fn an_empty_party_slot_does_not_decode_into_a_battler() {
    let err = from_save_pokemon(&Dex::new(), &Pokemon::default())
        .expect_err("SPECIES_NONE is not a fightable mon");
    assert!(matches!(err, PartyError::Battler(_)), "{err}");
    assert!(err.to_string().starts_with("saved party member:"));
}

#[test]
fn pp_ups_survive_the_round_trip_byte_for_byte() {
    let dex = Dex::new();
    let bonuses = battle::PpBonuses::from_bits(0b0000_0111);
    let mut mon = treecko_fixture().with_pp_bonuses(&dex, bonuses).unwrap();
    let slot_0_max = mon.max_pp(&dex, 0).unwrap();
    let base_pp = dex.move_data(mon.moves()[0].move_id).unwrap().pp;
    assert!(
        slot_0_max > base_pp,
        "fixture sanity: the upgraded slot must hold more than base PP"
    );
    mon.deduct_pp(0).unwrap();
    mon.deduct_pp(0).unwrap();

    let saved = to_save_pokemon(&dex, &mon);
    assert_eq!(
        saved.box_data.substructures().unwrap().growth[EXPECTED_GROWTH_PP_BONUSES],
        bonuses.bits(),
        "the growth substructure stores ppBonuses itself"
    );

    let restored = from_save_pokemon(&dex, &saved).expect("what we just wrote must decode");
    assert_eq!(restored.pp_bonuses(), bonuses);
    assert_eq!(restored.max_pp(&dex, 0).unwrap(), slot_0_max);
    assert_eq!(
        restored.max_pp(&dex, 1).unwrap(),
        mon.max_pp(&dex, 1).unwrap()
    );
    assert_eq!(
        restored.moves()[0].pp,
        slot_0_max - 2,
        "remaining PP is measured from the PP-Up-adjusted maximum"
    );
    assert_eq!(restored.moves(), mon.moves(), "moves and PP, slot for slot");

    let resaved = to_save_pokemon(&dex, &restored);
    assert_eq!(
        resaved.box_data.substructures().unwrap().growth[EXPECTED_GROWTH_PP_BONUSES],
        bonuses.bits(),
        "re-serialising must emit the identical byte, not zero"
    );
    assert_eq!(resaved, saved, "and the whole 100-byte value is unchanged");
}

#[test]
fn pp_bonus_bits_for_unknown_slots_are_not_stripped() {
    let dex = Dex::new();
    let bonuses = battle::PpBonuses::from_bits(0b1111_1111);
    let mon = BattlePokemon::new(
        &dex,
        TREECKO,
        12,
        Ivs::default(),
        FIXTURE_PERSONALITY,
        vec![TACKLE],
    )
    .unwrap()
    .with_pp_bonuses(&dex, bonuses)
    .unwrap();
    assert!(
        mon.moves().len() < battle::MAX_MON_MOVES,
        "fixture sanity: the fixture must leave at least one slot empty"
    );

    let saved = to_save_pokemon(&dex, &mon);
    let restored = from_save_pokemon(&dex, &saved).unwrap();

    assert_eq!(restored.pp_bonuses().bits(), 0b1111_1111);
    assert_eq!(
        to_save_pokemon(&dex, &restored)
            .box_data
            .substructures()
            .unwrap()
            .growth[EXPECTED_GROWTH_PP_BONUSES],
        0b1111_1111
    );
}

#[test]
fn healing_a_restored_mon_refills_to_the_upgraded_maximum() {
    let dex = Dex::new();
    let bonuses = battle::PpBonuses::from_bits(0b0000_0011);
    let mut mon = treecko_fixture().with_pp_bonuses(&dex, bonuses).unwrap();
    for _ in 0..5 {
        mon.deduct_pp(0).unwrap();
    }
    let saved = to_save_pokemon(&dex, &mon);

    let mut restored = from_save_pokemon(&dex, &saved).unwrap();
    restored.heal(&dex).unwrap();

    let base_pp = dex.move_data(restored.moves()[0].move_id).unwrap().pp;
    assert_eq!(restored.moves()[0].pp, restored.max_pp(&dex, 0).unwrap());
    assert!(
        restored.moves()[0].pp > base_pp,
        "a heal that stopped at base PP would strip the PP Ups again"
    );
}

#[test]
fn a_disagreeing_ability_slot_survives_the_save_round_trip() {
    const CLEAR_BODY: u16 = 29;
    const LIQUID_OOZE: u16 = 64;
    const EVEN_PERSONALITY: u32 = 0x1234_ABCC;

    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        TENTACOOL,
        20,
        Ivs::default(),
        EVEN_PERSONALITY,
        vec![TACKLE],
    )
    .expect("Tentacool is in the dex")
    .with_ability_slot(1);
    assert_eq!(
        mon.ability().0,
        LIQUID_OOZE,
        "fixture sanity: the override, not personality parity, decides"
    );
    assert_ne!(
        mon.ability().0,
        CLEAR_BODY,
        "fixture sanity: personality parity alone would have picked this"
    );

    let restored = from_save_pokemon(&dex, &to_save_pokemon(&dex, &mon))
        .expect("what we just wrote must decode");
    assert_eq!(restored.ability_slot(), 1);
    assert_eq!(
        restored.ability().0,
        LIQUID_OOZE,
        "the disagreeing slot survives the round trip instead of being \
         re-derived from the (even) personality"
    );
}

const RETAINED_HELD_ITEM: u16 = 200;
const RETAINED_STATUS: u32 = 1 << 6;
const RETAINED_MAIL: u8 = 2;
const RETAINED_FRIENDSHIP: u8 = 213;
const RETAINED_EVS_AND_CONDITION: [u8; engine::save::SUBSTRUCTURE_LEN] =
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

fn retained_evs() -> battle::Evs {
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

#[test]
fn the_retained_evs_stay_inside_a_total_an_upstream_save_can_hold() {
    let evs = retained_evs();
    let total = u16::from(evs.hp)
        + u16::from(evs.attack)
        + u16::from(evs.defense)
        + u16::from(evs.speed)
        + u16::from(evs.sp_attack)
        + u16::from(evs.sp_defense);
    assert!(
        total <= MAX_TOTAL_EVS,
        "fixture sanity: {total} EVs is a spread no upstream save can hold"
    );
}

fn stored_record_with_retained_fields() -> Pokemon {
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

#[test]
fn re_saving_a_loaded_mon_keeps_every_field_the_battle_model_does_not_carry() {
    let dex = Dex::new();
    let stored = stored_record_with_retained_fields();
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    lead.apply_damage(9);
    lead.deduct_pp(0).unwrap();

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    let before = stored.box_data.substructures().unwrap();
    let after = merged
        .box_data
        .substructures()
        .expect("the merge must leave the checksum valid");
    assert_eq!(
        &after.growth[EXPECTED_GROWTH_HELD_ITEM], &before.growth[EXPECTED_GROWTH_HELD_ITEM],
        "heldItem is the save's"
    );
    assert_eq!(
        after.growth[EXPECTED_GROWTH_FRIENDSHIP], RETAINED_FRIENDSHIP,
        "accumulated friendship is the save's, not the species' base value"
    );
    assert_ne!(
        after.growth[EXPECTED_GROWTH_FRIENDSHIP],
        dex.species(lead.species()).unwrap().base_friendship,
        "fixture sanity: a re-derived friendship would differ from this"
    );
    assert_eq!(
        after.evs_and_condition, RETAINED_EVS_AND_CONDITION,
        "EVs and contest condition are the save's, whole"
    );
    assert_eq!(
        &after.misc[MISC_ENCOUNTER_DATA], &before.misc[MISC_ENCOUNTER_DATA],
        "pokérus and the met/ball/OT-gender bytes are the save's"
    );
    assert_eq!(
        &after.misc[MISC_RIBBONS], &before.misc[MISC_RIBBONS],
        "the ribbon word is the save's"
    );
    assert_eq!(
        merged.status, RETAINED_STATUS,
        "non-volatile status is the save's"
    );
    assert_eq!(merged.mail, RETAINED_MAIL, "the mail slot is the save's");
    assert_eq!(
        merged.box_data.to_bytes()[BOX_RETAINED_HEADER],
        stored.box_data.to_bytes()[BOX_RETAINED_HEADER],
        "nickname, language, OT name and markings are the save's"
    );
    assert_eq!(merged.box_data.personality(), stored.box_data.personality());
    assert_eq!(merged.box_data.ot_id(), stored.box_data.ot_id());

    assert_eq!(
        [
            merged.max_hp,
            merged.attack,
            merged.defense,
            merged.speed,
            merged.special_attack,
            merged.special_defense,
        ],
        [
            stored.max_hp,
            stored.attack,
            stored.defense,
            stored.speed,
            stored.special_attack,
            stored.special_defense,
        ],
        "the EV-trained stat block is the save's, not the 0-EV block this \
         port recomputes"
    );
    assert_ne!(
        merged.max_hp,
        u16::try_from(lead.stats().max_hp).unwrap(),
        "fixture sanity: recomputing the block really would have moved it"
    );
    assert_eq!(
        merged.hp,
        u16::try_from(lead.current_hp()).unwrap(),
        "current HP is battle state, so it is the battler's either way"
    );
    assert!(
        merged.hp <= merged.max_hp,
        "and cannot contradict a retained maximum: the model's own maximum \
         is the 0-EV one, and EVs only add"
    );
}

#[test]
fn sub_level_experience_does_not_flatten_the_retained_stat_block() {
    let dex = Dex::new();
    let stored = stored_record_with_retained_fields();
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");

    let level_13 =
        assets::experience_for_level(dex.species(lead.species()).unwrap().growth_rate, 13).unwrap();
    let _ = lead
        .apply_experience(&dex, level_13 - 1 - lead.experience())
        .expect("an award short of the threshold is in range");
    assert_eq!(lead.level(), 12, "fixture sanity: no level was crossed");
    assert_ne!(
        lead.experience(),
        u32::from_le_bytes(
            stored.box_data.substructures().unwrap().growth[EXPECTED_GROWTH_EXPERIENCE]
                .try_into()
                .unwrap()
        ),
        "fixture sanity: the experience word really moved"
    );

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );
    let after = merged.box_data.substructures().unwrap();
    assert_eq!(
        u32::from_le_bytes(after.growth[EXPECTED_GROWTH_EXPERIENCE].try_into().unwrap()),
        lead.experience(),
        "the awarded experience is saved"
    );
    assert_eq!(
        [merged.max_hp, merged.attack, merged.defense],
        [stored.max_hp, stored.attack, stored.defense],
        "and the EV-trained block is retained: sub-level experience is not \
         an input to the stat formula"
    );
}

#[test]
fn re_saving_an_untouched_lead_writes_the_record_back_byte_for_byte() {
    let dex = Dex::new();
    let stored = stored_record_with_retained_fields();
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert_ne!(
        stored.max_hp,
        u16::try_from(lead.stats().max_hp).unwrap(),
        "fixture sanity: the stored block carries an EV contribution the \
         model cannot rebuild, so a re-derived block would differ"
    );

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    let (merged_bytes, stored_bytes) = (merged.to_bytes(), stored.to_bytes());
    let moved: Vec<usize> = (0..merged_bytes.len())
        .filter(|index| merged_bytes[*index] != stored_bytes[*index])
        .collect();
    assert_eq!(
        moved,
        Vec::<usize>::new(),
        "an untouched lead must re-save as the same 100 bytes"
    );

    let reloaded = from_save_pokemon(&dex, &merged).expect("the re-saved record must decode");
    let again = merge_into_save_pokemon(
        &dex,
        &reloaded,
        &merged,
        &mut hp_hidden_by_load(&dex, &merged, &reloaded),
    );
    assert_eq!(again.to_bytes(), stored.to_bytes());
}

#[test]
fn re_saving_a_loaded_mon_overlays_what_the_session_changed() {
    let dex = Dex::new();
    let stored = stored_record_with_retained_fields();
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");

    let treecko = dex.species(lead.species()).unwrap();
    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    let _ = lead
        .apply_experience(&dex, level_13 - lead.experience())
        .expect("a level-13 award is in range");
    assert_eq!(lead.level(), 13, "fixture sanity: the mon levelled up");
    lead.apply_damage(11);
    lead.deduct_pp(1).unwrap();
    lead.deduct_pp(1).unwrap();

    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);
    let after = merged.box_data.substructures().unwrap();

    assert_eq!(
        u32::from_le_bytes(after.growth[EXPECTED_GROWTH_EXPERIENCE].try_into().unwrap()),
        lead.experience(),
        "the growth word carries the experience the battle awarded"
    );
    assert_eq!(merged.level, 13, "and the level that came with it");
    let recompute = |level: u8, evs: battle::Evs| {
        battle::compute_stats_with_evs(
            lead.species(),
            treecko,
            level,
            lead.nature(),
            lead.ivs(),
            evs,
        )
    };
    let old_floor = recompute(stored.level, battle::Evs::default()).max_hp;
    let gap_old = u32::from(stored.max_hp) - old_floor;
    let gap_new = u32::from(merged.max_hp) - lead.stats().max_hp;
    let rebased_offset =
        u16::try_from(gap_new - gap_old).expect("the fixture's EVs keep this well under u16::MAX");
    assert_eq!(
        merged.hp,
        u16::try_from(lead.current_hp()).unwrap() + rebased_offset,
        "the level-up moved the EV-aware gap, so the saved HP carries that \
         movement even though nothing was clamped at load"
    );
    assert_ne!(merged.hp, stored.hp, "fixture sanity: the damage is real");

    let expected = recompute(lead.level(), retained_evs());
    assert_eq!(
        merged.max_hp,
        u16::try_from(expected.max_hp).unwrap(),
        "a level-up moved what the cached block is a function of, so the \
         block is recomputed from the record's retained EV bytes"
    );
    assert_ne!(
        merged.max_hp,
        u16::try_from(lead.stats().max_hp).unwrap(),
        "fixture sanity: the retained hp EV (252) really does raise the \
         saved block above the battler's own 0-EV cache"
    );
    assert_ne!(
        merged.max_hp, stored.max_hp,
        "fixture sanity: the retained block would have been the level-12 one"
    );
    assert_eq!(
        [
            merged.attack,
            merged.defense,
            merged.speed,
            merged.special_attack,
            merged.special_defense,
        ],
        [
            u16::try_from(expected.attack).unwrap(),
            u16::try_from(expected.defense).unwrap(),
            u16::try_from(expected.speed).unwrap(),
            u16::try_from(expected.sp_attack).unwrap(),
            u16::try_from(expected.sp_defense).unwrap(),
        ],
        "the whole block, not just the maximum HP"
    );
    assert_eq!(
        after.attacks,
        super::encode_attacks(&lead),
        "moves and per-slot PP, slot for slot"
    );
    assert_ne!(
        &after.attacks[EXPECTED_ATTACK_PP_OFFSET..],
        &stored.box_data.substructures().unwrap().attacks[EXPECTED_ATTACK_PP_OFFSET..],
        "fixture sanity: the spent PP is real"
    );

    let mut expected_reloaded = lead.clone();
    expected_reloaded.heal_hp(u32::from(rebased_offset));
    let reloaded = from_save_pokemon(&dex, &merged).expect("the merge must decode again");
    assert_eq!(
        reloaded, expected_reloaded,
        "and back out as the same battler, plus the rebased offset's point"
    );
}

#[test]
fn a_slot_holding_a_different_pokemon_is_rebuilt_rather_than_overlaid() {
    const DIFFERENT_PERSONALITY_BITS: u32 = 0x0F0F_0F0F;
    const DIFFERENT_ORIGINAL_TRAINER_ID: u32 = 0x0BAD_0BAD;

    let dex = Dex::new();
    let stored = stored_record_with_retained_fields();
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");

    let swapped_personality = BattlePokemon::new(
        &dex,
        lead.species(),
        lead.level(),
        lead.ivs(),
        lead.personality() ^ DIFFERENT_PERSONALITY_BITS,
        lead.moves().iter().map(|slot| slot.move_id).collect(),
    )
    .unwrap()
    .with_original_trainer_id(lead.original_trainer_id());
    assert_eq!(
        merge_into_save_pokemon(&dex, &swapped_personality, &stored, &mut 0),
        to_save_pokemon(&dex, &swapped_personality),
        "a different personality is a different mon"
    );

    let traded_away = lead
        .clone()
        .with_original_trainer_id(DIFFERENT_ORIGINAL_TRAINER_ID);
    assert_eq!(
        merge_into_save_pokemon(&dex, &traded_away, &stored, &mut 0),
        to_save_pokemon(&dex, &traded_away),
        "so is a different original trainer -- it is half the XOR key"
    );
}

#[test]
fn an_empty_slot_is_built_from_scratch() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(&dex, TREECKO, 5, Ivs::default(), 0, vec![POUND]).unwrap();
    let empty = Pokemon::default();
    assert_eq!(
        empty.box_data.personality(),
        mon.personality(),
        "fixture sanity: the identity gate alone would let this through, so \
         the species check is what decides"
    );
    let built = merge_into_save_pokemon(&dex, &mon, &empty, &mut 0);
    assert_eq!(built, to_save_pokemon(&dex, &mon));
    assert_eq!(built.mail, MAIL_NONE, "an empty slot has no mail to keep");
}

#[test]
fn the_merge_rewrites_the_iv_word_around_the_egg_bit() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    let mut substructures = stored.box_data.substructures().unwrap();
    let iv_word = u32::from_le_bytes(
        substructures.misc[EXPECTED_MISC_IV_WORD]
            .try_into()
            .unwrap(),
    );
    substructures.misc[EXPECTED_MISC_IV_WORD]
        .copy_from_slice(&(iv_word | EXPECTED_IS_EGG_BIT).to_le_bytes());
    stored.box_data.set_substructures(&substructures);

    let lead = from_save_pokemon(&dex, &stored)
        .expect("the fixture must decode")
        .with_ability_slot(1);
    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    let merged_word = u32::from_le_bytes(
        merged.box_data.substructures().unwrap().misc[EXPECTED_MISC_IV_WORD]
            .try_into()
            .unwrap(),
    );
    assert_eq!(
        merged_word & EXPECTED_IS_EGG_BIT,
        EXPECTED_IS_EGG_BIT,
        "the egg bit this port does not model stays exactly as it was"
    );
    assert_eq!(merged_word >> 31, 1, "abilityNum is the battler's");
    assert_eq!(unpack_ivs(merged_word), lead.ivs());
}

#[test]
fn continue_then_save_keeps_a_full_health_ev_trained_lead_at_full() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.hp = stored.max_hp;
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > lead.stats().max_hp,
        "fixture sanity: the stored full must exceed the model's maximum, \
         or the load clamp never fires"
    );

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    assert_eq!(merged.to_bytes(), stored.to_bytes());
}

#[test]
fn continue_then_save_keeps_an_over_model_max_current_hp() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.hp = stored.max_hp - 3;
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > lead.stats().max_hp,
        "fixture sanity: the stored value must sit above the model's \
         maximum, or the load clamp never fires"
    );

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    assert_eq!(merged.to_bytes(), stored.to_bytes());
}

#[test]
fn battle_damage_on_a_clamped_load_subtracts_from_the_stored_hp() {
    const DAMAGE: u32 = 10;
    const HIDDEN: u16 = 5;

    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    assert!(
        stored.hp < stored.max_hp,
        "fixture sanity: the stored hp must sit below the retained maximum"
    );
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    lead.apply_damage(DAMAGE);

    let merged = merge_into_save_pokemon(
        &dex,
        &lead,
        &stored,
        &mut hp_hidden_by_load(&dex, &stored, &lead),
    );

    assert_eq!(merged.hp, stored.hp - u16::try_from(DAMAGE).unwrap());
}

#[test]
fn a_stat_block_recompute_still_translates_the_load_clamp_offset() {
    const HIDDEN: u16 = 5;
    const DAMAGE: u32 = 10;

    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    assert_eq!(
        offset,
        i32::from(HIDDEN),
        "fixture sanity: the load clamp must fire"
    );

    lead.apply_damage(DAMAGE);
    let treecko = dex.species(lead.species()).unwrap();
    let next_level = assets::experience_for_level(treecko.growth_rate, lead.level() + 1).unwrap();
    lead.apply_experience(&dex, next_level - lead.experience())
        .expect("no move-learn prompt is pending");
    assert_ne!(
        lead.level(),
        stored.level,
        "fixture sanity: the level must move"
    );

    let old_floor = battle::compute_stats_with_evs(
        lead.species(),
        treecko,
        stored.level,
        lead.nature(),
        lead.ivs(),
        battle::Evs::default(),
    )
    .max_hp;
    let gap_old = u32::from(stored.max_hp) - old_floor;

    let first = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);
    let gap_new = u32::from(first.max_hp) - lead.stats().max_hp;
    assert_ne!(
        gap_new, gap_old,
        "fixture sanity: the retained bonus must leave the gap a different \
         size at the new level, or an unrebased offset would pass unnoticed"
    );
    let expected_offset = i64::from(HIDDEN) + i64::from(gap_new) - i64::from(gap_old);
    assert_eq!(
        i64::from(offset),
        expected_offset,
        "the recompute rebases the offset by how the gap moved, rather than \
         zeroing it (which would drop the session's own hidden points) or \
         carrying it unrebased (which mis-sizes it once the gap is not the \
         same at the old level as at the new one)"
    );
    let live = i64::from(u16::try_from(lead.current_hp()).unwrap());
    assert_eq!(
        i64::from(first.hp),
        (live + i64::from(offset)).min(i64::from(first.max_hp)),
        "current HP crosses the same load clamp the retained branch \
         applies, now against the block just recomputed for the new level"
    );

    let second = merge_into_save_pokemon(&dex, &lead, &first, &mut offset);
    assert_eq!(
        second.to_bytes(),
        first.to_bytes(),
        "an immediate re-save, now on the retained branch, must file the \
         same bytes the recompute branch just wrote"
    );
}

#[test]
fn a_fainted_lead_stays_fainted_through_a_stat_block_recompute() {
    const HIDDEN: u16 = 5;

    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    assert_eq!(
        offset,
        i32::from(HIDDEN),
        "fixture sanity: the load clamp must fire"
    );

    let treecko = dex.species(lead.species()).unwrap();
    let next_level = assets::experience_for_level(treecko.growth_rate, lead.level() + 1).unwrap();
    lead.apply_experience(&dex, next_level - lead.experience())
        .expect("no move-learn prompt is pending");
    assert_ne!(
        lead.level(),
        stored.level,
        "fixture sanity: the level must move, so the merge takes the \
         recompute branch"
    );

    lead.apply_damage(u32::MAX);
    assert!(lead.is_fainted(), "fixture sanity: the lead must faint");

    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);

    assert_eq!(
        merged.hp, 0,
        "a fainted lead saves 0 even under a freshly recomputed block with \
         real hidden points behind it -- the load-clamp offset must never \
         resurrect it"
    );
}

#[test]
fn continue_then_save_keeps_a_full_health_ev_trained_lead_at_full_after_levelling_up() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.level = 13;
    let treecko = dex.species(TREECKO).unwrap();
    let retained_evs = retained_evs();
    let stored_lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let old_ev_aware = battle::compute_stats_with_evs(
        stored_lead.species(),
        treecko,
        stored_lead.level(),
        stored_lead.nature(),
        stored_lead.ivs(),
        retained_evs,
    );
    stored.max_hp = u16::try_from(old_ev_aware.max_hp).unwrap();
    stored.hp = stored.max_hp;

    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > lead.stats().max_hp,
        "fixture sanity: the stored full must exceed the model's 0-EV \
         maximum, or the load clamp never fires"
    );
    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    assert_ne!(offset, 0, "fixture sanity: the load clamp must fire");

    let next_level = assets::experience_for_level(treecko.growth_rate, lead.level() + 1).unwrap();
    lead.apply_experience(&dex, next_level - lead.experience())
        .expect("no move-learn prompt is pending");
    assert_ne!(
        lead.level(),
        stored.level,
        "fixture sanity: the level must move"
    );

    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);

    let new_ev_aware = battle::compute_stats_with_evs(
        lead.species(),
        treecko,
        lead.level(),
        lead.nature(),
        lead.ivs(),
        retained_evs,
    );
    assert_eq!(
        merged.max_hp,
        u16::try_from(new_ev_aware.max_hp).unwrap(),
        "fixture sanity: the recomputed block is the level-14 EV-aware one"
    );
    assert_eq!(
        merged.hp, merged.max_hp,
        "a full-health lead that levels up must still be saved at full \
         under the newly recomputed maximum, not at the model's own \
         weaker 0-EV current_hp"
    );
}

#[test]
fn an_inconsistent_level_byte_still_saves_a_full_health_ev_trained_lead_at_full() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.level = 13;
    let treecko = dex.species(TREECKO).unwrap();
    let retained_evs = retained_evs();
    let fixture = treecko_fixture();
    let ev_aware_at_13 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        13,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    stored.max_hp = u16::try_from(ev_aware_at_13.max_hp).unwrap();
    stored.hp = stored.max_hp;

    let level_14 = assets::experience_for_level(treecko.growth_rate, 14).unwrap();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.growth[EXPECTED_GROWTH_EXPERIENCE].copy_from_slice(&level_14.to_le_bytes());
    stored.box_data.set_substructures(&substructures);

    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert_eq!(lead.level(), 14, "fixture sanity: the level reconciled up");
    assert_ne!(
        lead.level(),
        stored.level,
        "fixture sanity: the stored byte still disagrees with the level \
         the mon actually holds"
    );

    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);

    let ev_aware_at_14 = battle::compute_stats_with_evs(
        lead.species(),
        treecko,
        lead.level(),
        lead.nature(),
        lead.ivs(),
        retained_evs,
    );
    assert_eq!(
        merged.max_hp,
        u16::try_from(ev_aware_at_14.max_hp).unwrap(),
        "fixture sanity: the merge recomputed the level-14 EV-aware block"
    );
    assert_eq!(
        merged.hp, merged.max_hp,
        "a record whose level byte contradicts its experience word still \
         saves a full-health lead at full, not damaged by an offset measured against the \
         reconciled level instead of the record's own stored byte"
    );
}

#[test]
fn a_shrinking_ev_gap_uses_the_ev_aware_level_up_delta() {
    let dex = Dex::new();
    let treecko = dex.species(TREECKO).unwrap();
    let fixture = treecko_fixture();
    let retained_evs = battle::Evs {
        hp: 12,
        ..retained_evs()
    };

    let mut stored = stored_record_with_retained_fields();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[HP_EV_INDEX] = retained_evs.hp;
    stored.box_data.set_substructures(&substructures);

    let ev_aware_at_12 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        12,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    let ev_aware_at_13 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        13,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    let floor_at_12 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        12,
        fixture.nature(),
        fixture.ivs(),
        battle::Evs::default(),
    );
    let floor_at_13 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        13,
        fixture.nature(),
        fixture.ivs(),
        battle::Evs::default(),
    );
    assert_eq!(
        ev_aware_at_12.max_hp - floor_at_12.max_hp,
        1,
        "fixture sanity: the level-12 gap is one point"
    );
    assert_eq!(
        ev_aware_at_13.max_hp - floor_at_13.max_hp,
        0,
        "fixture sanity: the level-13 gap is none -- the gap shrinks, which \
         is the whole point of this fixture"
    );

    stored.level = 12;
    stored.max_hp = u16::try_from(ev_aware_at_12.max_hp).unwrap();
    stored.hp = 1;

    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    assert_eq!(
        offset, 0,
        "fixture sanity: a stored 1 HP is far below the 0-EV floor, so the \
         load clamp hides nothing"
    );

    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    lead.apply_experience(&dex, level_13 - lead.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(lead.level(), 13, "fixture sanity: the level must move");
    assert_eq!(
        lead.current_hp(),
        1 + (floor_at_13.max_hp - floor_at_12.max_hp),
        "fixture sanity: the live battler gained the 0-EV delta, which is \
         the wider one"
    );

    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);

    assert_eq!(
        merged.max_hp,
        u16::try_from(ev_aware_at_13.max_hp).unwrap(),
        "fixture sanity: the merge recomputed the level-13 EV-aware block"
    );
    assert_eq!(
        u32::from(merged.hp),
        1 + (ev_aware_at_13.max_hp - ev_aware_at_12.max_hp),
        "a level-up saves the EV-aware max-HP delta onto the \
         stored current HP, even where that delta is narrower than the \
         model's 0-EV one -- the rebase has to subtract the point the \
         shrinking gap took back"
    );
}

#[test]
fn a_live_lead_is_never_saved_as_fainted_when_the_ev_gap_shrinks() {
    let dex = Dex::new();
    let treecko = dex.species(TREECKO).unwrap();
    let fixture = treecko_fixture();
    let retained_evs = battle::Evs {
        hp: 12,
        ..retained_evs()
    };

    let mut stored = stored_record_with_retained_fields();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[HP_EV_INDEX] = retained_evs.hp;
    stored.box_data.set_substructures(&substructures);
    let ev_aware_at_12 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        12,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    stored.level = 12;
    stored.max_hp = u16::try_from(ev_aware_at_12.max_hp).unwrap();
    stored.hp = 1;

    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let mut offset = hp_hidden_by_load(&dex, &stored, &lead);
    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    lead.apply_experience(&dex, level_13 - lead.experience())
        .expect("no move-learn prompt is pending");
    lead.apply_damage(lead.current_hp() - 1);
    assert_eq!(lead.current_hp(), 1, "fixture sanity: one point left");
    assert!(!lead.is_fainted(), "fixture sanity: and still standing");

    let merged = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);

    assert_eq!(offset, -1, "fixture sanity: the rebase went negative");
    assert_eq!(
        merged.hp, 1,
        "a live battler saves at least 1 -- a 0 here would come back from \
         the next load as a fainted lead the session never fainted"
    );
}
