//! Inter-process locking and stale-session refusal.

use super::*;

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
            payload[DEFERRED_BYTE_OFFSET] = if sector.counter() == SECOND_SAVE_COUNTER {
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
                sector.data()[DEFERRED_BYTE_OFFSET],
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
