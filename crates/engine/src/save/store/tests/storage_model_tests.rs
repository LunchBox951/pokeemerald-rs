//! The store's typed `PokemonStorage`: persistence across rotation, byte
//! preservation, and new-game defaults.

use super::*;
use crate::save::storage::{PokemonStorage, StorageError};

fn stored_mon(species: u16) -> BoxPokemon {
    let mut parts = PokemonSubstructures::default();
    parts.growth[..2].copy_from_slice(&species.to_le_bytes());
    let mut mon = BoxPokemon::new(u32::from(species), 0x0A0B_0C0D);
    mon.set_substructures(&parts);
    mon.set_has_species(true);
    mon
}

#[test]
fn typed_storage_persists_through_every_rotation() {
    let (block1, block2) = (sample_block1(), sample_block2());
    let mut store = SaveStore::new();
    store.save(&block1, &block2);
    let mut storage = store.storage();

    for rotation in 0..=u16::try_from(NUM_SECTORS_PER_SLOT).unwrap() {
        storage.current_box = u8::try_from(rotation % 14).unwrap();
        storage
            .store_in_first_free_slot(usize::from(rotation % 14), stored_mon(rotation + 1))
            .unwrap();
        store.save_with_storage(&block1, &block2, &storage);

        let reloaded = store.load();
        assert_eq!(reloaded.status, SaveStatus::Ok, "rotation {rotation}");
        assert_eq!(reloaded.storage, storage, "rotation {rotation}");
        assert_eq!(store.storage(), storage, "rotation {rotation}");
    }
}

#[test]
fn imported_bytes_outside_the_model_survive_typed_saves() {
    let (block1, block2) = (sample_block1(), sample_block2());
    let mut store = SaveStore::new();
    store.save(&block1, &block2);
    store.base_pokemon_storage[1..4].copy_from_slice(&[0x11, 0x22, 0x33]);
    store.base_pokemon_storage[0] = 0xEE;
    store.base_pokemon_storage[0x83C2 + 13] = 0xEF;
    let imported = store.base_pokemon_storage.clone();

    store.save(&block1, &block2);
    assert_eq!(store.base_pokemon_storage, imported);
    assert_eq!(
        store.storage().current_box,
        0xEE,
        "invalid values are retained"
    );

    let mut storage = store.storage();
    storage.store_in_first_free_slot(0, stored_mon(25)).unwrap();
    store.save_with_storage(&block1, &block2, &storage);
    assert_eq!(&store.base_pokemon_storage[1..4], &[0x11, 0x22, 0x33]);

    let outcome = store.load();
    assert_eq!(outcome.storage, storage);
    assert_eq!(&store.base_pokemon_storage[1..4], &[0x11, 0x22, 0x33]);
}

#[test]
fn a_loaded_outcome_carries_the_adopted_generations_storage() {
    let mut store = SaveStore::new();
    store.save(&sample_block1(), &sample_block2());
    let outcome = store.load();
    assert_eq!(outcome.storage_source, StorageSource::Own);
    assert_eq!(
        outcome.storage.to_payload()[..],
        store.base_pokemon_storage[..]
    );
}

#[test]
fn clearing_the_base_for_a_new_game_writes_default_storage() {
    let (block1, block2) = (sample_block1(), sample_block2());
    let mut store = SaveStore::new();
    store.save(&block1, &block2);
    let mut storage = store.storage();
    storage
        .store_in_first_free_slot(3, stored_mon(150))
        .unwrap();
    store.save_with_storage(&block1, &block2, &storage);

    store.clear_base();
    store.save(&block1, &block2);
    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(outcome.storage, PokemonStorage::new_game());
    assert_eq!(outcome.storage.count_in_box(3), 0);
}

#[test]
fn deposit_and_withdraw_survive_save_and_reload() {
    let (mut block1, block2) = (sample_block1(), sample_block2());
    for (index, member) in block1.player_party.iter_mut().enumerate() {
        member.box_data = stored_mon(u16::try_from(index).unwrap() + 1);
        member.hp = 10;
    }
    block1.player_party_count = 6;
    let mut store = SaveStore::new();
    store.save(&block1, &block2);
    let mut storage = store.storage();
    let party_before = block1.player_party;

    let slot = storage.deposit_party_mon(&mut block1, 5, 7).unwrap();
    store.save_with_storage(&block1, &block2, &storage);
    let outcome = store.load();
    assert_eq!(outcome.storage, storage);
    assert_eq!(outcome.block1.player_party, block1.player_party);
    assert_eq!(outcome.block1.player_party_count, 5);

    let mut storage = outcome.storage;
    let mut block1 = outcome.block1;
    let record = party_before[5];
    storage
        .withdraw_to_party(
            &mut block1,
            crate::save::storage::BoxSlot { box_id: 7, slot },
            record,
        )
        .unwrap();
    assert_eq!(block1.player_party[5], record);
    assert_eq!(
        storage.deposit_party_mon(&mut block1, 9, 0),
        Err(StorageError::SlotOutOfRange)
    );
}
