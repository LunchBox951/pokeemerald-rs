//! Unit tests for the [`super`] party encoder (I-6, issue #232).

use super::{from_save_pokemon, pack_ivs, to_save_pokemon, unpack_ivs, PartyError, MAIL_NONE};
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

    let saved = to_save_pokemon(&dex, &mon, 0x0011_2233);
    let restored = from_save_pokemon(&dex, &saved).expect("what we just wrote must decode");

    assert_eq!(restored.species(), mon.species());
    assert_eq!(restored.level(), mon.level());
    assert_eq!(restored.personality(), mon.personality());
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

/// The saved bytes are upstream's, not an invented shape: the growth
/// substructure holds the species and the level's own experience, the party
/// block holds `MAIL_NONE`, and the box header carries the OT id the key is
/// built from.
#[test]
fn the_saved_bytes_sit_at_upstream_offsets() {
    let dex = Dex::new();
    let mon = a_battler();
    let saved = to_save_pokemon(&dex, &mon, 0x0011_2233);

    assert_eq!(saved.box_data.ot_id(), 0x0011_2233);
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
        assets::experience_for_level(treecko.growth_rate, 12).unwrap(),
        "experience is seeded from the growth curve, as CreateBoxMon does"
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
    let saved = to_save_pokemon(&dex, &mon, 7);
    let restored = from_save_pokemon(&dex, &saved).expect("a one-move mon must decode");
    assert_eq!(restored.moves().len(), 1);
}

/// A checksum-valid sector can still hold a mon whose *decrypted* region is
/// garbage. The decode must say so rather than hand the battle engine a
/// scrambled battler.
#[test]
fn a_corrupt_secure_region_is_reported_not_guessed_at() {
    let dex = Dex::new();
    let mut saved = to_save_pokemon(&dex, &a_battler(), 7);
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
