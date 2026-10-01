//! Loading five-sector slots and migrating them on the next save.

use super::*;

#[test]
fn a_legacy_five_sector_slot_loads_ok_and_is_migrated_on_the_next_save() {
    let mut store = SaveStore::new();
    write_legacy_era_slot(&mut store, 1, 3, 1);

    let outcome = store.load();
    assert_eq!(
        outcome.status,
        SaveStatus::Ok,
        "a five-sector slot must still load as intact"
    );
    assert_eq!(outcome.block2, sample_block2());
    assert_eq!(outcome.block1.money, sample_block1().money);
    assert_eq!(store.save_counter(), 1);

    // The next save must rewrite a complete, upstream-shaped 14-sector
    // generation -- migrating the file out of the legacy format.
    store.save(&outcome.block1, &outcome.block2);
    let newest_slot = usize::try_from(store.save_counter() % 2).unwrap();
    for id in 0..NUM_SECTORS_PER_SLOT_U16 {
        let pos = store.find_sector_in_slot(newest_slot, id);
        assert!(store
            .read_physical(newest_slot, pos)
            .is_valid(sector_payload_len(id).unwrap()));
    }
    assert_eq!(store.load().status, SaveStatus::Ok);
}

#[test]
fn a_legacy_slot_at_every_old_rotation_loads_ok() {
    for rotation in 0..5u16 {
        let mut store = SaveStore::new();
        write_legacy_era_slot(&mut store, 0, rotation, 0);
        assert_eq!(
            store.load().status,
            SaveStatus::Ok,
            "legacy rotation {rotation} must load"
        );
    }
}

/// A signed sector anywhere in physical positions 5-13 proves this slot
/// was written by the 14-sector code, not the five-sector writer, even if that sector's own checksum is
/// damaged. Such a slot must never be silently "healed" into a legacy
/// read: a genuinely torn full-slot write must surface as damage the
/// other slot's generation recovers from, not as a false Ok.
#[test]
fn a_stray_signed_tail_sector_disqualifies_legacy_recovery() {
    let mut store = SaveStore::new();
    write_legacy_era_slot(&mut store, 0, 0, 0);

    // A checksum-damaged (but signature-valid) sector at a physical
    // position the legacy era never touched.
    let bogus = Sector::write(SECTOR_ID_PKMN_STORAGE_START, &[0xAB; 10], 1);
    store.write_physical(0, usize::from(SECTOR_ID_PKMN_STORAGE_START), &bogus);
    store.corrupt_byte(
        0,
        usize::from(SECTOR_ID_PKMN_STORAGE_START),
        SECTOR_DATA_SIZE,
    );

    let outcome = store.load();
    assert_eq!(
        outcome.status,
        SaveStatus::Corrupt,
        "a stray signed tail sector must never be accepted as an intact legacy slot"
    );
    assert_eq!(store.save_counter(), 0);
}

/// A slot with a genuinely torn full-14 write (some ids missing or
/// invalid, no legacy shape) must never be silently accepted; `load`
/// must also never mutate the underlying flash bytes while validating.
#[test]
fn a_torn_full_slot_write_is_reported_corrupt_and_load_never_mutates_flash() {
    let mut store = SaveStore::new();
    store.save(&sample_block1(), &sample_block2());
    store.save(&sample_block1(), &sample_block2());

    // Damage one PokemonStorage sector in the newest (otherwise intact)
    // slot: this must not be silently ignored as "unmodelled".
    let newest_slot = usize::try_from(store.save_counter() % 2).unwrap();
    let pos = store.find_sector_in_slot(newest_slot, SECTOR_ID_PKMN_STORAGE_START);
    store.corrupt_byte(newest_slot, pos, 0);
    let older_slot = 1 - newest_slot;
    let older_pos = store.find_sector_in_slot(older_slot, SECTOR_ID_SAVEBLOCK2);
    store.corrupt_byte(older_slot, older_pos, 0);

    let before = store.flash_image().to_vec();
    let outcome = store.load();
    assert_eq!(
        outcome.status,
        SaveStatus::Corrupt,
        "damage to a modelled sector must never be masked, even in the unmodelled range"
    );
    assert_eq!(
        store.flash_image(),
        &before[..],
        "load must never mutate the underlying flash image"
    );
}

/// A newer five-sector generation beside an older full slot (a build
/// downgrade or an assembled image): `load` adopts the newer progress and
/// carries the full slot's verified storage forward under `Ok`.
#[test]
fn a_newer_legacy_slot_merges_its_blocks_with_the_older_full_slots_storage() {
    let mut store = SaveStore::new();
    // Slot 1 (odd counter 3): a genuine full 14-sector generation with
    // distinctive storage bytes.
    let block1 = sample_block1();
    let block2 = sample_block2();
    let block2_bytes = block2.to_bytes();
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];
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
        store.write_physical(1, usize::from(id), &Sector::write(id, payload, 3));
    }
    // Slot 0 (even counter 4): a legacy-shaped (ids 0-4 only) generation
    // with a NEWER counter than slot 1's real generation.
    write_legacy_era_slot(&mut store, 0, 0, 4);

    let outcome = store.load();
    assert_eq!(
        outcome.status,
        SaveStatus::Ok,
        "both the legacy and the full slot are individually intact"
    );
    assert_eq!(
        store.save_counter(),
        4,
        "the newer legacy generation's counter is adopted, not the older full slot's"
    );
    assert_eq!(
        &store.base_pokemon_storage[..],
        &storage_bytes[..],
        "the full slot's real storage bytes must still be carried forward"
    );
}

/// Migrating a five-sector file must not cost the player the progress it
/// holds. When the legacy slot carries the newer counter, its
/// SaveBlock1/SaveBlock2 are the player's latest state and the older
/// full slot's only unique contribution is its opaque `PokemonStorage`:
/// keeping both loses nothing, while preferring the full slot wholesale
/// silently reverts the save to the older generation under status Ok.
#[test]
fn migrating_a_newer_legacy_slot_keeps_its_progress_and_the_full_slot_storage() {
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
    write_full_slot(&mut store, 1, &older_block1, &block2, &storage_bytes, 3);
    write_legacy_slot(&mut store, 0, &newer_block1, &block2, 4);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(
        outcome.block1.money, newer_block1.money,
        "the newer legacy generation's player progress must survive migration"
    );
    assert_eq!(
        &store.base_pokemon_storage[..],
        &storage_bytes[..],
        "the older full slot's opaque storage must still be carried forward"
    );
}

/// Counter parity binds a generation to a physical slot only through
/// `SaveStore::save`; an assembled image can break it. The merge must take
/// `PokemonStorage` from the full slot the scan found, not the parity slot.
#[test]
fn a_legacy_full_merge_takes_storage_from_the_scanned_full_slot() {
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

    // Both counters share the legacy head's parity, so resolving the
    // donor slot through `physical_slot_for_counter` lands back on the
    // legacy slot itself.
    for (legacy_slot, legacy_counter, full_counter) in [(0usize, 6u32, 4u32), (1, 7, 5)] {
        let full_slot = 1 - legacy_slot;
        let mut store = SaveStore::new();
        write_full_slot(
            &mut store,
            full_slot,
            &older_block1,
            &block2,
            &storage_bytes,
            full_counter,
        );
        write_legacy_slot(
            &mut store,
            legacy_slot,
            &newer_block1,
            &block2,
            legacy_counter,
        );

        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Ok);
        assert_eq!(
            store.save_counter(),
            legacy_counter,
            "the newer legacy generation's counter is still adopted"
        );
        assert_eq!(
            outcome.block1.money, newer_block1.money,
            "the newer legacy generation's progress must survive"
        );
        assert_eq!(
            &store.base_pokemon_storage[..],
            &storage_bytes[..],
            "storage must come from the full slot the scan found, not from \
             whichever slot the donor counter's parity happens to name"
        );
    }
}

/// A legacy five-sector write over an imported full-generation slot
/// leaves that generation's tail sectors, still checksum-valid under
/// their own older counter, in positions 5-13. `scan_slot` must accept
/// the newer legacy progress despite that signed tail, and storage must
/// still come from the newest complete generation, not the stale tail.
#[test]
fn a_legacy_era_save_over_an_imported_image_keeps_its_progress_and_storage() {
    let block2 = sample_block2();
    // A distinct SaveBlock2 (and so encryption key) for the legacy
    // write: reusing `block2` would hide a regression where a stale
    // tail sector for id 0 overwrites the legacy head's own, since
    // identical bytes make that overwrite unobservable.
    let legacy_block2 = SaveBlock2 {
        player_trainer_id: [0x11; TRAINER_ID_LENGTH],
        encryption_key: 0x1111_2222,
        ..sample_block2()
    };
    let counter_13_block1 = SaveBlock1 {
        money: 13,
        ..sample_block1()
    };
    let counter_14_block1 = SaveBlock1 {
        money: 14,
        ..sample_block1()
    };
    let counter_15_block1 = SaveBlock1 {
        money: 15,
        ..sample_block1()
    };

    let mut store = SaveStore::new();
    // Twelve throwaway generations rotate the store to the exact
    // physical layout that two real, ordinary rotated full saves at
    // counters 13 and 14 leave behind, both landing in slot 1 then slot
    // 0 by parity, exactly as `SaveStore::save` would in play.
    for _ in 0..12 {
        store.save(&sample_block1(), &block2);
    }
    assert_eq!(store.save_counter(), 12);

    store.base_pokemon_storage.fill(0x0D);
    store.save(&counter_13_block1, &block2);
    assert_eq!(store.save_counter(), 13);
    assert_eq!(store.last_written_sector(), 13);
    let counter_13_storage = store.base_pokemon_storage.clone();

    store.base_pokemon_storage.fill(0x0E);
    store.save(&counter_14_block1, &block2);
    assert_eq!(store.save_counter(), 14);
    assert_eq!(store.last_written_sector(), 0);
    let counter_14_storage = store.base_pokemon_storage.clone();
    assert_ne!(&counter_13_storage[..], &counter_14_storage[..]);

    // The legacy writer touches only physical positions 0-4, leaving the
    // signed counter-13 tail from the imported full generation (slot 1,
    // physical positions 5-13) in place underneath it. That tail
    // includes id 0 (SaveBlock2) at physical position 13.
    write_legacy_slot(&mut store, 1, &counter_15_block1, &legacy_block2, 15);

    let outcome = store.load();
    assert_eq!(
        outcome.status,
        SaveStatus::Ok,
        "a stale, strictly-older signed tail must not disqualify the newer legacy generation"
    );
    assert_eq!(
        store.save_counter(),
        15,
        "the legacy generation's own counter must be adopted, not the stale tail's"
    );
    assert_eq!(
        outcome.block1.money, counter_15_block1.money,
        "the counter-15 legacy generation's progress must win"
    );
    assert_eq!(
        outcome.block2, legacy_block2,
        "the legacy generation's own SaveBlock2 must win, not the stale tail's id-0 remnant"
    );
    assert_eq!(
        store.last_written_sector(),
        0,
        "rotation must be recovered from the legacy head's own id 0, not the stale tail's"
    );
    assert_eq!(
        &store.base_pokemon_storage[..],
        &counter_14_storage[..],
        "the newest complete storage generation must win, not the stale incomplete tail"
    );
}

/// Both slots hold five-sector legacy heads over still-signed full
/// tails, the shape a five-sector writer leaves after two saves over an
/// imported image. With no full-format slot to donate from, `load` must
/// take storage from the newest complete verified tail rather than zero
/// it.
#[test]
fn two_legacy_slots_over_an_imported_image_keep_the_newest_verified_tail() {
    let block2 = sample_block2();
    let legacy_block2 = SaveBlock2 {
        player_trainer_id: [0x11; TRAINER_ID_LENGTH],
        encryption_key: 0x1111_2222,
        ..sample_block2()
    };
    let counter_15_block1 = SaveBlock1 {
        money: 15,
        ..sample_block1()
    };
    let counter_16_block1 = SaveBlock1 {
        money: 16,
        ..sample_block1()
    };

    let mut store = SaveStore::new();
    // Rotate to the layout two ordinary full saves at counters 13 and 14
    // leave: counter 13 in slot 1 at rotation 13, counter 14 in slot 0
    // at rotation 0 -- exactly what an imported cartridge image holds.
    for _ in 0..12 {
        store.save(&sample_block1(), &block2);
    }
    store.base_pokemon_storage.fill(0x0D);
    store.save(&sample_block1(), &block2);
    assert_eq!(store.save_counter(), 13);
    store.base_pokemon_storage.fill(0x0E);
    store.save(&sample_block1(), &block2);
    assert_eq!(store.save_counter(), 14);
    assert_eq!(store.last_written_sector(), 0);
    let counter_14_storage = store.base_pokemon_storage.clone();

    // Two five-sector saves, into slot 1 then slot 0 by parity. Slot 0's
    // counter-14 tail keeps ids 5-13 at positions 5-13 (rotation zero),
    // a complete storage generation; slot 1's counter-13 tail is the
    // rotation-13 remnant, missing id 5, and so cannot donate.
    write_legacy_slot(&mut store, 1, &counter_15_block1, &legacy_block2, 15);
    write_legacy_slot(&mut store, 0, &counter_16_block1, &legacy_block2, 16);

    let outcome = store.load();
    assert_eq!(
        outcome.status,
        SaveStatus::Ok,
        "each slot is an intact legacy generation over a strictly-older signed tail"
    );
    assert_eq!(
        store.save_counter(),
        16,
        "the newer legacy head's own counter must be adopted"
    );
    assert_eq!(
        outcome.block1.money, counter_16_block1.money,
        "the newer legacy generation's progress must win"
    );
    assert_eq!(
        outcome.block2, legacy_block2,
        "progress must come from the legacy head, never from a tail remnant"
    );
    assert_eq!(
        store.last_written_sector(),
        0,
        "rotation must still be recovered from the legacy head's own id 0"
    );
    assert_eq!(
        &store.base_pokemon_storage[..],
        &counter_14_storage[..],
        "the boxed Pokemon still present in the adopted slot's verified tail must \
         survive, not be zeroed and then overwritten by the next save"
    );
}

/// The same loss with only one accepted slot: a legacy head over a
/// complete, strictly-older signed tail while the other slot is fully
/// erased. `resolve` reaches this through its single-slot arms rather
/// than [`SaveStore::resolve_both_ok`], so the donor must be offered
/// there too.
#[test]
fn a_lone_legacy_slot_still_donates_its_own_verified_storage_tail() {
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
    // Slot 1 held a full generation at counter 2; a five-sector write at
    // counter 3 replaced positions 0-4 only. Slot 0 is untouched flash.
    write_full_slot(&mut store, 1, &older_block1, &block2, &storage_bytes, 2);
    write_legacy_slot(&mut store, 1, &newer_block1, &block2, 3);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(store.save_counter(), 3);
    assert_eq!(
        outcome.block1.money, newer_block1.money,
        "progress still comes from the legacy head, never the stale tail"
    );
    assert_eq!(
        &store.base_pokemon_storage[..],
        &storage_bytes[..],
        "the tail's verified storage must survive even with no second slot"
    );
}
