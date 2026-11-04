//! Donor-storage reconciliation and equal-counter replacement refusal.

use super::*;

const LEGACY_HEAD_COUNTER: u32 = 3;
const DONOR_COUNTER: u32 = 2;
const DONOR_STORAGE_FILL: u8 = 0xAB;
const FIRST_STORAGE_SECTOR_ID: u16 = 5;
const SECTORS_PER_SLOT: usize = 14;
const STORAGE_SECTOR_COUNT: usize = 9;

fn storage_chunk(storage: &[u8], id: u16) -> &[u8] {
    let offset = usize::from(id - FIRST_STORAGE_SECTOR_ID) * SECTOR_DATA_SIZE;
    &storage[offset..(offset + SECTOR_DATA_SIZE).min(storage.len())]
}

/// A full-format donor generation (counter 2, slot 0) under an intact
/// legacy five-sector head (counter 3, slot 1) that carries the progress.
fn write_legacy_head_with_donor(temp: &TempSave) -> (SaveBlock1, SaveBlock2, Vec<u8>) {
    write_legacy_head_with_donor_filled(temp, DONOR_STORAGE_FILL)
}

/// [`write_legacy_head_with_donor`] with every donor storage byte set to
/// `storage_fill`.
fn write_legacy_head_with_donor_filled(
    temp: &TempSave,
    storage_fill: u8,
) -> (SaveBlock1, SaveBlock2, Vec<u8>) {
    let block1 = SaveBlock1 {
        money: 8_888,
        ..SaveBlock1::default()
    };
    let block2 = SaveBlock2 {
        encryption_key: 0x0BAD_CAFE,
        ..SaveBlock2::default()
    };
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    let block2_bytes = block2.to_bytes();
    let storage = vec![storage_fill; engine::save::store::PKMN_STORAGE_PAYLOAD_LEN];
    let mut image = vec![0xFFu8; engine::save::FLASH_IMAGE_LEN];
    for id in 0..u16::try_from(SECTORS_PER_SLOT).unwrap() {
        let payload: &[u8] = if id == SAVEBLOCK2_SECTOR_ID {
            &block2_bytes[..]
        } else if id < FIRST_STORAGE_SECTOR_ID {
            legacy_block1_chunk(&block1_bytes, usize::from(id - FIRST_SAVEBLOCK1_SECTOR_ID))
        } else {
            storage_chunk(&storage, id)
        };
        let donor = Sector::write(id, payload, DONOR_COUNTER);
        write_sector(&mut image, usize::from(id), &donor);
        if id < FIRST_STORAGE_SECTOR_ID {
            let head = Sector::write(id, payload, LEGACY_HEAD_COUNTER);
            write_sector(&mut image, SECTORS_PER_SLOT + usize::from(id), &head);
        }
    }
    std::fs::write(&temp.path, &image).unwrap();
    (block1, block2, storage)
}

fn boot_legacy_head_with_donor(temp: &TempSave) -> (SaveSlot, SaveBlock1, SaveBlock2, Vec<u8>) {
    let (block1, block2, storage) = write_legacy_head_with_donor(temp);
    let mut slot = temp.slot();
    let saved = slot.load();
    assert_eq!(saved.status, SaveFileStatus::Ok);
    assert_eq!(saved.block1.money, 8_888);
    assert_eq!(slot.session_counter, Some(LEGACY_HEAD_COUNTER));
    assert!(slot.session_merged_donor);
    (slot, block1, block2, storage)
}

/// Corrupts the donor's nine storage sectors on disk, leaving the head.
fn damage_donor_storage(temp: &TempSave) {
    let mut image = std::fs::read(&temp.path).unwrap();
    let mut damaged = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let sector = read_sector(&image, index);
        if sector.signature() == SECTOR_SIGNATURE
            && sector.counter() == DONOR_COUNTER
            && sector.id() >= FIRST_STORAGE_SECTOR_ID
        {
            corrupt_sector_payload(&mut image, index);
            damaged += 1;
        }
    }
    assert_eq!(damaged, STORAGE_SECTOR_COUNT);
    std::fs::write(&temp.path, &image).unwrap();
}

/// Asserts generation `counter`'s storage sectors equal `storage` when
/// `survives`, or are a new game's default storage (never the donor's)
/// otherwise.
fn assert_newest_storage(temp: &TempSave, counter: u32, storage: &[u8], survives: bool) {
    let image = std::fs::read(&temp.path).unwrap();
    let mut checked = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let sector = read_sector(&image, index);
        if sector.signature() == SECTOR_SIGNATURE
            && sector.counter() == counter
            && sector.id() >= FIRST_STORAGE_SECTOR_ID
        {
            let want = storage_chunk(storage, sector.id());
            let got = &sector.data()[..want.len()];
            if survives {
                assert_eq!(got, want, "storage sector {} must survive", sector.id());
            } else {
                assert_ne!(got, want, "donor storage must not survive a new game");
                let defaults = engine::save::PokemonStorage::new_game().to_payload();
                assert_eq!(got, storage_chunk(&defaults[..], sector.id()));
            }
            checked += 1;
        }
    }
    assert_eq!(checked, STORAGE_SECTOR_COUNT);
}

#[test]
fn equal_counter_donor_loss_heals_the_boxes_on_the_next_save() {
    let temp = TempSave::new("donor-loss-heal");
    let (mut slot, block1, block2, storage) = boot_legacy_head_with_donor(&temp);
    damage_donor_storage(&temp);
    let outcome = slot
        .store(&block1, &block2, SaveLineage::Continued)
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);
    assert_newest_storage(&temp, LEGACY_HEAD_COUNTER + 1, &storage, true);
}

#[test]
fn equal_counter_same_image_does_not_restore_the_base_on_ordinary_saves() {
    use super::{reconcile_base, BaseReconcile};
    assert_eq!(reconcile_base(3, 3, true, false), BaseReconcile::Keep);
    assert_eq!(reconcile_base(3, 3, false, true), BaseReconcile::Restore);
    assert_eq!(reconcile_base(3, 3, false, false), BaseReconcile::Refuse);
    assert_eq!(reconcile_base(2, 3, true, false), BaseReconcile::Restore);
    assert_eq!(reconcile_base(4, 3, false, false), BaseReconcile::Keep);

    let temp = TempSave::new("donor-same-image");
    let (mut slot, block1, block2, storage) = boot_legacy_head_with_donor(&temp);
    for generation in 1..=2 {
        let outcome = slot
            .store(&block1, &block2, SaveLineage::Continued)
            .unwrap();
        assert_eq!(outcome, super::StoreOutcome::Written);
        assert_newest_storage(&temp, LEGACY_HEAD_COUNTER + generation, &storage, true);
    }
}

#[test]
fn donor_loss_does_not_bypass_refused_stale_session() {
    let temp = TempSave::new("donor-loss-stale");
    let (mut slot, block1, block2, _storage) = boot_legacy_head_with_donor(&temp);
    damage_donor_storage(&temp);
    let mut other = temp.slot();
    other.load();
    other
        .store(&block1, &block2, SaveLineage::Continued)
        .unwrap();
    let before = std::fs::read(&temp.path).unwrap();
    let outcome = slot
        .store(&block1, &block2, SaveLineage::Continued)
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::RefusedStaleSession);
    assert_eq!(std::fs::read(&temp.path).unwrap(), before);
    assert_eq!(slot.session_counter, Some(LEGACY_HEAD_COUNTER));
}

#[test]
fn equal_counter_donor_loss_respects_clears_base_for_a_new_game() {
    let temp = TempSave::new("donor-loss-clears-base");
    let (mut slot, block1, block2, storage) = boot_legacy_head_with_donor(&temp);
    damage_donor_storage(&temp);
    let outcome = slot.store(&block1, &block2, SaveLineage::NewGame).unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);
    assert_newest_storage(&temp, LEGACY_HEAD_COUNTER + 1, &storage, false);
}

#[test]
fn equal_counter_donor_loss_does_not_bypass_the_foreign_save_refusal() {
    let temp = TempSave::new("donor-loss-foreign");
    let (mut slot, block1, block2, _storage) = boot_legacy_head_with_donor(&temp);
    damage_donor_storage(&temp);
    let before = std::fs::read(&temp.path).unwrap();
    let outcome = slot
        .store_unless_foreign_save(&block1, &block2, SaveLineage::Continued)
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::RefusedExistingSave);
    assert_eq!(std::fs::read(&temp.path).unwrap(), before);
}

const REPLACED_COUNTER: u32 = 2;
const SESSION_STORAGE_FILL: u8 = 0x11;
const REPLACEMENT_STORAGE_FILL: u8 = 0x22;

/// The blocks both images of the equal-counter replacement share.
fn replacement_blocks() -> (SaveBlock1, SaveBlock2) {
    (
        SaveBlock1 {
            money: 4_242,
            ..SaveBlock1::default()
        },
        SaveBlock2 {
            encryption_key: 0x1234_5678,
            ..SaveBlock2::default()
        },
    )
}

/// Writes one intact full-format generation `counter` into its parity slot of
/// an otherwise erased image, with every storage byte set to `storage_fill`.
fn write_full_generation(
    temp: &TempSave,
    counter: u32,
    block1: &SaveBlock1,
    block2: &SaveBlock2,
    storage_fill: u8,
) {
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    let block2_bytes = block2.to_bytes();
    let storage = vec![storage_fill; engine::save::store::PKMN_STORAGE_PAYLOAD_LEN];
    let first = usize::try_from(counter % 2).unwrap() * SECTORS_PER_SLOT;
    let mut image = vec![0xFFu8; engine::save::FLASH_IMAGE_LEN];
    for id in 0..u16::try_from(SECTORS_PER_SLOT).unwrap() {
        let payload: &[u8] = if id == SAVEBLOCK2_SECTOR_ID {
            &block2_bytes[..]
        } else if id < FIRST_STORAGE_SECTOR_ID {
            legacy_block1_chunk(&block1_bytes, usize::from(id - FIRST_SAVEBLOCK1_SECTOR_ID))
        } else {
            storage_chunk(&storage, id)
        };
        write_sector(
            &mut image,
            first + usize::from(id),
            &Sector::write(id, payload, counter),
        );
    }
    std::fs::write(&temp.path, &image).unwrap();
}

/// Asserts all nine storage sectors of physical `slot` belong to generation
/// `counter` and hold `fill`.
fn assert_slot_storage(temp: &TempSave, slot: usize, counter: u32, fill: u8) {
    let image = std::fs::read(&temp.path).unwrap();
    let mut checked = 0;
    for index in slot * SECTORS_PER_SLOT..(slot + 1) * SECTORS_PER_SLOT {
        let sector = read_sector(&image, index);
        if sector.id() >= FIRST_STORAGE_SECTOR_ID {
            assert_eq!(sector.counter(), counter, "slot {slot} storage generation");
            let want = storage_chunk(
                &vec![fill; engine::save::store::PKMN_STORAGE_PAYLOAD_LEN],
                sector.id(),
            )
            .len();
            assert!(
                sector.data()[..want].iter().all(|&b| b == fill),
                "slot {slot} storage sector {} must hold {fill:#04x}",
                sector.id()
            );
            checked += 1;
        }
    }
    assert_eq!(checked, STORAGE_SECTOR_COUNT);
}

/// A continued session over save A must not carry A's
/// storage over a same-counter save B swapped in after load. Restoring the
/// session base would put A's storage in the next generation, and the save
/// after that would overwrite B's slot too.
#[test]
fn an_equal_counter_replacement_is_refused_rather_than_overwritten() {
    let temp = TempSave::new("equal-counter-replacement");
    let (block1, block2) = replacement_blocks();
    write_full_generation(
        &temp,
        REPLACED_COUNTER,
        &block1,
        &block2,
        SESSION_STORAGE_FILL,
    );
    let mut slot = temp.slot();
    assert_eq!(slot.load().status, SaveFileStatus::Ok);
    assert_eq!(slot.session_counter, Some(REPLACED_COUNTER));

    write_full_generation(
        &temp,
        REPLACED_COUNTER,
        &block1,
        &block2,
        REPLACEMENT_STORAGE_FILL,
    );
    assert_eq!(temp.slot().load().status, SaveFileStatus::Ok);
    let replacement = std::fs::read(&temp.path).unwrap();

    for _ in 0..2 {
        let outcome = slot
            .store(&block1, &block2, SaveLineage::Continued)
            .unwrap();
        assert_eq!(outcome, super::StoreOutcome::RefusedConflictingSave);
        assert_eq!(std::fs::read(&temp.path).unwrap(), replacement);
        assert_eq!(slot.session_counter, Some(REPLACED_COUNTER));
    }
    assert_slot_storage(&temp, 0, REPLACED_COUNTER, REPLACEMENT_STORAGE_FILL);
}

/// A session that merged a legacy donor still refuses a same-counter
/// replacement whose head is no longer a five-sector legacy head: the
/// session's provenance alone does not prove the donor was lost.
#[test]
fn a_merged_donor_session_refuses_a_full_format_replacement_at_its_counter() {
    let temp = TempSave::new("donor-session-replacement");
    let (mut slot, block1, block2, _storage) = boot_legacy_head_with_donor(&temp);
    write_full_generation(
        &temp,
        LEGACY_HEAD_COUNTER,
        &block1,
        &block2,
        REPLACEMENT_STORAGE_FILL,
    );
    assert_eq!(temp.slot().load().status, SaveFileStatus::Ok);
    let replacement = std::fs::read(&temp.path).unwrap();
    let outcome = slot
        .store(&block1, &block2, SaveLineage::Continued)
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::RefusedConflictingSave);
    assert_eq!(std::fs::read(&temp.path).unwrap(), replacement);
}

/// A replacement that keeps the same legacy head but
/// supplies a different valid donor still reloads as a merged donor with
/// matching head blocks. A donor that is present is not lost, so the session
/// refuses rather than restoring its boxes over the replacement donor.
#[test]
fn a_replacement_donor_under_the_same_legacy_head_is_refused() {
    let temp = TempSave::new("replacement-donor");
    let (mut slot, block1, block2, _storage) = boot_legacy_head_with_donor(&temp);
    write_legacy_head_with_donor_filled(&temp, REPLACEMENT_STORAGE_FILL);
    assert_eq!(temp.slot().load().status, SaveFileStatus::Ok);
    let replacement = std::fs::read(&temp.path).unwrap();

    for _ in 0..2 {
        let outcome = slot
            .store(&block1, &block2, SaveLineage::Continued)
            .unwrap();
        assert_eq!(outcome, super::StoreOutcome::RefusedConflictingSave);
        assert_eq!(std::fs::read(&temp.path).unwrap(), replacement);
        assert_eq!(slot.session_counter, Some(LEGACY_HEAD_COUNTER));
    }
    assert_slot_storage(&temp, 0, DONOR_COUNTER, REPLACEMENT_STORAGE_FILL);
}

/// One damaged donor storage sector withdraws the whole donor (only a
/// complete set of nine valid sectors may donate), so partial damage reloads
/// with no donor and still heals rather than being refused.
#[test]
fn partial_donor_damage_still_heals_the_boxes_on_the_next_save() {
    let temp = TempSave::new("partial-donor-loss");
    let (mut slot, block1, block2, storage) = boot_legacy_head_with_donor(&temp);
    let mut image = std::fs::read(&temp.path).unwrap();
    let index = (0..image.len() / SECTOR_SIZE)
        .find(|&index| {
            let sector = read_sector(&image, index);
            sector.signature() == SECTOR_SIGNATURE
                && sector.counter() == DONOR_COUNTER
                && sector.id() >= FIRST_STORAGE_SECTOR_ID
        })
        .unwrap();
    corrupt_sector_payload(&mut image, index);
    std::fs::write(&temp.path, &image).unwrap();
    let outcome = slot
        .store(&block1, &block2, SaveLineage::Continued)
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);
    assert_newest_storage(&temp, LEGACY_HEAD_COUNTER + 1, &storage, true);
}
