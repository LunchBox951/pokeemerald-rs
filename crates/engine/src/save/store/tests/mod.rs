//! Unit tests for [`SaveStore`], split by seam; this module holds the
//! fixtures every seam shares.

use super::*;
use crate::save::block::{Coords16, PlayerGender, WarpData, TRAINER_ID_LENGTH};
use crate::save::sector::SECTOR_SIGNATURE;
use crate::save::{BoxPokemon, ItemSlot, Pokemon, PokemonSubstructures};

mod corruption_tests;
mod legacy_migration_tests;
mod round_trip_tests;
mod storage_donor_tests;
mod torn_write_tests;
mod unusable_tail_tests;

fn sample_block2() -> SaveBlock2 {
    SaveBlock2 {
        player_name: *b"RUSTY\xFF\0\0",
        player_gender: PlayerGender::Female,
        player_trainer_id: [1, 2, 3, 4],
        special_save_warp_flags: 0,
        encryption_key: 0xA1B2_C3D4,
        options_text_speed: 1,
        options_window_frame_type: 5,
    }
}

fn sample_block1() -> SaveBlock1 {
    let mut block = SaveBlock1 {
        pos: Coords16 { x: 10, y: -20 },
        location: WarpData {
            map_group: 1,
            map_num: 2,
            warp_id: 3,
            x: 4,
            y: 5,
        },
        continue_game_warp: WarpData {
            map_group: -4,
            map_num: 5,
            warp_id: -6,
            x: -700,
            y: 800,
        },
        last_heal_location: WarpData {
            map_group: 7,
            map_num: -8,
            warp_id: 9,
            x: 1_000,
            y: -1_100,
        },
        player_party_count: 6,
        player_party: std::array::from_fn(sample_pokemon),
        money: 987_654,
        ..SaveBlock1::default()
    };
    block.bag.items[0] = ItemSlot {
        item_id: 1,
        quantity: 99,
    };
    block.bag.key_items[29] = ItemSlot {
        item_id: 2,
        quantity: 1,
    };
    block.bag.poke_balls[15] = ItemSlot {
        item_id: 3,
        quantity: 42,
    };
    block.bag.tms_hms[63] = ItemSlot {
        item_id: 4,
        quantity: 7,
    };
    block.bag.berries[45] = ItemSlot {
        item_id: 5,
        quantity: 88,
    };
    block.event_data.flag_set(42).unwrap();
    block
        .event_data
        .var_set(crate::event_data::VARS_START, 777)
        .unwrap();
    block
}

fn sample_pokemon(index: usize) -> Pokemon {
    let byte = u8::try_from(index).unwrap();
    let mut box_data = BoxPokemon::new(
        24 + u32::try_from(index).unwrap(),
        0xDEAD_0000 | u32::try_from(index).unwrap(),
    );
    box_data.set_substructures(&PokemonSubstructures {
        growth: [byte; 12],
        attacks: [byte.wrapping_add(1); 12],
        evs_and_condition: [byte.wrapping_add(2); 12],
        misc: [byte.wrapping_add(3); 12],
    });
    Pokemon {
        box_data,
        status: 0x1000_0000 + u32::try_from(index).unwrap(),
        level: 20 + byte,
        mail: 30 + byte,
        hp: 40 + u16::from(byte),
        max_hp: 50 + u16::from(byte),
        attack: 60 + u16::from(byte),
        defense: 70 + u16::from(byte),
        speed: 80 + u16::from(byte),
        special_attack: 90 + u16::from(byte),
        special_defense: 100 + u16::from(byte),
    }
}

/// Builds a slot exactly as the five-sector writer did: ids 0-4
/// only, placed at `(id + rotation) % 5`, physical positions 5-13 left
/// erased.
fn write_legacy_era_slot(store: &mut SaveStore, slot: usize, rotation: u16, counter: u32) {
    const LEGACY_SECTORS_PER_SLOT: u16 = 5;

    let block1 = sample_block1();
    let block2 = sample_block2();
    let block2_bytes = block2.to_bytes();
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    for id in 0..LEGACY_SECTORS_PER_SLOT {
        let len = sector_payload_len(id).expect("ids 0-4 are always modelled");
        let payload: &[u8] = if id == SECTOR_ID_SAVEBLOCK2 {
            &block2_bytes[..len]
        } else {
            let offset = usize::from(id - SECTOR_ID_SAVEBLOCK1_START) * SECTOR_DATA_SIZE;
            &block1_bytes[offset..offset + len]
        };
        let physical = usize::from((id + rotation) % LEGACY_SECTORS_PER_SLOT);
        store.write_physical(slot, physical, &Sector::write(id, payload, counter));
    }
}

/// Writes a complete 14-sector generation (ids 0-13, no rotation) into
/// `slot` under one counter, with the given opaque storage bytes.
fn write_full_slot(
    store: &mut SaveStore,
    slot: usize,
    block1: &SaveBlock1,
    block2: &SaveBlock2,
    storage: &[u8],
    counter: u32,
) {
    let block2_bytes = block2.to_bytes();
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    for id in 0..NUM_SECTORS_PER_SLOT_U16 {
        let len = sector_payload_len(id).unwrap();
        let payload: &[u8] = if id == SECTOR_ID_SAVEBLOCK2 {
            &block2_bytes[..len]
        } else if id < SECTOR_ID_PKMN_STORAGE_START {
            let offset = usize::from(id - SECTOR_ID_SAVEBLOCK1_START) * SECTOR_DATA_SIZE;
            &block1_bytes[offset..offset + len]
        } else {
            let offset = usize::from(id - SECTOR_ID_PKMN_STORAGE_START) * SECTOR_DATA_SIZE;
            &storage[offset..offset + len]
        };
        store.write_physical(slot, usize::from(id), &Sector::write(id, payload, counter));
    }
}

/// Writes a five-sector generation (ids 0-4 only, positions
/// 5-13 left untouched) carrying the given blocks, at rotation zero.
fn write_legacy_slot(
    store: &mut SaveStore,
    slot: usize,
    block1: &SaveBlock1,
    block2: &SaveBlock2,
    counter: u32,
) {
    write_legacy_slot_rotated(store, slot, block1, block2, counter, 0);
}

/// As [`write_legacy_slot`], at the given rotation: the five-sector writer
/// placed id `i` at physical position `(i + rotation) % 5`, so every
/// rotation but zero leaves a head no 14-sector write can imitate.
fn write_legacy_slot_rotated(
    store: &mut SaveStore,
    slot: usize,
    block1: &SaveBlock1,
    block2: &SaveBlock2,
    counter: u32,
    rotation: u16,
) {
    let block2_bytes = block2.to_bytes();
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    for id in 0..SECTOR_ID_PKMN_STORAGE_START {
        let len = sector_payload_len(id).unwrap();
        let payload: &[u8] = if id == SECTOR_ID_SAVEBLOCK2 {
            &block2_bytes[..len]
        } else {
            let offset = usize::from(id - SECTOR_ID_SAVEBLOCK1_START) * SECTOR_DATA_SIZE;
            &block1_bytes[offset..offset + len]
        };
        let physical = usize::from((id + rotation) % SECTOR_ID_PKMN_STORAGE_START);
        store.write_physical(slot, physical, &Sector::write(id, payload, counter));
    }
}

/// Rewrites the unchecksummed footer id of the sector at `position`,
/// leaving its payload, checksum, signature and counter untouched.
fn relabel_footer_id(store: &mut SaveStore, slot: usize, position: usize, id: u16) {
    // The footer ends id (u16), checksum (u16), signature, counter (u32s).
    let id_offset = SECTOR_SIZE - 2 * size_of::<u32>() - 2 * size_of::<u16>();
    let mut bytes = *store.read_physical(slot, position).as_bytes();
    bytes[id_offset..id_offset + 2].copy_from_slice(&id.to_le_bytes());
    store.write_physical(slot, position, &Sector::from_bytes(bytes));
}

/// Leaves the sector at `position` signed but unusable: a footer id outside
/// 0-13, or a payload byte flipped past its checksum.
fn make_unusable(store: &mut SaveStore, slot: usize, position: usize, by_footer_id: bool) {
    if by_footer_id {
        relabel_footer_id(store, slot, position, 15);
    } else {
        store.corrupt_byte(slot, position, 0);
    }
}
