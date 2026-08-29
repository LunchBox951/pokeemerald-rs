//! Persistence tests use per-test scratch paths and never the per-user save.

use engine::save::{
    SaveBlock1, SaveBlock2, SaveFile, SaveFileError, SaveStore, Sector, SECTOR_SIGNATURE,
    SECTOR_SIZE,
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

#[test]
fn storing_takes_the_inter_process_lock() {
    let temp = TempSave::new("lock-taken");
    let mut slot = temp.slot();
    slot.store(
        &SaveBlock1::default(),
        &SaveBlock2::default(),
        SaveLineage::Continued,
    )
    .unwrap();

    let mut lock_path = temp.path.clone().into_os_string();
    lock_path.push(".lock");
    let lock_path = std::path::PathBuf::from(lock_path);
    assert!(
        lock_path.exists(),
        "SaveSlot::store must acquire SaveFile::lock, which creates {}",
        lock_path.display()
    );
    drop(std::fs::remove_file(lock_path));
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
    assert!(super::counter_is_ahead(3, 7));
    assert!(!super::counter_is_ahead(7, 3));
    assert!(!super::counter_is_ahead(5, 5));
    assert!(super::counter_is_ahead(u32::MAX, 0));
    assert!(super::counter_is_ahead(u32::MAX, 1));
    assert!(super::counter_is_ahead(u32::MAX - 1, 2));
    assert!(!super::counter_is_ahead(0, u32::MAX));
    assert!(!super::counter_is_ahead(1, u32::MAX));
    assert!(super::counter_is_ahead(
        0,
        super::SERIAL_COUNTER_HALF_RANGE - 1
    ));
    assert!(!super::counter_is_ahead(
        0,
        super::SERIAL_COUNTER_HALF_RANGE
    ));
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
