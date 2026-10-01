//! Full-image geometry, save/load round trips, and sector rotation.

use super::*;

#[test]
fn flash_image_keeps_full_physical_geometry() {
    assert_eq!(NUM_SECTORS_PER_SLOT, 14);
    assert_eq!(NUM_SECTORS, 32);
    assert_eq!(FLASH_IMAGE_LEN, 131_072);
    assert_eq!(SaveStore::physical_offset(0, 0), 0);
    assert_eq!(
        SaveStore::physical_offset(1, 0),
        NUM_SECTORS_PER_SLOT * SECTOR_SIZE,
        "slot 1 sits at upstream's 14-sector offset"
    );
}

#[test]
fn counter_comparison_is_wraparound_aware() {
    assert!(second_counter_is_newer(3, 7));
    assert!(!second_counter_is_newer(7, 3));
    assert!(!second_counter_is_newer(5, 5));
    assert!(second_counter_is_newer(u32::MAX, 0));
    assert!(!second_counter_is_newer(0, u32::MAX));
}

#[test]
fn fresh_store_loads_as_empty() {
    let mut store = SaveStore::new();
    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Empty);
    assert_eq!(store.save_counter(), 0);
    assert_eq!(store.last_written_sector(), 0);
}

/// What a writer without the generation stamp does on a save: patch the
/// position over the loaded bytes, leave every other byte, and advance the
/// counter.
fn write_without_a_generation_stamp(store: &mut SaveStore, pos: Coords16) {
    let mut block1_bytes = store.base_block1.clone();
    block1_bytes[..4].copy_from_slice(&[
        pos.x.to_le_bytes()[0],
        pos.x.to_le_bytes()[1],
        pos.y.to_le_bytes()[0],
        pos.y.to_le_bytes()[1],
    ]);
    let block2_bytes = store.base_block2.clone();
    let storage_bytes = store.base_pokemon_storage.clone();
    let new_last_written_sector = (store.last_written_sector + 1) % NUM_SECTORS_PER_SLOT_U16;
    let new_save_counter = store.save_counter.wrapping_add(1);
    let slot = physical_slot_for_counter(new_save_counter);
    for sector_id in 0..NUM_SECTORS_PER_SLOT_U16 {
        let data: &[u8] = if sector_id == SECTOR_ID_SAVEBLOCK2 {
            &block2_bytes[..]
        } else if sector_id < SECTOR_ID_PKMN_STORAGE_START {
            chunk_of(
                &block1_bytes[..],
                (sector_id - SECTOR_ID_SAVEBLOCK1_START) as usize,
            )
        } else {
            chunk_of(
                &storage_bytes[..],
                (sector_id - SECTOR_ID_PKMN_STORAGE_START) as usize,
            )
        };
        let physical = ((sector_id + new_last_written_sector) % NUM_SECTORS_PER_SLOT_U16) as usize;
        store.write_physical(
            slot,
            physical,
            &Sector::write(sector_id, data, new_save_counter),
        );
    }
    store.last_written_sector = new_last_written_sector;
    store.save_counter = new_save_counter;
    store.base_block1 = block1_bytes;
}

/// A save from this build, reopened and saved by an unstamped writer that walks
/// off the position and back to it, keeps the marker, coordinates and
/// elevation pair, all still consistent; only the counter stamp differs, so
/// the pair must not be trusted.
#[test]
fn an_old_build_save_back_at_the_same_position_leaves_the_pair_untrusted() {
    let mut store = SaveStore::new();
    let mut block1 = sample_block1();
    block1.player_object_event.active = true;
    block1.player_object_event.current_elevation = 0;
    block1.player_object_event.previous_elevation = 3;
    let block2 = sample_block2();
    store.save(&block1, &block2);
    assert!(store.load().block1.player_object_event.active, "fresh save");

    write_without_a_generation_stamp(&mut store, Coords16 { x: 11, y: -20 });
    write_without_a_generation_stamp(&mut store, block1.pos);
    let reopened = store.load().block1;
    assert_eq!(reopened.pos, block1.pos);
    assert!(
        !reopened.player_object_event.active,
        "a pair stamped by an earlier save must not be trusted"
    );
}

#[test]
fn save_then_load_round_trips_identical_state() {
    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = sample_block2();

    store.save(&block1, &block2);
    let outcome = store.load();

    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(outcome.block2, block2);
    assert_eq!(outcome.block1.pos, block1.pos);
    assert_eq!(outcome.block1.location, block1.location);
    assert_eq!(outcome.block1.continue_game_warp, block1.continue_game_warp);
    assert_eq!(outcome.block1.last_heal_location, block1.last_heal_location);
    assert_eq!(outcome.block1.player_party_count, block1.player_party_count);
    assert_eq!(outcome.block1.player_party, block1.player_party);
    assert_eq!(outcome.block1.money, block1.money);
    assert_eq!(outcome.block1.bag, block1.bag);
    assert_eq!(outcome.block1.event_data.flag_get(42), Ok(true));
    assert_eq!(
        outcome
            .block1
            .event_data
            .var_get(crate::event_data::VARS_START),
        Ok(777)
    );
}

#[test]
fn a_rotation_preserves_deferred_bytes_the_model_does_not_own() {
    const UNMODELLED_BLOCK2_OFFSET: usize = 0x10;
    const UNMODELLED_BLOCK1_CHUNK0_OFFSET: usize = 0x100;
    const UNMODELLED_BLOCK1_CHUNK2_OFFSET: usize = 0x2000;

    let block1 = sample_block1();
    let block2 = sample_block2();
    let mut store = SaveStore::new();
    store.save(&block1, &block2);

    let mut payload = block2.to_bytes();
    payload[UNMODELLED_BLOCK2_OFFSET] = 0x5A;
    let sector = Sector::write(SECTOR_ID_SAVEBLOCK2, &payload, 1);
    let pos = store.find_sector_in_slot(1, SECTOR_ID_SAVEBLOCK2);
    store.write_physical(1, pos, &sector);

    let block1_bytes = block1.to_bytes(block2.encryption_key);
    for (offset, value) in [
        (UNMODELLED_BLOCK1_CHUNK0_OFFSET, 0xA5u8),
        (UNMODELLED_BLOCK1_CHUNK2_OFFSET, 0xC3u8),
    ] {
        let chunk_num = offset / SECTOR_DATA_SIZE;
        let id = SECTOR_ID_SAVEBLOCK1_START + u16::try_from(chunk_num).unwrap();
        let mut payload = chunk_of(&block1_bytes, chunk_num).to_vec();
        payload[offset % SECTOR_DATA_SIZE] = value;
        let sector = Sector::write(id, &payload, 1);
        let pos = store.find_sector_in_slot(1, id);
        store.write_physical(1, pos, &sector);
    }

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    store.save(&outcome.block1, &outcome.block2);

    let reloaded = store.load();
    assert_eq!(reloaded.status, SaveStatus::Ok);
    assert_eq!(store.save_counter(), 2, "the rotated slot is the winner");
    assert_eq!(store.base_block2[UNMODELLED_BLOCK2_OFFSET], 0x5A);
    assert_eq!(store.base_block1[UNMODELLED_BLOCK1_CHUNK0_OFFSET], 0xA5);
    assert_eq!(store.base_block1[UNMODELLED_BLOCK1_CHUNK2_OFFSET], 0xC3);
    assert_eq!(reloaded.block2, block2);
    assert_eq!(reloaded.block1.money, block1.money);
    assert_eq!(reloaded.block1.bag, block1.bag);
}

#[test]
fn clear_base_drops_the_loaded_deferred_bytes_from_the_next_save() {
    const UNMODELLED_BLOCK2_OFFSET: usize = 0x10;

    let block2 = sample_block2();
    let mut store = SaveStore::new();
    store.save(&sample_block1(), &block2);

    let mut payload = block2.to_bytes();
    payload[UNMODELLED_BLOCK2_OFFSET] = 0x5A;
    let sector = Sector::write(SECTOR_ID_SAVEBLOCK2, &payload, 1);
    let pos = store.find_sector_in_slot(1, SECTOR_ID_SAVEBLOCK2);
    store.write_physical(1, pos, &sector);

    assert_eq!(store.load().status, SaveStatus::Ok);
    assert_eq!(
        store.base_block2[UNMODELLED_BLOCK2_OFFSET], 0x5A,
        "the deferred byte is retained before the clear"
    );

    store.clear_base();
    store.save(&SaveBlock1::default(), &SaveBlock2::default());

    assert_eq!(store.load().status, SaveStatus::Ok);
    assert_eq!(
        store.base_block2[UNMODELLED_BLOCK2_OFFSET], 0,
        "a cleared base writes zeroed deferred bytes"
    );
}

#[test]
fn repeated_saves_round_trip_the_latest_state() {
    let mut store = SaveStore::new();
    let block2 = sample_block2();

    for i in 0..5u16 {
        let mut block1 = sample_block1();
        block1.pos = Coords16 {
            x: i.cast_signed(),
            y: 0,
        };
        store.save(&block1, &block2);
    }

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(outcome.block1.pos, Coords16 { x: 4, y: 0 });
}

#[test]
fn sector_rotation_advances_and_wraps_each_save() {
    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = sample_block2();

    assert_eq!(store.last_written_sector(), 0);
    for expected in 1..=(NUM_SECTORS_PER_SLOT_U16 * 2) {
        store.save(&block1, &block2);
        assert_eq!(
            store.last_written_sector(),
            expected % NUM_SECTORS_PER_SLOT_U16
        );
    }
    assert_eq!(
        store.save_counter(),
        u32::from(NUM_SECTORS_PER_SLOT_U16) * 2
    );
}

#[test]
fn each_save_alternates_the_physical_slot() {
    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = sample_block2();

    store.save(&block1, &block2);
    assert_eq!(store.save_counter() % 2, 1);
    let first_sector = store.read_physical(1, 0);
    assert_eq!(first_sector.signature(), SECTOR_SIGNATURE);
    assert_ne!(store.read_physical(0, 0).signature(), SECTOR_SIGNATURE);

    store.save(&block1, &block2);
    assert_eq!(store.save_counter() % 2, 0);
    assert_eq!(
        store.read_physical(0, 0).signature(),
        SECTOR_SIGNATURE,
        "second save must land in the other physical slot"
    );
}

#[test]
fn save_then_load_round_trips_male_gender() {
    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = SaveBlock2 {
        player_gender: PlayerGender::Male,
        ..sample_block2()
    };

    store.save(&block1, &block2);
    let outcome = store.load();

    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(outcome.block2, block2);
    assert_eq!(outcome.block1.money, block1.money);
}

#[test]
fn one_untouched_empty_slot_is_not_corrupt() {
    let mut store = SaveStore::new();
    let block1 = sample_block1();
    let block2 = sample_block2();

    store.save(&block1, &block2);

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(outcome.block2, block2);
}

#[test]
fn chunk_len_matches_the_saveblock_chunk_macro_semantics() {
    assert_eq!(
        (0..SAVE_BLOCK1_CHUNKS)
            .map(|chunk| chunk_len(SaveBlock1::PAYLOAD_LEN, chunk))
            .collect::<Vec<_>>(),
        [3968, 3968, 3968, 3848]
    );

    // sizeof(struct PokemonStorage) == 0x83D0 (see PKMN_STORAGE_PAYLOAD_LEN's
    // doc comment): eight full 3968-byte chunks plus a 2000-byte remainder.
    assert_eq!(
        (0..PKMN_STORAGE_CHUNKS)
            .map(|chunk| chunk_len(PKMN_STORAGE_PAYLOAD_LEN, chunk))
            .collect::<Vec<_>>(),
        [3968, 3968, 3968, 3968, 3968, 3968, 3968, 3968, 2000]
    );

    let two_chunk_payload_len = SECTOR_DATA_SIZE + 1;
    assert_eq!(chunk_len(two_chunk_payload_len, 0), SECTOR_DATA_SIZE);
    assert_eq!(chunk_len(two_chunk_payload_len, 1), 1);
    assert_eq!(chunk_len(two_chunk_payload_len, 2), 0);

    let short_payload_len = 10;
    assert_eq!(chunk_len(short_payload_len, 0), short_payload_len);
    assert_eq!(chunk_len(short_payload_len, 1), 0);
}

/// Upstream writes all 14 sectors of a slot at `sectorId + gLastWrittenSector`
/// modulo 14 (`pokeemerald/src/save.c:138-173`, `HandleWriteSector`), so
/// every rotation is a legitimate image to load.
#[test]
fn load_accepts_an_upstream_slot_at_every_rotation() {
    // Upstream writes generation N into slot N % NUM_SAVE_SLOTS.
    const COUNTER: u32 = 1;
    const SLOT_OF_COUNTER: usize = (COUNTER % 2) as usize;

    let block2 = sample_block2();
    let block1 = sample_block1();
    let block2_bytes = block2.to_bytes();
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    let storage_bytes: Vec<u8> = (0..PKMN_STORAGE_PAYLOAD_LEN)
        .map(|i| u8::try_from(i % 251).expect("modulus fits in u8"))
        .collect();

    for rotation in 0..NUM_SECTORS_PER_SLOT_U16 {
        let mut image = vec![ERASED_FLASH_BYTE; FLASH_IMAGE_LEN];
        for id in 0..NUM_SECTORS_PER_SLOT_U16 {
            let len = sector_payload_len(id).expect("every id 0-13 is modelled");
            let payload: &[u8] = if id == SECTOR_ID_SAVEBLOCK2 {
                &block2_bytes[..len]
            } else if id < SECTOR_ID_PKMN_STORAGE_START {
                let offset = usize::from(id - SECTOR_ID_SAVEBLOCK1_START) * SECTOR_DATA_SIZE;
                &block1_bytes[offset..offset + len]
            } else {
                let offset = usize::from(id - SECTOR_ID_PKMN_STORAGE_START) * SECTOR_DATA_SIZE;
                &storage_bytes[offset..offset + len]
            };
            let physical = usize::from((id + rotation) % NUM_SECTORS_PER_SLOT_U16);
            let start = SaveStore::physical_offset(SLOT_OF_COUNTER, physical);
            image[start..start + SECTOR_SIZE]
                .copy_from_slice(Sector::write(id, payload, COUNTER).as_bytes());
        }

        let mut store = SaveStore::from_flash_image(&image).expect("exact-length image");
        let outcome = store.load();
        assert_eq!(
            outcome.status,
            SaveStatus::Ok,
            "upstream slot written at rotation {rotation} must load"
        );
        assert_eq!(outcome.block2, block2, "rotation {rotation}");
        assert_eq!(outcome.block1.pos, block1.pos, "rotation {rotation}");
        assert_eq!(outcome.block1.money, block1.money, "rotation {rotation}");
        assert_eq!(
            &store.base_pokemon_storage[..],
            &storage_bytes[..],
            "rotation {rotation} must retain the opaque PokemonStorage bytes"
        );
    }
}

#[test]
fn opaque_pokemon_storage_round_trips_and_is_rewritten_every_save() {
    let block1 = sample_block1();
    let block2 = sample_block2();
    let mut store = SaveStore::new();
    store.save(&block1, &block2);

    // Imitate importing an upstream image with real box contents by
    // directly patching the retained opaque base, then re-saving so the
    // patched bytes get written through the normal save path.
    let mut patched_storage = store.base_pokemon_storage.clone();
    for (i, byte) in patched_storage.iter_mut().enumerate() {
        *byte = u8::try_from(i % 199).expect("modulus fits in u8");
    }
    store.base_pokemon_storage = patched_storage.clone();
    store.save(&block1, &block2);

    let newest_slot = usize::try_from(store.save_counter() % 2).unwrap();
    for id in SECTOR_ID_PKMN_STORAGE_START..NUM_SECTORS_PER_SLOT_U16 {
        let pos = store.find_sector_in_slot(newest_slot, id);
        let sector = store.read_physical(newest_slot, pos);
        let len = sector_payload_len(id).unwrap();
        assert!(
            sector.is_valid(len),
            "storage sector {id} must be rechecksummed on every save"
        );
        let offset = usize::from(id - SECTOR_ID_PKMN_STORAGE_START) * SECTOR_DATA_SIZE;
        assert_eq!(
            &sector.data()[..len],
            &patched_storage[offset..offset + len]
        );
    }

    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(&store.base_pokemon_storage[..], &patched_storage[..]);
}

#[test]
fn a_freshly_originated_save_writes_valid_placeholder_storage_sectors_immediately() {
    let mut store = SaveStore::new();
    assert_eq!(
        store.load().status,
        SaveStatus::Empty,
        "an unsaved store has no signed sectors yet"
    );

    store.save(&sample_block1(), &sample_block2());
    let newest_slot = usize::try_from(store.save_counter() % 2).unwrap();
    for id in 0..NUM_SECTORS_PER_SLOT_U16 {
        let pos = store.find_sector_in_slot(newest_slot, id);
        let sector = store.read_physical(newest_slot, pos);
        let len = sector_payload_len(id).unwrap();
        assert!(
            sector.is_valid(len),
            "the first save must satisfy upstream's all-14-valid invariant (id {id})"
        );
    }
    assert_eq!(
        store.load().status,
        SaveStatus::Ok,
        "the first save alone must be a complete, loadable 14-sector generation"
    );
}
