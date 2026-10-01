//! Interrupted writes and relabeled footers that must never read as an intact slot.

use super::*;

/// A real interrupted 14-sector write at rotation 0, torn after exactly
/// its first 5 (logical-order) sectors, leaves precisely the same shape
/// behind as a legacy five-sector write over an imported image: ids 0-4
/// fresh under the new counter, and the slot's own predecessor
/// generation (2 counters and 2 rotations older) filling the rest.
/// Upstream reports that shape `SAVE_STATUS_ERROR`
/// (`pokeemerald/src/save.c:543-546`), so `scan_slot` must never accept
/// it as a legacy migration.
#[test]
fn an_interrupted_full_write_at_rotation_zero_is_never_mistaken_for_legacy_migration() {
    let block1 = sample_block1();
    let block2 = sample_block2();
    let block2_bytes = block2.to_bytes();
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    let storage_bytes = vec![0x0Fu8; PKMN_STORAGE_PAYLOAD_LEN];

    let mut store = SaveStore::new();
    // A genuine, complete rotation-12 generation at counter 12.
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
        let physical = usize::from((id + 12) % NUM_SECTORS_PER_SLOT_U16);
        store.write_physical(1, physical, &Sector::write(id, payload, 12));
    }

    // The next write to this same slot (counter 14, matching parity)
    // tears after its first 5 sectors at rotation 0, leaving the rest of
    // the rotation-12 generation above untouched underneath it.
    for id in 0..SECTOR_ID_PKMN_STORAGE_START {
        let len = sector_payload_len(id).unwrap();
        let payload: &[u8] = if id == SECTOR_ID_SAVEBLOCK2 {
            &block2_bytes[..len]
        } else {
            let offset = usize::from(id - SECTOR_ID_SAVEBLOCK1_START) * SECTOR_DATA_SIZE;
            &block1_bytes[offset..offset + len]
        };
        store.write_physical(1, usize::from(id), &Sector::write(id, payload, 14));
    }

    let outcome = store.load();
    assert_eq!(
        outcome.status,
        SaveStatus::Corrupt,
        "an interrupted 14-sector write must never be mistaken for a legacy migration"
    );
}

/// Upstream's counter lineage is not always clean: the footer counter
/// sits outside the checksum, and upstream adopts the last valid
/// sector's counter as-is. Here slot 1's id-0 footer flips from 11 to
/// 1035, so upstream's next save is 1036, at rotation 0 over slot 0's
/// generation 10, and it tears after 5 sectors. The tail is then 1026
/// counters behind the head rather than 2, but still sits at rotation
/// 12; a counter test alone would load that head with generation 11's
/// boxes as one `Ok` save.
#[test]
fn a_torn_rotation_zero_write_after_a_flipped_counter_is_never_legacy_migration() {
    let block2 = sample_block2();
    let storage_bytes = vec![0x0Fu8; PKMN_STORAGE_PAYLOAD_LEN];
    let lay_full_slot_at_rotation =
        |store: &mut SaveStore, slot: usize, money: u32, counter: u32, rotation: usize| {
            let block1 = SaveBlock1 {
                money,
                ..sample_block1()
            };
            write_full_slot(store, slot, &block1, &block2, &storage_bytes, counter);
            let sectors: Vec<Sector> = (0..NUM_SECTORS_PER_SLOT)
                .map(|i| store.read_physical(slot, i))
                .collect();
            for (id, sector) in sectors.iter().enumerate() {
                store.write_physical(slot, (id + rotation) % NUM_SECTORS_PER_SLOT, sector);
            }
        };

    let mut store = SaveStore::new();
    lay_full_slot_at_rotation(&mut store, 0, 10, 10, 12);
    lay_full_slot_at_rotation(&mut store, 1, 11, 11, 13);
    let counter_offset = SECTOR_SIZE - size_of::<u32>();
    let mut bytes = *store.read_physical(1, 13).as_bytes();
    bytes[counter_offset + 1] ^= 0x04;
    store.write_physical(1, 13, &Sector::from_bytes(bytes));
    assert_eq!(store.read_physical(1, 13).counter(), 1035);
    write_legacy_slot(
        &mut store,
        0,
        &SaveBlock1 {
            money: 1036,
            ..sample_block1()
        },
        &block2,
        1036,
    );

    assert_eq!(store.scan_slot(0).integrity, SlotIntegrity::Error);
    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Error);
    assert_eq!(store.save_counter(), 1035);
    assert_eq!(outcome.block1.money, 11);
}

/// The torn-write guard behind the stale-tail consensus: a rotation-0
/// full write torn after six sectors leaves its id 5 at position 5
/// under the new counter over eight sectors of the rotation-12
/// predecessor. Eight of nine tail counters agree, but the tail mixes
/// two layouts, so it is never one stale generation and the slot must
/// stay unaccepted, exactly as upstream's missing ids 6 and 7 make it.
#[test]
fn a_full_write_torn_past_the_head_is_never_a_stale_tail_with_one_outlier() {
    let block1 = sample_block1();
    let block2 = sample_block2();
    let block2_bytes = block2.to_bytes();
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    let storage_bytes = vec![0x0Fu8; PKMN_STORAGE_PAYLOAD_LEN];
    let payload_for = |id: u16| -> Vec<u8> {
        let len = sector_payload_len(id).unwrap();
        if id == SECTOR_ID_SAVEBLOCK2 {
            block2_bytes[..len].to_vec()
        } else if id < SECTOR_ID_PKMN_STORAGE_START {
            let offset = usize::from(id - SECTOR_ID_SAVEBLOCK1_START) * SECTOR_DATA_SIZE;
            block1_bytes[offset..offset + len].to_vec()
        } else {
            let offset = usize::from(id - SECTOR_ID_PKMN_STORAGE_START) * SECTOR_DATA_SIZE;
            storage_bytes[offset..offset + len].to_vec()
        }
    };

    let mut store = SaveStore::new();
    for id in 0..NUM_SECTORS_PER_SLOT_U16 {
        let physical = usize::from((id + 12) % NUM_SECTORS_PER_SLOT_U16);
        store.write_physical(1, physical, &Sector::write(id, &payload_for(id), 12));
    }
    for id in 0..=SECTOR_ID_PKMN_STORAGE_START {
        store.write_physical(1, usize::from(id), &Sector::write(id, &payload_for(id), 14));
    }

    assert_eq!(
        store.load().status,
        SaveStatus::Corrupt,
        "a torn full write must not pass as a legacy head over a stale tail"
    );
}

/// Why the legacy head, unlike its stale tail, stays unanimous: a
/// pre-#1227 write torn after four sectors over an imported rotation-10
/// generation leaves ids 4, 0, 1, 2, 3 in positions 0-4 -- every head id
/// once, all checksum-valid, four footers agreeing -- yet id 4 is
/// `SaveBlock1` from a different generation. A four-of-five consensus
/// would load that splice as `Ok`.
#[test]
fn a_torn_legacy_write_over_a_rotated_remnant_is_never_an_intact_head() {
    let block1 = sample_block1();
    let block2 = sample_block2();
    let storage_bytes = vec![0x0Fu8; PKMN_STORAGE_PAYLOAD_LEN];
    let block2_bytes = block2.to_bytes();
    let block1_bytes = block1.to_bytes(block2.encryption_key);

    let mut store = SaveStore::new();
    write_full_slot(&mut store, 1, &block1, &block2, &storage_bytes, 40);
    // Re-lay the same generation at rotation 10.
    let sectors: Vec<Sector> = (0..NUM_SECTORS_PER_SLOT)
        .map(|i| store.read_physical(1, i))
        .collect();
    for (id, sector) in sectors.iter().enumerate() {
        store.write_physical(1, (id + 10) % NUM_SECTORS_PER_SLOT, sector);
    }
    for id in 0..4u16 {
        let len = sector_payload_len(id).unwrap();
        let payload: &[u8] = if id == SECTOR_ID_SAVEBLOCK2 {
            &block2_bytes[..len]
        } else {
            let offset = usize::from(id - SECTOR_ID_SAVEBLOCK1_START) * SECTOR_DATA_SIZE;
            &block1_bytes[offset..offset + len]
        };
        let physical = usize::from((id + 1) % SECTOR_ID_PKMN_STORAGE_START);
        store.write_physical(1, physical, &Sector::write(id, payload, 1));
    }
    assert_eq!(store.read_physical(1, 0).id(), 4);
    assert_eq!(store.read_physical(1, 0).counter(), 40);

    assert_eq!(store.scan_slot(1).integrity, SlotIntegrity::Error);
}

/// Builds the donor-slot shape for the relabeled-head-sector tests: an
/// imported rotation-13 full generation (counter 13, storage `0xAB`) in
/// slot 1, overwritten by a pre-#1227 write at `legacy_rotation`
/// (counter 15), whose id-1 footer is then flipped into id 5. The
/// genuine id 5 sat at position 4 and is gone, so every storage id is
/// held once. Slot 0 is a newer legacy generation over erased flash
/// with no storage of its own. Returns the `SaveBlock1` bytes the
/// relabeled sector carries.
fn relabeled_head_sector_over_a_rotation_13_remnant(
    store: &mut SaveStore,
    legacy_rotation: u16,
) -> Vec<u8> {
    let block2 = sample_block2();
    let legacy_block1 = SaveBlock1 {
        money: 15,
        ..sample_block1()
    };
    let newer_block1 = SaveBlock1 {
        money: 16,
        ..sample_block1()
    };
    let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];
    write_full_slot(store, 1, &sample_block1(), &block2, &storage_bytes, 13);
    let sectors: Vec<Sector> = (0..NUM_SECTORS_PER_SLOT)
        .map(|i| store.read_physical(1, i))
        .collect();
    for (id, sector) in sectors.iter().enumerate() {
        store.write_physical(1, (id + 13) % NUM_SECTORS_PER_SLOT, sector);
    }
    write_legacy_slot_rotated(store, 1, &legacy_block1, &block2, 15, legacy_rotation);
    let relabeled = usize::from((SECTOR_ID_SAVEBLOCK1_START + legacy_rotation) % 5);
    let id_offset = SECTOR_SIZE - 2 * size_of::<u32>() - 2 * size_of::<u16>();
    let mut bytes = *store.read_physical(1, relabeled).as_bytes();
    bytes[id_offset] ^= 0x04;
    store.write_physical(1, relabeled, &Sector::from_bytes(bytes));
    assert_eq!(
        store.read_physical(1, relabeled).id(),
        SECTOR_ID_PKMN_STORAGE_START
    );
    assert!(store
        .read_physical(1, relabeled)
        .is_valid(sector_payload_len(SECTOR_ID_PKMN_STORAGE_START).unwrap()));
    write_legacy_slot(store, 0, &newer_block1, &block2, 16);
    legacy_block1.to_bytes(block2.encryption_key)[..SECTOR_DATA_SIZE].to_vec()
}

fn assert_relabeled_head_sector_is_never_donated(legacy_rotation: u16) {
    let mut store = SaveStore::new();
    let head_chunk = relabeled_head_sector_over_a_rotation_13_remnant(&mut store, legacy_rotation);
    assert_eq!(store.scan_slot(1).integrity, SlotIntegrity::Error);
    assert!(
        store.scan_slot(1).storage_counter.is_none(),
        "a legacy head sector relabeled into the missing id 5 must not complete a donor set \
         (legacy rotation {legacy_rotation})"
    );
    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Error);
    assert_eq!(store.save_counter(), 16);
    assert_ne!(
        &store.base_pokemon_storage[..SECTOR_DATA_SIZE],
        &head_chunk[..],
        "SaveBlock1 bytes must never be loaded as a box chunk (legacy rotation {legacy_rotation})"
    );
}

/// The legacy head's id 1 sits at position 1 and implies rotation 10,
/// while the remnant's ids 6-13 imply rotation 13.
#[test]
fn a_relabeled_legacy_head_sector_off_the_remnant_rotation_is_never_donated() {
    assert_relabeled_head_sector_is_never_donated(0);
}

/// At legacy rotation 3 the head's id 1 sits at position 4, exactly
/// where the remnant's own id 5 was, so the relabeled set is
/// rotation-coherent; only its outlier counter -- the head's own
/// generation -- gives it away.
#[test]
fn a_relabeled_legacy_head_sector_at_the_remnant_rotation_is_never_donated() {
    assert_relabeled_head_sector_is_never_donated(3);
}
