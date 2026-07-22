//! Rotating save-slot store (S-5).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the save-slot
//! machinery in `src/save.c`: `WriteSaveSectorOrSlot`/`HandleWriteSector`
//! (full-slot write + sector rotation), `GetSaveValidStatus` (which of the
//! two save slots is current/intact) and `CopySaveSlotData` (copying the
//! winning slot's sectors back into RAM structs), collapsed into one
//! in-memory [`SaveStore`] over a `Vec<u8>` buffer.
//!
//! # Reduced sector count
//!
//! Upstream gives each save slot `NUM_SECTORS_PER_SLOT` = 14 sectors:
//! `SECTOR_ID_SAVEBLOCK2` (id 0), `SECTOR_ID_SAVEBLOCK1_START..=
//! SECTOR_ID_SAVEBLOCK1_END` (ids 1-4, `SaveBlock1` chunked across up to 4
//! sectors), and `SECTOR_ID_PKMN_STORAGE_START..=SECTOR_ID_PKMN_STORAGE_END`
//! (ids 5-13, the PC box system). This port has no `PokemonStorage` model
//! yet, so [`SECTORS_PER_SLOT`] is 5 (ids 0-4 only) rather than 14 — a
//! deliberate, documented reduction, not a silent one. The rotation math,
//! footer format, checksum algorithm, and slot-fallback decision table below
//! are otherwise unchanged from upstream and generalize cleanly to a larger
//! `SECTORS_PER_SLOT` once Pokémon storage lands.
//!
//! `SaveBlock1`'s payload is still chunked across up to
//! [`SAVE_BLOCK1_CHUNKS`] sectors the way `SAVEBLOCK_CHUNK` computes it
//! upstream (`offset = chunkNum * SECTOR_DATA_SIZE`, `size =
//! min(len - offset, SECTOR_DATA_SIZE)` if `len >= offset` else `0`), even
//! though today's small payload only ever fills chunk 0 — the chunking
//! mechanism itself is what future slices need as the modeled `SaveBlock1`
//! grows.
//!
//! # Simplifications
//!
//! This is an in-memory `Vec<u8>` buffer, not real flash
//! (`SaveStore::save`/[`SaveStore::load`] never fail the way a physical
//! flash write can) — upstream's `ProgramFlashSectorAndVerify` failure path
//! and its `gDamagedSaveSectors`/rollback-to-`gLastKnownGoodSector` recovery
//! exist to handle *write* failures a `Vec<u8>` cannot experience, so they
//! have no counterpart here. What upstream calls "corruption" — a sector
//! whose signature or checksum fails to validate on *load* — is fully
//! modeled; see [`SaveStore::load`]. File I/O (persisting the buffer to
//! disk) is intentionally left for a later slice; the issue's scope treats
//! it as optional.

use super::block::{SaveBlock1, SaveBlock2};
use super::sector::{Sector, SECTOR_DATA_SIZE, SECTOR_SIGNATURE, SECTOR_SIZE};

/// `NUM_SAVE_SLOTS` — two rotating save slots; each full save alternates
/// which one is written.
pub const NUM_SAVE_SLOTS: usize = 2;

/// `SECTOR_ID_SAVEBLOCK2`.
pub const SECTOR_ID_SAVEBLOCK2: u16 = 0;
/// `SECTOR_ID_SAVEBLOCK1_START`.
pub const SECTOR_ID_SAVEBLOCK1_START: u16 = 1;
/// The number of sectors `SaveBlock1`'s payload may be chunked across
/// (`SECTOR_ID_SAVEBLOCK1_END - SECTOR_ID_SAVEBLOCK1_START + 1` upstream).
pub const SAVE_BLOCK1_CHUNKS: usize = 4;

/// The number of sectors per save slot this port models (see the module
/// docs for why this is smaller than upstream's `NUM_SECTORS_PER_SLOT`).
pub const SECTORS_PER_SLOT: usize = 1 + SAVE_BLOCK1_CHUNKS;

// Typed views of the small constants above, for the `u16`/`u32` contexts
// upstream's sector ids, `gLastWrittenSector`, and `gSaveCounter % NUM_SAVE_SLOTS`
// arithmetic need. Each constant here is small and known at compile time
// (5, 2, 4), so the narrowing cast never actually truncates; computing it
// once here avoids repeating `as u16`/`as u32` (each individually
// re-triggering the same lint) at every use site below.
#[allow(clippy::cast_possible_truncation)]
const SECTORS_PER_SLOT_U16: u16 = SECTORS_PER_SLOT as u16;
#[allow(clippy::cast_possible_truncation)]
const NUM_SAVE_SLOTS_U32: u32 = NUM_SAVE_SLOTS as u32;
#[allow(clippy::cast_possible_truncation)]
const SAVE_BLOCK1_CHUNKS_U16: u16 = SAVE_BLOCK1_CHUNKS as u16;

/// Overall result of resolving which save slot (if any) is current, mirrors
/// the `SAVE_STATUS_*` values `GetSaveValidStatus` returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStatus {
    /// Neither slot has ever been written (no sector in either slot has the
    /// correct signature).
    Empty,
    /// A slot was found with every sector intact.
    Ok,
    /// Both slots have problems in a way that leaves no fully-intact slot to
    /// prefer (e.g. one partially corrupt, the other never written).
    Corrupt,
    /// One slot is fully intact and current; the other slot exists but has
    /// at least one corrupt sector. The intact slot's data was still loaded
    /// — this mirrors upstream returning `SAVE_STATUS_ERROR` from
    /// `TryLoadSaveSlot` while `CopySaveSlotData` has already copied the
    /// good slot's data.
    Error,
}

/// The outcome of [`SaveStore::load`]: the resolved [`SaveStatus`] plus the
/// best-effort reconstructed blocks. Sectors that didn't validate leave
/// their corresponding bytes at their [`Default`] value (this port has no
/// "previous RAM contents" to fall back on the way upstream's
/// already-resident `gSaveBlock1Ptr`/`gSaveBlock2Ptr` do).
#[derive(Debug, Clone)]
pub struct LoadOutcome {
    /// Which slot (if any) was used, and how intact it was.
    pub status: SaveStatus,
    /// The reconstructed `SaveBlock1` state.
    pub block1: SaveBlock1,
    /// The reconstructed `SaveBlock2` state.
    pub block2: SaveBlock2,
}

/// The size, in bytes, of chunk `chunk_num` of a `total_len`-byte payload,
/// mirroring the upstream `SAVEBLOCK_CHUNK` macro (`src/save.c`):
///
/// ```text
/// size = sizeof(structure) >= chunkNum * SECTOR_DATA_SIZE
///     ? min(sizeof(structure) - chunkNum * SECTOR_DATA_SIZE, SECTOR_DATA_SIZE)
///     : 0
/// ```
fn chunk_len(total_len: usize, chunk_num: usize) -> usize {
    let offset = chunk_num * SECTOR_DATA_SIZE;
    if total_len >= offset {
        (total_len - offset).min(SECTOR_DATA_SIZE)
    } else {
        0
    }
}

/// The byte slice for chunk `chunk_num` of `payload`, per [`chunk_len`].
/// Never panics: `chunk_len` guarantees `offset + len <= payload.len()`
/// whenever `len > 0`, and an empty slice is always safe to return.
fn chunk_of(payload: &[u8], chunk_num: usize) -> &[u8] {
    let len = chunk_len(payload.len(), chunk_num);
    let offset = chunk_num * SECTOR_DATA_SIZE;
    if len == 0 {
        &[]
    } else {
        &payload[offset..offset + len]
    }
}

/// The payload length expected for sector id `id`, or `None` if `id` isn't
/// one of [`SECTORS_PER_SLOT`]'s ids. Used both when writing (to size each
/// sector's checksum) and when validating on load (to know how many payload
/// bytes to recompute the checksum over).
fn expected_len_for(id: u16) -> Option<usize> {
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

/// Whether counter `b` should be treated as more recent than counter `a`,
/// mirroring the wraparound-aware comparison inlined in
/// `GetSaveValidStatus` when both slots are otherwise `SAVE_STATUS_OK`.
/// Ordinarily this is just `a < b`, but when one counter is `u32::MAX` and
/// the other is `0` (the counter having just wrapped around), the wrapped
/// one (`0`) is the newer save.
fn counter_b_is_newer(a: u32, b: u32) -> bool {
    if (a == u32::MAX && b == 0) || (a == 0 && b == u32::MAX) {
        a.wrapping_add(1) < b.wrapping_add(1)
    } else {
        a < b
    }
}

/// Per-slot scan result: whether the slot's sectors validate as a coherent
/// whole, and (if any sector validated) the save counter it was written
/// with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RawSlotStatus {
    Empty,
    Ok,
    Error,
}

struct SlotScan {
    status: RawSlotStatus,
    counter: u32,
}

/// A rotating two-slot save store over an in-memory byte buffer. See the
/// module docs for the algorithm and its (documented) reductions from
/// upstream.
#[derive(Debug, Clone)]
pub struct SaveStore {
    /// `NUM_SAVE_SLOTS * SECTORS_PER_SLOT` sectors, `SECTOR_SIZE` bytes
    /// each, laid out slot-major (mirrors `gRamSaveSectorLocations`'
    /// underlying flash addressing: slot 0 occupies sectors
    /// `0..SECTORS_PER_SLOT`, slot 1 occupies
    /// `SECTORS_PER_SLOT..2*SECTORS_PER_SLOT`).
    buffer: Vec<u8>,
    /// `gLastWrittenSector` — the sector-rotation offset within a slot.
    last_written_sector: u16,
    /// `gSaveCounter` — incremented on every full-slot write; its parity
    /// selects which physical slot gets written/read.
    save_counter: u32,
}

impl Default for SaveStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SaveStore {
    /// A fresh store: every byte `0xFF`, as if the backing flash had never
    /// been written. Real NOR/AGB flash erases to all-`1` bits, not zero
    /// `(behavioral-fidelity)`; this matters here because a footer's `id`
    /// field on unwritten flash must read as `0xFFFF`, never `0`, so
    /// [`SaveStore::load`]'s unconditional "`id == SECTOR_ID_SAVEBLOCK2`"
    /// check (mirroring upstream `CopySaveSlotData`) never fires on a
    /// sector that was never actually written.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: vec![0xFFu8; NUM_SAVE_SLOTS * SECTORS_PER_SLOT * SECTOR_SIZE],
            last_written_sector: 0,
            save_counter: 0,
        }
    }

    /// `gLastWrittenSector`'s current value, for tests/observability.
    #[must_use]
    pub const fn last_written_sector(&self) -> u16 {
        self.last_written_sector
    }

    /// `gSaveCounter`'s current value, for tests/observability.
    #[must_use]
    pub const fn save_counter(&self) -> u32 {
        self.save_counter
    }

    fn physical_offset(slot: usize, sector_in_slot: usize) -> usize {
        (slot * SECTORS_PER_SLOT + sector_in_slot) * SECTOR_SIZE
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

    /// Corrupt an arbitrary byte in the backing buffer. Test-only hook for
    /// exercising [`SaveStore::load`]'s corruption detection without
    /// reaching into private fields from outside the crate.
    #[cfg(test)]
    fn corrupt_byte(&mut self, slot: usize, sector_in_slot: usize, byte_offset: usize) {
        let idx = Self::physical_offset(slot, sector_in_slot) + byte_offset;
        self.buffer[idx] ^= 0xFF;
    }

    /// Find which physical position within `slot` currently holds the
    /// sector with footer `id`. Test-only: sector rotation makes a given
    /// logical id's physical position depend on save history, so tests
    /// locate it via the footer rather than re-deriving the rotation
    /// formula.
    #[cfg(test)]
    fn find_sector_in_slot(&self, slot: usize, id: u16) -> usize {
        (0..SECTORS_PER_SLOT)
            .find(|&i| self.read_physical(slot, i).id() == id)
            .expect("id must be present in a fully-written slot")
    }

    /// Write a full save slot, mirroring `WriteSaveSectorOrSlot(
    /// FULL_SAVE_SLOT, ...)` -> `HandleWriteSector` for each of
    /// [`SECTORS_PER_SLOT`] sectors: `gLastWrittenSector` advances by one
    /// (mod [`SECTORS_PER_SLOT`]) and `gSaveCounter` advances by one first,
    /// then every sector is (re)written with the new counter, at a
    /// physical position rotated by the new `gLastWrittenSector` within
    /// whichever physical slot `gSaveCounter % NUM_SAVE_SLOTS` now selects.
    pub fn save(&mut self, block1: &SaveBlock1, block2: &SaveBlock2) {
        let block2_bytes = block2.to_bytes();
        let block1_bytes = block1.to_bytes();

        let new_last_written_sector = (self.last_written_sector + 1) % SECTORS_PER_SLOT_U16;
        let new_save_counter = self.save_counter.wrapping_add(1);
        let slot = (new_save_counter % NUM_SAVE_SLOTS_U32) as usize;

        for sector_id in 0..SECTORS_PER_SLOT_U16 {
            let data: &[u8] = if sector_id == SECTOR_ID_SAVEBLOCK2 {
                &block2_bytes
            } else {
                let chunk_num = (sector_id - SECTOR_ID_SAVEBLOCK1_START) as usize;
                chunk_of(&block1_bytes, chunk_num)
            };
            let physical_in_slot =
                ((sector_id + new_last_written_sector) % SECTORS_PER_SLOT_U16) as usize;
            let sector = Sector::write(sector_id, data, new_save_counter);
            self.write_physical(slot, physical_in_slot, &sector);
        }

        self.last_written_sector = new_last_written_sector;
        self.save_counter = new_save_counter;
    }

    /// Scan one physical slot's sectors, mirroring the per-slot loop inside
    /// `GetSaveValidStatus`.
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
            if let Some(expected_len) = expected_len_for(id) {
                if sector.is_valid(expected_len) {
                    counter = sector.counter();
                    valid_ids |= 1 << id;
                }
            }
        }

        let all_valid_mask = (1u32 << u32::from(SECTORS_PER_SLOT_U16)) - 1;
        let status = if !signature_valid {
            RawSlotStatus::Empty
        } else if valid_ids == all_valid_mask {
            RawSlotStatus::Ok
        } else {
            RawSlotStatus::Error
        };
        SlotScan { status, counter }
    }

    /// Resolve which physical slot (if any) is current, mirroring
    /// `GetSaveValidStatus`'s decision table. Returns the overall
    /// [`SaveStatus`], the winning physical slot index (`None` only when
    /// both slots are [`SaveStatus::Empty`]), and the save counter to adopt.
    fn resolve(a: &SlotScan, b: &SlotScan) -> (SaveStatus, Option<usize>, u32) {
        use RawSlotStatus::{Empty, Error, Ok};
        match (a.status, b.status) {
            (Ok, Ok) => {
                let winner = usize::from(counter_b_is_newer(a.counter, b.counter));
                let counter = if winner == 0 { a.counter } else { b.counter };
                (SaveStatus::Ok, Some(winner), counter)
            }
            (Ok, Error) => (SaveStatus::Error, Some(0), a.counter),
            (Ok, Empty) => (SaveStatus::Ok, Some(0), a.counter),
            (Error, Ok) => (SaveStatus::Error, Some(1), b.counter),
            (Empty, Ok) => (SaveStatus::Ok, Some(1), b.counter),
            (Empty, Empty) => (SaveStatus::Empty, None, 0),
            // Both slots have problems and neither is a clean OK: upstream
            // resets to slot 0 (`gSaveCounter = 0`) and still attempts a
            // best-effort copy from it.
            (Error | Empty, Error) | (Error, Empty) => (SaveStatus::Corrupt, Some(0), 0),
        }
    }

    /// Load the current save state, mirroring `TryLoadSaveSlot(
    /// FULL_SAVE_SLOT, ...)` -> `GetSaveValidStatus` + `CopySaveSlotData`.
    ///
    /// Resolves which of the two slots is current (see [`SaveStatus`]) and
    /// adopts its save counter (`gSaveCounter`). Then — exactly as upstream
    /// `TryLoadSaveSlot` *always* calls `CopySaveSlotData` after
    /// `GetSaveValidStatus`, regardless of the status just computed — scans
    /// the slot `gSaveCounter % NUM_SAVE_SLOTS` now selects (slot 0 when
    /// both slots were empty, matching `GetSaveValidStatus` resetting
    /// `gSaveCounter` to 0 in that case) and copies every sector that
    /// individually validates (signature + checksum) into the returned
    /// blocks. A slot that is [`SaveStatus::Error`] (one or more corrupt
    /// sectors) or [`SaveStatus::Corrupt`] still yields whatever *did*
    /// validate for its unaffected sectors.
    #[must_use]
    pub fn load(&mut self) -> LoadOutcome {
        let scans = [self.scan_slot(0), self.scan_slot(1)];
        let (status, winner, counter) = Self::resolve(&scans[0], &scans[1]);
        self.save_counter = counter;
        // `GetSaveValidStatus` also resets `gLastWrittenSector = 0` in its
        // two terminal branches (both slots empty, both slots errored)
        // before `CopySaveSlotData`'s `id == 0` recovery gets a chance to
        // re-derive it from a surviving footer.
        if matches!(status, SaveStatus::Empty | SaveStatus::Corrupt) {
            self.last_written_sector = 0;
        }
        let copy_slot = winner.unwrap_or(0);

        let mut block2_bytes = vec![0u8; SaveBlock2::PAYLOAD_LEN];
        let mut block1_bytes = vec![0u8; SaveBlock1::PAYLOAD_LEN];

        for i in 0..SECTORS_PER_SLOT_U16 {
            let sector = self.read_physical(copy_slot, usize::from(i));
            let id = sector.id();

            // Mirrors CopySaveSlotData's unconditional `if (id == 0)
            // gLastWrittenSector = i` — read before the validity check, so
            // it recovers the rotation offset even for a sector whose
            // footer id happens to be intact but whose checksum isn't. Safe
            // unconditionally: unlike `locations[id]` upstream indexes with
            // `id` right after, comparing `id` itself never risks an
            // out-of-range access.
            if id == SECTOR_ID_SAVEBLOCK2 {
                self.last_written_sector = i;
            }

            let Some(expected_len) = expected_len_for(id) else {
                continue;
            };
            if expected_len == 0 {
                // An empty chunk (SaveBlock1's payload doesn't reach this
                // sector's offset) has nothing to copy; its offset would
                // fall past block1_bytes' end.
                continue;
            }
            if !sector.is_valid(expected_len) {
                continue;
            }

            if id == SECTOR_ID_SAVEBLOCK2 {
                block2_bytes[..expected_len].copy_from_slice(&sector.data()[..expected_len]);
            } else {
                let chunk_num = (id - SECTOR_ID_SAVEBLOCK1_START) as usize;
                let offset = chunk_num * SECTOR_DATA_SIZE;
                block1_bytes[offset..offset + expected_len]
                    .copy_from_slice(&sector.data()[..expected_len]);
            }
        }

        let block2 = SaveBlock2::from_bytes(&block2_bytes).unwrap_or_default();
        let block1 = SaveBlock1::from_bytes(&block1_bytes).unwrap_or_default();

        LoadOutcome {
            status,
            block1,
            block2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::block::{Coords16, PlayerGender, WarpData};

    #[test]
    fn counter_comparison_is_wraparound_aware() {
        // Ordinary ordering.
        assert!(counter_b_is_newer(3, 7));
        assert!(!counter_b_is_newer(7, 3));
        assert!(!counter_b_is_newer(5, 5));
        // The wraparound special case `GetSaveValidStatus` singles out:
        // a counter that just wrapped to 0 is newer than u32::MAX...
        assert!(counter_b_is_newer(u32::MAX, 0));
        // ...in either slot order.
        assert!(!counter_b_is_newer(0, u32::MAX));
    }

    fn sample_block2() -> SaveBlock2 {
        SaveBlock2 {
            player_name: *b"RUSTY\xFF\0\0",
            player_gender: PlayerGender::Female,
            player_trainer_id: [1, 2, 3, 4],
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
            ..SaveBlock1::default()
        };
        block.event_data.flag_set(42).unwrap();
        block
            .event_data
            .var_set(crate::event_data::VARS_START, 777)
            .unwrap();
        block
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
        // The other physical slot must remain untouched (still zeroed).
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

        // Two saves: slot 1 (counter 1) then slot 0 (counter 2) is current.
        store.save(&block1, &block2);
        store.save(&block1, &block2);
        assert_eq!(store.save_counter(), 2);

        // Corrupt the SaveBlock2 payload byte in the *current* slot (slot
        // 0). Rotation puts SECTOR_ID_SAVEBLOCK2 at whatever physical
        // position `last_written_sector` currently selects, so look it up
        // by footer id rather than assuming position 0.
        let sector_in_slot = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
        store.corrupt_byte(0, sector_in_slot, 0);

        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Error);
        // Fell back to slot 1's (still-intact) data.
        assert_eq!(outcome.block2, block2);
        assert_eq!(store.save_counter(), 1);
    }

    #[test]
    fn both_slots_corrupt_reports_corrupt_status() {
        let mut store = SaveStore::new();
        let block1 = sample_block1();
        let block2 = sample_block2();

        store.save(&block1, &block2); // slot 1
        store.save(&block1, &block2); // slot 0

        let in_slot0 = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
        let in_slot1 = store.find_sector_in_slot(1, SECTOR_ID_SAVEBLOCK2);
        store.corrupt_byte(0, in_slot0, 0);
        store.corrupt_byte(1, in_slot1, 0);

        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Corrupt);
        assert_eq!(store.save_counter(), 0);
        // `load` still scans slot 0 (gSaveCounter % NUM_SAVE_SLOTS == 0)
        // and recovers the rotation offset from wherever
        // SECTOR_ID_SAVEBLOCK2's footer id actually is, exactly as upstream
        // `CopySaveSlotData` does unconditionally — even though the overall
        // status is Corrupt.
        assert_eq!(
            store.last_written_sector(),
            u16::try_from(in_slot0).unwrap()
        );
    }

    #[test]
    fn one_untouched_empty_slot_is_not_corrupt() {
        let mut store = SaveStore::new();
        let block1 = sample_block1();
        let block2 = sample_block2();

        // Exactly one save: only slot 1 has ever been written, slot 0
        // stays fully empty (zeroed).
        store.save(&block1, &block2);

        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Ok);
        assert_eq!(outcome.block2, block2);
    }

    #[test]
    fn chunk_len_matches_the_saveblock_chunk_macro_semantics() {
        // A payload that exactly fills one chunk and spills one byte into
        // the next.
        let total = SECTOR_DATA_SIZE + 1;
        assert_eq!(chunk_len(total, 0), SECTOR_DATA_SIZE);
        assert_eq!(chunk_len(total, 1), 1);
        assert_eq!(chunk_len(total, 2), 0);

        // A payload smaller than one sector only fills chunk 0.
        assert_eq!(chunk_len(10, 0), 10);
        assert_eq!(chunk_len(10, 1), 0);
    }
}
