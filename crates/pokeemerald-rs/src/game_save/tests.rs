//! Persistence tests use per-test scratch paths and never the per-user save.

use engine::save::{
    SaveBlock1, SaveBlock2, SaveFile, SaveFileError, SaveStore, Sector, SECTOR_DATA_SIZE,
    SECTOR_SIGNATURE, SECTOR_SIZE,
};

use super::{SaveFileStatus, SaveLineage, SaveSlot};

const SAVEBLOCK2_SECTOR_ID: u16 = 0;
const FIRST_SAVEBLOCK1_SECTOR_ID: u16 = 1;
const DEFERRED_PLAY_TIME_BYTE_OFFSET: usize = 0x10;
const FIRST_SAVE_COUNTER: u32 = 1;
const SECOND_SAVE_COUNTER: u32 = 2;
const OLDER_DEFERRED_BYTE: u8 = 0x11;
const CURRENT_DEFERRED_BYTE: u8 = 0x5A;

struct TempSave {
    dir: std::path::PathBuf,
    path: std::path::PathBuf,
}

impl TempSave {
    /// A save in a directory of its own.
    ///
    /// [`engine::save::SaveFile::lock`] takes one lock per save *directory*,
    /// so scratch saves sharing one directory would serialise on a single
    /// lock -- and a test that removed it would strip the exclusion the
    /// others were relying on.
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-game-save-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("scratch directory must be creatable");
        let path = dir.join("pokeemerald.sav");
        Self { dir, path }
    }

    fn slot(&self) -> SaveSlot {
        SaveSlot::at(SaveFile::at(self.path.clone()))
    }
}

impl Drop for TempSave {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.dir));
    }
}

fn no_flash_slot() -> SaveSlot {
    SaveSlot::disabled()
}

fn read_sector(image: &[u8], index: usize) -> Sector {
    let start = index * SECTOR_SIZE;
    Sector::from_bytes(image[start..start + SECTOR_SIZE].try_into().unwrap())
}

fn write_sector(image: &mut [u8], index: usize, sector: &Sector) {
    let start = index * SECTOR_SIZE;
    image[start..start + SECTOR_SIZE].copy_from_slice(sector.as_bytes());
}

fn corrupt_sector_payload(image: &mut [u8], index: usize) {
    image[index * SECTOR_SIZE] ^= u8::MAX;
}

#[test]
fn only_ok_and_error_offer_continue() {
    assert!(SaveFileStatus::Ok.menu_shows_continue());
    assert!(SaveFileStatus::Error.menu_shows_continue());
    assert!(!SaveFileStatus::Corrupt.menu_shows_continue());
    assert!(!SaveFileStatus::Empty.menu_shows_continue());
    assert!(!SaveFileStatus::NoFlash.menu_shows_continue());
}

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

/// `SaveSlot::none` (the headless-real-scenario medium, `App::new_headless_real`)
/// must load as `Empty`, not `NoFlash`: unlike `NoFlash`, `Empty` re-defaults
/// `SaveBlock2` (`SaveFileStatus::boot_clears_save_block2`), so a scripted
/// NEW GAME gets the same boot-defaulted options a real never-saved boot
/// does, instead of `NoFlash`'s zero-filled placeholder bytes.
#[test]
fn a_none_slot_loads_as_empty_not_no_flash() {
    let saved = SaveSlot::none().load();
    assert_eq!(saved.status, SaveFileStatus::Empty);
    assert!(saved.status.boot_clears_save_block2());
    assert!(!saved.status.menu_shows_continue());
}

/// A `none` slot never opens a file, so a write attempt is refused exactly
/// like `disabled`'s -- it must never touch a player's save.
#[test]
fn a_none_slot_never_writes() {
    let mut slot = SaveSlot::none();
    let err = slot
        .store(
            &SaveBlock1::default(),
            &SaveBlock2::default(),
            SaveLineage::NewGame,
        )
        .expect_err("a none slot has no file to write to");
    assert!(matches!(err, SaveFileError::NoDataDirectory));
}

#[test]
fn a_written_slot_loads_back_ok_with_its_blocks() {
    let temp = TempSave::new("ok");
    let mut slot = temp.slot();
    let block2 = SaveBlock2 {
        encryption_key: 0xDEAD_BEEF,
        ..SaveBlock2::default()
    };
    let block1 = SaveBlock1 {
        money: 12_345,
        ..SaveBlock1::default()
    };

    slot.store(&block1, &block2, SaveLineage::Continued)
        .unwrap();

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

#[test]
fn saving_over_an_unreadable_file_fails_instead_of_destroying_it() {
    let temp = TempSave::new("no-clobber");
    std::fs::write(&temp.path, b"not a save file").unwrap();

    let err = temp
        .slot()
        .store(
            &SaveBlock1::default(),
            &SaveBlock2::default(),
            SaveLineage::Continued,
        )
        .unwrap_err();
    assert!(matches!(err, SaveFileError::BadLength { .. }));
    assert_eq!(std::fs::read(&temp.path).unwrap(), b"not a save file");
}

#[test]
fn saving_without_a_resolvable_path_reports_it_rather_than_pretending_to_save() {
    let err = no_flash_slot()
        .store(
            &SaveBlock1::default(),
            &SaveBlock2::default(),
            SaveLineage::Continued,
        )
        .unwrap_err();
    assert!(matches!(err, SaveFileError::NoDataDirectory));
}

#[test]
fn consecutive_saves_advance_the_save_counter_and_alternate_slots() {
    let temp = TempSave::new("rotation");
    let mut slot = temp.slot();
    let block2 = SaveBlock2::default();

    let mut counters = Vec::new();
    for money in [100u32, 200, 300] {
        let block1 = SaveBlock1 {
            money,
            ..SaveBlock1::default()
        };
        slot.store(&block1, &block2, SaveLineage::Continued)
            .unwrap();

        let mut store = SaveFile::at(&temp.path).read().unwrap().unwrap();
        let outcome = store.load();
        assert_eq!(outcome.status, engine::save::SaveStatus::Ok);
        assert_eq!(outcome.block1.money, money, "the newest save must win");
        counters.push(store.save_counter());
    }
    assert_eq!(counters, vec![1, 2, 3]);
}

#[test]
fn a_damaged_lone_save_falls_back_to_the_no_save_menu() {
    let temp = TempSave::new("corrupt");
    let mut slot = temp.slot();
    slot.store(
        &SaveBlock1::default(),
        &SaveBlock2::default(),
        SaveLineage::Continued,
    )
    .unwrap();

    let mut image = std::fs::read(&temp.path).unwrap();
    assert_eq!(image.len(), engine::save::FLASH_IMAGE_LEN);
    let first_written_slot_start = image.len() / 2;
    image[first_written_slot_start] ^= u8::MAX;
    std::fs::write(&temp.path, &image).unwrap();

    let saved = slot.load();
    assert_eq!(saved.status, SaveFileStatus::Corrupt);
    assert!(
        !saved.status.menu_shows_continue(),
        "a corrupt save must fall back to NEW GAME"
    );
}

#[test]
fn the_file_is_the_stores_flash_image_verbatim() {
    let temp = TempSave::new("image");
    let block1 = SaveBlock1::default();
    let block2 = SaveBlock2::default();
    temp.slot()
        .store(&block1, &block2, SaveLineage::Continued)
        .unwrap();

    let mut expected = SaveStore::new();
    expected.save(&block1, &block2);
    assert_eq!(std::fs::read(&temp.path).unwrap(), expected.flash_image());
}

#[test]
fn a_failed_boot_read_disables_saving_even_after_the_medium_recovers() {
    let temp = TempSave::new("latch");
    std::fs::create_dir_all(&temp.path).unwrap();
    let mut slot = temp.slot();
    let saved = slot.load();
    assert_eq!(saved.status, SaveFileStatus::NoFlash);

    std::fs::remove_dir_all(&temp.path).unwrap();
    let block1 = SaveBlock1 {
        money: 424_242,
        ..SaveBlock1::default()
    };
    let block2 = SaveBlock2::default();
    let mut recovered = SaveStore::new();
    recovered.save(&block1, &block2);
    std::fs::write(&temp.path, recovered.flash_image()).unwrap();
    let original = std::fs::read(&temp.path).unwrap();

    let err = slot
        .store(&SaveBlock1::default(), &block2, SaveLineage::Continued)
        .expect_err("a NoFlash session must never write");
    assert!(matches!(err, SaveFileError::NoDataDirectory));
    assert_eq!(
        std::fs::read(&temp.path).unwrap(),
        original,
        "the recovered save must be byte-identical -- this session never loaded it"
    );
}

#[test]
fn a_new_game_store_refuses_to_overwrite_a_continuable_save() {
    let temp = TempSave::new("consent-refuse");
    let mut slot = temp.slot();
    let block2 = SaveBlock2::default();
    slot.store(
        &SaveBlock1 {
            money: 999,
            ..SaveBlock1::default()
        },
        &block2,
        SaveLineage::Continued,
    )
    .unwrap();
    let original = std::fs::read(&temp.path).unwrap();

    let outcome = slot
        .store_unless_foreign_save(&SaveBlock1::default(), &block2, SaveLineage::NewGame)
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::RefusedExistingSave);
    assert_eq!(
        std::fs::read(&temp.path).unwrap(),
        original,
        "a refused store must leave the file byte-identical"
    );
}

#[test]
fn a_new_game_store_writes_over_nothing_and_over_a_corrupt_save() {
    let temp = TempSave::new("consent-allow");
    let mut slot = temp.slot();
    let block2 = SaveBlock2::default();

    let outcome = slot
        .store_unless_foreign_save(&SaveBlock1::default(), &block2, SaveLineage::NewGame)
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);

    let mut image = std::fs::read(&temp.path).unwrap();
    let written_sector = image.len() / 2;
    image[written_sector] ^= 0xFF;
    std::fs::write(&temp.path, &image).unwrap();
    let outcome = slot
        .store_unless_foreign_save(&SaveBlock1::default(), &block2, SaveLineage::NewGame)
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);
}

#[test]
fn a_new_game_over_a_corrupt_save_carries_no_deferred_bytes() {
    let temp = TempSave::new("newgame-corrupt-deferred");
    let block2 = SaveBlock2 {
        encryption_key: 0xDEAD_BEEF,
        ..SaveBlock2::default()
    };
    {
        let mut slot = temp.slot();
        slot.store(&SaveBlock1::default(), &block2, SaveLineage::Continued)
            .unwrap();
        slot.store(&SaveBlock1::default(), &block2, SaveLineage::Continued)
            .unwrap();
    }

    let mut image = std::fs::read(&temp.path).unwrap();
    let mut planted_previous_adventure_byte = false;
    let mut damaged_saveblock1_sectors = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let existing_sector = read_sector(&image, index);
        if existing_sector.signature() != SECTOR_SIGNATURE {
            continue;
        }
        if existing_sector.id() == SAVEBLOCK2_SECTOR_ID
            && existing_sector.counter() == SECOND_SAVE_COUNTER
        {
            let mut payload = block2.to_bytes();
            payload[DEFERRED_PLAY_TIME_BYTE_OFFSET] = CURRENT_DEFERRED_BYTE;
            let replacement =
                Sector::write(SAVEBLOCK2_SECTOR_ID, &payload, existing_sector.counter());
            write_sector(&mut image, index, &replacement);
            planted_previous_adventure_byte = true;
        } else if existing_sector.id() == FIRST_SAVEBLOCK1_SECTOR_ID {
            corrupt_sector_payload(&mut image, index);
            damaged_saveblock1_sectors += 1;
        }
    }
    assert!(
        planted_previous_adventure_byte && damaged_saveblock1_sectors == 2,
        "the fixture must plant one sector and damage one per slot"
    );
    std::fs::write(&temp.path, &image).unwrap();

    let mut slot = temp.slot();
    assert_eq!(slot.load().status, SaveFileStatus::Corrupt);
    let outcome = slot
        .store_unless_foreign_save(
            &SaveBlock1::default(),
            &SaveBlock2::default(),
            SaveLineage::NewGame,
        )
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);

    let saved = temp.slot().load();
    assert!(saved.status.menu_shows_continue());
    assert_eq!(
        saved.block2.encryption_key, 0,
        "the loaded save is the new game, not the old trainer's"
    );
    let image = std::fs::read(&temp.path).unwrap();
    let mut checked = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let sector = read_sector(&image, index);
        if sector.signature() == SECTOR_SIGNATURE
            && sector.id() == SAVEBLOCK2_SECTOR_ID
            && sector.counter() == FIRST_SAVE_COUNTER
        {
            assert_eq!(
                sector.data()[DEFERRED_PLAY_TIME_BYTE_OFFSET],
                0,
                "a new game's deferred bytes start from zero"
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked, 1,
        "the rewritten image must hold exactly the new game's SaveBlock2 sector"
    );
}

#[test]
fn a_new_game_session_clears_the_base_on_an_ordinary_store_too() {
    let temp = TempSave::new("newgame-normal-store-deferred");
    let previous_trainer = SaveBlock2 {
        encryption_key: 0xDEAD_BEEF,
        ..SaveBlock2::default()
    };
    {
        let mut slot = temp.slot();
        slot.store(
            &SaveBlock1::default(),
            &previous_trainer,
            SaveLineage::Continued,
        )
        .unwrap();
    }

    let mut image = std::fs::read(&temp.path).unwrap();
    let mut planted = false;
    for index in 0..image.len() / SECTOR_SIZE {
        let sector = read_sector(&image, index);
        if sector.signature() == SECTOR_SIGNATURE && sector.id() == SAVEBLOCK2_SECTOR_ID {
            let mut payload = previous_trainer.to_bytes();
            payload[DEFERRED_PLAY_TIME_BYTE_OFFSET] = CURRENT_DEFERRED_BYTE;
            let replacement = Sector::write(SAVEBLOCK2_SECTOR_ID, &payload, sector.counter());
            write_sector(&mut image, index, &replacement);
            planted = true;
        }
    }
    assert!(planted, "the fixture must plant one deferred byte");
    std::fs::write(&temp.path, &image).unwrap();

    let mut slot = temp.slot();
    assert!(slot.load().status.menu_shows_continue());
    let outcome = slot
        .store(
            &SaveBlock1::default(),
            &SaveBlock2::default(),
            SaveLineage::NewGame,
        )
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);

    let image = std::fs::read(&temp.path).unwrap();
    let mut checked = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let sector = read_sector(&image, index);
        if sector.signature() == SECTOR_SIGNATURE
            && sector.id() == SAVEBLOCK2_SECTOR_ID
            && sector.counter() == SECOND_SAVE_COUNTER
        {
            assert_eq!(
                sector.data()[DEFERRED_PLAY_TIME_BYTE_OFFSET],
                0,
                "a new game's write must not carry the replaced trainer's \
                 deferred bytes, whichever TrySavingData arm reached it"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 1, "exactly the new game's SaveBlock2 sector");
    assert_eq!(
        temp.slot().load().block2.encryption_key,
        0,
        "the loaded save is the new game, not the replaced trainer's"
    );
}

/// `SaveSlot::store` must run its whole read-modify-write cycle under
/// `SaveFile::lock`, so a locker that hands the lock to a store cannot take
/// it back before that store has written.
#[test]
fn storing_takes_the_inter_process_lock() {
    let temp = TempSave::new("lock-taken");
    let file = SaveFile::at(temp.path.clone());
    let guard = file.lock().expect("the scratch save must be lockable");
    let (acquired, lock_acquired) = std::sync::mpsc::channel();

    let contender = {
        let mut slot = temp.slot();
        std::thread::spawn(move || {
            slot.store_under(
                &SaveBlock1::default(),
                &SaveBlock2::default(),
                true,
                SaveLineage::Continued,
                |file| {
                    let held = file.lock()?;
                    acquired.send(()).unwrap();
                    Ok(held)
                },
            )
            .unwrap();
        })
    };

    drop(guard);
    lock_acquired
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("SaveSlot::store never took SaveFile::lock");
    // Blocks until the store's own guard drops, which happens after its write.
    let reacquired = file
        .lock()
        .expect("the lock must return once the store releases it");
    assert!(
        temp.path.exists(),
        "SaveSlot::store released SaveFile::lock before writing {}",
        temp.path.display()
    );
    drop(reacquired);
    contender.join().expect("the contender must not panic");
}

/// `SaveSlot::store`, the production entry point, reaches `SaveFile::lock`
/// itself: a lock slot that cannot carry a lock stops the store before it
/// reads or writes, which no other path would do.
#[test]
fn storing_through_the_production_entry_point_takes_the_save_file_lock() {
    let temp = TempSave::new("lock-entry-point-slot");
    std::fs::create_dir(temp.dir.join(engine::save::LOCK_FILE_NAME))
        .expect("the scratch lock slot must be occupiable");

    let mut slot = temp.slot();
    let refused = slot
        .store(
            &SaveBlock1::default(),
            &SaveBlock2::default(),
            SaveLineage::Continued,
        )
        .expect_err("SaveSlot::store must take SaveFile::lock, which refuses this slot");

    assert!(
        matches!(refused, SaveFileError::LockPathNotAPlainFile { .. }),
        "SaveSlot::store did not take SaveFile::lock: {refused:?}"
    );
    assert!(
        !temp.path.exists(),
        "SaveSlot::store wrote {} without SaveFile::lock",
        temp.path.display()
    );
}

#[test]
fn a_session_never_overwrites_progress_saved_after_its_own_load() {
    let temp = TempSave::new("stale-session");
    let block2 = SaveBlock2::default();

    let mut initial_writer = temp.slot();
    initial_writer
        .store(
            &SaveBlock1 {
                money: 100,
                ..SaveBlock1::default()
            },
            &block2,
            SaveLineage::Continued,
        )
        .unwrap();

    let mut session_a = temp.slot();
    let mut session_b = temp.slot();
    assert_eq!(session_a.load().status, SaveFileStatus::Ok);
    assert_eq!(session_b.load().status, SaveFileStatus::Ok);

    assert_eq!(
        session_b
            .store(
                &SaveBlock1 {
                    money: 200,
                    ..SaveBlock1::default()
                },
                &block2,
                SaveLineage::Continued,
            )
            .unwrap(),
        super::StoreOutcome::Written
    );
    let session_b_image = std::fs::read(&temp.path).unwrap();

    assert_eq!(
        session_a
            .store(
                &SaveBlock1 {
                    money: 100,
                    ..SaveBlock1::default()
                },
                &block2,
                SaveLineage::Continued,
            )
            .unwrap(),
        super::StoreOutcome::RefusedStaleSession
    );
    assert_eq!(std::fs::read(&temp.path).unwrap(), session_b_image);
    assert_eq!(
        temp.slot().load().block1.money,
        200,
        "the surviving save must be B's, the newest persisted progress"
    );

    assert_eq!(
        session_b
            .store(
                &SaveBlock1 {
                    money: 300,
                    ..SaveBlock1::default()
                },
                &block2,
                SaveLineage::Continued,
            )
            .unwrap(),
        super::StoreOutcome::Written
    );
}

#[test]
fn a_damaged_newest_slot_does_not_refuse_the_sessions_exit_write() {
    let temp = TempSave::new("damaged-newest-slot");
    let block2 = SaveBlock2 {
        encryption_key: 0xBEEF_CAFE,
        ..SaveBlock2::default()
    };
    let mut slot = temp.slot();
    slot.store(&SaveBlock1::default(), &block2, SaveLineage::Continued)
        .unwrap();
    slot.store(&SaveBlock1::default(), &block2, SaveLineage::Continued)
        .unwrap();
    assert_eq!(slot.load().status, SaveFileStatus::Ok);

    let mut image = std::fs::read(&temp.path).unwrap();
    let mut damaged = false;
    for index in 0..image.len() / SECTOR_SIZE {
        let sector = read_sector(&image, index);
        if sector.signature() == SECTOR_SIGNATURE
            && sector.counter() == SECOND_SAVE_COUNTER
            && !damaged
        {
            corrupt_sector_payload(&mut image, index);
            damaged = true;
        }
    }
    assert!(damaged, "the fixture must damage one newest-slot sector");
    std::fs::write(&temp.path, &image).unwrap();

    let outcome = slot
        .store(
            &SaveBlock1 {
                money: 777,
                ..SaveBlock1::default()
            },
            &block2,
            SaveLineage::Continued,
        )
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);
    let saved = temp.slot().load();
    assert_eq!(saved.status, SaveFileStatus::Ok, "the image is healed");
    assert_eq!(
        saved.block1.money, 777,
        "the surviving save is this session's, not the older slot's"
    );
}

#[test]
fn healing_a_damaged_newest_slot_keeps_the_sessions_deferred_lineage() {
    let temp = TempSave::new("heal-lineage");
    let block2 = SaveBlock2 {
        encryption_key: 0xFEED_F00D,
        ..SaveBlock2::default()
    };
    {
        let mut slot = temp.slot();
        slot.store(&SaveBlock1::default(), &block2, SaveLineage::Continued)
            .unwrap();
        slot.store(&SaveBlock1::default(), &block2, SaveLineage::Continued)
            .unwrap();
    }

    let mut image = std::fs::read(&temp.path).unwrap();
    let mut planted = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let sector = read_sector(&image, index);
        if sector.signature() == SECTOR_SIGNATURE && sector.id() == SAVEBLOCK2_SECTOR_ID {
            let mut payload = block2.to_bytes();
            payload[DEFERRED_PLAY_TIME_BYTE_OFFSET] = if sector.counter() == SECOND_SAVE_COUNTER {
                CURRENT_DEFERRED_BYTE
            } else {
                OLDER_DEFERRED_BYTE
            };
            let replacement = Sector::write(SAVEBLOCK2_SECTOR_ID, &payload, sector.counter());
            write_sector(&mut image, index, &replacement);
            planted += 1;
        }
    }
    assert_eq!(planted, 2, "both generations' SaveBlock2 sectors planted");
    std::fs::write(&temp.path, &image).unwrap();

    let mut slot = temp.slot();
    assert_eq!(slot.load().status, SaveFileStatus::Ok);

    let mut image = std::fs::read(&temp.path).unwrap();
    let mut damaged = false;
    for index in 0..image.len() / SECTOR_SIZE {
        let sector = read_sector(&image, index);
        if sector.signature() == SECTOR_SIGNATURE
            && sector.counter() == SECOND_SAVE_COUNTER
            && sector.id() != SAVEBLOCK2_SECTOR_ID
            && !damaged
        {
            corrupt_sector_payload(&mut image, index);
            damaged = true;
        }
    }
    assert!(damaged, "the fixture must damage one newest-slot sector");
    std::fs::write(&temp.path, &image).unwrap();

    let outcome = slot
        .store(
            &SaveBlock1 {
                money: 777,
                ..SaveBlock1::default()
            },
            &block2,
            SaveLineage::Continued,
        )
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);

    let image = std::fs::read(&temp.path).unwrap();
    let mut checked = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let sector = read_sector(&image, index);
        if sector.signature() == SECTOR_SIGNATURE
            && sector.id() == SAVEBLOCK2_SECTOR_ID
            && sector.counter() == SECOND_SAVE_COUNTER
        {
            assert_eq!(
                sector.data()[DEFERRED_PLAY_TIME_BYTE_OFFSET],
                CURRENT_DEFERRED_BYTE,
                "the heal must carry the session's deferred lineage, \
                 not roll back to the older slot's"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 1, "exactly the healed SaveBlock2 sector");
    assert_eq!(
        temp.slot().load().block1.money,
        777,
        "the healed save is this session's state"
    );
}

#[test]
fn counter_is_ahead_orders_across_the_wrap_at_any_distance() {
    const PINNED_HALF_RANGE: u32 = 1 << 31;

    assert!(super::counter_is_ahead(3, 7));
    assert!(!super::counter_is_ahead(7, 3));
    assert!(!super::counter_is_ahead(5, 5));
    assert!(super::counter_is_ahead(u32::MAX, 0));
    assert!(super::counter_is_ahead(u32::MAX, 1));
    assert!(super::counter_is_ahead(u32::MAX - 1, 2));
    assert!(!super::counter_is_ahead(0, u32::MAX));
    assert!(!super::counter_is_ahead(1, u32::MAX));
    assert_eq!(super::SERIAL_COUNTER_HALF_RANGE, PINNED_HALF_RANGE);
    assert!(super::counter_is_ahead(0, PINNED_HALF_RANGE - 1));
    assert!(!super::counter_is_ahead(0, PINNED_HALF_RANGE));
}

#[test]
fn a_stale_session_is_refused_even_across_the_counter_wrap() {
    let block2 = SaveBlock2::default();
    let temp = TempSave::new("stale-across-wrap");

    let mut writer = temp.slot();
    writer
        .store(
            &SaveBlock1 {
                money: 200,
                ..SaveBlock1::default()
            },
            &block2,
            SaveLineage::Continued,
        )
        .unwrap();
    let newest = std::fs::read(&temp.path).unwrap();

    let mut stale = temp.slot();
    stale.load();
    stale.session_counter = Some(u32::MAX);
    assert_eq!(
        stale
            .store(
                &SaveBlock1 {
                    money: 100,
                    ..SaveBlock1::default()
                },
                &block2,
                SaveLineage::Continued,
            )
            .unwrap(),
        super::StoreOutcome::RefusedStaleSession
    );
    assert_eq!(
        std::fs::read(&temp.path).unwrap(),
        newest,
        "the newest persisted progress must be byte-identical after the refusal"
    );
}

/// [`SaveBlock1`] chunk `chunk_num` per upstream's `SAVEBLOCK_CHUNK`
/// (`pokeemerald/src/save.c:44-49`).
fn legacy_block1_chunk(bytes: &[u8; SaveBlock1::PAYLOAD_LEN], chunk_num: usize) -> &[u8] {
    let offset = chunk_num * SECTOR_DATA_SIZE;
    let len = (SaveBlock1::PAYLOAD_LEN - offset).min(SECTOR_DATA_SIZE);
    &bytes[offset..offset + len]
}

/// Builds the five-sector on-disk shape directly, since [`SaveStore`] no
/// longer produces it, to prove a real legacy file still loads and then
/// migrates.
#[test]
fn a_legacy_five_sector_save_file_loads_ok_and_migrates_on_the_next_store() {
    const LEGACY_SECTORS_PER_SLOT: usize = 14;
    const LEGACY_COUNTER: u32 = 4;

    let temp = TempSave::new("legacy-five-sector");
    let block1 = SaveBlock1 {
        money: 54_321,
        ..SaveBlock1::default()
    };
    let block2 = SaveBlock2 {
        encryption_key: 0x1234_5678,
        ..SaveBlock2::default()
    };
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    let block2_bytes = block2.to_bytes();

    let mut image = vec![0xFFu8; engine::save::FLASH_IMAGE_LEN];
    let slot_index = (LEGACY_COUNTER % 2) as usize;
    for id in 0..5u16 {
        let payload: &[u8] = if id == SAVEBLOCK2_SECTOR_ID {
            &block2_bytes[..]
        } else {
            legacy_block1_chunk(&block1_bytes, usize::from(id - FIRST_SAVEBLOCK1_SECTOR_ID))
        };
        let sector = Sector::write(id, payload, LEGACY_COUNTER);
        let global_index = slot_index * LEGACY_SECTORS_PER_SLOT + usize::from(id);
        write_sector(&mut image, global_index, &sector);
    }
    std::fs::write(&temp.path, &image).unwrap();

    let mut slot = temp.slot();
    let saved = slot.load();
    assert_eq!(
        saved.status,
        SaveFileStatus::Ok,
        "a five-sector file must still load as intact"
    );
    assert!(saved.status.menu_shows_continue());
    assert_eq!(saved.block1.money, 54_321);
    assert_eq!(saved.block2.encryption_key, 0x1234_5678);

    slot.store(&saved.block1, &saved.block2, SaveLineage::Continued)
        .unwrap();

    let migrated = std::fs::read(&temp.path).unwrap();
    let newest_slot = usize::try_from((LEGACY_COUNTER + 1) % 2).unwrap();
    let mut valid_ids = 0u32;
    for position in 0..LEGACY_SECTORS_PER_SLOT {
        let sector = read_sector(&migrated, newest_slot * LEGACY_SECTORS_PER_SLOT + position);
        assert_eq!(sector.signature(), SECTOR_SIGNATURE);
        valid_ids |= 1 << sector.id();
    }
    assert_eq!(
        valid_ids,
        (1u32 << LEGACY_SECTORS_PER_SLOT) - 1,
        "the migrated slot must satisfy upstream's all-14-sector invariant"
    );

    let reloaded = temp.slot().load();
    assert_eq!(reloaded.status, SaveFileStatus::Ok);
    assert!(reloaded.status.menu_shows_continue());
    assert_eq!(reloaded.block1.money, 54_321);
    assert_eq!(reloaded.block2.encryption_key, 0x1234_5678);
}

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
/// `survives`, or are all zero otherwise.
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
                assert!(got.iter().all(|&b| b == 0), "storage must be cleared");
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

#[test]
fn at_path_accepts_an_absent_file_as_a_fresh_durable_medium() {
    let save = TempSave::new("at-path-fresh");
    let slot = SaveSlot::at_path(&save.path).expect("an absent save is a fresh medium");
    assert!(
        slot.file.is_some(),
        "the slot must be file-backed, not none()/disabled()"
    );
}

#[test]
fn at_path_reopens_what_an_earlier_session_wrote() {
    let save = TempSave::new("at-path-restart");
    let image = SaveStore::new().flash_image().to_vec();
    std::fs::write(&save.path, &image).unwrap();
    let slot = SaveSlot::at_path(&save.path).expect("a valid image reopens");
    assert_eq!(
        slot.file.as_ref().map(SaveFile::path),
        Some(save.path.as_path())
    );
}

#[test]
fn at_path_fails_closed_when_the_medium_is_unusable() {
    let save = TempSave::new("at-path-unusable");
    std::fs::write(&save.path, b"wrong length").unwrap();
    assert!(matches!(
        SaveSlot::at_path(&save.path),
        Err(SaveFileError::BadLength { .. })
    ));

    std::fs::remove_file(&save.path).unwrap();
    std::fs::create_dir(&save.path).unwrap();
    assert!(SaveSlot::at_path(&save.path).is_err());
}
