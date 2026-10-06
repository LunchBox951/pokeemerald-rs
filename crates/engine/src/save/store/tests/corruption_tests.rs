//! Fallback between slots when a sector fails its checksum.

use super::*;

#[test]
fn corrupted_sector_in_the_current_slot_falls_back_to_the_other_slot() {
    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = sample_block2();

    store.save(&block1, &block2);
    store.save(&block1, &block2);
    assert_eq!(store.save_counter(), 2);

    let sector_in_slot = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
    store.corrupt_byte(0, sector_in_slot, 0);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Error);
    assert_eq!(outcome.block2, block2);
    assert_eq!(store.save_counter(), 1);
}

#[test]
fn corrupted_later_block1_sector_falls_back_to_the_intact_slot() {
    let mut store = SaveStore::new();
    let mut older = sample_block1();
    older.pos.x = 111;
    let mut newer = sample_block1();
    newer.pos.x = 222;
    let block2 = sample_block2();

    store.save(&older, &block2);
    store.save(&newer, &block2);

    let later_id = SECTOR_ID_SAVEBLOCK1_START + 3;
    let later_sector = store.find_sector_in_slot(0, later_id);
    store.corrupt_byte(0, later_sector, 0);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Error);
    assert_eq!(outcome.block1.pos.x, 111);
    assert_eq!(store.save_counter(), 1);
}

#[test]
fn both_corrupt_slots_copy_slot_zero_and_recover_its_rotation() {
    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = sample_block2();

    store.save(&block1, &block2);
    store.save(&block1, &block2);

    let in_slot0 = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
    let in_slot1 = store.find_sector_in_slot(1, SECTOR_ID_SAVEBLOCK2);
    store.corrupt_byte(0, in_slot0, 0);
    store.corrupt_byte(1, in_slot1, 0);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Corrupt);
    assert_eq!(store.save_counter(), 0);
    assert_eq!(
        store.last_written_sector(),
        u16::try_from(in_slot0).unwrap()
    );
}

#[test]
fn corrupting_the_last_saveblock2_payload_byte_is_detected() {
    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = sample_block2();

    store.save(&block1, &block2);
    store.save(&block1, &block2);

    let sector_in_slot = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
    store.corrupt_byte(0, sector_in_slot, SaveBlock2::PAYLOAD_LEN - 1);

    assert!(!store
        .read_physical(0, sector_in_slot)
        .is_valid(SaveBlock2::PAYLOAD_LEN));

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Error);
    assert_eq!(outcome.block2, block2);
    assert_eq!(store.save_counter(), 1);
}

#[test]
fn copy_slot_follows_adopted_counter_parity_not_validation_winner() {
    const SAVE_COUNTER_OFFSET: usize = SECTOR_SIZE - size_of::<u32>();

    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2_slot1 = SaveBlock2 {
        player_trainer_id: [0x11; TRAINER_ID_LENGTH],
        ..sample_block2()
    };
    let block2_slot0 = SaveBlock2 {
        player_trainer_id: [0x22; TRAINER_ID_LENGTH],
        ..sample_block2()
    };

    store.save(&block1, &block2_slot1);
    store.save(&block1, &block2_slot0);

    for i in 0..NUM_SECTORS_PER_SLOT {
        store.corrupt_byte(0, i, SAVE_COUNTER_OFFSET);
    }

    let outcome = store.load();
    assert_eq!(store.save_counter(), u32::from(!2u8));
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(
        outcome.block2, block2_slot1,
        "copy must follow counter parity (slot 1), not the validation winner (slot 0)"
    );
}

#[test]
fn corrupt_recovery_with_intact_block2_decodes_encrypted_fields_to_plaintext_defaults() {
    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = sample_block2();
    assert_ne!(block2.encryption_key, 0, "test needs a nonzero key");

    store.save(&block1, &block2);
    store.save(&block1, &block2);

    let block1_chunk0 = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK1_START);
    store.corrupt_byte(0, block1_chunk0, 0);
    let slot1_block2 = store.find_sector_in_slot(1, SECTOR_ID_SAVEBLOCK2);
    store.corrupt_byte(1, slot1_block2, 0);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Corrupt);
    assert_eq!(outcome.block2, block2);
    assert_eq!(outcome.block1.money, 0);
    assert_ne!(outcome.block1.money, outcome.block2.encryption_key);
    let bag = &outcome.block1.bag;
    for slot in bag
        .items
        .iter()
        .chain(&bag.key_items)
        .chain(&bag.poke_balls)
        .chain(&bag.tms_hms)
        .chain(&bag.berries)
    {
        assert_eq!(
            slot.quantity, 0,
            "empty bag slots must decode to quantity 0"
        );
        assert_eq!(slot.item_id, 0);
    }
}

#[test]
fn corrupt_recovery_without_a_recovered_key_decodes_encrypted_fields_to_plaintext_defaults() {
    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = sample_block2();
    assert_ne!(block2.encryption_key, 0, "test needs a nonzero key");
    assert_ne!(block1.money, 0, "test needs nonzero encrypted state");

    store.save(&block1, &block2);
    store.save(&block1, &block2);

    let slot0_block2 = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
    store.corrupt_byte(0, slot0_block2, 0);
    let slot1_block2 = store.find_sector_in_slot(1, SECTOR_ID_SAVEBLOCK2);
    store.corrupt_byte(1, slot1_block2, 0);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Corrupt);
    assert_eq!(outcome.block2.encryption_key, 0);
    assert_eq!(outcome.block1.money, 0);
    let bag = &outcome.block1.bag;
    for slot in bag
        .items
        .iter()
        .chain(&bag.key_items)
        .chain(&bag.poke_balls)
        .chain(&bag.tms_hms)
        .chain(&bag.berries)
    {
        assert_eq!(slot.quantity, 0, "bag quantities must decode to 0");
    }
    assert_eq!(bag.items[0].item_id, 1);
    assert_eq!(bag.key_items[29].item_id, 2);
    assert_eq!(bag.poke_balls[15].item_id, 3);
    assert_eq!(bag.tms_hms[63].item_id, 4);
    assert_eq!(bag.berries[45].item_id, 5);
}

#[test]
fn checksum_valid_out_of_range_gender_retains_key_and_decrypts_bag() {
    const PLAYER_GENDER_OFFSET: usize = 0x08;
    const OUT_OF_RANGE_GENDER: u8 = 9;

    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = sample_block2();
    assert_ne!(block1.money, 0, "test needs nonzero encrypted state");

    store.save(&block1, &block2);
    store.save(&block1, &block2);

    let mut payload = block2.to_bytes();
    payload[PLAYER_GENDER_OFFSET] = OUT_OF_RANGE_GENDER;
    let mutated_sector = Sector::write(SECTOR_ID_SAVEBLOCK2, &payload, 2);
    assert!(mutated_sector.is_valid(SaveBlock2::PAYLOAD_LEN));
    let slot0_block2 = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
    store.write_physical(0, slot0_block2, &mutated_sector);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(store.save_counter(), 2, "the newer slot stays selected");
    assert_eq!(
        outcome.block2.player_gender,
        PlayerGender::Other(OUT_OF_RANGE_GENDER)
    );
    assert_eq!(outcome.block2.player_trainer_id, block2.player_trainer_id);
    assert_eq!(outcome.block2.encryption_key, block2.encryption_key);
    assert_eq!(outcome.block1.money, block1.money);
    assert_eq!(outcome.block1.bag, block1.bag);
}

#[test]
fn equal_and_opposite_checksum_byte_mutations_keep_newer_slot_selected() {
    const PLAYER_NAME_FIFTH_BYTE_OFFSET: usize = 0x04;
    const PLAYER_GENDER_OFFSET: usize = 0x08;
    const CHECKSUM_CANCELING_BIT: u8 = 1 << 3;
    const OUT_OF_RANGE_GENDER: u8 = 9;

    let mut store = SaveStore::new();
    let older_block1 = sample_block1();
    let older_block2 = SaveBlock2 {
        player_trainer_id: [0xAA; TRAINER_ID_LENGTH],
        encryption_key: 0x1111_1111,
        ..sample_block2()
    };
    let mut newer_block1 = sample_block1();
    newer_block1.pos.x = 222;
    let newer_block2 = SaveBlock2 {
        player_name: *b"RUSTY\xFF\0\0",
        player_gender: PlayerGender::Female,
        player_trainer_id: [0x22; TRAINER_ID_LENGTH],
        special_save_warp_flags: 0,
        gcn_link_flags: 0,
        encryption_key: 0xA1B2_C3D4,
        options_text_speed: 1,
        options_window_frame_type: 5,
    };

    store.save(&older_block1, &older_block2);
    store.save(&newer_block1, &newer_block2);

    let sector_in_slot = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
    let before = store.read_physical(0, sector_in_slot);
    let mut bytes = *before.as_bytes();
    bytes[PLAYER_GENDER_OFFSET] ^= CHECKSUM_CANCELING_BIT;
    bytes[PLAYER_NAME_FIFTH_BYTE_OFFSET] ^= CHECKSUM_CANCELING_BIT;
    let mutated = Sector::from_bytes(bytes);
    assert_eq!(
        mutated.stored_checksum(),
        before.stored_checksum(),
        "the mutation never touches the footer"
    );
    assert!(
        mutated.is_valid(SaveBlock2::PAYLOAD_LEN),
        "the two opposite-direction bit-3 flips cancel in the additive checksum"
    );
    store.write_physical(0, sector_in_slot, &mutated);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(store.save_counter(), 2, "the newer counter stays selected");
    assert_eq!(
        outcome.block2.player_gender,
        PlayerGender::Other(OUT_OF_RANGE_GENDER)
    );
    assert_eq!(outcome.block2.encryption_key, newer_block2.encryption_key);
    assert_eq!(
        outcome.block2.player_trainer_id,
        newer_block2.player_trainer_id
    );
    assert_eq!(outcome.block1.pos, newer_block1.pos);
    assert_eq!(outcome.block1.money, newer_block1.money);
    assert_eq!(outcome.block1.bag, newer_block1.bag);
}
