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
//! The full-size `SaveBlock1` payload is chunked across all
//! [`SAVE_BLOCK1_CHUNKS`] sectors the way `SAVEBLOCK_CHUNK` computes it
//! upstream (`offset = chunkNum * SECTOR_DATA_SIZE`, `size =
//! min(len - offset, SECTOR_DATA_SIZE)` if `len >= offset` else `0`).
//! PC Pokémon storage and its nine additional sectors remain deferred.
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
/// best-effort reconstructed blocks. Fields backed by a sector that didn't
/// validate decode to their [`Default`] value (this port has no "previous RAM
/// contents" to fall back on the way upstream's already-resident
/// `gSaveBlock1Ptr`/`gSaveBlock2Ptr` do). This holds even for the
/// key-encrypted fields (money, bag quantities), in both failure directions:
/// a non-validating `SaveBlock1` sector yields plaintext defaults — money `0`,
/// quantities `0` — even when a nonzero `encryption_key` was recovered from an
/// intact `SaveBlock2`; and, conversely, a validating `SaveBlock1` sector's
/// key-encrypted fields also decode to those defaults when the `SaveBlock2`
/// sector did *not* validate (so no usable key was recovered), rather than
/// leaking genuine ciphertext decrypted under the fallback key `0`.
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
const fn chunk_len(total_len: usize, chunk_num: usize) -> usize {
    let offset = chunk_num * SECTOR_DATA_SIZE;
    if total_len > offset {
        let remaining = total_len - offset;
        if remaining < SECTOR_DATA_SIZE {
            remaining
        } else {
            SECTOR_DATA_SIZE
        }
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
        let block1_bytes = block1.to_bytes(block2.encryption_key);

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

    /// Resolve the overall [`SaveStatus`] and the save counter to adopt,
    /// mirroring `GetSaveValidStatus`'s decision table. Upstream returns only
    /// a status and leaves `gSaveCounter` set to the adopted value; the
    /// physical slot to read is then re-derived from that counter's parity in
    /// `CopySaveSlotData`, so this returns no separate "winner" slot index.
    fn resolve(a: &SlotScan, b: &SlotScan) -> (SaveStatus, u32) {
        use RawSlotStatus::{Empty, Error, Ok};
        match (a.status, b.status) {
            (Ok, Ok) => {
                let counter = if counter_b_is_newer(a.counter, b.counter) {
                    b.counter
                } else {
                    a.counter
                };
                (SaveStatus::Ok, counter)
            }
            (Ok, Error) => (SaveStatus::Error, a.counter),
            (Ok, Empty) => (SaveStatus::Ok, a.counter),
            (Error, Ok) => (SaveStatus::Error, b.counter),
            (Empty, Ok) => (SaveStatus::Ok, b.counter),
            (Empty, Empty) => (SaveStatus::Empty, 0),
            // Both slots have problems and neither is a clean OK: upstream
            // resets to slot 0 (`gSaveCounter = 0`) and still attempts a
            // best-effort copy from it.
            (Error | Empty, Error) | (Error, Empty) => (SaveStatus::Corrupt, 0),
        }
    }

    /// Load the current save state, mirroring `TryLoadSaveSlot(
    /// FULL_SAVE_SLOT, ...)` -> `GetSaveValidStatus` + `CopySaveSlotData`.
    ///
    /// Resolves the overall status (see [`SaveStatus`]) and adopts a save
    /// counter (`gSaveCounter`). Then — exactly as upstream `TryLoadSaveSlot`
    /// *always* calls `CopySaveSlotData` after `GetSaveValidStatus`,
    /// regardless of the status just computed — scans the slot the *adopted
    /// counter's parity* selects (`gSaveCounter % NUM_SAVE_SLOTS`; slot 0 when
    /// both slots were empty, matching `GetSaveValidStatus` resetting
    /// `gSaveCounter` to 0 in that case) and copies every sector that
    /// individually validates (signature + checksum) into the returned
    /// blocks. The copied slot is chosen by counter parity, not by which slot
    /// won validation — the two can differ when a payload-valid slot carries a
    /// corrupted footer counter (see the copy-slot comment below). A slot that
    /// is [`SaveStatus::Error`] (one or more corrupt sectors) or
    /// [`SaveStatus::Corrupt`] still yields whatever *did* validate for its
    /// unaffected sectors.
    #[must_use]
    pub fn load(&mut self) -> LoadOutcome {
        let scans = [self.scan_slot(0), self.scan_slot(1)];
        let (status, counter) = Self::resolve(&scans[0], &scans[1]);
        self.save_counter = counter;
        // `GetSaveValidStatus` also resets `gLastWrittenSector = 0` in its
        // two terminal branches (both slots empty, both slots errored)
        // before `CopySaveSlotData`'s `id == 0` recovery gets a chance to
        // re-derive it from a surviving footer.
        if matches!(status, SaveStatus::Empty | SaveStatus::Corrupt) {
            self.last_written_sector = 0;
        }
        // `CopySaveSlotData` selects its slot purely from the adopted counter's
        // parity — `NUM_SECTORS_PER_SLOT * (gSaveCounter % NUM_SAVE_SLOTS)` —
        // *not* from `GetSaveValidStatus`'s physical winner. The footer counter
        // is outside the payload checksum, so a payload-valid but
        // counter-corrupted slot can make the two disagree; matching upstream
        // means keying off the adopted counter here `(behavioral-fidelity)`.
        let copy_slot = (self.save_counter % NUM_SAVE_SLOTS_U32) as usize;

        let mut block2_bytes = vec![0u8; SaveBlock2::PAYLOAD_LEN];
        let mut block1_bytes = vec![0u8; SaveBlock1::PAYLOAD_LEN];
        // Which SaveBlock1 chunks were copied from a validating sector. A
        // chunk left `false` keeps `block1_bytes`' zero fill, which — under a
        // recovered nonzero key — would XOR-decrypt to `key`/`low16(key)`
        // rather than the plaintext defaults `LoadOutcome` promises; the
        // re-seed pass below fixes that.
        let mut block1_chunk_valid = [false; SAVE_BLOCK1_CHUNKS];
        // Whether the SaveBlock2 *sector* validated (signature + checksum),
        // so its payload bytes were copied and a usable key was recovered.
        // `SaveBlock2::from_bytes` decodes every payload losslessly — an
        // out-of-range `player_gender` byte becomes `PlayerGender::Other`
        // rather than a decode error, mirroring `CopySaveSlotData` copying a
        // checksum-valid sector through unexamined — so sector validity is
        // both necessary and sufficient here; see the key-recovery step
        // below.
        let mut block2_sector_valid = false;

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
                block2_sector_valid = true;
            } else {
                let chunk_num = (id - SECTOR_ID_SAVEBLOCK1_START) as usize;
                let offset = chunk_num * SECTOR_DATA_SIZE;
                block1_bytes[offset..offset + expected_len]
                    .copy_from_slice(&sector.data()[..expected_len]);
                block1_chunk_valid[chunk_num] = true;
            }
        }

        // A usable key is recovered exactly when the SaveBlock2 sector
        // validated: `from_bytes` only fails on `Truncated`, which the
        // fixed-size `block2_bytes` above can never be, so decoding a
        // validated sector's payload always succeeds and copies its raw
        // encryption key — including a checksum-valid sector carrying an
        // out-of-range gender byte, matching upstream `CopySaveSlotData`.
        let (block2, key_recovered) = if block2_sector_valid {
            (
                SaveBlock2::from_bytes(&block2_bytes).unwrap_or_default(),
                true,
            )
        } else {
            (SaveBlock2::default(), false)
        };

        // Re-seed any SaveBlock1 chunk that did *not* validate with the
        // *encryption of the default block* under the recovered key, so that
        // decoding it yields plaintext defaults (money 0, quantities 0)
        // regardless of the key. Without this, a corrupt SaveBlock1 chunk
        // paired with an intact SaveBlock2 (recovering a nonzero key) would
        // decode its zero bytes to `money == key` and every empty bag slot to
        // `quantity == low16(key)`, diverging from upstream and breaking the
        // `LoadOutcome` contract. Validated chunks are untouched, so the
        // intact and single-corrupt-sector fallback paths stay byte-identical.
        if block1_chunk_valid.iter().any(|valid| !valid) {
            let default_bytes = SaveBlock1::default().to_bytes(block2.encryption_key);
            for (chunk_num, _) in block1_chunk_valid
                .iter()
                .enumerate()
                .filter(|(_, valid)| !**valid)
            {
                let len = chunk_len(SaveBlock1::PAYLOAD_LEN, chunk_num);
                if len == 0 {
                    continue;
                }
                let offset = chunk_num * SECTOR_DATA_SIZE;
                block1_bytes[offset..offset + len]
                    .copy_from_slice(&default_bytes[offset..offset + len]);
            }
        }

        let mut block1 =
            SaveBlock1::from_bytes(&block1_bytes, block2.encryption_key).unwrap_or_default();

        // Mirror of the re-seed pass above, for the opposite failure: no
        // usable key was recovered (the SaveBlock2 sector was missing/corrupt,
        // or validated but failed to decode), so `encryption_key` fell back to
        // 0. A SaveBlock1 chunk that *did* validate then holds genuine
        // ciphertext, which decrypting under key 0 would surface verbatim
        // (e.g. `money == money ^ real_key`, bag quantities as raw ciphertext)
        // — again breaking the `LoadOutcome` contract that key-encrypted
        // fields decode to plaintext defaults whenever no key was recovered.
        // Reset only the key-encrypted state — money and each bag *quantity*
        // (item_ids are stored in the clear, so a validated chunk's genuine
        // ids must survive) — leaving all other validated plaintext untouched.
        if !key_recovered {
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

        LoadOutcome {
            status,
            block1,
            block2,
        }
    }
}

const _: () = assert!(chunk_len(SaveBlock1::PAYLOAD_LEN, 0) == 3968);
const _: () = assert!(chunk_len(SaveBlock1::PAYLOAD_LEN, 1) == 3968);
const _: () = assert!(chunk_len(SaveBlock1::PAYLOAD_LEN, 2) == 3968);
const _: () = assert!(chunk_len(SaveBlock1::PAYLOAD_LEN, 3) == 3848);
const _: () = assert!(chunk_len(SaveBlock1::PAYLOAD_LEN, SAVE_BLOCK1_CHUNKS) == 0);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::block::{Coords16, PlayerGender, WarpData, TRAINER_ID_LENGTH};
    use crate::save::{BoxPokemon, ItemSlot, Pokemon, PokemonSubstructures};

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
    fn corrupting_the_last_saveblock2_payload_byte_is_detected() {
        // The entire exact-size block, including deferred bytes, remains
        // checksum-covered. Pin the final byte as a boundary regression.
        let mut store = SaveStore::new();
        let block1 = sample_block1();
        let block2 = sample_block2();

        store.save(&block1, &block2); // slot 1 (counter 1)
        store.save(&block1, &block2); // slot 0 is current (counter 2)

        let sector_in_slot = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
        store.corrupt_byte(0, sector_in_slot, SaveBlock2::PAYLOAD_LEN - 1);

        // The sector must fail validation over the full 4-aligned span.
        assert!(!store
            .read_physical(0, sector_in_slot)
            .is_valid(SaveBlock2::PAYLOAD_LEN));

        // And load must fall back to slot 1's intact copy, not return Ok with
        // the corrupted trainer id.
        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Error);
        assert_eq!(outcome.block2, block2);
        assert_eq!(store.save_counter(), 1);
    }

    #[test]
    fn copy_slot_follows_adopted_counter_parity_not_validation_winner() {
        // Regression for `CopySaveSlotData`'s slot selection: upstream copies
        // from `NUM_SECTORS_PER_SLOT * (gSaveCounter % NUM_SAVE_SLOTS)` —
        // purely the adopted counter's parity, not which slot won validation.
        // The footer counter is outside the payload checksum, so corrupting
        // only the counter keeps a slot payload-valid while flipping its
        // parity, making the two disagree.
        let mut store = SaveStore::new();
        let block1 = sample_block1();
        // Distinguish the slots by trainer id so the copied slot is
        // identifiable.
        let block2_slot1 = SaveBlock2 {
            player_trainer_id: [0x11; TRAINER_ID_LENGTH],
            ..sample_block2()
        };
        let block2_slot0 = SaveBlock2 {
            player_trainer_id: [0x22; TRAINER_ID_LENGTH],
            ..sample_block2()
        };

        store.save(&block1, &block2_slot1); // slot 1, counter 1
        store.save(&block1, &block2_slot0); // slot 0, counter 2 (physical winner)

        // Flip the low byte of every sector's footer counter in slot 0:
        // 2 -> 253 (`2 ^ 0xFF`). Still the newest (253 > 1) so slot 0 remains
        // the validation winner, but now odd — the same parity divergence as
        // the doc's "2 -> 3" example, reachable via the XOR-flip test hook.
        // The payload checksum is untouched, so slot 0 stays fully valid.
        for i in 0..SECTORS_PER_SLOT {
            store.corrupt_byte(0, i, SECTOR_SIZE - 4);
        }

        let outcome = store.load();
        // Adopted counter is 253 (odd); upstream copies slot 253 % 2 == 1.
        assert_eq!(store.save_counter(), 253);
        assert_eq!(outcome.status, SaveStatus::Ok);
        assert_eq!(
            outcome.block2, block2_slot1,
            "copy must follow counter parity (slot 1), not the validation winner (slot 0)"
        );
    }

    #[test]
    fn corrupt_recovery_with_intact_block2_decodes_encrypted_fields_to_plaintext_defaults() {
        // Regression: when no intact fallback slot exists (overall status
        // Corrupt) but the copied slot's SaveBlock2 sector still validates —
        // recovering a nonzero encryption_key — a corrupt SaveBlock1 chunk-0
        // sector (holding money at 0x490 and the whole bag at 0x560..0x848)
        // must still decode those key-encrypted fields to plaintext defaults,
        // not to `key`/`low16(key)`.
        let mut store = SaveStore::new();
        let block1 = sample_block1();
        let block2 = sample_block2();
        assert_ne!(block2.encryption_key, 0, "test needs a nonzero key");

        store.save(&block1, &block2); // slot 1, counter 1
        store.save(&block1, &block2); // slot 0, counter 2 (copy slot after reset)

        // Corrupt the parity-selected slot's (slot 0, since the reset counter
        // is 0) SaveBlock1 chunk-0 sector, but leave its SaveBlock2 sector
        // intact so the key is still recovered.
        let block1_chunk0 = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK1_START);
        store.corrupt_byte(0, block1_chunk0, 0);
        // Also corrupt slot 1 entirely (its SaveBlock2 sector) so there is no
        // intact fallback slot: the overall status becomes Corrupt.
        let slot1_block2 = store.find_sector_in_slot(1, SECTOR_ID_SAVEBLOCK2);
        store.corrupt_byte(1, slot1_block2, 0);

        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Corrupt);
        // SaveBlock2 validated, so the nonzero key was recovered...
        assert_eq!(outcome.block2, block2);
        // ...yet the corrupt SaveBlock1 chunk decodes to plaintext defaults,
        // never `money == encryption_key`.
        assert_eq!(outcome.block1.money, 0);
        assert_ne!(outcome.block1.money, outcome.block2.encryption_key);
        // Every bag slot — including the empty ones — must read quantity 0,
        // not `low16(key)`.
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
        // Mirror of the case above: when no intact fallback slot exists
        // (status Corrupt) and the copied slot's SaveBlock2 sector does *not*
        // validate, no usable encryption_key is recovered (it falls back to
        // 0). A SaveBlock1 chunk-0 sector that *does* validate then holds
        // genuine ciphertext, which must still decode its key-encrypted fields
        // (money at 0x490, bag quantities in 0x560..0x848) to plaintext
        // defaults rather than leaking the raw ciphertext under key 0.
        let mut store = SaveStore::new();
        let block1 = sample_block1();
        let block2 = sample_block2();
        assert_ne!(block2.encryption_key, 0, "test needs a nonzero key");
        assert_ne!(block1.money, 0, "test needs nonzero encrypted state");

        store.save(&block1, &block2); // slot 1, counter 1
        store.save(&block1, &block2); // slot 0, counter 2 (copy slot after reset)

        // Corrupt only the parity-selected slot's (slot 0) SaveBlock2 sector,
        // leaving its SaveBlock1 chunk-0 sector intact so the genuine
        // money/bag ciphertext survives.
        let slot0_block2 = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
        store.corrupt_byte(0, slot0_block2, 0);
        // Also corrupt slot 1 so there is no intact fallback slot: the overall
        // status becomes Corrupt.
        let slot1_block2 = store.find_sector_in_slot(1, SECTOR_ID_SAVEBLOCK2);
        store.corrupt_byte(1, slot1_block2, 0);

        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Corrupt);
        // No key was recovered (SaveBlock2 sector invalid).
        assert_eq!(outcome.block2.encryption_key, 0);
        // The still-valid SaveBlock1 ciphertext must not leak: money must be
        // the plaintext default 0, never `money ^ real_key`.
        assert_eq!(outcome.block1.money, 0);
        // Every bag quantity must read the plaintext default 0, not raw
        // ciphertext.
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
        // item_ids are stored in the clear, so the validated chunk's genuine
        // ids (from `sample_block1`) must survive the quantity-only cleanup —
        // only the key-XOR'd quantity is reset, not the whole slot.
        assert_eq!(bag.items[0].item_id, 1);
        assert_eq!(bag.key_items[29].item_id, 2);
        assert_eq!(bag.poke_balls[15].item_id, 3);
        assert_eq!(bag.tms_hms[63].item_id, 4);
        assert_eq!(bag.berries[45].item_id, 5);
    }

    #[test]
    fn checksum_valid_out_of_range_gender_retains_key_and_decrypts_bag() {
        // Regression for issue #130: a SaveBlock2 sector whose checksum
        // validates but whose raw gender byte is out of range must NOT be
        // treated as a decode failure. Canonical `CopySaveSlotData` copies a
        // checksum-valid sector byte-for-byte with no semantic gender check,
        // so the encryption key must still be recovered and used to decrypt
        // the rest of the slot.
        const PLAYER_GENDER_OFFSET: usize = 0x08;
        let mut store = SaveStore::new();
        let block1 = sample_block1();
        let block2 = sample_block2();
        assert_ne!(block1.money, 0, "test needs nonzero encrypted state");

        store.save(&block1, &block2); // slot 1, counter 1
        store.save(&block1, &block2); // slot 0, counter 2 (current)

        // Replace slot 0's SaveBlock2 sector with one whose payload carries an
        // out-of-range gender byte but a *freshly recomputed* checksum, so it
        // still passes `is_valid`. The counter matches the slot's (2) so
        // slot 0 still scans as the winner. Slot 1 is left fully intact, so
        // both slots validate — the newer slot must still win with
        // `SaveStatus::Ok`, not fall back or downgrade to `Error`.
        let mut payload = block2.to_bytes();
        payload[PLAYER_GENDER_OFFSET] = 9; // neither MALE (0) nor FEMALE (1)
        let mutated_sector = Sector::write(SECTOR_ID_SAVEBLOCK2, &payload, 2);
        assert!(mutated_sector.is_valid(SaveBlock2::PAYLOAD_LEN));
        let slot0_block2 = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
        store.write_physical(0, slot0_block2, &mutated_sector);

        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Ok);
        assert_eq!(store.save_counter(), 2, "the newer slot stays selected");
        // The raw out-of-range gender byte survives losslessly...
        assert_eq!(outcome.block2.player_gender, PlayerGender::Other(9));
        // ...identity fields and the real encryption key are retained...
        assert_eq!(outcome.block2.player_trainer_id, block2.player_trainer_id);
        assert_eq!(outcome.block2.encryption_key, block2.encryption_key);
        // ...and money/bag decrypt to their saved plaintext, not zero.
        assert_eq!(outcome.block1.money, block1.money);
        assert_eq!(outcome.block1.bag, block1.bag);
    }

    #[test]
    fn checksum_preserving_xor_mutation_keeps_newer_slot_selected() {
        // Regression for issue #130's own reproduction: XOR two bytes that
        // each sit at the low (least-significant) byte of a distinct
        // little-endian u32 checksum word, one turning bit 0x08 on and the
        // other turning the same bit off. `CalculateChecksum` sums whole
        // little-endian u32 words, so the two +8/-8 deltas cancel and the
        // *stored* footer checksum needs no recomputation at all — proving
        // the corruption is checksum-invisible, not merely checksum-valid by
        // construction.
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
            // player_name[4] == 0x59 has bit 0x08 set, matching the issue's
            // own repro constant, so XOR-ing it and the gender byte by 0x08
            // moves the checksum by -8 and +8 respectively.
            player_name: *b"RUST\x59\xFF\0\0",
            player_gender: PlayerGender::Female, // raw byte 1: bit 0x08 clear
            player_trainer_id: [0x22; TRAINER_ID_LENGTH],
            encryption_key: 0xA1B2_C3D4,
        };

        store.save(&older_block1, &older_block2); // slot 1, counter 1
        store.save(&newer_block1, &newer_block2); // slot 0, counter 2 (current)

        let sector_in_slot = store.find_sector_in_slot(0, SECTOR_ID_SAVEBLOCK2);
        let before = store.read_physical(0, sector_in_slot);
        let mut bytes = *before.as_bytes();
        bytes[0x08] ^= 0x08; // gender byte: FEMALE (1) -> out-of-range (9)
        bytes[0x04] ^= 0x08; // player_name[4]: 0x59 -> 0x51, cancels the delta
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
        assert_eq!(outcome.block2.player_gender, PlayerGender::Other(9));
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
        // Companion to `save_then_load_round_trips_identical_state` (which
        // uses Female): an ordinary MALE (0) save must round-trip unchanged
        // too, alongside the new `Other` gender coverage above.
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

        // Exactly one save: only slot 1 has ever been written, slot 0
        // stays fully empty (zeroed).
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
