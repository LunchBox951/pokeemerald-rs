//! Ordinary save-store behaviour, new-game handling, and `SaveSlot::at_path`.

use super::*;

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
            payload[DEFERRED_BYTE_OFFSET] = CURRENT_DEFERRED_BYTE;
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
                sector.data()[DEFERRED_BYTE_OFFSET],
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
            payload[DEFERRED_BYTE_OFFSET] = CURRENT_DEFERRED_BYTE;
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
                sector.data()[DEFERRED_BYTE_OFFSET],
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
