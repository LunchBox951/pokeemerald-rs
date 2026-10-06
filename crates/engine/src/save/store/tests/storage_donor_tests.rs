//! Which stale storage tail a legacy head may borrow, and which it must not.

use super::*;

/// A five-sector head at rotation 1 over an older slot whose tail has one
/// damaged sector: no torn full write produces a rotated head, so the head
/// stays intact and only the damaged tail loses donor eligibility.
#[test]
fn a_rotated_legacy_head_survives_a_damaged_stale_tail() {
    let block2 = sample_block2();
    let older_block1 = SaveBlock1 {
        money: 111,
        ..sample_block1()
    };
    let newer_block1 = SaveBlock1 {
        money: 222,
        ..sample_block1()
    };
    let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];

    let mut store = SaveStore::new();
    write_full_slot(&mut store, 0, &older_block1, &block2, &storage_bytes, 2);
    write_full_slot(&mut store, 1, &older_block1, &block2, &storage_bytes, 1);
    // Flash damage to one storage sector of the older slot's tail.
    store.corrupt_byte(1, 7, 4);
    write_legacy_slot_rotated(&mut store, 1, &newer_block1, &block2, 3, 1);

    let outcome = store.load();
    assert_eq!(
        outcome.status,
        SaveStatus::Ok,
        "a complete legacy head no torn full write can imitate stays intact"
    );
    assert_eq!(
        store.save_counter(),
        3,
        "the legacy generation the player just saved must be adopted"
    );
    assert_eq!(
        outcome.block1.money, newer_block1.money,
        "reverting to the older full slot would silently undo that session"
    );
    assert_eq!(
        &store.base_pokemon_storage[..],
        &storage_bytes[..],
        "storage still comes from the intact full slot, never the damaged tail"
    );
}

/// A counterpart slot that is `Error` only through a damaged save-block
/// sector still holds nine valid storage sectors; the accepted five-sector
/// generation borrows them rather than loading zeroed boxes.
#[test]
fn a_damaged_full_slot_still_donates_its_intact_storage() {
    let block2 = sample_block2();
    let older_block1 = SaveBlock1 {
        money: 111,
        ..sample_block1()
    };
    let newer_block1 = SaveBlock1 {
        money: 222,
        ..sample_block1()
    };
    let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];

    let mut store = SaveStore::new();
    // Slot 0: a full generation at counter 2 with one damaged
    // SaveBlock1 chunk (id 1, at position 1 for a rotation-zero write).
    write_full_slot(&mut store, 0, &older_block1, &block2, &storage_bytes, 2);
    store.corrupt_byte(0, 1, 4);
    // Slot 1: a five-sector generation over erased flash, so
    // it has no stale tail of its own to donate.
    write_legacy_slot(&mut store, 1, &newer_block1, &block2, 3);

    assert_eq!(
        store.scan_slot(0).integrity,
        SlotIntegrity::Error,
        "the damaged SaveBlock chunk must still disqualify the slot itself"
    );

    let outcome = store.load();
    assert_eq!(
        outcome.status,
        SaveStatus::Error,
        "a damaged slot is still reported, exactly as upstream does"
    );
    assert_eq!(store.save_counter(), 3);
    assert_eq!(
        outcome.block1.money, newer_block1.money,
        "progress still comes from the intact legacy generation"
    );
    assert_eq!(
        &store.base_pokemon_storage[..],
        &storage_bytes[..],
        "the damaged slot's nine intact storage sectors must be salvaged, not \
         zeroed and then overwritten by the next save"
    );
}

/// A footer id flipped from 1 to 5 leaves both copies checksum-valid under
/// one counter with every storage id present; a donor copy would splice
/// `SaveBlock1` bytes over the first box chunk.
#[test]
fn a_duplicated_storage_id_is_never_donated_as_storage() {
    const ROTATION: u16 = 10;
    let block2 = sample_block2();
    let older_block1 = SaveBlock1 {
        money: 111,
        ..sample_block1()
    };
    let newer_block1 = SaveBlock1 {
        money: 222,
        ..sample_block1()
    };
    let block2_bytes = block2.to_bytes();
    let block1_bytes = older_block1.to_bytes(block2.encryption_key);
    let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];

    let mut store = SaveStore::new();
    // Slot 0: a complete full generation at counter 10, rotation 10.
    for id in 0..NUM_SECTORS_PER_SLOT_U16 {
        let len = sector_payload_len(id).unwrap();
        let payload: &[u8] = if id == SECTOR_ID_SAVEBLOCK2 {
            &block2_bytes[..len]
        } else if id < SECTOR_ID_PKMN_STORAGE_START {
            let offset = usize::from(id - SECTOR_ID_SAVEBLOCK1_START) * SECTOR_DATA_SIZE;
            &block1_bytes[offset..offset + len]
        } else {
            let offset = usize::from(id - SECTOR_ID_PKMN_STORAGE_START) * SECTOR_DATA_SIZE;
            &storage_bytes[offset..offset + len]
        };
        let physical = usize::from((id + ROTATION) % NUM_SECTORS_PER_SLOT_U16);
        store.write_physical(0, physical, &Sector::write(id, payload, 10));
    }
    // One bit of id 1's footer flips, relabeling it id 5.
    let relabeled = usize::from((SECTOR_ID_SAVEBLOCK1_START + ROTATION) % NUM_SECTORS_PER_SLOT_U16);
    // The footer ends id (u16), checksum (u16), signature, counter (u32s).
    let id_offset = SECTOR_SIZE - 2 * size_of::<u32>() - 2 * size_of::<u16>();
    let mut bytes = *store.read_physical(0, relabeled).as_bytes();
    bytes[id_offset] ^= 0x04;
    store.write_physical(0, relabeled, &Sector::from_bytes(bytes));
    assert_eq!(
        store.read_physical(0, relabeled).id(),
        SECTOR_ID_PKMN_STORAGE_START
    );
    assert!(store
        .read_physical(0, relabeled)
        .is_valid(sector_payload_len(SECTOR_ID_PKMN_STORAGE_START).unwrap()));
    // Slot 1: a newer five-sector generation with no storage of its own.
    write_legacy_slot(&mut store, 1, &newer_block1, &block2, 11);

    assert!(
        store.scan_slot(0).storage_counter.is_none(),
        "a slot holding two copies of a storage id must not be offered as a donor"
    );
    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Error);
    assert_eq!(outcome.block1.money, newer_block1.money);
    let first_chunk = &store.base_pokemon_storage[..SECTOR_DATA_SIZE];
    assert_ne!(
        first_chunk,
        &block1_bytes[..SECTOR_DATA_SIZE],
        "SaveBlock1 bytes must never be loaded as a box chunk"
    );
}

/// One damaged footer counter (unchecksummed, `pokeemerald/src/save.c:674-685`)
/// in the only complete storage set must not cost the boxes when the other
/// eight chunks agree on their generation.
#[test]
fn one_damaged_storage_counter_still_leaves_a_complete_tail_donatable() {
    let block2 = sample_block2();
    let older_block1 = SaveBlock1 {
        money: 111,
        ..sample_block1()
    };
    let newer_block1 = SaveBlock1 {
        money: 222,
        ..sample_block1()
    };
    let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];

    let mut store = SaveStore::new();
    write_full_slot(&mut store, 1, &older_block1, &block2, &storage_bytes, 2);
    // The counter's high byte of storage id 9, at position 9: footer
    // only, so its checksum still holds.
    store.corrupt_byte(1, 9, SECTOR_SIZE - 1);
    assert!(store.read_physical(1, 9).is_valid(SECTOR_DATA_SIZE));
    write_legacy_slot_rotated(&mut store, 1, &older_block1, &block2, 3, 1);
    write_legacy_slot(&mut store, 0, &newer_block1, &block2, 4);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(store.save_counter(), 4);
    assert_eq!(outcome.block1.money, newer_block1.money);
    assert!(
        store.base_pokemon_storage[..] == storage_bytes[..],
        "an isolated counter outlier must not zero an otherwise complete storage set"
    );
}

/// A damaged storage counter that happens to equal the intact legacy
/// head's own counter must not withdraw the only complete storage tail.
#[test]
fn a_damaged_storage_counter_matching_the_legacy_head_still_donates() {
    let block2 = sample_block2();
    let older_block1 = SaveBlock1 {
        money: 111,
        ..sample_block1()
    };
    let newer_block1 = SaveBlock1 {
        money: 222,
        ..sample_block1()
    };
    let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];

    let mut store = SaveStore::new();
    write_full_slot(&mut store, 0, &older_block1, &block2, &storage_bytes, 28);
    // Flip bit 1 of id 9's footer counter: 28 -> 30. Footer only, so
    // the checksum still holds.
    let damaged = store.read_physical(0, 9);
    let offset = (9 - usize::from(SECTOR_ID_PKMN_STORAGE_START)) * SECTOR_DATA_SIZE;
    let rewritten = Sector::write(9, &storage_bytes[offset..offset + SECTOR_DATA_SIZE], 30);
    assert_eq!(damaged.data(), rewritten.data());
    store.write_physical(0, 9, &rewritten);
    assert!(store.read_physical(0, 9).is_valid(SECTOR_DATA_SIZE));
    write_legacy_slot(&mut store, 1, &older_block1, &block2, 29);
    write_legacy_slot(&mut store, 0, &newer_block1, &block2, 30);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(store.save_counter(), 30);
    assert_eq!(outcome.block1.money, newer_block1.money);
    assert!(
        store.base_pokemon_storage[..] == storage_bytes[..],
        "the only complete storage tail must survive a counter outlier equal to the head"
    );
}

/// One outlier is flash damage; two disagreeing footers are no longer a
/// set this scan can vouch for, so the donor rule stays strict there.
#[test]
fn two_damaged_storage_counters_withdraw_the_tail_as_a_donor() {
    let block2 = sample_block2();
    let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];

    let mut store = SaveStore::new();
    write_full_slot(&mut store, 1, &sample_block1(), &block2, &storage_bytes, 2);
    store.corrupt_byte(1, 9, SECTOR_SIZE - 1);
    write_legacy_slot_rotated(&mut store, 1, &sample_block1(), &block2, 3, 1);
    assert_eq!(store.scan_slot(1).storage_counter, Some(2));

    store.corrupt_byte(1, 11, SECTOR_SIZE - 1);
    assert!(
        store.scan_slot(1).storage_counter.is_none(),
        "two counter outliers must not be offered as a donor"
    );
}

/// A tail missing even one storage id is not a generation that ever
/// existed, so it is never donated: splicing its surviving chunks over
/// zeroes would hand the player a half-real PC. Slot 1's rotation-13
/// remnant loses id 5 to the legacy head that overwrote position 4.
#[test]
fn an_incomplete_stale_tail_is_never_donated_as_storage() {
    let block2 = sample_block2();
    let legacy_block1 = SaveBlock1 {
        money: 15,
        ..sample_block1()
    };

    let mut store = SaveStore::new();
    for _ in 0..12 {
        store.save(&sample_block1(), &block2);
    }
    store.base_pokemon_storage.fill(0x0D);
    store.save(&sample_block1(), &block2);
    assert_eq!(store.save_counter(), 13);
    assert_eq!(store.last_written_sector(), 13);

    // Slot 1 only: positions 5-13 keep ids 6-13 and 0, never id 5.
    write_legacy_slot(&mut store, 1, &legacy_block1, &block2, 15);
    assert!(
        store.scan_slot(1).storage_counter.is_none(),
        "a tail missing a storage id must not be offered as a donor"
    );
}

#[test]
fn stale_tail_counter_precedence_is_wraparound_aware() {
    assert!(older_generation_precedes(3, 7));
    assert!(!older_generation_precedes(7, 3));
    assert!(!older_generation_precedes(5, 5));
    // A legitimate stale tail an arbitrary number of generations behind
    // the legacy head, including across the u32 wrap.
    assert!(older_generation_precedes(u32::MAX, 1));
    // A tail that is actually the *newer* side of the same wrap must
    // never be read as older just because its raw value is smaller.
    assert!(!older_generation_precedes(0, u32::MAX - 1));
}

/// An identity legacy head over the rotation-13 remnant an imported
/// image leaves in slot 1: one bit of one stale tail sector's
/// unchecksummed counter footer is flash damage in data the slot never
/// loads as progress, and must not cost the player the legacy
/// generation they just saved.
#[test]
fn an_identity_legacy_head_survives_one_damaged_stale_tail_counter() {
    let block2 = sample_block2();
    let legacy_block1 = SaveBlock1 {
        money: 15,
        ..sample_block1()
    };

    let mut store = SaveStore::new();
    for _ in 0..12 {
        store.save(&sample_block1(), &block2);
    }
    store.save(&sample_block1(), &block2);
    store.save(&sample_block1(), &block2);
    assert_eq!(store.save_counter(), 14);

    write_legacy_slot(&mut store, 1, &legacy_block1, &block2, 15);
    // The counter's high byte of the tail sector at position 7: footer
    // only, so its checksum still holds.
    store.corrupt_byte(1, 7, SECTOR_SIZE - 1);
    assert!(store.read_physical(1, 7).is_valid(SECTOR_DATA_SIZE));

    let outcome = store.load();
    assert_eq!(
        outcome.status,
        SaveStatus::Ok,
        "one outlier counter in a stale tail must not reject the legacy head"
    );
    assert_eq!(store.save_counter(), 15);
    assert_eq!(
        outcome.block1.money, legacy_block1.money,
        "reverting to the older counter-14 slot would undo the legacy session"
    );
}

/// The same damage over a complete rotation-zero tail: every id then
/// validates, so without the consensus the slot falls through to the
/// full-format path under the tail's own older counter, and the other
/// slot's older legacy generation wins under a still-reported `Ok`.
#[test]
fn an_identity_legacy_head_over_a_complete_tail_keeps_its_counter_despite_one_damaged_tail_counter()
{
    let block2 = sample_block2();
    let counter_15_block1 = SaveBlock1 {
        money: 15,
        ..sample_block1()
    };
    let counter_16_block1 = SaveBlock1 {
        money: 16,
        ..sample_block1()
    };

    let mut store = SaveStore::new();
    for _ in 0..12 {
        store.save(&sample_block1(), &block2);
    }
    store.base_pokemon_storage.fill(0x0D);
    store.save(&sample_block1(), &block2);
    store.base_pokemon_storage.fill(0x0E);
    store.save(&sample_block1(), &block2);
    assert_eq!(store.last_written_sector(), 0);
    let counter_14_storage = store.base_pokemon_storage.clone();

    write_legacy_slot(&mut store, 1, &counter_15_block1, &block2, 15);
    write_legacy_slot(&mut store, 0, &counter_16_block1, &block2, 16);
    store.corrupt_byte(0, 9, SECTOR_SIZE - 1);
    assert!(store.read_physical(0, 9).is_valid(SECTOR_DATA_SIZE));

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(
        store.save_counter(),
        16,
        "the newer legacy head's own counter must be adopted, not the tail's"
    );
    assert_eq!(outcome.block1.money, counter_16_block1.money);
    assert_eq!(&store.base_pokemon_storage[..], &counter_14_storage[..]);
}

/// Ids 0 and 4 are zero-padded and the checksum sums words, so either
/// verifies at a storage id's length. Relabeled into the lost id 5 at
/// position 4, each would complete a coherent set from a generation still
/// holding ids 1-3; neither may donate.
#[test]
fn a_relabeled_short_save_block_sector_is_never_donated_as_storage() {
    for (head_rotation, head_id) in [(0u16, 4u16), (4, SECTOR_ID_SAVEBLOCK2)] {
        let block2 = sample_block2();
        let mut store = SaveStore::new();
        for _ in 0..12 {
            store.save(&sample_block1(), &block2);
        }
        store.base_pokemon_storage.fill(0x0D);
        store.save(&sample_block1(), &block2);
        assert_eq!(store.save_counter(), 13);
        assert_eq!(store.last_written_sector(), 13);

        write_legacy_slot_rotated(&mut store, 1, &sample_block1(), &block2, 15, head_rotation);
        assert_eq!(store.read_physical(1, 4).id(), head_id);
        relabel_footer_id(&mut store, 1, 4, SECTOR_ID_PKMN_STORAGE_START);
        assert!(store
            .read_physical(1, 4)
            .is_valid(sector_payload_len(SECTOR_ID_PKMN_STORAGE_START).unwrap()));

        assert!(
            store.scan_slot(1).storage_counter.is_none(),
            "id {head_id} relabeled into the lost id 5 must not complete a donor set"
        );
    }
}

/// An identity five-sector head over an older rotation-zero generation
/// whose stale id-5 footer flipped to 7: one damaged footer in data never
/// loaded as progress keeps the head, and the tail is never donated.
#[test]
fn an_identity_legacy_head_survives_one_flipped_stale_tail_id() {
    let block2 = sample_block2();
    let older_block1 = SaveBlock1 {
        money: 111,
        ..sample_block1()
    };
    let newer_block1 = SaveBlock1 {
        money: 222,
        ..sample_block1()
    };
    let stale_storage = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];
    let counterpart_storage = vec![0xCDu8; PKMN_STORAGE_PAYLOAD_LEN];

    let mut store = SaveStore::new();
    write_full_slot(&mut store, 1, &older_block1, &block2, &stale_storage, 1);
    write_full_slot(
        &mut store,
        0,
        &older_block1,
        &block2,
        &counterpart_storage,
        2,
    );
    write_legacy_slot(&mut store, 1, &newer_block1, &block2, 3);
    relabel_footer_id(&mut store, 1, 5, 7);
    assert!(store
        .read_physical(1, 5)
        .is_valid(sector_payload_len(7).unwrap()));

    let scan = store.scan_slot(1);
    assert_eq!(
        scan.integrity,
        SlotIntegrity::Ok,
        "one flipped stale-tail id must not reject the legacy head"
    );
    assert!(scan.legacy);
    assert!(
        scan.storage_counter.is_none(),
        "a tail missing id 5 and holding id 7 twice must not be donated"
    );
    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(store.save_counter(), 3);
    assert_eq!(
        outcome.block1.money, newer_block1.money,
        "reverting to the older counterpart would undo the legacy session"
    );
    assert_eq!(&store.base_pokemon_storage[..], &counterpart_storage[..]);
}

/// A legacy write (counter 14, rotation 4) torn after its first sector puts
/// id 0 at position 4, over a rotation-12 generation's storage id 6, and
/// that footer is damaged to id 6. No other sector carries counter 14, so
/// the relabel guard has no save-block generation to match.
#[test]
fn a_relabeled_sole_sector_of_a_torn_legacy_write_is_never_donated() {
    let mut store = SaveStore::new();
    write_full_slot(
        &mut store,
        0,
        &sample_block1(),
        &sample_block2(),
        &vec![0x0Fu8; PKMN_STORAGE_PAYLOAD_LEN],
        12,
    );
    let sectors: Vec<Sector> = (0..NUM_SECTORS_PER_SLOT)
        .map(|i| store.read_physical(0, i))
        .collect();
    for (id, sector) in sectors.iter().enumerate() {
        store.write_physical(0, (id + 12) % NUM_SECTORS_PER_SLOT, sector);
    }
    // Counter-12 id 0 (position 12) is checksum-damaged.
    store.corrupt_byte(0, 12, 0);
    assert_eq!(store.read_physical(0, 4).id(), 6);
    // Torn legacy write: only id 0 lands, at position (0 + 4) % 5.
    let block2 = SaveBlock2 {
        player_name: *b"TORNWR\xFF\0",
        ..sample_block2()
    };
    let block2_bytes = block2.to_bytes();
    store.write_physical(0, 4, &Sector::write(0, &block2_bytes, 14));
    relabel_footer_id(&mut store, 0, 4, 6);
    assert!(store
        .read_physical(0, 4)
        .is_valid(sector_payload_len(6).unwrap()));

    write_legacy_slot(&mut store, 1, &sample_block1(), &sample_block2(), 13);

    let scan = store.scan_slot(0);
    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Error);
    assert_eq!(store.save_counter(), 13);
    let chunk1 =
        &store.base_pokemon_storage[SECTOR_DATA_SIZE..SECTOR_DATA_SIZE + block2_bytes.len()];
    assert!(
        chunk1 != &block2_bytes[..],
        "SaveBlock2 bytes (trainer name) must never be loaded as a box chunk"
    );
    assert!(
        scan.storage_counter.is_none(),
        "a relabeled sole save-block sector must not complete a donor set"
    );
}

/// Full generations 13 (slot 1, rotation 13) and 14 (slot 0, rotation 0,
/// one storage chunk damaged), then legacy saves 15-18 at rotations 1-4.
/// Relabelling generation 17's `SaveBlock1` chunk (id 1, position 4) as
/// storage id 5 must not let slot 1 donate a storage set built from it.
#[test]
fn a_relabelled_save_block_chunk_after_repeated_legacy_writes_never_donates() {
    let block2 = sample_block2();
    let block1 = sample_block1();
    let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    let block2_bytes = block2.to_bytes();
    let payload = |id: u16| -> Vec<u8> {
        let len = sector_payload_len(id).unwrap();
        if id == SECTOR_ID_SAVEBLOCK2 {
            block2_bytes[..len].to_vec()
        } else if id < SECTOR_ID_PKMN_STORAGE_START {
            let o = usize::from(id - SECTOR_ID_SAVEBLOCK1_START) * SECTOR_DATA_SIZE;
            block1_bytes[o..o + len].to_vec()
        } else {
            let o = usize::from(id - SECTOR_ID_PKMN_STORAGE_START) * SECTOR_DATA_SIZE;
            storage_bytes[o..o + len].to_vec()
        }
    };
    let mut store = SaveStore::new();
    for (slot, counter, rotation) in [(1usize, 13u32, 13u16), (0, 14, 0)] {
        for id in 0..NUM_SECTORS_PER_SLOT_U16 {
            let pos = usize::from((id + rotation) % NUM_SECTORS_PER_SLOT_U16);
            store.write_physical(slot, pos, &Sector::write(id, &payload(id), counter));
        }
    }
    store.corrupt_byte(0, 7, 4);
    for (counter, rotation) in [(15u32, 1u16), (16, 2), (17, 3), (18, 4)] {
        let slot = (counter % 2) as usize;
        write_legacy_slot_rotated(&mut store, slot, &block1, &block2, counter, rotation);
    }
    // Generation 17 put id 1 at (1 + 3) % 5 = 4; relabel it as id 5.
    let mut bytes = *store.read_physical(1, 4).as_bytes();
    assert_eq!(
        u16::from_le_bytes([bytes[SECTOR_SIZE - 12], bytes[SECTOR_SIZE - 11]]),
        1
    );
    bytes[SECTOR_SIZE - 12..SECTOR_SIZE - 10].copy_from_slice(&5u16.to_le_bytes());
    store.write_physical(1, 4, &Sector::from_bytes(bytes));

    assert_eq!(
        store.scan_slot(1).storage_counter,
        None,
        "generation 17 lacks id 1, so its relabelled chunk must not complete a storage set"
    );
    let _ = store.load();
    assert_ne!(
        &store.base_pokemon_storage[..SECTOR_DATA_SIZE],
        &payload(1)[..SECTOR_DATA_SIZE],
        "SaveBlock1 bytes must never load as box storage"
    );
}

/// `LoadOutcome::storage_source` names where the boxes came from, so the
/// session layer can tell a lost donor from a different save.
#[test]
fn a_load_reports_whether_its_storage_came_from_a_legacy_donor() {
    let block1 = sample_block1();
    let block2 = sample_block2();
    let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];

    let mut own = SaveStore::new();
    write_full_slot(&mut own, 0, &block1, &block2, &storage_bytes, 2);
    assert_eq!(own.load().storage_source, StorageSource::Own);

    let mut merged = SaveStore::new();
    write_full_slot(&mut merged, 0, &block1, &block2, &storage_bytes, 2);
    write_legacy_slot(&mut merged, 1, &block1, &block2, 3);
    let outcome = merged.load();
    assert_eq!(outcome.storage_source, StorageSource::LegacyDonor);
    assert!(outcome.storage_source.is_legacy_head());

    let mut bare = SaveStore::new();
    write_legacy_slot(&mut bare, 1, &block1, &block2, 3);
    let outcome = bare.load();
    assert_eq!(outcome.storage_source, StorageSource::LegacyWithoutDonor);
    assert!(outcome.storage_source.is_legacy_head());
    assert!(!StorageSource::Own.is_legacy_head());
}
