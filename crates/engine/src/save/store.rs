//! Two-slot rotating save storage.
//!
//! [`SaveStore`] preserves the full 128 KiB flash geometry while reading and
//! writing the five logical sectors occupied by [`SaveBlock2`] and
//! [`SaveBlock1`]. The remaining nine sectors in each physical slot are
//! unmodelled: fresh stores initialize them erased, while imported images
//! preserve them without interpreting or writing them.
//!
//! The in-memory store validates sector signatures and checksums, but writes
//! cannot reproduce partial hardware failures. File persistence belongs to
//! [`super::file`].

use super::block::{SaveBlock1, SaveBlock2};
use super::sector::{Sector, SECTOR_DATA_SIZE, SECTOR_SIGNATURE, SECTOR_SIZE};

/// Number of alternating save slots.
pub const NUM_SAVE_SLOTS: usize = 2;

/// Logical sector containing [`SaveBlock2`].
pub const SECTOR_ID_SAVEBLOCK2: u16 = 0;
/// First logical sector containing a [`SaveBlock1`] chunk.
pub const SECTOR_ID_SAVEBLOCK1_START: u16 = 1;
/// Number of logical sectors containing [`SaveBlock1`] chunks.
pub const SAVE_BLOCK1_CHUNKS: usize = 4;

/// Number of logical sectors currently written in each save slot.
pub const SECTORS_PER_SLOT: usize = 1 + SAVE_BLOCK1_CHUNKS;

/// Number of physical sectors reserved for each save slot.
pub const NUM_SECTORS_PER_SLOT: usize = 14;

/// Number of physical sectors in the 128 KiB flash image.
pub const NUM_SECTORS: usize = 32;

const _: () = assert!(SECTORS_PER_SLOT <= NUM_SECTORS_PER_SLOT);
const _: () = assert!(NUM_SAVE_SLOTS * NUM_SECTORS_PER_SLOT <= NUM_SECTORS);

/// Exact byte length of a [`SaveStore`] flash image.
///
/// The full geometry keeps image length and slot offsets stable as more
/// logical sectors are modelled. Increasing [`SECTORS_PER_SLOT`] still
/// requires placeholder sectors or migration because slot scanning requires
/// every modelled sector to validate.
pub const FLASH_IMAGE_LEN: usize = NUM_SECTORS * SECTOR_SIZE;

#[expect(
    clippy::cast_possible_truncation,
    reason = "compile-time assertions bound the sector count to the flash geometry"
)]
const SECTORS_PER_SLOT_U16: u16 = SECTORS_PER_SLOT as u16;
#[expect(
    clippy::cast_possible_truncation,
    reason = "the two save slots fit in u32"
)]
const NUM_SAVE_SLOTS_U32: u32 = NUM_SAVE_SLOTS as u32;
#[expect(
    clippy::cast_possible_truncation,
    reason = "the four SaveBlock1 chunks fit in u16"
)]
const SAVE_BLOCK1_CHUNKS_U16: u16 = SAVE_BLOCK1_CHUNKS as u16;
const ERASED_FLASH_BYTE: u8 = u8::MAX;

/// Result of validating both save slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStatus {
    /// Neither slot contains a signed sector.
    Empty,
    /// The selected slot is intact.
    Ok,
    /// Neither slot is intact and at least one is non-empty.
    Corrupt,
    /// Exactly one slot is intact and the other is corrupt.
    Error,
}

/// State reconstructed by [`SaveStore::load`].
///
/// Fields from invalid sectors use plaintext defaults. Key-encrypted
/// [`SaveBlock1`] fields are retained only when both their chunk and the
/// [`SaveBlock2`] key sector validate.
#[derive(Debug, Clone)]
pub struct LoadOutcome {
    /// Result of validating both save slots.
    pub status: SaveStatus,
    /// Reconstructed [`SaveBlock1`] state.
    pub block1: SaveBlock1,
    /// Reconstructed [`SaveBlock2`] state.
    pub block2: SaveBlock2,
}

const fn chunk_len(total_len: usize, chunk_num: usize) -> usize {
    let offset = chunk_num * SECTOR_DATA_SIZE;
    let remaining = total_len.saturating_sub(offset);
    if remaining < SECTOR_DATA_SIZE {
        remaining
    } else {
        SECTOR_DATA_SIZE
    }
}

fn chunk_of(payload: &[u8], chunk_num: usize) -> &[u8] {
    let len = chunk_len(payload.len(), chunk_num);
    let offset = chunk_num * SECTOR_DATA_SIZE;
    if len == 0 {
        &[]
    } else {
        &payload[offset..offset + len]
    }
}

fn sector_payload_len(id: u16) -> Option<usize> {
    if id == SECTOR_ID_SAVEBLOCK2 {
        Some(SaveBlock2::PAYLOAD_LEN)
    } else if (SECTOR_ID_SAVEBLOCK1_START..SECTOR_ID_SAVEBLOCK1_START + SAVE_BLOCK1_CHUNKS_U16)
        .contains(&id)
    {
        let chunk_num = (id - SECTOR_ID_SAVEBLOCK1_START) as usize;
        Some(chunk_len(SaveBlock1::PAYLOAD_LEN, chunk_num))
    } else {
        None
    }
}

/// Compares counters from adjacent save generations, including the sole
/// `u32::MAX` to zero wrap.
#[must_use]
fn second_counter_is_newer(first: u32, second: u32) -> bool {
    match (first, second) {
        (u32::MAX, 0) => true,
        (0, u32::MAX) => false,
        _ => first < second,
    }
}

fn physical_slot_for_counter(counter: u32) -> usize {
    (counter % NUM_SAVE_SLOTS_U32) as usize
}

fn fill_invalid_chunks_with_encrypted_defaults(
    block1_bytes: &mut [u8; SaveBlock1::PAYLOAD_LEN],
    valid_chunks: [bool; SAVE_BLOCK1_CHUNKS],
    encryption_key: u32,
) {
    let default_bytes = SaveBlock1::default().to_bytes(encryption_key);
    for (chunk_num, _) in valid_chunks
        .iter()
        .enumerate()
        .filter(|(_, valid)| !**valid)
    {
        let len = chunk_len(SaveBlock1::PAYLOAD_LEN, chunk_num);
        let offset = chunk_num * SECTOR_DATA_SIZE;
        block1_bytes[offset..offset + len].copy_from_slice(&default_bytes[offset..offset + len]);
    }
}

fn clear_key_encrypted_fields(block1: &mut SaveBlock1) {
    block1.money = 0;
    for slot in block1
        .bag
        .items
        .iter_mut()
        .chain(&mut block1.bag.key_items)
        .chain(&mut block1.bag.poke_balls)
        .chain(&mut block1.bag.tms_hms)
        .chain(&mut block1.bag.berries)
    {
        slot.quantity = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotIntegrity {
    Empty,
    Ok,
    Error,
}

struct SlotScan {
    integrity: SlotIntegrity,
    counter: u32,
}

struct CopiedSlotPayloads {
    block1: Box<[u8; SaveBlock1::PAYLOAD_LEN]>,
    block2: Box<[u8; SaveBlock2::PAYLOAD_LEN]>,
    valid_block1_chunks: [bool; SAVE_BLOCK1_CHUNKS],
    block2_valid: bool,
}

/// Raw payloads retained across saves for fields the model does not own.
pub type BaseSnapshot = (
    Box<[u8; SaveBlock1::PAYLOAD_LEN]>,
    Box<[u8; SaveBlock2::PAYLOAD_LEN]>,
);

/// A rotating two-slot save store over an in-memory flash image.
#[derive(Debug, Clone)]
pub struct SaveStore {
    buffer: Vec<u8>,
    last_written_sector: u16,
    save_counter: u32,
    base_block1: Box<[u8; SaveBlock1::PAYLOAD_LEN]>,
    base_block2: Box<[u8; SaveBlock2::PAYLOAD_LEN]>,
}

impl Default for SaveStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SaveStore {
    /// Creates a fully erased flash image.
    ///
    /// Erased all-one footer IDs cannot be mistaken for
    /// [`SECTOR_ID_SAVEBLOCK2`] when loading recovers the rotation offset.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: vec![ERASED_FLASH_BYTE; FLASH_IMAGE_LEN],
            last_written_sector: 0,
            save_counter: 0,
            base_block1: Box::new([0u8; SaveBlock1::PAYLOAD_LEN]),
            base_block2: Box::new([0u8; SaveBlock2::PAYLOAD_LEN]),
        }
    }

    /// Returns the complete persistent flash image.
    ///
    /// Runtime counters are excluded and reconstructed from sector footers by
    /// [`SaveStore::load`].
    #[must_use]
    pub fn flash_image(&self) -> &[u8] {
        &self.buffer
    }

    /// Rebuilds a store from an exact-length flash image.
    ///
    /// Runtime counters start at zero until [`SaveStore::load`] reconstructs
    /// them from sector footers.
    #[must_use]
    pub fn from_flash_image(image: &[u8]) -> Option<Self> {
        if image.len() != FLASH_IMAGE_LEN {
            return None;
        }
        Some(Self {
            buffer: image.to_vec(),
            last_written_sector: 0,
            save_counter: 0,
            base_block1: Box::new([0u8; SaveBlock1::PAYLOAD_LEN]),
            base_block2: Box::new([0u8; SaveBlock2::PAYLOAD_LEN]),
        })
    }

    /// Returns the current intra-slot rotation offset.
    #[must_use]
    pub const fn last_written_sector(&self) -> u16 {
        self.last_written_sector
    }

    /// Copies the retained raw payloads used as the base of the next save.
    ///
    /// A session can restore this snapshot before healing a corrupt image so
    /// unmodelled bytes come from that session rather than an older fallback
    /// slot.
    #[must_use]
    pub fn base_snapshot(&self) -> BaseSnapshot {
        (self.base_block1.clone(), self.base_block2.clone())
    }

    /// Restores raw payloads returned by [`SaveStore::base_snapshot`].
    pub fn restore_base(&mut self, (block1, block2): BaseSnapshot) {
        self.base_block1 = block1;
        self.base_block2 = block2;
    }

    /// Clears unmodelled payload bytes before saving a new game lineage.
    pub fn clear_base(&mut self) {
        self.base_block1.fill(0);
        self.base_block2.fill(0);
    }

    /// Returns the current wrapping save counter.
    #[must_use]
    pub const fn save_counter(&self) -> u32 {
        self.save_counter
    }

    fn physical_offset(slot: usize, sector_in_slot: usize) -> usize {
        (slot * NUM_SECTORS_PER_SLOT + sector_in_slot) * SECTOR_SIZE
    }

    fn read_physical(&self, slot: usize, sector_in_slot: usize) -> Sector {
        let start = Self::physical_offset(slot, sector_in_slot);
        let bytes: [u8; SECTOR_SIZE] = self.buffer[start..start + SECTOR_SIZE]
            .try_into()
            .expect("slice of exactly SECTOR_SIZE bytes");
        Sector::from_bytes(bytes)
    }

    fn write_physical(&mut self, slot: usize, sector_in_slot: usize, sector: &Sector) {
        let start = Self::physical_offset(slot, sector_in_slot);
        self.buffer[start..start + SECTOR_SIZE].copy_from_slice(sector.as_bytes());
    }

    #[cfg(test)]
    fn corrupt_byte(&mut self, slot: usize, sector_in_slot: usize, byte_offset: usize) {
        let idx = Self::physical_offset(slot, sector_in_slot) + byte_offset;
        self.buffer[idx] = !self.buffer[idx];
    }

    #[cfg(test)]
    fn find_sector_in_slot(&self, slot: usize, id: u16) -> usize {
        (0..SECTORS_PER_SLOT)
            .find(|&i| self.read_physical(slot, i).id() == id)
            .expect("id must be present in a fully-written slot")
    }

    /// Writes both blocks into the next rotated physical slot.
    pub fn save(&mut self, block1: &SaveBlock1, block2: &SaveBlock2) {
        let mut block2_bytes = self.base_block2.clone();
        let mut block1_bytes = self.base_block1.clone();
        block2.patch_bytes(&mut block2_bytes);
        block1.patch_bytes(&mut block1_bytes, block2.encryption_key);

        let new_last_written_sector = (self.last_written_sector + 1) % SECTORS_PER_SLOT_U16;
        let new_save_counter = self.save_counter.wrapping_add(1);
        let slot = physical_slot_for_counter(new_save_counter);

        for sector_id in 0..SECTORS_PER_SLOT_U16 {
            let data: &[u8] = if sector_id == SECTOR_ID_SAVEBLOCK2 {
                &block2_bytes[..]
            } else {
                let chunk_num = (sector_id - SECTOR_ID_SAVEBLOCK1_START) as usize;
                chunk_of(&block1_bytes[..], chunk_num)
            };
            let physical_in_slot =
                ((sector_id + new_last_written_sector) % SECTORS_PER_SLOT_U16) as usize;
            let sector = Sector::write(sector_id, data, new_save_counter);
            self.write_physical(slot, physical_in_slot, &sector);
        }

        self.last_written_sector = new_last_written_sector;
        self.save_counter = new_save_counter;
        self.base_block1 = block1_bytes;
        self.base_block2 = block2_bytes;
    }

    fn scan_slot(&self, slot: usize) -> SlotScan {
        let mut signature_valid = false;
        let mut valid_ids: u32 = 0;
        let mut counter = 0u32;

        for i in 0..SECTORS_PER_SLOT {
            let sector = self.read_physical(slot, i);
            if sector.signature() != SECTOR_SIGNATURE {
                continue;
            }
            signature_valid = true;
            let id = sector.id();
            if let Some(expected_len) = sector_payload_len(id) {
                if sector.is_valid(expected_len) {
                    counter = sector.counter();
                    valid_ids |= 1 << id;
                }
            }
        }

        let all_valid_mask = (1u32 << u32::from(SECTORS_PER_SLOT_U16)) - 1;
        let integrity = if !signature_valid {
            SlotIntegrity::Empty
        } else if valid_ids == all_valid_mask {
            SlotIntegrity::Ok
        } else {
            SlotIntegrity::Error
        };
        SlotScan { integrity, counter }
    }

    fn resolve(slot0: &SlotScan, slot1: &SlotScan) -> (SaveStatus, u32) {
        use SlotIntegrity::{Empty, Error, Ok};
        match (slot0.integrity, slot1.integrity) {
            (Ok, Ok) => {
                let counter = if second_counter_is_newer(slot0.counter, slot1.counter) {
                    slot1.counter
                } else {
                    slot0.counter
                };
                (SaveStatus::Ok, counter)
            }
            (Ok, Error) => (SaveStatus::Error, slot0.counter),
            (Ok, Empty) => (SaveStatus::Ok, slot0.counter),
            (Error, Ok) => (SaveStatus::Error, slot1.counter),
            (Empty, Ok) => (SaveStatus::Ok, slot1.counter),
            (Empty, Empty) => (SaveStatus::Empty, 0),
            (Error | Empty, Error) | (Error, Empty) => (SaveStatus::Corrupt, 0),
        }
    }

    fn copy_valid_slot_payloads(&mut self, slot: usize) -> CopiedSlotPayloads {
        let mut copied = CopiedSlotPayloads {
            block1: Box::new([0; SaveBlock1::PAYLOAD_LEN]),
            block2: Box::new([0; SaveBlock2::PAYLOAD_LEN]),
            valid_block1_chunks: [false; SAVE_BLOCK1_CHUNKS],
            block2_valid: false,
        };

        for physical_index in 0..SECTORS_PER_SLOT_U16 {
            let sector = self.read_physical(slot, usize::from(physical_index));
            let id = sector.id();

            // CopySaveSlotData recovers rotation from sector id zero before validation.
            if id == SECTOR_ID_SAVEBLOCK2 {
                self.last_written_sector = physical_index;
            }

            let Some(payload_len) = sector_payload_len(id) else {
                continue;
            };
            if payload_len == 0 || !sector.is_valid(payload_len) {
                continue;
            }

            if id == SECTOR_ID_SAVEBLOCK2 {
                copied.block2[..payload_len].copy_from_slice(&sector.data()[..payload_len]);
                copied.block2_valid = true;
            } else {
                let chunk_num = usize::from(id - SECTOR_ID_SAVEBLOCK1_START);
                let offset = chunk_num * SECTOR_DATA_SIZE;
                copied.block1[offset..offset + payload_len]
                    .copy_from_slice(&sector.data()[..payload_len]);
                copied.valid_block1_chunks[chunk_num] = true;
            }
        }

        copied
    }

    /// Loads the best available state and reports the selected slot's health.
    ///
    /// The resolved counter's parity selects the slot to copy, even if a
    /// checksum-valid payload has a corrupt footer counter that makes this
    /// differ from the slot preferred during validation.
    #[must_use]
    pub fn load(&mut self) -> LoadOutcome {
        let scans = [self.scan_slot(0), self.scan_slot(1)];
        let (status, counter) = Self::resolve(&scans[0], &scans[1]);
        self.save_counter = counter;
        if matches!(status, SaveStatus::Empty | SaveStatus::Corrupt) {
            self.last_written_sector = 0;
        }
        let copy_slot = physical_slot_for_counter(self.save_counter);
        let mut copied = self.copy_valid_slot_payloads(copy_slot);

        let block2 = if copied.block2_valid {
            SaveBlock2::from_bytes(&copied.block2[..]).unwrap_or_default()
        } else {
            SaveBlock2::default()
        };

        fill_invalid_chunks_with_encrypted_defaults(
            &mut copied.block1,
            copied.valid_block1_chunks,
            block2.encryption_key,
        );

        let mut block1 =
            SaveBlock1::from_bytes(&copied.block1[..], block2.encryption_key).unwrap_or_default();

        if !copied.block2_valid {
            clear_key_encrypted_fields(&mut block1);
        }

        self.base_block1 = copied.block1;
        self.base_block2 = copied.block2;

        LoadOutcome {
            status,
            block1,
            block2,
        }
    }
}

const _: () = assert!(SaveBlock1::PAYLOAD_LEN <= SAVE_BLOCK1_CHUNKS * SECTOR_DATA_SIZE);
const _: () = assert!(SaveBlock1::PAYLOAD_LEN > (SAVE_BLOCK1_CHUNKS - 1) * SECTOR_DATA_SIZE);
const _: () = assert!(chunk_len(SaveBlock1::PAYLOAD_LEN, SAVE_BLOCK1_CHUNKS) == 0);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::block::{Coords16, PlayerGender, WarpData, TRAINER_ID_LENGTH};
    use crate::save::{BoxPokemon, ItemSlot, Pokemon, PokemonSubstructures};

    #[test]
    fn flash_image_keeps_full_physical_geometry() {
        assert_eq!(NUM_SECTORS_PER_SLOT, 14);
        assert_eq!(NUM_SECTORS, 32);
        assert_eq!(FLASH_IMAGE_LEN, 131_072);
        assert_eq!(SaveStore::physical_offset(0, 0), 0);
        assert_eq!(
            SaveStore::physical_offset(1, 0),
            NUM_SECTORS_PER_SLOT * SECTOR_SIZE,
            "slot 1 sits at upstream's 14-sector offset even while only 5 are written"
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

    fn sample_block2() -> SaveBlock2 {
        SaveBlock2 {
            player_name: *b"RUSTY\xFF\0\0",
            player_gender: PlayerGender::Female,
            player_trainer_id: [1, 2, 3, 4],
            encryption_key: 0xA1B2_C3D4,
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

    #[test]
    fn fresh_store_loads_as_empty() {
        let mut store = SaveStore::new();
        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Empty);
        assert_eq!(store.save_counter(), 0);
        assert_eq!(store.last_written_sector(), 0);
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
        for expected in 1..=(SECTORS_PER_SLOT_U16 * 2) {
            store.save(&block1, &block2);
            assert_eq!(store.last_written_sector(), expected % SECTORS_PER_SLOT_U16);
        }
        assert_eq!(store.save_counter(), u32::from(SECTORS_PER_SLOT_U16) * 2);
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

        for i in 0..SECTORS_PER_SLOT {
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
            encryption_key: 0xA1B2_C3D4,
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

        let two_chunk_payload_len = SECTOR_DATA_SIZE + 1;
        assert_eq!(chunk_len(two_chunk_payload_len, 0), SECTOR_DATA_SIZE);
        assert_eq!(chunk_len(two_chunk_payload_len, 1), 1);
        assert_eq!(chunk_len(two_chunk_payload_len, 2), 0);

        let short_payload_len = 10;
        assert_eq!(chunk_len(short_payload_len, 0), short_payload_len);
        assert_eq!(chunk_len(short_payload_len, 1), 0);
    }
}
