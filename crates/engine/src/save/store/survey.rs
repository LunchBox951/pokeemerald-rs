//! Per-slot survey and cross-slot verdict for [`super::SaveStore::load`]:
//! what each slot's 14 physical sectors say, and which generation wins.

use super::super::sector::{Sector, SECTOR_SIGNATURE};
use super::{
    sector_payload_len, SaveStatus, NUM_SAVE_SLOTS, NUM_SAVE_SLOTS_U32, NUM_SECTORS_PER_SLOT,
    NUM_SECTORS_PER_SLOT_U16, PKMN_STORAGE_CHUNKS, SECTOR_ID_PKMN_STORAGE_START,
};

/// The rotation of the same-slot predecessor a rotation-zero write lays
/// over: two saves, so two rotations, behind.
const TORN_WRITE_PREDECESSOR_ROTATION: usize = NUM_SECTORS_PER_SLOT - NUM_SAVE_SLOTS;
const LEGACY_ERA_IDS_MASK: u32 = (1 << SECTOR_ID_PKMN_STORAGE_START) - 1;
/// The nine opaque `PokemonStorage` sector ids (5-13) as a bitmask.
const PKMN_STORAGE_IDS_MASK: u32 =
    ((1u32 << PKMN_STORAGE_CHUNKS) - 1) << SECTOR_ID_PKMN_STORAGE_START;

/// Compares counters from adjacent save generations, including the sole
/// `u32::MAX` to zero wrap.
#[must_use]
pub(super) fn second_counter_is_newer(first: u32, second: u32) -> bool {
    match (first, second) {
        (u32::MAX, 0) => true,
        (0, u32::MAX) => false,
        _ => first < second,
    }
}

/// Whether `older` precedes `newer` as a wrapping serial number (RFC 1982).
/// Sound for any generation gap, unlike [`second_counter_is_newer`], which
/// assumes adjacent generations.
#[must_use]
pub(super) fn older_generation_precedes(older: u32, newer: u32) -> bool {
    let delta = newer.wrapping_sub(older);
    delta != 0 && delta < (1 << 31)
}

/// The value all but at most one of `values` share, held by a strict
/// majority of them.
pub(super) fn one_outlier_consensus<T: Copy + Eq>(values: &[T]) -> Option<T> {
    values.iter().copied().find(|&candidate| {
        let agreeing = values.iter().filter(|&&v| v == candidate).count();
        agreeing + 1 >= values.len() && 2 * agreeing > values.len()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SlotIntegrity {
    Empty,
    /// All 14 sectors validate, or the slot holds a five-sector generation
    /// (see [`super::SaveStore::scan_slot`]).
    Ok,
    Error,
}

pub(super) struct SlotScan {
    pub(super) integrity: SlotIntegrity,
    pub(super) counter: u32,
    /// Whether an `Ok` integrity came from the legacy five-sector fallback
    /// rather than all 14 sectors validating. See [`resolve`].
    pub(super) legacy: bool,
    /// The generation of a complete, checksum-valid storage set (ids 5-13)
    /// found anywhere in this slot, whatever the slot's own integrity; see
    /// `SlotSurvey::storage_generation` for what qualifies.
    pub(super) storage_counter: Option<u32>,
}

/// The physical slots [`super::SaveStore::load`] copies each half of its result
/// from.
pub(super) struct Resolution {
    pub(super) status: SaveStatus,
    /// The adopted generation number: reported by `save_counter()` and used
    /// to pick both the next save's physical slot and (absent a merge) the
    /// slot every field is copied from.
    pub(super) counter: u32,
    /// The physical slot to source `PokemonStorage` from when the adopted
    /// generation carries none of its own ([`storage_donor`]). A
    /// scanned index, not a counter: an assembled image need not keep the
    /// parity [`super::SaveStore::save`] maintains.
    pub(super) storage_from_slot: Option<usize>,
    /// Whether `counter`'s own slot was accepted through the legacy
    /// five-sector fallback: [`super::SaveStore::copy_valid_slot_payloads`] must
    /// then never read that slot's physical positions 5-13, whether erased
    /// or a stale tolerated tail (see [`super::SaveStore::scan_slot`]).
    pub(super) legacy: bool,
}

/// The per-position observations [`super::SaveStore::scan_slot`] accumulates over a
/// slot's 14 physical sectors, and the verdict it draws from them.
///
/// Split from `scan_slot` so the reading of each sector and the judgement
/// made from the whole slot stay separately legible.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent tally over one scan loop, not caller configuration; \
              grouping them into sub-structs would only rename the tallies"
)]
pub(super) struct SlotSurvey {
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
    /// The rotation each checksum-valid tail sector's id/position pairing
    /// implies, parallel to `tail_counters`.
    tail_rotations: [usize; PKMN_STORAGE_CHUNKS],
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
    pub(super) fn new() -> Self {
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
            tail_rotations: [0; PKMN_STORAGE_CHUNKS],
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
    pub(super) fn observe(&mut self, i: usize, sector: &Sector) {
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
            // whose unchecksummed footer id was damaged into this one (every
            // save-block payload fits within ids 5-12's), and `load`'s donor copy
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
            self.tail_rotations[self.tail_valid_count] = rotation;
            self.tail_valid_count += 1;
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

    /// The generation a complete, unique storage set belongs to: one
    /// rotation and one counter, with at most one counter outlier (footers
    /// are unchecksummed, `pokeemerald/src/save.c:674-685`). An outlier that
    /// completes no save-block generation and is the next write into this
    /// slot is a relabeled save-block sector and withdraws the set.
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
        let outlier_is_accepted = self.storage_counters.iter().all(|&counter| {
            counter == generation
                || self.save_block_generation_is_complete(counter)
                || !self.is_next_write_into_this_slot(generation, counter)
        });
        outlier_is_accepted.then_some(generation)
    }

    /// Whether `counter` is the write after one this slot already carries:
    /// upstream alternates slots by counter parity (`save.c:138-173`), so the
    /// next write into a slot is two counters on.
    fn is_next_write_into_this_slot(&self, generation: u32, counter: u32) -> bool {
        let successor_of = |c: u32| c.wrapping_add(NUM_SAVE_SLOTS_U32) == counter;
        successor_of(generation)
            || self.save_block_counters[..self.save_block_count]
                .iter()
                .any(|&c| successor_of(c))
    }

    /// Whether the save-block sectors carrying `counter` hold all of ids 0-4.
    fn save_block_generation_is_complete(&self, counter: u32) -> bool {
        /// Ids 0-4: every save-block payload fits within ids 5-12's, and
        /// the zero padding after the shorter ids 0 and 4 adds nothing to
        /// the word-sum checksum, so each verifies under a storage id.
        const SAVE_BLOCK_IDS: u32 = LEGACY_ERA_IDS_MASK;
        let ids_present = self.save_block_counters[..self.save_block_count]
            .iter()
            .zip(&self.save_block_ids[..self.save_block_count])
            .filter(|&(&c, _)| c == counter)
            .fold(0u32, |ids, (_, &id)| ids | 1 << id);
        ids_present & SAVE_BLOCK_IDS == SAVE_BLOCK_IDS
    }

    /// The generation a stale tail belongs to, as its rotation and
    /// counter: one layout and one counter, each with at most one outlier
    /// sector (both footers are unchecksummed). A rotation outlier under the
    /// head's own counter is refused; that is a torn full write, not a
    /// remnant.
    fn tail_generation(&self) -> Option<(usize, u32)> {
        let counters = &self.tail_counters[..self.tail_valid_count];
        let rotations = &self.tail_rotations[..self.tail_valid_count];
        let rotation = one_outlier_consensus(rotations)?;
        let counter = one_outlier_consensus(counters)?;
        let mut outliers = rotations
            .iter()
            .zip(counters)
            .filter(|&(&r, &c)| r != rotation || c != counter);
        let outlier = outliers.next();
        if outliers.next().is_some() {
            return None;
        }
        let rotation_outlier_is_the_heads_generation =
            outlier.is_some_and(|(&r, &c)| r != rotation && self.legacy_counter == Some(c));
        (!rotation_outlier_is_the_heads_generation).then_some((rotation, counter))
    }

    pub(super) fn verdict(&self) -> SlotScan {
        let tail = self.tail_generation();
        let tail_counter = tail.map(|(_, counter)| counter);
        // An identity head over a rotation-12 tail is the shape a full
        // write torn after five sectors leaves; upstream reads it Error.
        let ambiguous_with_a_torn_full_write = self.head_is_identity
            && tail.is_some_and(|(rotation, _)| rotation == TORN_WRITE_PREDECESSOR_ROTATION);

        let stale_tail_is_donor_only = self.tail_signature_seen
            && self.tail_all_recognized_valid
            && tail_counter.is_some()
            && self
                .legacy_counter
                .zip(tail_counter)
                .is_some_and(|(legacy, tail)| older_generation_precedes(tail, legacy))
            && !ambiguous_with_a_torn_full_write;

        // Only an identity head can be a torn full write, so a rotated head
        // is accepted over any remnant, unless a tail sector carries its own
        // counter: the five-sector writer never wrote positions 5-13, so
        // that head is a full generation missing an id, Error upstream.
        let tail_shares_head_generation = self.legacy_counter.is_some_and(|legacy| {
            self.tail_counters[..self.tail_valid_count].contains(&legacy)
                && !tail_counter.is_some_and(|tail| older_generation_precedes(tail, legacy))
        });
        let head_cannot_be_a_torn_full_write =
            !self.head_is_identity && !tail_shares_head_generation;
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

/// Picks the generation to load. The newer intact slot wins; when it is
/// a five-sector generation, `PokemonStorage` comes from the newest
/// complete verified storage set in either slot instead of zeros.
pub(super) fn resolve(slot0: &SlotScan, slot1: &SlotScan) -> Resolution {
    use SlotIntegrity::{Empty, Error, Ok};
    let (status, counter, storage_from_slot, legacy) = match (slot0.integrity, slot1.integrity) {
        (Ok, Ok) => resolve_both_ok(slot0, slot1),
        (Ok, Error) => (
            SaveStatus::Error,
            slot0.counter,
            storage_donor(slot0.legacy, slot0, slot1),
            slot0.legacy,
        ),
        (Ok, Empty) => (
            SaveStatus::Ok,
            slot0.counter,
            storage_donor(slot0.legacy, slot0, slot1),
            slot0.legacy,
        ),
        (Error, Ok) => (
            SaveStatus::Error,
            slot1.counter,
            storage_donor(slot1.legacy, slot0, slot1),
            slot1.legacy,
        ),
        (Empty, Ok) => (
            SaveStatus::Ok,
            slot1.counter,
            storage_donor(slot1.legacy, slot0, slot1),
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

/// The `(Ok, Ok)` half of [`resolve`], split out because it is
/// the only combination where a slot's fields can be worth merging from
/// its counterpart.
fn resolve_both_ok(slot0: &SlotScan, slot1: &SlotScan) -> (SaveStatus, u32, Option<usize>, bool) {
    if slot0.legacy == slot1.legacy {
        let counter = if second_counter_is_newer(slot0.counter, slot1.counter) {
            slot1.counter
        } else {
            slot0.counter
        };
        return (
            SaveStatus::Ok,
            counter,
            storage_donor(slot0.legacy, slot0, slot1),
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

/// The storage donor for an adopted five-sector generation: whichever
/// slot holds the newest complete verified storage set, even one too
/// damaged to be `Ok` itself. Counters compare as wrapping serials.
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
