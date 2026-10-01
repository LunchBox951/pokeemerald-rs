//! Two-slot rotating save storage.
//!
//! [`SaveStore`] rotates, scans, and rewrites all fourteen physical sectors
//! of each slot, matching upstream's `sSaveSlotLayout`
//! (`pokeemerald/src/save.c:43-72`, `pokeemerald/include/save.h:19-30`): id 0
//! is [`SaveBlock2`], ids 1-4 are [`SaveBlock1`] chunks, and ids 5-13 are the
//! nine `PokemonStorage` chunks, carried opaquely without being interpreted.
//! A fresh store emits valid all-zero placeholder chunks so every generation
//! it writes satisfies upstream's all-14-sectors-valid invariant.
//!
//! [`SaveStore::load`] also accepts a slot written under this project's
//! earlier five-sector format (ids 0-4 only; physical positions 5-13 never
//! touched); the next [`SaveStore::save`] rewrites it in the full format.
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

/// Exact `sizeof(struct PokemonStorage)`
/// (`pokeemerald/include/pokemon_storage_system.h:20-24`): one `currentBox`
/// byte, three bytes of alignment padding before the `BoxPokemon` array
/// (`boxNames` sits at the declared offset 0x8344, `boxes` at 0x0004), 14x30
/// eighty-byte `BoxPokemon` records (`pokeemerald/include/pokemon.h:178-217`),
/// 14 nine-byte `boxNames` entries, and 14 `boxWallpapers` bytes:
/// `0x4 + 0x8340 + 0x7E + 0xE == 0x83D0`.
pub const PKMN_STORAGE_PAYLOAD_LEN: usize = 0x83D0;

/// Exact byte length of a [`SaveStore`] flash image.
///
/// Two 14-sector slots plus the four unmodelled Hall-of-Fame/Trainer-Hill/
/// Recorded-Battle sectors fill the 128 KiB chip
/// (`pokeemerald/src/save.c:24-31` sector layout comment).
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
const LEGACY_ERA_IDS_MASK: u32 = (1 << SECTOR_ID_PKMN_STORAGE_START) - 1;
/// The nine opaque `PokemonStorage` sector ids (5-13) as a bitmask.
const PKMN_STORAGE_IDS_MASK: u32 =
    ((1u32 << PKMN_STORAGE_CHUNKS) - 1) << SECTOR_ID_PKMN_STORAGE_START;

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

/// Whether `older` precedes `newer` as a wrapping serial number (RFC 1982),
/// i.e. `newer` is reachable from `older` by fewer than half the counter
/// space. Unlike [`second_counter_is_newer`] -- sound only for two
/// generations already known to be adjacent -- this is the comparison
/// [`SaveStore::scan_slot`] needs for a stale tail's counter, which can sit
/// an arbitrary number of generations behind the legacy head that
/// overwrote it.
#[must_use]
fn older_generation_precedes(older: u32, newer: u32) -> bool {
    let delta = newer.wrapping_sub(older);
    delta != 0 && delta < (1 << 31)
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
    /// All 14 sectors validate, or the slot matches this project's pre-#1227
    /// five-sector-era shape (see [`SaveStore::scan_slot`]).
    Ok,
    Error,
}

struct SlotScan {
    integrity: SlotIntegrity,
    counter: u32,
    /// Whether an `Ok` integrity came from the legacy five-sector fallback
    /// rather than all 14 sectors validating. See [`SaveStore::resolve`].
    legacy: bool,
    /// The generation of a complete, checksum-valid set of ids 5-13, each
    /// held once at one rotation, anywhere in this slot (at most one footer
    /// counter may disagree, and never with a save-block generation's own;
    /// see `SlotSurvey::storage_generation`): the box data still
    /// salvageable from it, whatever the slot's own integrity. A legacy head's stale
    /// tail sets it; so does a slot whose save-block sectors are too damaged
    /// for the slot to be `Ok` at all. Completeness is required because a
    /// partial set would mix stale chunks with zeroed ones into a storage
    /// image that never existed.
    storage_counter: Option<u32>,
}

/// The physical slots [`SaveStore::load`] copies each half of its result
/// from.
struct Resolution {
    status: SaveStatus,
    /// The adopted generation number: reported by `save_counter()` and used
    /// to pick both the next save's physical slot and (absent a merge) the
    /// slot every field is copied from.
    counter: u32,
    /// The *physical* index of the slot to source `PokemonStorage` from,
    /// when that is not simply the adopted generation's own slot: the
    /// counterpart full-format slot of a legacy/full merge, or whichever
    /// slot still holds a complete verified storage generation when the
    /// adopted one is legacy (see [`SaveStore::storage_donor`]).
    /// The scanned index is carried through rather than a counter, because
    /// [`physical_slot_for_counter`] recovers a slot only under the parity
    /// invariant [`SaveStore::save`] maintains -- which
    /// [`SaveStore::scan_slot`], accepting each slot on its own contents,
    /// never enforces on an externally assembled image.
    storage_from_slot: Option<usize>,
    /// Whether `counter`'s own slot was accepted through the legacy
    /// five-sector fallback: [`SaveStore::copy_valid_slot_payloads`] must
    /// then never read that slot's physical positions 5-13, whether erased
    /// or a stale tolerated tail (see [`SaveStore::scan_slot`]).
    legacy: bool,
}

/// The per-position observations [`SaveStore::scan_slot`] accumulates over a
/// slot's 14 physical sectors, and the verdict it draws from them.
///
/// Split from `scan_slot` so the reading of each sector and the judgement
/// made from the whole slot stay separately legible.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent tally over one scan loop, not caller configuration; \
              grouping them into sub-structs would only rename the tallies"
)]
struct SlotSurvey {
    signature_valid: bool,
    valid_ids: u32,
    counter: u32,
    head_valid_ids: u32,
    head_is_identity: bool,
    tail_signature_seen: bool,
    tail_all_recognized_valid: bool,
    /// The footer counter of each checksum-valid tail sector, in position
    /// order; only the first `tail_valid_count` entries are meaningful.
    tail_counters: [u32; PKMN_STORAGE_CHUNKS],
    tail_valid_count: usize,
    /// The rotation every checksum-valid tail sector's id/position pairing
    /// implies, while they all imply the same one.
    tail_rotation: Option<usize>,
    tail_rotation_coherent: bool,
    tail_matches_predecessor_of_identity_head: bool,
    storage_valid_ids: u32,
    /// The footer counter of each checksum-valid storage id (5-13), indexed
    /// by chunk; meaningful only for the ids set in `storage_valid_ids`.
    storage_counters: [u32; PKMN_STORAGE_CHUNKS],
    storage_ids_unique: bool,
    /// The rotation every checksum-valid storage id's position implies,
    /// while they all imply the same one.
    storage_rotation: Option<usize>,
    storage_rotation_coherent: bool,
    /// The footer counter and id of every checksum-valid save-block sector
    /// (ids 0-4) anywhere in the slot, in position order; only the first
    /// `save_block_count` entries are meaningful.
    save_block_counters: [u32; NUM_SECTORS_PER_SLOT],
    save_block_ids: [u16; NUM_SECTORS_PER_SLOT],
    save_block_count: usize,
    legacy_counter: Option<u32>,
    legacy_consistent: bool,
}

impl SlotSurvey {
    fn new() -> Self {
        Self {
            signature_valid: false,
            valid_ids: 0,
            counter: 0,
            head_valid_ids: 0,
            head_is_identity: true,
            tail_signature_seen: false,
            tail_all_recognized_valid: true,
            tail_counters: [0; PKMN_STORAGE_CHUNKS],
            tail_valid_count: 0,
            tail_rotation: None,
            tail_rotation_coherent: true,
            tail_matches_predecessor_of_identity_head: true,
            storage_valid_ids: 0,
            storage_counters: [0; PKMN_STORAGE_CHUNKS],
            storage_ids_unique: true,
            storage_rotation: None,
            storage_rotation_coherent: true,
            save_block_counters: [0; NUM_SECTORS_PER_SLOT],
            save_block_ids: [0; NUM_SECTORS_PER_SLOT],
            save_block_count: 0,
            legacy_counter: None,
            legacy_consistent: true,
        }
    }

    /// Folds the sector at physical position `i` into the survey.
    fn observe(&mut self, i: usize, sector: &Sector) {
        if sector.signature() != SECTOR_SIGNATURE {
            return;
        }
        let in_tail = i >= usize::from(SECTOR_ID_PKMN_STORAGE_START);
        if in_tail {
            self.tail_signature_seen = true;
        }
        self.signature_valid = true;
        let id = sector.id();
        let is_valid_sector = sector_payload_len(id).is_some_and(|len| sector.is_valid(len));
        if in_tail && !is_valid_sector {
            self.tail_all_recognized_valid = false;
        }
        if !is_valid_sector {
            return;
        }
        let counter = sector.counter();
        self.counter = counter;
        self.valid_ids |= 1 << id;
        let rotation = (i + NUM_SECTORS_PER_SLOT - usize::from(id) % NUM_SECTORS_PER_SLOT)
            % NUM_SECTORS_PER_SLOT;
        if id < SECTOR_ID_PKMN_STORAGE_START {
            self.save_block_counters[self.save_block_count] = counter;
            self.save_block_ids[self.save_block_count] = id;
            self.save_block_count += 1;
        }
        // Tracked over the whole slot, not just positions 5-13: a rotated
        // full generation scatters its storage ids across every position,
        // and a damaged slot's surviving storage is worth just as much.
        if id >= SECTOR_ID_PKMN_STORAGE_START {
            // One generation writes each id once. A second checksum-valid
            // copy under the same counter can only be a save-block sector
            // whose unchecksummed footer id was damaged into this one (ids
            // 1-3 share ids 5-12's payload length), and `load`'s donor copy
            // would splice it over the real chunk.
            if self.storage_valid_ids & (1 << id) != 0 {
                self.storage_ids_unique = false;
            }
            self.storage_valid_ids |= 1 << id;
            self.storage_counters[usize::from(id - SECTOR_ID_PKMN_STORAGE_START)] = counter;
            match self.storage_rotation {
                None => self.storage_rotation = Some(rotation),
                Some(r) if r == rotation => {}
                Some(_) => self.storage_rotation_coherent = false,
            }
        }
        if in_tail {
            self.tail_counters[self.tail_valid_count] = counter;
            self.tail_valid_count += 1;
            match self.tail_rotation {
                None => self.tail_rotation = Some(rotation),
                Some(r) if r == rotation => {}
                Some(_) => self.tail_rotation_coherent = false,
            }
            // A torn rotation-0 write also leaves its predecessor
            // generation (rotation 12) at this exact id/position pairing.
            if usize::from(id) != (i + 2) % NUM_SECTORS_PER_SLOT {
                self.tail_matches_predecessor_of_identity_head = false;
            }
        } else if id < SECTOR_ID_PKMN_STORAGE_START {
            self.head_valid_ids |= 1 << id;
            if usize::from(id) != i {
                self.head_is_identity = false;
            }
            match self.legacy_counter {
                None => self.legacy_counter = Some(counter),
                Some(c) if c == counter => {}
                Some(_) => self.legacy_consistent = false,
            }
        }
    }

    /// The generation a complete, unique storage set belongs to: the
    /// counter at least eight of its nine footers agree on.
    ///
    /// The counter sits outside the checksummed payload (upstream's
    /// `CalculateChecksum` sums `data` only, `pokeemerald/src/save.c:674-685`;
    /// likewise [`Sector::is_valid`]), so one damaged footer leaves a chunk
    /// whose bytes still verify. Upstream never compares counters across a
    /// slot's sectors at all (`GetSaveValidStatus`, `save.c:514-585`). A
    /// torn write cannot produce this shape either: it lays new ids 0.. in
    /// order two rotations past the slot's previous generation, so the old
    /// and new sectors it leaves always meet at a duplicated pair of ids and
    /// a missing pair, and a set with neither in 5-13 draws on one
    /// generation only. Anything wider than one outlier is still refused.
    ///
    /// The footer id is unchecksummed too, and ids 1-3 share ids 5-12's
    /// payload length, so a save-block sector relabeled into a storage id
    /// the slot has lost (a legacy head over a rotated remnant drops
    /// whichever storage ids sat in positions 0-4) would complete the set
    /// as that one outlier. One generation lays every id at one rotation,
    /// so the set must imply one; and a relabeled sector keeps its own
    /// generation's genuine counter, which the slot's surviving save-block
    /// sectors still carry. Legacy rotation 3 places id 1 exactly where a
    /// rotation-13 remnant's id 5 sat, so only the counter test catches
    /// that relabel. A damaged counter can land on that same value by
    /// chance, though, and one generation writes each id once: the
    /// relabeled sector *is* that generation's id 1, 2 or 3, so a
    /// generation still holding all three of them relabeled nothing, and
    /// its counter on a storage footer is damage, not a disguised head
    /// sector. Only an outlier matching a generation that has lost one of
    /// ids 1-3 withdraws the set.
    fn storage_generation(&self) -> Option<u32> {
        if !self.storage_rotation_coherent {
            return None;
        }
        let generation = self.storage_counters.iter().copied().find(|&candidate| {
            self.storage_counters
                .iter()
                .filter(|&&counter| counter == candidate)
                .count()
                >= PKMN_STORAGE_CHUNKS - 1
        })?;
        let outlier_may_be_a_relabeled_save_block = self.storage_counters.iter().any(|&counter| {
            counter != generation && self.save_block_generation_may_have_relabeled(counter)
        });
        (!outlier_may_be_a_relabeled_save_block).then_some(generation)
    }

    /// Whether the save-block sectors carrying `counter` could have lost one
    /// of ids 1-3 to a relabeled footer: some sector of that generation is
    /// present, and not all three relabel-capable ids are.
    fn save_block_generation_may_have_relabeled(&self, counter: u32) -> bool {
        /// Ids 1-3: the save-block ids whose payload length matches ids 5-12.
        const RELABEL_CAPABLE_IDS: u32 = 0b1110;
        let mut seen = false;
        let mut ids_present = 0u32;
        for (&c, &id) in self.save_block_counters[..self.save_block_count]
            .iter()
            .zip(&self.save_block_ids[..self.save_block_count])
        {
            if c == counter {
                seen = true;
                ids_present |= 1 << id;
            }
        }
        seen && ids_present & RELABEL_CAPABLE_IDS != RELABEL_CAPABLE_IDS
    }

    /// The generation a stale tail belongs to: one full generation's
    /// layout (every valid sector implying the same rotation) whose
    /// counters all agree but for at most one outlier.
    ///
    /// As with [`Self::storage_generation`], the counter is an
    /// unchecksummed footer, so one damaged counter in data the slot never
    /// loads as progress must not reject the legacy head in front of it.
    /// Rotation coherence keeps a real torn write out: the tail sectors a
    /// torn write at rotation zero lays down beyond position 4 carry ids at
    /// that rotation, while the older generation it failed to finish
    /// overwriting sits two rotations behind, so their mix never shares one
    /// rotation. The head itself stays unanimous.
    fn tail_generation(&self) -> Option<u32> {
        if !self.tail_rotation_coherent {
            return None;
        }
        let counters = &self.tail_counters[..self.tail_valid_count];
        counters.iter().copied().find(|&candidate| {
            let agreeing = counters.iter().filter(|&&c| c == candidate).count();
            agreeing + 1 >= counters.len() && 2 * agreeing > counters.len()
        })
    }

    fn verdict(&self) -> SlotScan {
        let tail_counter = self.tail_generation();
        // A same-slot predecessor sits 2 rotations behind, so an identity
        // head over a rotation-12 tail is indistinguishable from a real
        // write torn after 5 sectors; upstream reports that shape Error,
        // never Ok. The layout decides, not the counters: the predecessor
        // is 2 counters older only while upstream's counter lineage is
        // clean, and one flipped footer bit, which upstream adopts as its
        // next counter, breaks that.
        let ambiguous_with_a_torn_full_write = self.head_is_identity
            && self.tail_valid_count > 0
            && self.tail_matches_predecessor_of_identity_head;

        let stale_tail_is_donor_only = self.tail_signature_seen
            && self.tail_all_recognized_valid
            && tail_counter.is_some()
            && self
                .legacy_counter
                .zip(tail_counter)
                .is_some_and(|(legacy, tail)| older_generation_precedes(tail, legacy))
            && !ambiguous_with_a_torn_full_write;

        // Ids 0-4 land in positions 0-4 together only at rotation zero,
        // where id `i` sits at position `i`; every other rotation pushes id
        // 4 (or more) past position 4. So a non-identity head cannot be a
        // 14-sector write torn after five sectors -- it can only have come
        // from the five-sector writer -- and what sits behind it is stale
        // remnant whatever its state. Damage there costs the tail its
        // donor eligibility, never the head its progress.
        let head_cannot_be_a_torn_full_write = !self.head_is_identity;
        let all_valid_mask = (1u32 << u32::from(NUM_SECTORS_PER_SLOT_U16)) - 1;
        let legacy_intact = self.legacy_consistent
            && self.head_valid_ids == LEGACY_ERA_IDS_MASK
            && (!self.tail_signature_seen
                || stale_tail_is_donor_only
                || head_cannot_be_a_torn_full_write);
        let integrity = if !self.signature_valid {
            SlotIntegrity::Empty
        } else if self.valid_ids == all_valid_mask || legacy_intact {
            SlotIntegrity::Ok
        } else {
            SlotIntegrity::Error
        };
        // A stale tail is scanned after the legacy head, so the trailing
        // `counter` above would otherwise report the tail's older counter
        // instead of the legacy generation's own.
        let counter = if legacy_intact {
            self.legacy_counter
                .expect("legacy_intact requires a legacy head counter")
        } else {
            self.counter
        };
        // Only a complete set holding each id exactly once is worth
        // donating. It also fills all nine tail positions when it is a
        // legacy head's stale tail, so such a tail can never carry the id-0
        // remnant that would steal the head's rotation in
        // `copy_valid_slot_payloads`.
        let storage_counter =
            if self.storage_valid_ids == PKMN_STORAGE_IDS_MASK && self.storage_ids_unique {
                self.storage_generation()
            } else {
                None
            };
        SlotScan {
            integrity,
            counter,
            legacy: legacy_intact,
            storage_counter,
        }
    }
}

struct CopiedSlotPayloads {
    block1: Box<[u8; SaveBlock1::PAYLOAD_LEN]>,
    block2: Box<[u8; SaveBlock2::PAYLOAD_LEN]>,
    pokemon_storage: Box<[u8; PKMN_STORAGE_PAYLOAD_LEN]>,
    valid_block1_chunks: [bool; SAVE_BLOCK1_CHUNKS],
    block2_valid: bool,
}

/// Raw payloads retained across saves for fields the model does not own:
/// [`SaveBlock1`], [`SaveBlock2`], and the nine opaque `PokemonStorage`
/// chunks (ids 5-13).
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
            base_pokemon_storage: boxed_zeroed_pokemon_storage(),
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
    /// Runtime counters and retained bytes (including `PokemonStorage`) start
    /// blank until [`SaveStore::load`] reconstructs them from the image's own
    /// sector footers and payloads: call it before [`SaveStore::save`], or
    /// the write will patch onto that blank base and discard whatever the
    /// image actually held.
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

    /// Copies the retained raw payloads used as the base of the next save.
    ///
    /// A session can restore this snapshot before healing a corrupt image so
    /// unmodelled bytes come from that session rather than an older fallback
    /// slot.
    #[must_use]
    pub fn base_snapshot(&self) -> BaseSnapshot {
        BaseSnapshot {
            block1: self.base_block1.clone(),
            block2: self.base_block2.clone(),
            pokemon_storage: self.base_pokemon_storage.clone(),
        }
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

    /// Writes all 14 logical sectors into the next rotated physical slot
    /// under one save counter, as upstream's `WriteSaveSectorOrSlot` does for
    /// every write (`pokeemerald/src/save.c:138-173`; the single-sector path
    /// is never reached upstream either). Opaque `PokemonStorage` chunks
    /// (ids 5-13) are rewritten unchanged so every generation stays fully
    /// checksummed, matching `HandleWriteSector` rewriting every sector on a
    /// full-slot write.
    pub fn save(&mut self, block1: &SaveBlock1, block2: &SaveBlock2) {
        let mut block2_bytes = self.base_block2.clone();
        let mut block1_bytes = self.base_block1.clone();
        block2.patch_bytes(&mut block2_bytes);
        block1.patch_bytes(&mut block1_bytes, block2.encryption_key);
        let storage_bytes = self.base_pokemon_storage.clone();

        let new_last_written_sector = (self.last_written_sector + 1) % NUM_SECTORS_PER_SLOT_U16;
        let new_save_counter = self.save_counter.wrapping_add(1);
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

    /// Scans all 14 physical positions of `slot`, matching upstream's
    /// `GetSaveValidStatus` (`pokeemerald/src/save.c:514-585`): intact when
    /// all 14 ids validate, or when physical positions 0-4 hold ids 0-4
    /// under one counter (this project's pre-#1227 five-sector format).
    ///
    /// A signed sector in positions 5-13 disqualifies that legacy reading
    /// only when the head could itself be the start of a torn 14-sector
    /// write, which takes an identity head (rotation zero); a head at any
    /// other rotation is accepted over whatever remnant sits behind it. An
    /// identity head needs a stale, strictly older, fully valid tail that
    /// is not laid out at rotation 12, where a genuinely torn write leaves
    /// its predecessor. Either way the
    /// tail is a storage-donor candidate only ([`SaveStore::resolve`]),
    /// never progress ([`SaveStore::copy_valid_slot_payloads`] skips it).
    fn scan_slot(&self, slot: usize) -> SlotScan {
        let mut survey = SlotSurvey::new();
        for i in 0..NUM_SECTORS_PER_SLOT {
            survey.observe(i, &self.read_physical(slot, i));
        }
        survey.verdict()
    }

    /// When both slots are intact and exactly one is the legacy five-sector
    /// fallback, the newer generation still wins, but never by discarding
    /// the other slot's own verified data: if the full-format slot is newer,
    /// it already has everything and wins outright as usual; if the legacy
    /// slot is newer, its SaveBlock1/SaveBlock2 -- the player's latest
    /// progress -- are combined with the full slot's `PokemonStorage` (its
    /// only unique, verified contribution) rather than reverting the save to
    /// the older generation, or dropping the full slot's storage, under a
    /// still-reported `SaveStatus::Ok`. A legacy slot with a newer counter
    /// than a full slot is reachable via a build downgrade (a full write,
    /// then a pre-#1227 five-sector write into the other slot) or an
    /// externally assembled image, not via an ordinary import.
    ///
    /// When no full-format slot is left to merge from -- a cartridge image
    /// imported into a pre-#1227 build and saved twice leaves a legacy head
    /// over an older signed tail in *both* slots -- the adopted generation
    /// instead donates storage from the newer of the two verified stale
    /// storage generations ([`SlotScan::storage_counter`]) -- which may sit
    /// in a slot too damaged to be `Ok` itself. Those bytes are the
    /// player's boxed Pokemon, still physically present in flash because
    /// that writer only ever touched positions 0-4; zeroing them here would
    /// make the next full-slot write destroy them.
    fn resolve(slot0: &SlotScan, slot1: &SlotScan) -> Resolution {
        use SlotIntegrity::{Empty, Error, Ok};
        let (status, counter, storage_from_slot, legacy) = match (slot0.integrity, slot1.integrity)
        {
            (Ok, Ok) => Self::resolve_both_ok(slot0, slot1),
            (Ok, Error) => (
                SaveStatus::Error,
                slot0.counter,
                Self::storage_donor(slot0.legacy, slot0, slot1),
                slot0.legacy,
            ),
            (Ok, Empty) => (
                SaveStatus::Ok,
                slot0.counter,
                Self::storage_donor(slot0.legacy, slot0, slot1),
                slot0.legacy,
            ),
            (Error, Ok) => (
                SaveStatus::Error,
                slot1.counter,
                Self::storage_donor(slot1.legacy, slot0, slot1),
                slot1.legacy,
            ),
            (Empty, Ok) => (
                SaveStatus::Ok,
                slot1.counter,
                Self::storage_donor(slot1.legacy, slot0, slot1),
                slot1.legacy,
            ),
            (Empty, Empty) => (SaveStatus::Empty, 0, None, false),
            (Error | Empty, Error) | (Error, Empty) => (SaveStatus::Corrupt, 0, None, false),
        };
        Resolution {
            status,
            counter,
            storage_from_slot,
            legacy,
        }
    }

    /// The `(Ok, Ok)` half of [`SaveStore::resolve`], split out because it is
    /// the only combination where a slot's fields can be worth merging from
    /// its counterpart.
    fn resolve_both_ok(
        slot0: &SlotScan,
        slot1: &SlotScan,
    ) -> (SaveStatus, u32, Option<usize>, bool) {
        if slot0.legacy == slot1.legacy {
            let counter = if second_counter_is_newer(slot0.counter, slot1.counter) {
                slot1.counter
            } else {
                slot0.counter
            };
            return (
                SaveStatus::Ok,
                counter,
                Self::storage_donor(slot0.legacy, slot0, slot1),
                slot0.legacy,
            );
        }
        let (legacy, full) = if slot0.legacy {
            (slot0, slot1)
        } else {
            (slot1, slot0)
        };
        if second_counter_is_newer(full.counter, legacy.counter) {
            // The full slot is whichever one `legacy` is not: slot 1 when
            // slot 0 holds the legacy generation, slot 0 otherwise.
            let full_slot = usize::from(slot0.legacy);
            (SaveStatus::Ok, legacy.counter, Some(full_slot), true)
        } else {
            (SaveStatus::Ok, full.counter, None, false)
        }
    }

    /// The storage donor for an adopted generation that carries none of its
    /// own.
    ///
    /// A legacy generation never wrote ids 5-13, so without a donor
    /// [`SaveStore::load`] hands back zeroed boxes and the next full write
    /// makes that permanent. Any slot still holding a complete verified
    /// storage generation beats that -- including one whose save-block
    /// sectors are too damaged for the slot to be `Ok` at all, which is
    /// exactly the single-bad-sector case two slots exist to survive. Of
    /// two candidates the newer wins; their counters can sit an arbitrary
    /// number of generations apart, so they are compared as wrapping serial
    /// numbers. A full-format adopted generation needs no donor: its own
    /// storage is already live.
    fn storage_donor(adopted_is_legacy: bool, slot0: &SlotScan, slot1: &SlotScan) -> Option<usize> {
        if !adopted_is_legacy {
            return None;
        }
        match (slot0.storage_counter, slot1.storage_counter) {
            (Some(left), Some(right)) => Some(usize::from(older_generation_precedes(left, right))),
            (Some(_), None) => Some(0),
            (None, Some(_)) => Some(1),
            (None, None) => None,
        }
    }

    /// `legacy` marks the copy that owns a slot's *progress*: a legacy
    /// generation only ever wrote physical positions 0-4, so 5-13 are
    /// skipped outright rather than read as its blocks -- whether they are
    /// plain erased flash or a stale tail [`SaveStore::scan_slot`]
    /// tolerated. [`SaveStore::load`]'s separate storage-donor pass instead
    /// passes `false` precisely to harvest those positions, keeping only
    /// `pokemon_storage` from the result.
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
                // A slot in the pre-#1227 five-sector format never wrote ids
                // 5-13, so this legitimately stays zeroed for it (issue
                // #235's empty-placeholder migration).
                let chunk_num = usize::from(id - SECTOR_ID_PKMN_STORAGE_START);
                let offset = chunk_num * SECTOR_DATA_SIZE;
                copied.pokemon_storage[offset..offset + payload_len]
                    .copy_from_slice(&sector.data()[..payload_len]);
            }
        }

        copied
    }

    /// Validates both slots and loads payloads selected by the resolved
    /// counter's parity.
    ///
    /// The resolved counter's parity selects the slot to copy, even if a
    /// checksum-valid payload has a corrupt footer counter that makes this
    /// differ from the slot preferred during validation. A legacy/full merge
    /// (see [`SaveStore::resolve`]) instead copies `PokemonStorage` from the
    /// other slot; that copy runs first so the final, block-owning copy is
    /// the one that recovers [`SaveStore::last_written_sector`].
    #[must_use]
    pub fn load(&mut self) -> LoadOutcome {
        let scans = [self.scan_slot(0), self.scan_slot(1)];
        let resolution = Self::resolve(&scans[0], &scans[1]);
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

#[cfg(test)]
mod tests;
