//! Two-slot rotating save storage over upstream's `sSaveSlotLayout`
//! (`pokeemerald/src/save.c:43-72`), also reading five-sector saves.

use super::block::{SaveBlock1, SaveBlock2};
use super::sector::{Sector, SECTOR_DATA_SIZE, SECTOR_SIZE};
#[cfg(test)]
use survey::{older_generation_precedes, second_counter_is_newer, SlotIntegrity};
use survey::{resolve, SlotScan, SlotSurvey};

/// Number of alternating save slots.
pub const NUM_SAVE_SLOTS: usize = 2;

/// Logical sector containing [`SaveBlock2`].
pub const SECTOR_ID_SAVEBLOCK2: u16 = 0;
/// First logical sector containing a [`SaveBlock1`] chunk.
pub const SECTOR_ID_SAVEBLOCK1_START: u16 = 1;
/// Number of logical sectors containing [`SaveBlock1`] chunks.
pub const SAVE_BLOCK1_CHUNKS: usize = 4;
/// First logical sector containing an opaque `PokemonStorage` chunk
/// (`pokeemerald/include/save.h` `SECTOR_ID_PKMN_STORAGE_START`).
pub const SECTOR_ID_PKMN_STORAGE_START: u16 = 5;
/// Number of logical sectors containing opaque `PokemonStorage` chunks.
pub const PKMN_STORAGE_CHUNKS: usize = 9;

/// Number of physical sectors reserved for each save slot; every logical id
/// 0-13 is modelled (`pokeemerald/include/save.h` `NUM_SECTORS_PER_SLOT`).
pub const NUM_SECTORS_PER_SLOT: usize = 14;

/// Number of physical sectors in the 128 KiB flash image.
pub const NUM_SECTORS: usize = 32;

const _: () = assert!(
    SECTOR_ID_SAVEBLOCK1_START as usize + SAVE_BLOCK1_CHUNKS
        == SECTOR_ID_PKMN_STORAGE_START as usize
);
const _: () =
    assert!(SECTOR_ID_PKMN_STORAGE_START as usize + PKMN_STORAGE_CHUNKS == NUM_SECTORS_PER_SLOT);
const _: () = assert!(NUM_SAVE_SLOTS * NUM_SECTORS_PER_SLOT <= NUM_SECTORS);

/// `sizeof(struct PokemonStorage)`
/// (`pokeemerald/include/pokemon_storage_system.h:20-24`).
pub const PKMN_STORAGE_PAYLOAD_LEN: usize = 0x83D0;

/// Byte length of the 128 KiB flash image (`pokeemerald/src/save.c:24-31`).
pub const FLASH_IMAGE_LEN: usize = NUM_SECTORS * SECTOR_SIZE;

#[expect(
    clippy::cast_possible_truncation,
    reason = "compile-time assertions bound the sector count to the flash geometry"
)]
const NUM_SECTORS_PER_SLOT_U16: u16 = NUM_SECTORS_PER_SLOT as u16;
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
#[expect(
    clippy::cast_possible_truncation,
    reason = "the nine PokemonStorage chunks fit in u16"
)]
const PKMN_STORAGE_CHUNKS_U16: u16 = PKMN_STORAGE_CHUNKS as u16;
const ERASED_FLASH_BYTE: u8 = u8::MAX;

/// Result of validating both save slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStatus {
    /// Neither slot contains a signed sector.
    Empty,
    /// At least one slot is intact and the other is intact or empty.
    Ok,
    /// Neither slot is intact and at least one is non-empty.
    Corrupt,
    /// Exactly one slot is intact and the other is corrupt.
    Error,
}

/// State reconstructed by [`SaveStore::load`]; key-encrypted fields need
/// both their chunk and the [`SaveBlock2`] key sector valid.
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

/// Zero-filled `PokemonStorage` payload, heap-allocated directly: at 32.9 KiB
/// it exceeds the stack-array size clippy allows for a boxed literal.
fn boxed_zeroed_pokemon_storage() -> Box<[u8; PKMN_STORAGE_PAYLOAD_LEN]> {
    vec![0u8; PKMN_STORAGE_PAYLOAD_LEN]
        .into_boxed_slice()
        .try_into()
        .expect("vec![0u8; PKMN_STORAGE_PAYLOAD_LEN] has exactly PKMN_STORAGE_PAYLOAD_LEN bytes")
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
    } else if (SECTOR_ID_PKMN_STORAGE_START..SECTOR_ID_PKMN_STORAGE_START + PKMN_STORAGE_CHUNKS_U16)
        .contains(&id)
    {
        let chunk_num = (id - SECTOR_ID_PKMN_STORAGE_START) as usize;
        Some(chunk_len(PKMN_STORAGE_PAYLOAD_LEN, chunk_num))
    } else {
        None
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

struct CopiedSlotPayloads {
    block1: Box<[u8; SaveBlock1::PAYLOAD_LEN]>,
    block2: Box<[u8; SaveBlock2::PAYLOAD_LEN]>,
    pokemon_storage: Box<[u8; PKMN_STORAGE_PAYLOAD_LEN]>,
    valid_block1_chunks: [bool; SAVE_BLOCK1_CHUNKS],
    block2_valid: bool,
}

/// Raw [`SaveBlock1`], [`SaveBlock2`], and `PokemonStorage` payloads
/// retained for the fields the model does not own.
#[derive(Debug, Clone)]
pub struct BaseSnapshot {
    block1: Box<[u8; SaveBlock1::PAYLOAD_LEN]>,
    block2: Box<[u8; SaveBlock2::PAYLOAD_LEN]>,
    pokemon_storage: Box<[u8; PKMN_STORAGE_PAYLOAD_LEN]>,
}

/// A rotating two-slot save store over an in-memory flash image.
#[derive(Debug, Clone)]
pub struct SaveStore {
    buffer: Vec<u8>,
    last_written_sector: u16,
    save_counter: u32,
    base_block1: Box<[u8; SaveBlock1::PAYLOAD_LEN]>,
    base_block2: Box<[u8; SaveBlock2::PAYLOAD_LEN]>,
    base_pokemon_storage: Box<[u8; PKMN_STORAGE_PAYLOAD_LEN]>,
}

impl Default for SaveStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SaveStore {
    /// Creates a fully erased (all-ones) flash image.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: vec![ERASED_FLASH_BYTE; FLASH_IMAGE_LEN],
            last_written_sector: 0,
            save_counter: 0,
            base_block1: Box::new([0u8; SaveBlock1::PAYLOAD_LEN]),
            base_block2: Box::new([0u8; SaveBlock2::PAYLOAD_LEN]),
            base_pokemon_storage: boxed_zeroed_pokemon_storage(),
        }
    }

    /// Returns the persistent flash image; [`SaveStore::load`] rebuilds the
    /// runtime counters from its footers.
    #[must_use]
    pub fn flash_image(&self) -> &[u8] {
        &self.buffer
    }

    /// Rebuilds a store from an exact-length flash image; [`SaveStore::load`]
    /// must run before [`SaveStore::save`].
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
            base_pokemon_storage: boxed_zeroed_pokemon_storage(),
        })
    }

    /// Returns the current intra-slot rotation offset.
    #[must_use]
    pub const fn last_written_sector(&self) -> u16 {
        self.last_written_sector
    }

    /// Copies the retained raw payloads that base the next save.
    #[must_use]
    pub fn base_snapshot(&self) -> BaseSnapshot {
        BaseSnapshot {
            block1: self.base_block1.clone(),
            block2: self.base_block2.clone(),
            pokemon_storage: self.base_pokemon_storage.clone(),
        }
    }

    /// Whether the retained raw payloads equal `snapshot`, without copying.
    #[must_use]
    pub fn base_matches(&self, snapshot: &BaseSnapshot) -> bool {
        self.base_block1 == snapshot.block1
            && self.base_block2 == snapshot.block2
            && self.base_pokemon_storage == snapshot.pokemon_storage
    }

    /// Restores raw payloads returned by [`SaveStore::base_snapshot`].
    pub fn restore_base(&mut self, snapshot: BaseSnapshot) {
        self.base_block1 = snapshot.block1;
        self.base_block2 = snapshot.block2;
        self.base_pokemon_storage = snapshot.pokemon_storage;
    }

    /// Clears unmodelled payload bytes before saving a new game lineage.
    pub fn clear_base(&mut self) {
        self.base_block1.fill(0);
        self.base_block2.fill(0);
        self.base_pokemon_storage.fill(0);
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
        (0..NUM_SECTORS_PER_SLOT)
            .find(|&i| self.read_physical(slot, i).id() == id)
            .expect("id must be present in a fully-written slot")
    }

    /// Writes all 14 sectors into the next rotated slot as upstream's
    /// `WriteSaveSectorOrSlot` (`pokeemerald/src/save.c:138-173`).
    pub fn save(&mut self, block1: &SaveBlock1, block2: &SaveBlock2) {
        let mut block2_bytes = self.base_block2.clone();
        let mut block1_bytes = self.base_block1.clone();
        block2.patch_bytes(&mut block2_bytes);
        block1.patch_bytes(&mut block1_bytes, block2.encryption_key);
        let storage_bytes = self.base_pokemon_storage.clone();

        let new_last_written_sector = (self.last_written_sector + 1) % NUM_SECTORS_PER_SLOT_U16;
        let new_save_counter = self.save_counter.wrapping_add(1);
        block1.stamp_generation(&mut block1_bytes, new_save_counter);
        let slot = physical_slot_for_counter(new_save_counter);

        for sector_id in 0..NUM_SECTORS_PER_SLOT_U16 {
            let data: &[u8] = if sector_id == SECTOR_ID_SAVEBLOCK2 {
                &block2_bytes[..]
            } else if sector_id < SECTOR_ID_PKMN_STORAGE_START {
                let chunk_num = (sector_id - SECTOR_ID_SAVEBLOCK1_START) as usize;
                chunk_of(&block1_bytes[..], chunk_num)
            } else {
                let chunk_num = (sector_id - SECTOR_ID_PKMN_STORAGE_START) as usize;
                chunk_of(&storage_bytes[..], chunk_num)
            };
            let physical_in_slot =
                ((sector_id + new_last_written_sector) % NUM_SECTORS_PER_SLOT_U16) as usize;
            let sector = Sector::write(sector_id, data, new_save_counter);
            self.write_physical(slot, physical_in_slot, &sector);
        }

        self.last_written_sector = new_last_written_sector;
        self.save_counter = new_save_counter;
        self.base_block1 = block1_bytes;
        self.base_block2 = block2_bytes;
        self.base_pokemon_storage = storage_bytes;
    }

    /// Surveys `slot` as upstream's `GetSaveValidStatus`
    /// (`pokeemerald/src/save.c:514-585`), also accepting a five-sector head.
    fn scan_slot(&self, slot: usize) -> SlotScan {
        let mut survey = SlotSurvey::new();
        for i in 0..NUM_SECTORS_PER_SLOT {
            survey.observe(i, &self.read_physical(slot, i));
        }
        survey.verdict()
    }

    /// Copies valid payloads from `slot`; `legacy` skips positions 5-13,
    /// which a five-sector generation never wrote.
    fn copy_valid_slot_payloads(&mut self, slot: usize, legacy: bool) -> CopiedSlotPayloads {
        let mut copied = CopiedSlotPayloads {
            block1: Box::new([0; SaveBlock1::PAYLOAD_LEN]),
            block2: Box::new([0; SaveBlock2::PAYLOAD_LEN]),
            pokemon_storage: boxed_zeroed_pokemon_storage(),
            valid_block1_chunks: [false; SAVE_BLOCK1_CHUNKS],
            block2_valid: false,
        };

        for physical_index in 0..NUM_SECTORS_PER_SLOT_U16 {
            if legacy && physical_index >= SECTOR_ID_PKMN_STORAGE_START {
                continue;
            }
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
            } else if id < SECTOR_ID_PKMN_STORAGE_START {
                let chunk_num = usize::from(id - SECTOR_ID_SAVEBLOCK1_START);
                let offset = chunk_num * SECTOR_DATA_SIZE;
                copied.block1[offset..offset + payload_len]
                    .copy_from_slice(&sector.data()[..payload_len]);
                copied.valid_block1_chunks[chunk_num] = true;
            } else {
                // A five-sector slot never wrote ids 5-13; its storage stays
                // zeroed.
                let chunk_num = usize::from(id - SECTOR_ID_PKMN_STORAGE_START);
                let offset = chunk_num * SECTOR_DATA_SIZE;
                copied.pokemon_storage[offset..offset + payload_len]
                    .copy_from_slice(&sector.data()[..payload_len]);
            }
        }

        copied
    }

    /// Loads the resolved counter's parity slot as upstream's
    /// `CopySaveSlotData`, after any storage donor.
    #[must_use]
    pub fn load(&mut self) -> LoadOutcome {
        let scans = [self.scan_slot(0), self.scan_slot(1)];
        let resolution = resolve(&scans[0], &scans[1]);
        self.save_counter = resolution.counter;
        if matches!(resolution.status, SaveStatus::Empty | SaveStatus::Corrupt) {
            self.last_written_sector = 0;
        }
        let status = resolution.status;
        let storage_override = resolution.storage_from_slot.map(|slot| {
            // storage_from_slot is a scanned index (SaveStore::resolve):
            // a full-format slot's own storage, or a legacy slot's verified
            // stale tail. Either way the tail positions are what is wanted,
            // so this pass never skips them.
            self.copy_valid_slot_payloads(slot, false).pokemon_storage
        });
        let copy_slot = physical_slot_for_counter(self.save_counter);
        let mut copied = self.copy_valid_slot_payloads(copy_slot, resolution.legacy);
        if let Some(pokemon_storage) = storage_override {
            copied.pokemon_storage = pokemon_storage;
        }

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
        block1.require_generation(&copied.block1[..], self.save_counter);

        if !copied.block2_valid {
            clear_key_encrypted_fields(&mut block1);
        }

        self.base_block1 = copied.block1;
        self.base_block2 = copied.block2;
        self.base_pokemon_storage = copied.pokemon_storage;

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
const _: () = assert!(PKMN_STORAGE_PAYLOAD_LEN <= PKMN_STORAGE_CHUNKS * SECTOR_DATA_SIZE);
const _: () = assert!(PKMN_STORAGE_PAYLOAD_LEN > (PKMN_STORAGE_CHUNKS - 1) * SECTOR_DATA_SIZE);
const _: () = assert!(chunk_len(PKMN_STORAGE_PAYLOAD_LEN, PKMN_STORAGE_CHUNKS) == 0);

mod survey;
#[cfg(test)]
mod tests;
