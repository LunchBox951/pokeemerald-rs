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
    /// held once, anywhere in this slot (at most one footer counter may
    /// disagree; see `SlotSurvey::storage_generation`): the box data still
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
// Each flag is one independent "has this property held so far" tally over a
// single loop, not a configuration a caller passes: grouping them into
// sub-structs would only rename the tallies.
#[allow(clippy::struct_excessive_bools)]
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
        }
        if in_tail {
            self.tail_counters[self.tail_valid_count] = counter;
            self.tail_valid_count += 1;
            let rotation = (i + NUM_SECTORS_PER_SLOT - usize::from(id) % NUM_SECTORS_PER_SLOT)
                % NUM_SECTORS_PER_SLOT;
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
    fn storage_generation(&self) -> Option<u32> {
        self.storage_counters.iter().copied().find(|&candidate| {
            self.storage_counters
                .iter()
                .filter(|&&counter| counter == candidate)
                .count()
                >= PKMN_STORAGE_CHUNKS - 1
        })
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
        // Same-slot generations sit exactly 2 counters and 2 rotations
        // apart, so this is indistinguishable from a real write torn after 5
        // sectors; upstream reports that shape Error, never Ok.
        let ambiguous_with_a_torn_full_write = self.head_is_identity
            && self.tail_matches_predecessor_of_identity_head
            && self
                .legacy_counter
                .zip(tail_counter)
                .is_some_and(|(legacy, tail)| legacy == tail.wrapping_add(2));

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
    /// is not the one shape a genuinely torn write leaves. Either way the
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

    fn sample_block2() -> SaveBlock2 {
        SaveBlock2 {
            player_name: *b"RUSTY\xFF\0\0",
            player_gender: PlayerGender::Female,
            player_trainer_id: [1, 2, 3, 4],
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

        for i in 0..NUM_SECTORS_PER_SLOT {
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
            options_text_speed: 1,
            options_window_frame_type: 5,
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

    /// Builds a slot exactly as this project's pre-#1227 code did: ids 0-4
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

    #[test]
    fn a_legacy_five_sector_slot_loads_ok_and_is_migrated_on_the_next_save() {
        let mut store = SaveStore::new();
        write_legacy_era_slot(&mut store, 1, 3, 1);

        let outcome = store.load();
        assert_eq!(
            outcome.status,
            SaveStatus::Ok,
            "a pre-#1227 slot must still load as intact (issue #235)"
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
    /// was written by the current (post-#1227) 14-sector code, not the
    /// legacy five-sector era, even if that sector's own checksum is
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

    /// A full-14 slot can carry real `PokemonStorage` bytes (e.g. an
    /// imported image) while a legacy-shaped generation with a numerically
    /// newer counter sits in the other slot -- reachable via a build
    /// downgrade (a full write, then a pre-#1227 five-sector write into the
    /// other slot) or an externally assembled image. `load` must adopt the
    /// newer legacy generation (never silently revert to the older full
    /// slot) while still carrying the full slot's verified storage bytes
    /// forward, rather than dropping either under a still-reported
    /// `SaveStatus::Ok`. `migrating_a_newer_legacy_slot_keeps_its_progress_
    /// and_the_full_slot_storage` below covers the case where the two
    /// slots' blocks actually differ.
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

    /// Writes a pre-#1227 five-sector generation (ids 0-4 only, positions
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

    /// As [`write_legacy_slot`], at the given rotation: the pre-#1227 writer
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

    /// A rotation-zero cartridge image whose older slot has one damaged
    /// sector in its tail. The pre-#1227 build accepted such an image (its
    /// scan only read positions 0-4), recovered rotation 0 from the newest
    /// slot's id 0, and wrote its next five-sector generation at rotation 1
    /// over the older slot -- leaving a legacy head that no 14-sector write
    /// can produce, since a torn full write only ever fills positions 0-4
    /// from rotation zero, where id `i` lands at position `i`. Withholding
    /// that damaged tail as a storage donor is right; rejecting the intact
    /// legacy head with it throws away the session the player just saved.
    #[test]
    fn a_rotated_legacy_head_survives_a_damaged_stale_tail() {
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
        write_full_slot(&mut store, 0, &older_block1, &block2, &storage_bytes, 2);
        write_full_slot(&mut store, 1, &older_block1, &block2, &storage_bytes, 1);
        // Flash damage to one storage sector of the older slot's tail.
        store.corrupt_byte(1, 7, 4);
        write_legacy_slot_rotated(&mut store, 1, &newer_block1, &block2, 3, 1);

        let outcome = store.load();
        assert_eq!(
            outcome.status,
            SaveStatus::Ok,
            "a complete legacy head no torn full write can imitate stays intact"
        );
        assert_eq!(
            store.save_counter(),
            3,
            "the legacy generation the player just saved must be adopted"
        );
        assert_eq!(
            outcome.block1.money, newer_block1.money,
            "reverting to the older full slot would silently undo that session"
        );
        assert_eq!(
            &store.base_pokemon_storage[..],
            &storage_bytes[..],
            "storage still comes from the intact full slot, never the damaged tail"
        );
    }

    /// Two-slot redundancy exists so one bad sector cannot cost the player
    /// anything. When the counterpart slot is `Error` only because a
    /// save-block sector is damaged, all nine of its storage sectors can
    /// still validate -- the boxes are intact and merely unreachable
    /// through that slot's blocks. The accepted legacy generation wrote no
    /// storage of its own, so without a donor `load` hands back zeroes,
    /// `SaveFileStatus::Error` still offers CONTINUE, and the next save
    /// rewrites those zeroes over the surviving sectors.
    #[test]
    fn a_damaged_full_slot_still_donates_its_intact_storage() {
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
        // Slot 0: a full generation at counter 2 with one damaged
        // SaveBlock1 chunk (id 1, at position 1 for a rotation-zero write).
        write_full_slot(&mut store, 0, &older_block1, &block2, &storage_bytes, 2);
        store.corrupt_byte(0, 1, 4);
        // Slot 1: a pre-#1227 five-sector generation over erased flash, so
        // it has no stale tail of its own to donate.
        write_legacy_slot(&mut store, 1, &newer_block1, &block2, 3);

        assert_eq!(
            store.scan_slot(0).integrity,
            SlotIntegrity::Error,
            "the damaged SaveBlock chunk must still disqualify the slot itself"
        );

        let outcome = store.load();
        assert_eq!(
            outcome.status,
            SaveStatus::Error,
            "a damaged slot is still reported, exactly as upstream does"
        );
        assert_eq!(store.save_counter(), 3);
        assert_eq!(
            outcome.block1.money, newer_block1.money,
            "progress still comes from the intact legacy generation"
        );
        assert_eq!(
            &store.base_pokemon_storage[..],
            &storage_bytes[..],
            "the damaged slot's nine intact storage sectors must be salvaged, not \
             zeroed and then overwritten by the next save"
        );
    }

    /// A footer id is outside the sector checksum, and ids 1-3 (`SaveBlock1`
    /// chunks 0-2) share ids 5-12's payload length, so a bit flip turning id 1
    /// into id 5 leaves both copies checksum-valid under one counter with
    /// all nine storage ids still present. At rotation 10 the relabeled
    /// chunk sits at position 11, after the real id 5 at position 1, so a
    /// donor copy would splice `SaveBlock1` bytes over the first box chunk
    /// and the next save would persist them.
    #[test]
    fn a_duplicated_storage_id_is_never_donated_as_storage() {
        const ROTATION: u16 = 10;
        let block2 = sample_block2();
        let older_block1 = SaveBlock1 {
            money: 111,
            ..sample_block1()
        };
        let newer_block1 = SaveBlock1 {
            money: 222,
            ..sample_block1()
        };
        let block2_bytes = block2.to_bytes();
        let block1_bytes = older_block1.to_bytes(block2.encryption_key);
        let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];

        let mut store = SaveStore::new();
        // Slot 0: a complete full generation at counter 10, rotation 10.
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
            let physical = usize::from((id + ROTATION) % NUM_SECTORS_PER_SLOT_U16);
            store.write_physical(0, physical, &Sector::write(id, payload, 10));
        }
        // One bit of id 1's footer flips, relabeling it id 5.
        let relabeled =
            usize::from((SECTOR_ID_SAVEBLOCK1_START + ROTATION) % NUM_SECTORS_PER_SLOT_U16);
        // The footer ends id (u16), checksum (u16), signature, counter (u32s).
        let id_offset = SECTOR_SIZE - 2 * size_of::<u32>() - 2 * size_of::<u16>();
        let mut bytes = *store.read_physical(0, relabeled).as_bytes();
        bytes[id_offset] ^= 0x04;
        store.write_physical(0, relabeled, &Sector::from_bytes(bytes));
        assert_eq!(
            store.read_physical(0, relabeled).id(),
            SECTOR_ID_PKMN_STORAGE_START
        );
        assert!(store
            .read_physical(0, relabeled)
            .is_valid(sector_payload_len(SECTOR_ID_PKMN_STORAGE_START).unwrap()));
        // Slot 1: a newer pre-#1227 generation with no storage of its own.
        write_legacy_slot(&mut store, 1, &newer_block1, &block2, 11);

        assert!(
            store.scan_slot(0).storage_counter.is_none(),
            "a slot holding two copies of a storage id must not be offered as a donor"
        );
        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Error);
        assert_eq!(outcome.block1.money, newer_block1.money);
        let first_chunk = &store.base_pokemon_storage[..SECTOR_DATA_SIZE];
        assert_ne!(
            first_chunk,
            &block1_bytes[..SECTOR_DATA_SIZE],
            "SaveBlock1 bytes must never be loaded as a box chunk"
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

    /// `scan_slot` accepts each slot on its own contents; nothing in it ties
    /// a generation's counter to the physical slot it sits in. That mapping
    /// is only an invariant `SaveStore::save` maintains (as upstream's
    /// `gSaveCounter % NUM_SAVE_SLOTS` does), so an externally assembled
    /// image -- the same input class the legacy/full merge exists for -- can
    /// present a full slot whose counter parity points at the *other* slot.
    /// The merge must then still take `PokemonStorage` from the full slot
    /// the scan actually found, never from the legacy slot's erased tail.
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

        // Two pre-#1227 saves, into slot 1 then slot 0 by parity. Slot 0's
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
        // Slot 1 held a full generation at counter 2; a pre-#1227 write at
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

    /// A sector's footer counter sits outside the payload its checksum
    /// covers, upstream (`pokeemerald/src/save.c:674-685` sums `data` only)
    /// and here ([`Sector::is_valid`]), so flash damage there leaves every
    /// id and checksum intact. Both slots hold legacy heads, and only slot
    /// 1's stale tail is a complete storage set; one bit of one of its
    /// counters must not cost the player every boxed Pokemon when the other
    /// eight chunks still agree on their generation.
    #[test]
    fn one_damaged_storage_counter_still_leaves_a_complete_tail_donatable() {
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
        write_full_slot(&mut store, 1, &older_block1, &block2, &storage_bytes, 2);
        // The counter's high byte of storage id 9, at position 9: footer
        // only, so its checksum still holds.
        store.corrupt_byte(1, 9, SECTOR_SIZE - 1);
        assert!(store.read_physical(1, 9).is_valid(SECTOR_DATA_SIZE));
        write_legacy_slot_rotated(&mut store, 1, &older_block1, &block2, 3, 1);
        write_legacy_slot(&mut store, 0, &newer_block1, &block2, 4);

        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Ok);
        assert_eq!(store.save_counter(), 4);
        assert_eq!(outcome.block1.money, newer_block1.money);
        assert!(
            store.base_pokemon_storage[..] == storage_bytes[..],
            "an isolated counter outlier must not zero an otherwise complete storage set"
        );
    }

    /// One outlier is flash damage; two disagreeing footers are no longer a
    /// set this scan can vouch for, so the donor rule stays strict there.
    #[test]
    fn two_damaged_storage_counters_withdraw_the_tail_as_a_donor() {
        let block2 = sample_block2();
        let storage_bytes = vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN];

        let mut store = SaveStore::new();
        write_full_slot(&mut store, 1, &sample_block1(), &block2, &storage_bytes, 2);
        store.corrupt_byte(1, 9, SECTOR_SIZE - 1);
        write_legacy_slot_rotated(&mut store, 1, &sample_block1(), &block2, 3, 1);
        assert_eq!(store.scan_slot(1).storage_counter, Some(2));

        store.corrupt_byte(1, 11, SECTOR_SIZE - 1);
        assert!(
            store.scan_slot(1).storage_counter.is_none(),
            "two counter outliers must not be offered as a donor"
        );
    }

    /// A tail missing even one storage id is not a generation that ever
    /// existed, so it is never donated: splicing its surviving chunks over
    /// zeroes would hand the player a half-real PC. Slot 1's rotation-13
    /// remnant loses id 5 to the legacy head that overwrote position 4.
    #[test]
    fn an_incomplete_stale_tail_is_never_donated_as_storage() {
        let block2 = sample_block2();
        let legacy_block1 = SaveBlock1 {
            money: 15,
            ..sample_block1()
        };

        let mut store = SaveStore::new();
        for _ in 0..12 {
            store.save(&sample_block1(), &block2);
        }
        store.base_pokemon_storage.fill(0x0D);
        store.save(&sample_block1(), &block2);
        assert_eq!(store.save_counter(), 13);
        assert_eq!(store.last_written_sector(), 13);

        // Slot 1 only: positions 5-13 keep ids 6-13 and 0, never id 5.
        write_legacy_slot(&mut store, 1, &legacy_block1, &block2, 15);
        assert!(
            store.scan_slot(1).storage_counter.is_none(),
            "a tail missing a storage id must not be offered as a donor"
        );
    }

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

    #[test]
    fn stale_tail_counter_precedence_is_wraparound_aware() {
        assert!(older_generation_precedes(3, 7));
        assert!(!older_generation_precedes(7, 3));
        assert!(!older_generation_precedes(5, 5));
        // A legitimate stale tail an arbitrary number of generations behind
        // the legacy head, including across the u32 wrap.
        assert!(older_generation_precedes(u32::MAX, 1));
        // A tail that is actually the *newer* side of the same wrap must
        // never be read as older just because its raw value is smaller.
        assert!(!older_generation_precedes(0, u32::MAX - 1));
    }

    /// An identity legacy head over the rotation-13 remnant an imported
    /// image leaves in slot 1: one bit of one stale tail sector's
    /// unchecksummed counter footer is flash damage in data the slot never
    /// loads as progress, and must not cost the player the legacy
    /// generation they just saved.
    #[test]
    fn an_identity_legacy_head_survives_one_damaged_stale_tail_counter() {
        let block2 = sample_block2();
        let legacy_block1 = SaveBlock1 {
            money: 15,
            ..sample_block1()
        };

        let mut store = SaveStore::new();
        for _ in 0..12 {
            store.save(&sample_block1(), &block2);
        }
        store.save(&sample_block1(), &block2);
        store.save(&sample_block1(), &block2);
        assert_eq!(store.save_counter(), 14);

        write_legacy_slot(&mut store, 1, &legacy_block1, &block2, 15);
        // The counter's high byte of the tail sector at position 7: footer
        // only, so its checksum still holds.
        store.corrupt_byte(1, 7, SECTOR_SIZE - 1);
        assert!(store.read_physical(1, 7).is_valid(SECTOR_DATA_SIZE));

        let outcome = store.load();
        assert_eq!(
            outcome.status,
            SaveStatus::Ok,
            "one outlier counter in a stale tail must not reject the legacy head"
        );
        assert_eq!(store.save_counter(), 15);
        assert_eq!(
            outcome.block1.money, legacy_block1.money,
            "reverting to the older counter-14 slot would undo the legacy session"
        );
    }

    /// The same damage over a complete rotation-zero tail: every id then
    /// validates, so without the consensus the slot falls through to the
    /// full-format path under the tail's own older counter, and the other
    /// slot's older legacy generation wins under a still-reported `Ok`.
    #[test]
    fn an_identity_legacy_head_over_a_complete_tail_keeps_its_counter_despite_one_damaged_tail_counter(
    ) {
        let block2 = sample_block2();
        let counter_15_block1 = SaveBlock1 {
            money: 15,
            ..sample_block1()
        };
        let counter_16_block1 = SaveBlock1 {
            money: 16,
            ..sample_block1()
        };

        let mut store = SaveStore::new();
        for _ in 0..12 {
            store.save(&sample_block1(), &block2);
        }
        store.base_pokemon_storage.fill(0x0D);
        store.save(&sample_block1(), &block2);
        store.base_pokemon_storage.fill(0x0E);
        store.save(&sample_block1(), &block2);
        assert_eq!(store.last_written_sector(), 0);
        let counter_14_storage = store.base_pokemon_storage.clone();

        write_legacy_slot(&mut store, 1, &counter_15_block1, &block2, 15);
        write_legacy_slot(&mut store, 0, &counter_16_block1, &block2, 16);
        store.corrupt_byte(0, 9, SECTOR_SIZE - 1);
        assert!(store.read_physical(0, 9).is_valid(SECTOR_DATA_SIZE));

        let outcome = store.load();
        assert_eq!(outcome.status, SaveStatus::Ok);
        assert_eq!(
            store.save_counter(),
            16,
            "the newer legacy head's own counter must be adopted, not the tail's"
        );
        assert_eq!(outcome.block1.money, counter_16_block1.money);
        assert_eq!(&store.base_pokemon_storage[..], &counter_14_storage[..]);
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
}
