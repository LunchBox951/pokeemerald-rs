//! Unit tests for [`super::SaveSlot`] and the `gSaveFileStatus` -> menu-type
//! mapping.
//!
//! Every test drives its own scratch file under `std::env::temp_dir()`,
//! removed on drop; none reads or writes the real per-user save.

use engine::save::{SaveBlock1, SaveBlock2, SaveFile, SaveFileError, SaveStore};

use super::{SaveFileStatus, SaveSlot};

/// A scratch save path, removed on drop (including on unwind).
struct TempSave {
    path: std::path::PathBuf,
}

impl TempSave {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-game-save-{label}-{}-{:?}.sav",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_file(&path));
        Self { path }
    }

    fn slot(&self) -> SaveSlot {
        SaveSlot::at(SaveFile::at(self.path.clone()))
    }
}

impl Drop for TempSave {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.path));
    }
}

/// A slot with no resolvable path -- upstream's `gFlashMemoryPresent !=
/// TRUE`. Built by hand because [`SaveSlot::default_location`] can only
/// produce it on a host with no data-directory environment at all.
fn no_flash_slot() -> SaveSlot {
    SaveSlot { file: None }
}

// -- `gSaveFileStatus` -> `tMenuType` (main_menu.c:641-670) ---------------

#[test]
fn only_ok_and_error_offer_continue() {
    // `SAVE_STATUS_OK` (`:643-648`) and `SAVE_STATUS_ERROR` (`:654-660`)
    // both set `tMenuType = HAS_SAVED_GAME`.
    assert!(SaveFileStatus::Ok.menu_shows_continue());
    assert!(SaveFileStatus::Error.menu_shows_continue());
    // `SAVE_STATUS_CORRUPT` (`:649-653`), `SAVE_STATUS_EMPTY` (`:661-665`),
    // and `SAVE_STATUS_NO_FLASH` (`:666-670`) all set `HAS_NO_SAVED_GAME`.
    assert!(!SaveFileStatus::Corrupt.menu_shows_continue());
    assert!(!SaveFileStatus::Empty.menu_shows_continue());
    assert!(!SaveFileStatus::NoFlash.menu_shows_continue());
}

// -- boot load ------------------------------------------------------------

#[test]
fn a_slot_with_no_file_loads_as_empty_not_as_an_error() {
    let temp = TempSave::new("empty");
    let saved = temp.slot().load();
    assert_eq!(saved.status, SaveFileStatus::Empty);
    assert!(!saved.status.menu_shows_continue());
}

#[test]
fn a_slot_with_no_resolvable_path_loads_as_no_flash() {
    let saved = no_flash_slot().load();
    assert_eq!(saved.status, SaveFileStatus::NoFlash);
    assert!(!saved.status.menu_shows_continue());
}

#[test]
fn a_written_slot_loads_back_ok_with_its_blocks() {
    let temp = TempSave::new("ok");
    let slot = temp.slot();
    let block2 = SaveBlock2 {
        encryption_key: 0xDEAD_BEEF,
        ..SaveBlock2::default()
    };
    let block1 = SaveBlock1 {
        money: 12_345,
        ..SaveBlock1::default()
    };

    slot.store(&block1, &block2).unwrap();

    let saved = slot.load();
    assert_eq!(saved.status, SaveFileStatus::Ok);
    assert!(saved.status.menu_shows_continue());
    assert_eq!(saved.block1.money, 12_345);
    assert_eq!(saved.block2.encryption_key, 0xDEAD_BEEF);
}

#[test]
fn a_file_that_is_not_a_save_image_loads_as_no_flash() {
    let temp = TempSave::new("junk");
    std::fs::write(&temp.path, b"not a save file").unwrap();

    let saved = temp.slot().load();
    assert_eq!(
        saved.status,
        SaveFileStatus::NoFlash,
        "an unreadable medium is upstream's SAVE_STATUS_NO_FLASH, \
         not a corrupt save"
    );
    assert!(!saved.status.menu_shows_continue());
}

/// The "never overwrite what we could not read" rule
/// (`TrySavingData`'s `gFlashMemoryPresent` guard, `save.c:765-771`): a file
/// that is not a save image must survive a save attempt untouched, so a
/// misplaced file (or a save from a future format) is recoverable by hand
/// rather than destroyed on the way out.
#[test]
fn saving_over_an_unreadable_file_fails_instead_of_destroying_it() {
    let temp = TempSave::new("no-clobber");
    std::fs::write(&temp.path, b"not a save file").unwrap();

    let err = temp
        .slot()
        .store(&SaveBlock1::default(), &SaveBlock2::default())
        .unwrap_err();
    assert!(matches!(err, SaveFileError::BadLength { .. }));
    assert_eq!(std::fs::read(&temp.path).unwrap(), b"not a save file");
}

#[test]
fn saving_without_a_resolvable_path_reports_it_rather_than_pretending_to_save() {
    let err = no_flash_slot()
        .store(&SaveBlock1::default(), &SaveBlock2::default())
        .unwrap_err();
    assert!(matches!(err, SaveFileError::NoDataDirectory));
}

/// Upstream's rotation, end to end through the file: `HandleSavingData`
/// bumps `gSaveCounter` on every full-slot write and its parity picks the
/// physical slot, so consecutive saves alternate slots. This port re-derives
/// both counters from the image on each save
/// ([`SaveSlot::store`]'s doc comment), which must produce the same
/// sequence a RAM-resident counter would.
#[test]
fn consecutive_saves_advance_the_save_counter_and_alternate_slots() {
    let temp = TempSave::new("rotation");
    let slot = temp.slot();
    let block2 = SaveBlock2::default();

    let mut counters = Vec::new();
    for money in [100u32, 200, 300] {
        let block1 = SaveBlock1 {
            money,
            ..SaveBlock1::default()
        };
        slot.store(&block1, &block2).unwrap();

        let mut store = SaveFile::at(&temp.path).read().unwrap().unwrap();
        let outcome = store.load();
        assert_eq!(outcome.status, engine::save::SaveStatus::Ok);
        assert_eq!(outcome.block1.money, money, "the newest save must win");
        counters.push(store.save_counter());
    }
    assert_eq!(counters, vec![1, 2, 3]);
}

/// A slot whose one written save has a damaged sector, with nothing in the
/// other slot, is `SAVE_STATUS_CORRUPT` -- and upstream answers that with
/// `HAS_NO_SAVED_GAME` plus "the save file has been erased"
/// (`main_menu.c:649-653`). The discard semantics themselves are
/// `engine::save::store`'s (issues #130/#138); what is under test here is
/// that this crate reads them through unchanged rather than second-guessing
/// them.
#[test]
fn a_damaged_lone_save_falls_back_to_the_no_save_menu() {
    let temp = TempSave::new("corrupt");
    let slot = temp.slot();
    slot.store(&SaveBlock1::default(), &SaveBlock2::default())
        .unwrap();

    // Flip one payload byte of the slot that first save actually wrote.
    // `gSaveCounter` advances to 1 before the write and its parity selects
    // the physical slot, so a first save lands in slot 1 -- the second half
    // of the image. Slot 0 is still erased, leaving no intact slot at all.
    let mut image = std::fs::read(&temp.path).unwrap();
    assert_eq!(image.len(), engine::save::FLASH_IMAGE_LEN);
    let slot_one = image.len() / 2;
    image[slot_one] ^= 0xFF;
    std::fs::write(&temp.path, &image).unwrap();

    let saved = slot.load();
    assert_eq!(saved.status, SaveFileStatus::Corrupt);
    assert!(
        !saved.status.menu_shows_continue(),
        "a corrupt save must fall back to NEW GAME"
    );
}

/// The image on disk is exactly the store's own flash image -- no wrapper,
/// no header of this port's invention. Pinned so a future format change is a
/// deliberate act with a test to update, not a silent one.
#[test]
fn the_file_is_the_stores_flash_image_verbatim() {
    let temp = TempSave::new("image");
    let block1 = SaveBlock1::default();
    let block2 = SaveBlock2::default();
    temp.slot().store(&block1, &block2).unwrap();

    let mut expected = SaveStore::new();
    expected.save(&block1, &block2);
    assert_eq!(std::fs::read(&temp.path).unwrap(), expected.flash_image());
}
