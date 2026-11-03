//! Byte layout, new-game defaults, and transfer invariants of
//! [`PokemonStorage`].

use super::*;
use crate::save::PokemonSubstructures;

const PERSONALITIES: u32 = 24;

fn mon_with(species: u16, held_item: u16, egg: bool, personality: u32) -> BoxPokemon {
    let mut parts = PokemonSubstructures::default();
    parts.growth[..2].copy_from_slice(&species.to_le_bytes());
    parts.growth[2..4].copy_from_slice(&held_item.to_le_bytes());
    if egg {
        parts.misc[MISC_IS_EGG_BYTE] |= MISC_IS_EGG_BIT;
    }
    let mut mon = BoxPokemon::new(personality, 0x0102_0304);
    mon.set_substructures(&parts);
    mon.set_has_species(species != 0);
    mon
}

fn mon(species: u16) -> BoxPokemon {
    mon_with(species, 0, false, u32::from(species))
}

fn party_mon(species: u16, hp: u16) -> Pokemon {
    Pokemon {
        box_data: mon(species),
        hp,
        max_hp: 50,
        level: 5,
        mail: MAIL_NONE,
        ..Pokemon::default()
    }
}

fn block_with_party(members: &[Pokemon]) -> SaveBlock1 {
    let mut block = SaveBlock1::default();
    for (slot, member) in block.player_party.iter_mut().zip(members) {
        *slot = *member;
    }
    for slot in block.player_party.iter_mut().skip(members.len()) {
        *slot = zeroed_party_mon();
    }
    block.player_party_count = u8::try_from(members.len()).unwrap();
    block
}

fn patterned_payload() -> Vec<u8> {
    (0..PokemonStorage::PAYLOAD_LEN)
        .map(|i| u8::try_from(i % 251).unwrap())
        .collect()
}

fn assert_unchanged_on_error(
    storage: &mut PokemonStorage,
    block: &mut SaveBlock1,
    expected: StorageError,
    attempt: impl FnOnce(&mut PokemonStorage, &mut SaveBlock1) -> Result<(), StorageError>,
) {
    let (storage_before, party_before, count_before) = (
        storage.clone(),
        block.player_party,
        block.player_party_count,
    );
    assert_eq!(attempt(storage, block), Err(expected));
    assert_eq!(*storage, storage_before);
    assert_eq!(block.player_party, party_before);
    assert_eq!(block.player_party_count, count_before);
}

#[test]
fn layout_matches_the_upstream_struct() {
    assert_eq!(PokemonStorage::PAYLOAD_LEN, 0x83D0);
    let mut bytes = vec![0u8; PokemonStorage::PAYLOAD_LEN];
    bytes[0] = 9;
    bytes[BOXES_OFFSET] = 0xA1;
    bytes[BOXES_OFFSET + 29 * 80] = 0xB2;
    bytes[BOXES_OFFSET + 13 * 30 * 80 + 29 * 80 + 79] = 0xC3;
    bytes[0x8344] = 0xD4;
    bytes[0x8344 + 13 * 9 + 8] = 0xE5;
    bytes[0x83C2] = 0xF6;
    bytes[0x83CF] = 0x17;
    let storage = PokemonStorage::from_bytes(&bytes).unwrap();
    assert_eq!(storage.current_box, 9);
    assert_eq!(storage.box_mon(0, 0).unwrap().to_bytes()[0], 0xA1);
    assert_eq!(storage.box_mon(0, 29).unwrap().to_bytes()[0], 0xB2);
    assert_eq!(storage.box_mon(13, 29).unwrap().to_bytes()[79], 0xC3);
    assert_eq!(storage.box_names[0][0], 0xD4);
    assert_eq!(storage.box_names[13][8], 0xE5);
    assert_eq!(storage.box_wallpapers[0], 0xF6);
    assert_eq!(storage.box_wallpapers[13], 0x17);
    assert_eq!(&storage.to_payload()[4..0x8341], &bytes[4..0x8341]);
}

#[test]
fn wrong_lengths_are_rejected() {
    for len in [
        0,
        PokemonStorage::PAYLOAD_LEN - 1,
        PokemonStorage::PAYLOAD_LEN + 1,
    ] {
        assert_eq!(
            PokemonStorage::from_bytes(&vec![0; len]),
            Err(SaveError::Truncated {
                expected: PokemonStorage::PAYLOAD_LEN,
                got: len
            })
        );
    }
}

#[test]
fn arbitrary_bytes_decode_and_patch_back_identically() {
    let bytes = patterned_payload();
    let storage = PokemonStorage::from_bytes(&bytes).unwrap();
    let mut patched: Box<[u8; PokemonStorage::PAYLOAD_LEN]> =
        bytes.clone().into_boxed_slice().try_into().unwrap();
    storage.patch_bytes(&mut patched);
    assert_eq!(&patched[..], &bytes[..]);
}

#[test]
fn patching_preserves_the_padding_after_current_box() {
    let mut base: Box<[u8; PokemonStorage::PAYLOAD_LEN]> = vec![0u8; PokemonStorage::PAYLOAD_LEN]
        .into_boxed_slice()
        .try_into()
        .unwrap();
    base[1..4].copy_from_slice(&[0x11, 0x22, 0x33]);
    let mut storage = PokemonStorage::new_game();
    storage.current_box = 5;
    storage.patch_bytes(&mut base);
    assert_eq!(&base[1..4], &[0x11, 0x22, 0x33]);
    assert_eq!(base[0], 5);
    assert_eq!(&storage.to_payload()[1..4], &[0, 0, 0]);
}

#[test]
fn new_game_defaults_match_reset_pokemon_storage_system() {
    let storage = PokemonStorage::new_game();
    let payload = storage.to_payload();
    assert_eq!(payload[0], 0);
    assert!(
        payload[1..0x8344].iter().all(|&b| b == 0),
        "all 420 boxes are zeroed"
    );
    assert_eq!(&storage.box_names[0][..5], &[0xBC, 0xC9, 0xD2, 0xA2, 0xFF]);
    assert_eq!(&storage.box_names[8][..5], &[0xBC, 0xC9, 0xD2, 0xAA, 0xFF]);
    assert_eq!(
        &storage.box_names[9][..6],
        &[0xBC, 0xC9, 0xD2, 0xA2, 0xA1, 0xFF]
    );
    assert_eq!(
        &storage.box_names[13][..6],
        &[0xBC, 0xC9, 0xD2, 0xA2, 0xA5, 0xFF]
    );
    assert!(storage.box_names[0][5..].iter().all(|&b| b == 0));
    assert!(storage.box_names[13][6..].iter().all(|&b| b == 0));
    assert_eq!(
        storage.box_wallpapers,
        [0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3, 0, 1]
    );
}

#[test]
fn occupancy_follows_the_secure_species() {
    assert!(!holds_pokemon(&BoxPokemon::default()));
    assert!(holds_pokemon(&mon(25)));
    let mut flag_only = BoxPokemon::default();
    flag_only.set_has_species(true);
    assert!(
        !holds_pokemon(&flag_only),
        "hasSpecies alone does not occupy a slot"
    );
    let mut species_without_flag = mon(25);
    species_without_flag.set_has_species(false);
    assert!(holds_pokemon(&species_without_flag));

    let mut bytes = BoxPokemon::default().to_bytes();
    bytes[28] = 1;
    assert!(
        holds_pokemon(&BoxPokemon::from_bytes(bytes)),
        "a failed checksum reads as a Bad Egg"
    );
    let mut bad_egg = BoxPokemon::default().to_bytes();
    bad_egg[19] = 1;
    assert!(holds_pokemon(&BoxPokemon::from_bytes(bad_egg)));
    for personality in 0..PERSONALITIES {
        assert!(holds_pokemon(&mon_with(7, 0, false, personality)));
    }
}

#[test]
fn first_free_slot_fills_in_order_and_reports_a_full_box() {
    let mut storage = PokemonStorage::new_game();
    for slot in 0..IN_BOX_COUNT {
        assert_eq!(storage.first_free_slot(2), Some(slot));
        assert_eq!(
            storage.store_in_first_free_slot(2, mon(1 + u16::try_from(slot).unwrap())),
            Ok(slot)
        );
    }
    assert_eq!(storage.count_in_box(2), IN_BOX_COUNT);
    assert_eq!(storage.first_free_slot(2), None);
    let before = storage.clone();
    assert_eq!(
        storage.store_in_first_free_slot(2, mon(9)),
        Err(StorageError::BoxFull)
    );
    assert_eq!(storage, before);
    assert_eq!(
        storage.store_in_first_free_slot(14, mon(9)),
        Err(StorageError::BoxOutOfRange)
    );
    assert_eq!(
        storage.store_in_first_free_slot(3, BoxPokemon::default()),
        Err(StorageError::RecordEmpty)
    );
    assert_eq!(storage.count_in_box(14), 0);
}

#[test]
fn free_space_is_per_the_has_species_sanity_bit() {
    let mut storage = PokemonStorage::new_game();
    assert!(storage.has_free_space());
    for box_id in 0..TOTAL_BOXES_COUNT {
        for _ in 0..IN_BOX_COUNT {
            storage.store_in_first_free_slot(box_id, mon(1)).unwrap();
        }
    }
    assert!(!storage.has_free_space());
}

const fn at(box_id: usize, slot: usize) -> BoxSlot {
    BoxSlot { box_id, slot }
}

#[test]
fn move_and_swap_relocate_whole_records() {
    let mut storage = PokemonStorage::new_game();
    storage.store_in_first_free_slot(0, mon(1)).unwrap();
    storage.store_in_first_free_slot(0, mon(2)).unwrap();

    storage.move_box_mon(at(0, 0), at(13, 29)).unwrap();
    assert_eq!(*storage.box_mon(13, 29).unwrap(), mon(1));
    assert_eq!(*storage.box_mon(0, 0).unwrap(), BoxPokemon::default());

    storage.swap_box_mons(at(0, 1), at(13, 29)).unwrap();
    assert_eq!(*storage.box_mon(0, 1).unwrap(), mon(1));
    assert_eq!(*storage.box_mon(13, 29).unwrap(), mon(2));
}

#[test]
fn rejected_box_transfers_change_nothing() {
    let mut storage = PokemonStorage::new_game();
    storage.store_in_first_free_slot(0, mon(1)).unwrap();
    storage.store_in_first_free_slot(0, mon(2)).unwrap();
    let before = storage.clone();
    let cases = [
        (at(0, 0), at(0, 0), StorageError::SameSlot),
        (at(0, 5), at(1, 0), StorageError::SlotEmpty),
        (at(0, 0), at(0, 1), StorageError::SlotOccupied),
        (at(14, 0), at(0, 2), StorageError::BoxOutOfRange),
        (at(0, 0), at(0, 30), StorageError::SlotOutOfRange),
    ];
    for (from, to, expected) in cases {
        assert_eq!(storage.move_box_mon(from, to), Err(expected));
        assert_eq!(storage, before);
    }
    assert_eq!(
        storage.swap_box_mons(at(0, 0), at(0, 5)),
        Err(StorageError::SlotEmpty)
    );
    assert_eq!(
        storage.swap_box_mons(at(0, 0), at(0, 0)),
        Err(StorageError::SameSlot)
    );
    assert_eq!(storage, before);
}

#[test]
fn deposit_compacts_the_party_with_whole_records_and_mail_none_tails() {
    let mut storage = PokemonStorage::new_game();
    let members = [party_mon(1, 10), party_mon(2, 10), party_mon(3, 10)];
    let mut block = block_with_party(&members);
    assert_eq!(storage.deposit_party_mon(&mut block, 0, 4), Ok(0));
    assert_eq!(*storage.box_mon(4, 0).unwrap(), members[0].box_data);
    assert_eq!(block.player_party_count, 2);
    assert_eq!(block.player_party[0], members[1]);
    assert_eq!(block.player_party[1], members[2]);
    for tail in &block.player_party[2..] {
        assert_eq!(*tail, zeroed_party_mon());
        assert_eq!(tail.mail, 0xFF);
    }
}

#[test]
fn deposit_rejections_leave_both_sides_untouched() {
    let mut storage = PokemonStorage::new_game();
    let mut lone = block_with_party(&[party_mon(1, 10)]);
    assert_unchanged_on_error(
        &mut storage,
        &mut lone,
        StorageError::JustOnePartyMember,
        |s, b| s.deposit_party_mon(b, 0, 0).map(drop),
    );

    let mut fainted_partner = block_with_party(&[party_mon(1, 10), party_mon(2, 0)]);
    assert_unchanged_on_error(
        &mut storage,
        &mut fainted_partner,
        StorageError::LastAblePartyMember,
        |s, b| s.deposit_party_mon(b, 0, 0).map(drop),
    );
    let egg = Pokemon {
        box_data: mon_with(3, 0, true, 3),
        hp: 10,
        ..Pokemon::default()
    };
    let mut egg_partner = block_with_party(&[party_mon(1, 10), egg]);
    assert_unchanged_on_error(
        &mut storage,
        &mut egg_partner,
        StorageError::LastAblePartyMember,
        |s, b| s.deposit_party_mon(b, 0, 0).map(drop),
    );

    let mail_holder = Pokemon {
        box_data: mon_with(4, 121, false, 4),
        hp: 10,
        ..Pokemon::default()
    };
    let mut mail = block_with_party(&[mail_holder, party_mon(2, 10)]);
    assert_unchanged_on_error(&mut storage, &mut mail, StorageError::HoldsMail, |s, b| {
        s.deposit_party_mon(b, 0, 0).map(drop)
    });

    let mut corrupt_bytes = mail_holder.box_data.to_bytes();
    corrupt_bytes[28] ^= 1;
    let corrupt_mail_holder = Pokemon {
        box_data: BoxPokemon::from_bytes(corrupt_bytes),
        ..mail_holder
    };
    let mut corrupt_mail = block_with_party(&[corrupt_mail_holder, party_mon(2, 10)]);
    assert_unchanged_on_error(
        &mut storage,
        &mut corrupt_mail,
        StorageError::HoldsMail,
        |s, b| s.deposit_party_mon(b, 0, 0).map(drop),
    );

    let mut pair = block_with_party(&[party_mon(1, 10), party_mon(2, 10)]);
    assert_unchanged_on_error(&mut storage, &mut pair, StorageError::SlotEmpty, |s, b| {
        s.deposit_party_mon(b, 3, 0).map(drop)
    });
    assert_unchanged_on_error(
        &mut storage,
        &mut pair,
        StorageError::SlotOutOfRange,
        |s, b| s.deposit_party_mon(b, 6, 0).map(drop),
    );
    assert_unchanged_on_error(
        &mut storage,
        &mut pair,
        StorageError::BoxOutOfRange,
        |s, b| s.deposit_party_mon(b, 0, 14).map(drop),
    );

    for _ in 0..IN_BOX_COUNT {
        storage.store_in_first_free_slot(0, mon(9)).unwrap();
    }
    assert_unchanged_on_error(&mut storage, &mut pair, StorageError::BoxFull, |s, b| {
        s.deposit_party_mon(b, 0, 0).map(drop)
    });
}

#[test]
fn a_fainted_member_may_leave_while_an_able_one_remains() {
    let mut storage = PokemonStorage::new_game();
    let mut block = block_with_party(&[party_mon(1, 0), party_mon(2, 10)]);
    assert_eq!(storage.deposit_party_mon(&mut block, 0, 0), Ok(0));
    assert_eq!(block.player_party_count, 1);
}

#[test]
fn deposit_ignores_a_stale_raw_party_count_and_gaps() {
    let mut storage = PokemonStorage::new_game();
    let mut block = block_with_party(&[party_mon(1, 10), party_mon(2, 10)]);
    block.player_party[3] = party_mon(3, 10);
    block.player_party_count = 0xFE;
    assert_eq!(storage.deposit_party_mon(&mut block, 1, 0), Ok(0));
    assert_eq!(block.player_party_count, 2);
    assert_eq!(block.player_party[1].box_data, mon(3));
}

#[test]
fn withdraw_appends_the_converted_record_and_clears_the_box_slot() {
    let mut storage = PokemonStorage::new_game();
    storage.store_in_first_free_slot(1, mon(7)).unwrap();
    let mut block = block_with_party(&[party_mon(1, 10)]);
    let record = Pokemon {
        box_data: mon(7),
        level: 12,
        hp: 33,
        ..zeroed_party_mon()
    };
    storage
        .withdraw_to_party(&mut block, at(1, 0), record)
        .unwrap();
    assert_eq!(block.player_party_count, 2);
    assert_eq!(block.player_party[1], record);
    assert_eq!(*storage.box_mon(1, 0).unwrap(), BoxPokemon::default());
}

#[test]
fn withdraw_rejections_leave_both_sides_untouched() {
    let mut storage = PokemonStorage::new_game();
    storage.store_in_first_free_slot(1, mon(7)).unwrap();
    let record = Pokemon {
        box_data: mon(7),
        ..zeroed_party_mon()
    };
    let mut block = block_with_party(&[party_mon(1, 10)]);
    assert_unchanged_on_error(&mut storage, &mut block, StorageError::SlotEmpty, |s, b| {
        s.withdraw_to_party(b, at(1, 1), record)
    });
    assert_unchanged_on_error(
        &mut storage,
        &mut block,
        StorageError::BoxOutOfRange,
        |s, b| s.withdraw_to_party(b, at(14, 0), record),
    );
    assert_unchanged_on_error(
        &mut storage,
        &mut block,
        StorageError::RecordMismatch,
        |s, b| s.withdraw_to_party(b, at(1, 0), party_mon(8, 1)),
    );
    let members: Vec<Pokemon> = (1..=6).map(|species| party_mon(species, 10)).collect();
    let mut full = block_with_party(&members);
    assert_unchanged_on_error(&mut storage, &mut full, StorageError::PartyFull, |s, b| {
        s.withdraw_to_party(b, at(1, 0), record)
    });
}
