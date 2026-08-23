//! Unit tests for the [`super`] party encoder (I-6, issue #232).

use super::{
    from_save_pokemon, hp_hidden_by_load, merge_into_save_pokemon, pack_ivs, to_save_pokemon,
    unpack_ivs, PartyError, MAIL_NONE,
};
use battle::{BattlePokemon, Dex, Ivs};
use engine::save::{BoxPokemon, Pokemon};

/// `SPECIES_TREECKO` at the provisional starter's own level, with
/// deliberately *distinct* IVs (so a packer that wrote one field twice, or
/// in the wrong order, fails) and a personality that is not zero (so the
/// substructure order and XOR key are both non-trivial).
fn a_battler() -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        assets::SpeciesId(277),
        12,
        Ivs {
            hp: 1,
            attack: 2,
            defense: 3,
            speed: 4,
            sp_attack: 5,
            sp_defense: 6,
        },
        0x1234_ABCD,
        battle::initial_moveset(assets::SpeciesId(277), 12),
    )
    .expect("Treecko with its level-12 learnset is in the dex")
    .with_original_trainer_id(0x89AB_CDEF)
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
    assert_eq!(word & 0x1F, 1, "hpIV occupies bits 0..5");
    assert_eq!((word >> 5) & 0x1F, 2);
    assert_eq!((word >> 10) & 0x1F, 3);
    assert_eq!((word >> 15) & 0x1F, 4);
    assert_eq!((word >> 20) & 0x1F, 5);
    assert_eq!((word >> 25) & 0x1F, 6);
    assert_eq!(word >> 30, 0, "isEgg and abilityNum must stay clear");
    assert_eq!(unpack_ivs(word), ivs);
}

/// The `isEgg`/`abilityNum` bits share the IV word; decoding must mask them
/// off rather than letting them bleed into Sp. Defense.
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
    assert_eq!(unpack_ivs(pack_ivs(ivs) | 0xC000_0000), ivs);
}

/// The acceptance property this module exists for: a battler saved and
/// reloaded is the same battler, not one re-derived from a species default.
#[test]
fn a_battler_round_trips_through_the_save_layout() {
    let dex = Dex::new();
    let mut mon = a_battler();
    // Play with it first, so the round trip has real *state* to carry --
    // a full-HP, full-PP mon would round-trip through a serializer that
    // dropped both.
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

/// Sub-level experience earned in battle is state, not a function of the
/// level (`BattlePokemon::apply_experience`, issue #237) -- a round trip
/// that re-derived the growth word from the level would silently reset
/// every battle's progress on save/reload.
#[test]
fn sub_level_experience_survives_the_round_trip() {
    let dex = Dex::new();
    let mut mon = a_battler();
    // Not enough to reach level 13, so the only observable difference is
    // the experience total itself -- and no level crossed means no learnset
    // walk, hence nothing to ask the player about.
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

/// A move learned mid-battle by crossing a learnset level
/// (`battle::BattlePokemon::apply_experience`'s learnset walk, issue #252)
/// is moveset *state*, exactly like the hand-picked moves
/// [`a_battler_round_trips_through_the_save_layout`] already covers -- it
/// must survive the save round trip in its own right, not just as a side
/// effect of that more general assertion.
#[test]
fn a_move_learned_by_levelling_up_survives_the_round_trip() {
    let dex = Dex::new();
    let mut mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(280), // SPECIES_TORCHIC
        15,
        Ivs {
            hp: 1,
            attack: 2,
            defense: 3,
            speed: 4,
            sp_attack: 5,
            sp_defense: 6,
        },
        0x1234_ABCD,
        vec![assets::MoveId(10), assets::MoveId(45)], // Scratch, Growl
    )
    .expect("Torchic with a two-move starting set is in the dex")
    .with_original_trainer_id(0x89AB_CDEF);

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
        vec![assets::MoveId(10), assets::MoveId(45), assets::MoveId(64)], // + Peck
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

/// A growth word at or past the next level's threshold reconciles the way
/// upstream's own `GetLevelFromMonExp` does: the level rises to match the
/// experience, rather than trusting a level/experience pair upstream could
/// never store.
#[test]
fn an_experience_total_past_the_next_threshold_levels_the_decoded_mon_up() {
    let dex = Dex::new();
    let mon = a_battler();
    let mut saved = to_save_pokemon(&dex, &mon);

    let treecko = dex.species(mon.species()).unwrap();
    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    let mut substructures = saved.box_data.substructures().unwrap();
    substructures.growth[4..8].copy_from_slice(&level_13.to_le_bytes());
    saved.box_data.set_substructures(&substructures);

    let restored = from_save_pokemon(&dex, &saved).expect("valid bytes must decode");
    assert_eq!(restored.level(), 13, "the level follows the experience");
    assert_eq!(restored.experience(), level_13);
}

/// The level reconciliation above derives the level *only*. Upstream's
/// load path (`CalculateMonStats` -> `GetLevelFromMonExp`,
/// `pokeemerald/src/pokemon.c`) copies the attacks substructure verbatim
/// and never runs `MonTryLearningNewMove`, so decoding an inconsistent
/// save must not teach the crossed levels' learnset moves -- merely
/// loading a save may never mutate its own authoritative moveset
/// (`BattlePokemon::reconcile_saved_experience` vs the in-battle
/// `apply_experience` walk, issue #252).
#[test]
fn decoding_an_inconsistent_save_levels_up_without_teaching_moves() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(280), // SPECIES_TORCHIC
        15,
        Ivs {
            hp: 1,
            attack: 2,
            defense: 3,
            speed: 4,
            sp_attack: 5,
            sp_defense: 6,
        },
        0x1234_ABCD,
        vec![assets::MoveId(10), assets::MoveId(45)], // Scratch, Growl
    )
    .expect("Torchic with a two-move starting set is in the dex")
    .with_original_trainer_id(0x89AB_CDEF);
    let mut saved = to_save_pokemon(&dex, &mon);

    // Level 16 is Torchic's Peck (`MOVE_PECK`, id 64) learnset entry --
    // the in-battle award in `a_move_learned_by_levelling_up_survives_the_
    // round_trip` proves crossing it teaches; this decode must not.
    let torchic = dex.species(mon.species()).unwrap();
    let level_16 = assets::experience_for_level(torchic.growth_rate, 16).unwrap();
    let mut substructures = saved.box_data.substructures().unwrap();
    substructures.growth[4..8].copy_from_slice(&level_16.to_le_bytes());
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
        vec![assets::MoveId(10), assets::MoveId(45)],
        "but the moveset stays exactly the saved attacks substructure -- \
         no Peck: load is not a level-up"
    );
}

/// The saved bytes are upstream's, not an invented shape: the growth
/// substructure holds the species and the accumulated experience, the party
/// block holds `MAIL_NONE`, and the box header carries the OT id the key is
/// built from.
#[test]
fn the_saved_bytes_sit_at_upstream_offsets() {
    let dex = Dex::new();
    let mon = a_battler();
    let saved = to_save_pokemon(&dex, &mon);

    assert_eq!(saved.box_data.ot_id(), mon.original_trainer_id());
    assert_eq!(saved.level, 12);
    assert_eq!(saved.mail, MAIL_NONE);
    assert_eq!(saved.max_hp, u16::try_from(mon.stats().max_hp).unwrap());

    let substructures = saved.box_data.substructures().unwrap();
    assert_eq!(
        u16::from_le_bytes([substructures.growth[0], substructures.growth[1]]),
        mon.species().0
    );
    let treecko = dex.species(mon.species()).unwrap();
    assert_eq!(
        u32::from_le_bytes(substructures.growth[4..8].try_into().unwrap()),
        mon.experience(),
        "the growth word holds the mon's own accumulated experience"
    );
    assert_eq!(
        mon.experience(),
        assets::experience_for_level(treecko.growth_rate, 12).unwrap(),
        "which, for a freshly built mon, is the growth-curve seed \
         CreateBoxMon writes"
    );
    assert_eq!(substructures.growth[9], treecko.base_friendship);
    assert_eq!(
        substructures.evs_and_condition,
        [0; engine::save::SUBSTRUCTURE_LEN],
        "no EVs are modelled, so the EV substructure is written all-zero"
    );
    assert_eq!(
        u16::from_le_bytes([substructures.attacks[0], substructures.attacks[1]]),
        mon.moves()[0].move_id.0
    );
    assert_eq!(substructures.attacks[8], mon.moves()[0].pp);
}

/// A trailing `MOVE_NONE` slot is an *empty* slot upstream, not a known
/// move -- a decoder that carried it through would build a battler
/// `BattlePokemon::new` refuses outright.
#[test]
fn empty_move_slots_are_dropped_rather_than_decoded_as_moves() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(277),
        5,
        Ivs::default(),
        0,
        vec![assets::MoveId(1)],
    )
    .unwrap();
    let saved = to_save_pokemon(&dex, &mon);
    let restored = from_save_pokemon(&dex, &saved).expect("a one-move mon must decode");
    assert_eq!(restored.moves().len(), 1);
}

/// A checksum-valid sector can still hold a mon whose *decrypted* region is
/// garbage. The decode must say so rather than hand the battle engine a
/// scrambled battler.
#[test]
fn a_corrupt_secure_region_is_reported_not_guessed_at() {
    let dex = Dex::new();
    let mut saved = to_save_pokemon(&dex, &a_battler());
    let mut bytes = saved.box_data.to_bytes();
    bytes[40] ^= 0x80;
    saved.box_data = BoxPokemon::from_bytes(bytes);

    assert!(matches!(
        from_save_pokemon(&dex, &saved),
        Err(PartyError::Substructures(_))
    ));
}

/// An all-zero party slot (`SPECIES_NONE`, no moves) is what an *empty*
/// party member is. Decoding one must fail closed rather than produce a
/// zero-stat battler that could then be sent into a fight.
#[test]
fn an_empty_party_slot_does_not_decode_into_a_battler() {
    let err = from_save_pokemon(&Dex::new(), &Pokemon::default())
        .expect_err("SPECIES_NONE is not a fightable mon");
    assert!(matches!(err, PartyError::Battler(_)), "{err}");
    assert!(err.to_string().starts_with("saved party member:"));
}

/// The save-data defect issue #304 fixes: `ppBonuses` used to be written as
/// `0`, so loading and saving a file silently spent every PP Up the player
/// had ever used. The byte now round-trips exactly, and the capacity it
/// encodes is real on the way back in.
#[test]
fn pp_ups_survive_the_round_trip_byte_for_byte() {
    let dex = Dex::new();
    // Three PP Ups on slot 0, one on slot 1 -- distinct per slot, so a
    // packer that wrote one field twice or shifted it wrong fails.
    let bonuses = battle::PpBonuses::from_bits(0b0000_0111);
    let mut mon = a_battler().with_pp_bonuses(&dex, bonuses).unwrap();
    let slot_0_max = mon.max_pp(&dex, 0).unwrap();
    let base_pp = dex.move_data(mon.moves()[0].move_id).unwrap().pp;
    assert!(
        slot_0_max > base_pp,
        "fixture sanity: the upgraded slot must hold more than base PP"
    );
    // Spend a few, so the decode has to place remaining PP against the
    // *adjusted* maximum rather than against base PP.
    mon.deduct_pp(0).unwrap();
    mon.deduct_pp(0).unwrap();

    let saved = to_save_pokemon(&dex, &mon);
    assert_eq!(
        saved.box_data.substructures().unwrap().growth[8],
        bonuses.bits(),
        "the growth substructure's /*0x08*/ byte is ppBonuses itself"
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
        resaved.box_data.substructures().unwrap().growth[8],
        bonuses.bits(),
        "re-serialising must emit the identical byte, not zero"
    );
    assert_eq!(resaved, saved, "and the whole 100-byte value is unchanged");
}

/// A byte upstream itself could never write -- PP Ups recorded against a
/// slot this mon has no move for -- is still carried through untouched.
/// Save data is not quietly rewritten because this port cannot explain it.
#[test]
fn pp_bonus_bits_for_unknown_slots_are_not_stripped() {
    let dex = Dex::new();
    let bonuses = battle::PpBonuses::from_bits(0b1111_1111);
    // A deliberately one-move mon, so three of the byte's four fields
    // belong to slots that hold no move at all.
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(277),
        12,
        Ivs::default(),
        0x1234_ABCD,
        vec![assets::MoveId(33)],
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
            .growth[8],
        0b1111_1111
    );
}

/// The white-out heal restores a saved mon to its *upgraded* maximum, not
/// to the move's base PP (`HealPlayerParty`'s own `CalculatePPWithBonus`).
#[test]
fn healing_a_restored_mon_refills_to_the_upgraded_maximum() {
    let dex = Dex::new();
    let bonuses = battle::PpBonuses::from_bits(0b0000_0011);
    let mut mon = a_battler().with_pp_bonuses(&dex, bonuses).unwrap();
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

/// The ability slot round-trips through the save's `abilityNum` bit
/// (`PokemonSubstruct3`'s bit 31, the misc IV word's top bit) rather than
/// being re-derived from personality on load -- a real save can hold a mon
/// whose stored slot disagrees with its personality parity (nothing
/// upstream re-derives `abilityNum` after `CreateBoxMon` writes it once),
/// and this port must not silently swap such a mon's ability on load
/// (issue #322).
///
/// `SPECIES_TENTACOOL` (`72`) is the dual-ability fixture already used by
/// `battle`'s own ability tests: slot 0 is Clear Body, slot 1 is Liquid
/// Ooze (`gSpeciesInfo`). An *even* personality selects slot 0 by default
/// ([`battle::BattlePokemon::new`]), so overriding to slot 1 here is
/// deliberately the disagreeing case.
#[test]
fn a_disagreeing_ability_slot_survives_the_save_round_trip() {
    const TENTACOOL: u16 = 72;
    const CLEAR_BODY: u16 = 29;
    const LIQUID_OOZE: u16 = 64;

    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(TENTACOOL),
        20,
        Ivs::default(),
        0x1234_ABCC, // even -- personality parity alone would pick slot 0
        vec![assets::MoveId(33)],
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

/// `ITEM_LEFTOVERS` (`pokeemerald/include/constants/items.h:230`) -- a real
/// held item, so the sentinel is a value upstream could actually store.
const SENTINEL_HELD_ITEM: u16 = 200;
/// `STATUS1_PARALYSIS` (`pokeemerald/include/constants/battle.h:120`).
const SENTINEL_STATUS: u32 = 1 << 6;
/// A party mail slot index, not `MAIL_NONE`: mail is held *by* the mon and
/// is lost with it (`GiveMailToMon`, `pokeemerald/src/mail_data.c:111`).
const SENTINEL_MAIL: u8 = 2;
/// Friendship well away from Treecko's `base_friendship` of 70, so the
/// re-derived value and the accumulated one cannot be confused.
const SENTINEL_FRIENDSHIP: u8 = 213;
/// `PokemonSubstruct2` whole: six EVs then the five contest conditions and
/// sheen (`pokeemerald/include/pokemon.h:115-129`), every byte distinct so a
/// merge that shifted the substructure fails rather than passes.
const SENTINEL_EVS_AND_CONDITION: [u8; engine::save::SUBSTRUCTURE_LEN] =
    [252, 6, 0, 252, 4, 8, 11, 22, 33, 44, 55, 66];
/// What `CalculateMonStats` added to the stored stat block for those EVs,
/// one addend per stat in `max_hp`/`attack`/`defense`/`speed`/`sp_attack`/
/// `sp_defense` order, each distinct so a merge that wrote the block back
/// shuffled fails rather than passes.
///
/// The exact numbers are not upstream's arithmetic and do not need to be:
/// this port cannot rebuild an EV contribution at all
/// ([`battle::BattlePokemon`] carries no EVs), so what the fixture needs is
/// only that the stored block is *not* the 0-EV block the model recomputes.
/// That makes retaining it observable -- and makes an unconditional
/// overwrite the permanent weakening issue #344's review caught.
const SENTINEL_STAT_BONUS: [u16; 6] = [7, 15, 1, 15, 1, 2];

/// A party slot as a *save file* holds it: this encoder's own output for
/// [`a_battler`], then stamped with a sentinel in every field the battle
/// model does not carry.
///
/// This is the fixture issue #344 needs. The defect was not that these
/// bytes decoded wrong -- nothing decodes them -- but that re-saving a
/// loaded mon rebuilt the record from the battler alone and so wrote each
/// of them back as a zero.
fn a_stored_record() -> Pokemon {
    let mut record = to_save_pokemon(&Dex::new(), &a_battler());

    let mut substructures = record.box_data.substructures().unwrap();
    substructures.growth[2..4].copy_from_slice(&SENTINEL_HELD_ITEM.to_le_bytes());
    substructures.growth[9] = SENTINEL_FRIENDSHIP;
    substructures.evs_and_condition = SENTINEL_EVS_AND_CONDITION;
    // `PokemonSubstruct3`'s pre-IV bytes: pokérus, met location, and the
    // packed met level/game/ball/OT gender word.
    substructures.misc[0] = 0x24;
    substructures.misc[1] = 0x59;
    substructures.misc[2..4].copy_from_slice(&0xB2C5u16.to_le_bytes());
    // The ribbon word (`/*0x08*/`).
    substructures.misc[8..12].copy_from_slice(&0x1234_5678u32.to_le_bytes());
    record.box_data.set_substructures(&substructures);

    // The box header's own deferred bytes, which `BoxPokemon` retains
    // verbatim: nickname, language, OT name, and markings.
    let mut bytes = record.box_data.to_bytes();
    bytes[8..18].copy_from_slice(&[0xBB; 10]);
    bytes[18] = 5;
    bytes[20..27].copy_from_slice(&[0xCC; 7]);
    bytes[27] = 0b0000_1010;
    record.box_data = BoxPokemon::from_bytes(bytes);

    record.status = SENTINEL_STATUS;
    record.mail = SENTINEL_MAIL;

    // The cached stat block an EV-trained mon carries: the numbers this
    // port recomputes, raised by the EV contribution it cannot. `hp` is
    // left alone -- it is the mon's *current* HP, and leaving it at the
    // 0-EV maximum keeps the fixture a mon this port can hold exactly
    // (`from_save_pokemon` would otherwise clamp it down to that maximum,
    // which is the modelling gap rather than the merge).
    record.max_hp += SENTINEL_STAT_BONUS[0];
    record.attack += SENTINEL_STAT_BONUS[1];
    record.defense += SENTINEL_STAT_BONUS[2];
    record.speed += SENTINEL_STAT_BONUS[3];
    record.special_attack += SENTINEL_STAT_BONUS[4];
    record.special_defense += SENTINEL_STAT_BONUS[5];
    record
}

/// The save-data defect issue #344 fixes: loading a save reduced slot 0 to
/// a battler and saving rebuilt the record from that battler alone, so
/// every field with no home in the model came back a zero. Re-saving a
/// loaded mon now overlays the battler onto its own stored record.
#[test]
fn re_saving_a_loaded_mon_keeps_every_field_the_battle_model_does_not_carry() {
    let dex = Dex::new();
    let stored = a_stored_record();
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    // Play with it, so the merge is the interesting case rather than an
    // accidental byte-for-byte re-emit.
    lead.apply_damage(9);
    lead.deduct_pp(0).unwrap();

    let merged =
        merge_into_save_pokemon(&dex, &lead, &stored, &mut hp_hidden_by_load(&stored, &lead));

    let before = stored.box_data.substructures().unwrap();
    let after = merged
        .box_data
        .substructures()
        .expect("the merge must leave the checksum valid");
    assert_eq!(
        &after.growth[2..4],
        &before.growth[2..4],
        "heldItem is the save's"
    );
    assert_eq!(
        after.growth[9], SENTINEL_FRIENDSHIP,
        "accumulated friendship is the save's, not the species' base value"
    );
    assert_ne!(
        after.growth[9],
        dex.species(lead.species()).unwrap().base_friendship,
        "fixture sanity: a re-derived friendship would differ from this"
    );
    assert_eq!(
        after.evs_and_condition, SENTINEL_EVS_AND_CONDITION,
        "EVs and contest condition are the save's, whole"
    );
    assert_eq!(
        &after.misc[0..4],
        &before.misc[0..4],
        "pokérus and the met/ball/OT-gender bytes are the save's"
    );
    assert_eq!(
        &after.misc[8..12],
        &before.misc[8..12],
        "the ribbon word is the save's"
    );
    assert_eq!(
        merged.status, SENTINEL_STATUS,
        "non-volatile status is the save's"
    );
    assert_eq!(merged.mail, SENTINEL_MAIL, "the mail slot is the save's");
    assert_eq!(
        merged.box_data.to_bytes()[8..28],
        stored.box_data.to_bytes()[8..28],
        "nickname, language, OT name and markings are the save's"
    );
    assert_eq!(merged.box_data.personality(), stored.box_data.personality());
    assert_eq!(merged.box_data.ot_id(), stored.box_data.ot_id());

    // The cached stat block is retained on the same terms, because nothing
    // it is a function of moved: damage and spent PP are not inputs to
    // `CalculateMonStats`.
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

/// Sub-level experience is the common case a battle leaves behind, and it
/// is not an input to `CalculateMonStats`: the EV-trained block must
/// survive it (module docs -- the guard tests species and level, not the
/// experience word, which is overlaid regardless).
#[test]
fn sub_level_experience_does_not_flatten_the_retained_stat_block() {
    let dex = Dex::new();
    let stored = a_stored_record();
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
            stored.box_data.substructures().unwrap().growth[4..8]
                .try_into()
                .unwrap()
        ),
        "fixture sanity: the experience word really moved"
    );

    let merged =
        merge_into_save_pokemon(&dex, &lead, &stored, &mut hp_hidden_by_load(&stored, &lead));
    let after = merged.box_data.substructures().unwrap();
    assert_eq!(
        u32::from_le_bytes(after.growth[4..8].try_into().unwrap()),
        lead.experience(),
        "the awarded experience is filed"
    );
    assert_eq!(
        [merged.max_hp, merged.attack, merged.defense],
        [stored.max_hp, stored.attack, stored.defense],
        "and the EV-trained block is retained: sub-level experience is not \
         an input to the stat formula"
    );
}

/// The strongest form of the retention rule, and what the review of issue
/// #344 asked for: a lead that a session merely loaded and saved again,
/// without touching it, must write its record back *byte for byte*.
///
/// Offsets 88..=99 -- the six cached stat bytes -- were the ones that moved
/// before the merge made that block conditional, because it re-derived
/// them from the 0-EV model. Nothing on either side would put them back:
/// upstream's load path runs no `CalculateMonStats` (`super`'s module
/// docs), so an EV-trained file was filed permanently weaker by the act of
/// being loaded and saved.
#[test]
fn re_saving_an_untouched_lead_writes_the_record_back_byte_for_byte() {
    let dex = Dex::new();
    let stored = a_stored_record();
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert_ne!(
        stored.max_hp,
        u16::try_from(lead.stats().max_hp).unwrap(),
        "fixture sanity: the stored block carries an EV contribution the \
         model cannot rebuild, so a re-derived block would differ"
    );

    let merged =
        merge_into_save_pokemon(&dex, &lead, &stored, &mut hp_hidden_by_load(&stored, &lead));

    let (merged_bytes, stored_bytes) = (merged.to_bytes(), stored.to_bytes());
    let moved: Vec<usize> = (0..merged_bytes.len())
        .filter(|index| merged_bytes[*index] != stored_bytes[*index])
        .collect();
    assert_eq!(
        moved,
        Vec::<usize>::new(),
        "an untouched lead must re-save as the same 100 bytes"
    );

    // And it stays that way: a player who continues and saves repeatedly
    // without battling cannot drift the record one byte per session.
    let reloaded = from_save_pokemon(&dex, &merged).expect("the re-saved record must decode");
    let again = merge_into_save_pokemon(
        &dex,
        &reloaded,
        &merged,
        &mut hp_hidden_by_load(&merged, &reloaded),
    );
    assert_eq!(again.to_bytes(), stored.to_bytes());
}

/// The other half of the boundary: what the session *did* change has to
/// land in the merged record, or retention would just be a stale save.
#[test]
fn re_saving_a_loaded_mon_overlays_what_the_session_changed() {
    let dex = Dex::new();
    let stored = a_stored_record();
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

    let merged =
        merge_into_save_pokemon(&dex, &lead, &stored, &mut hp_hidden_by_load(&stored, &lead));
    let after = merged.box_data.substructures().unwrap();

    assert_eq!(
        u32::from_le_bytes(after.growth[4..8].try_into().unwrap()),
        lead.experience(),
        "the growth word carries the experience the battle awarded"
    );
    assert_eq!(merged.level, 13, "and the level that came with it");
    assert_eq!(merged.hp, u16::try_from(lead.current_hp()).unwrap());
    assert_ne!(merged.hp, stored.hp, "fixture sanity: the damage is real");
    assert_eq!(
        merged.max_hp,
        u16::try_from(lead.stats().max_hp).unwrap(),
        "a level-up moved what the cached block is a function of, so the \
         block is recomputed rather than retained -- it cannot be left \
         disagreeing with the level above it (module docs)"
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
            u16::try_from(lead.stats().attack).unwrap(),
            u16::try_from(lead.stats().defense).unwrap(),
            u16::try_from(lead.stats().speed).unwrap(),
            u16::try_from(lead.stats().sp_attack).unwrap(),
            u16::try_from(lead.stats().sp_defense).unwrap(),
        ],
        "the whole block, not just the maximum HP"
    );
    assert_eq!(
        after.attacks,
        super::encode_attacks(&lead),
        "moves and per-slot PP, slot for slot"
    );
    assert_ne!(
        &after.attacks[8..12],
        &stored.box_data.substructures().unwrap().attacks[8..12],
        "fixture sanity: the spent PP is real"
    );

    let reloaded = from_save_pokemon(&dex, &merged).expect("the merge must decode again");
    assert_eq!(reloaded, lead, "and back out as the same battler");
}

/// The identity gate. Personality and the OT id are the substructure XOR
/// key *and* the mon's identity, so a slot holding a different Pokémon must
/// be rebuilt rather than overlaid -- grafting one mon's moveset onto
/// another's ribbons and met data would be worse than the zeroing the merge
/// exists to fix.
#[test]
fn a_slot_holding_a_different_pokemon_is_rebuilt_rather_than_overlaid() {
    let dex = Dex::new();
    let stored = a_stored_record();
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");

    let swapped_personality = BattlePokemon::new(
        &dex,
        lead.species(),
        lead.level(),
        lead.ivs(),
        lead.personality() ^ 0x0F0F_0F0F,
        lead.moves().iter().map(|slot| slot.move_id).collect(),
    )
    .unwrap()
    .with_original_trainer_id(lead.original_trainer_id());
    assert_eq!(
        merge_into_save_pokemon(&dex, &swapped_personality, &stored, &mut 0),
        to_save_pokemon(&dex, &swapped_personality),
        "a different personality is a different mon"
    );

    let traded_away = lead.clone().with_original_trainer_id(0x0BAD_0BAD);
    assert_eq!(
        merge_into_save_pokemon(&dex, &traded_away, &stored, &mut 0),
        to_save_pokemon(&dex, &traded_away),
        "so is a different original trainer -- it is half the XOR key"
    );
}

/// A new game's slot 0 is `SPECIES_NONE` and has no retained bytes at all,
/// so the first save of a fresh file must build the record from scratch
/// instead of overlaying onto an empty slot.
#[test]
fn an_empty_slot_is_built_from_scratch() {
    let dex = Dex::new();
    let mon = BattlePokemon::new(
        &dex,
        assets::SpeciesId(277),
        5,
        Ivs::default(),
        0,
        vec![assets::MoveId(1)],
    )
    .unwrap();
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

/// `isEgg` and `abilityNum` share the IV word, and the model owns exactly
/// one of them. The merge must rewrite the word around the egg bit rather
/// than through it (module docs).
#[test]
fn the_merge_rewrites_the_iv_word_around_the_egg_bit() {
    let dex = Dex::new();
    let mut stored = a_stored_record();
    let mut substructures = stored.box_data.substructures().unwrap();
    let iv_word = u32::from_le_bytes(substructures.misc[4..8].try_into().unwrap());
    substructures.misc[4..8].copy_from_slice(&(iv_word | super::IS_EGG_BIT).to_le_bytes());
    stored.box_data.set_substructures(&substructures);

    let lead = from_save_pokemon(&dex, &stored)
        .expect("the fixture must decode")
        .with_ability_slot(1);
    let merged =
        merge_into_save_pokemon(&dex, &lead, &stored, &mut hp_hidden_by_load(&stored, &lead));

    let merged_word = u32::from_le_bytes(
        merged.box_data.substructures().unwrap().misc[4..8]
            .try_into()
            .unwrap(),
    );
    assert_eq!(
        merged_word & super::IS_EGG_BIT,
        super::IS_EGG_BIT,
        "the egg bit this port does not model stays exactly as it was"
    );
    assert_eq!(merged_word >> 31, 1, "abilityNum is the battler's");
    assert_eq!(unpack_ivs(merged_word), lead.ivs());
}

/// Issue #344's review, second round: an EV-trained lead saved at *full*
/// health has `hp == max_hp` above the model's 0-EV maximum, and
/// [`from_save_pokemon`] clamps the live copy down to the model's full.
/// The merge must translate that clamp back out rather than file the
/// clamped number: Continue -> SAVE of a full-health mon stays a no-op
/// instead of marking the mon damaged.
#[test]
fn continue_then_save_keeps_a_full_health_ev_trained_lead_at_full() {
    let dex = Dex::new();
    let mut stored = a_stored_record();
    stored.hp = stored.max_hp;
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > lead.stats().max_hp,
        "fixture sanity: the stored full must exceed the model's maximum, \
         or the load clamp never fires"
    );

    let merged =
        merge_into_save_pokemon(&dex, &lead, &stored, &mut hp_hidden_by_load(&stored, &lead));

    assert_eq!(merged.to_bytes(), stored.to_bytes());
}

/// The same clamp with the stored current HP strictly *between* the
/// model's maximum and the retained one: the load pins the live copy at
/// the model's full, and the merge files the stored byte back rather than
/// either boundary.
#[test]
fn continue_then_save_keeps_an_over_model_max_current_hp() {
    let dex = Dex::new();
    let mut stored = a_stored_record();
    stored.hp = stored.max_hp - 3;
    let lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > lead.stats().max_hp,
        "fixture sanity: the stored value must sit above the model's \
         maximum, or the load clamp never fires"
    );

    let merged =
        merge_into_save_pokemon(&dex, &lead, &stored, &mut hp_hidden_by_load(&stored, &lead));

    assert_eq!(merged.to_bytes(), stored.to_bytes());
}

/// The clamp translation under battle damage: the stored points above the
/// model's maximum were hidden from the session, so damage taken must
/// subtract from the *stored* value, not from its clamp -- upstream's
/// arithmetic is absolute. Stored `model_max + 5` taking 10 damage files
/// `stored - 10`, not `model_max - 10`.
#[test]
fn battle_damage_on_a_clamped_load_subtracts_from_the_stored_hp() {
    const DAMAGE: u32 = 10;
    const HIDDEN: u16 = 5;

    let dex = Dex::new();
    let mut stored = a_stored_record();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    assert!(
        stored.hp < stored.max_hp,
        "fixture sanity: the stored hp must sit below the retained maximum"
    );
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    lead.apply_damage(DAMAGE);

    let merged =
        merge_into_save_pokemon(&dex, &lead, &stored, &mut hp_hidden_by_load(&stored, &lead));

    assert_eq!(merged.hp, stored.hp - u16::try_from(DAMAGE).unwrap());
}

/// Issue #344's review, third round: the load-clamp offset is only a fact
/// about the *retained* stat block. When a level-up makes the merge
/// recompute the block, the record it writes has the model's own
/// `max_hp` and hides nothing, so the merge must zero the session offset
/// it was handed. Carrying the stale offset into the next save's
/// retained-block branch healed the lead by exactly its value; saving
/// twice with no gameplay in between must file the same bytes.
#[test]
fn a_stat_block_recompute_retires_the_load_clamp_offset() {
    const HIDDEN: u16 = 5;
    const DAMAGE: u32 = 10;

    let dex = Dex::new();
    let mut stored = a_stored_record();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    let mut lead = from_save_pokemon(&dex, &stored).expect("the fixture must decode");
    let mut offset = hp_hidden_by_load(&stored, &lead);
    assert_eq!(offset, HIDDEN, "fixture sanity: the load clamp must fire");

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

    let first = merge_into_save_pokemon(&dex, &lead, &stored, &mut offset);
    assert_eq!(
        offset, 0,
        "the recompute wrote the model's own block, so no stored points stay hidden"
    );

    let second = merge_into_save_pokemon(&dex, &lead, &first, &mut offset);
    assert_eq!(
        second.to_bytes(),
        first.to_bytes(),
        "an immediate re-save must not heal the lead by the retired offset"
    );
}
