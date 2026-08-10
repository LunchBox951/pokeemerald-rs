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
/// TRUE`, and the save-isolated medium headless validation uses.
fn no_flash_slot() -> SaveSlot {
    SaveSlot::disabled()
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
    let mut slot = temp.slot();
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
    let mut slot = temp.slot();
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
    let mut slot = temp.slot();
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

/// The `NoFlash` latch (issue #214 review): a boot read that *fails* --
/// rather than finding no file -- must disable saving for the whole
/// session, even if the medium recovers afterwards. Upstream's
/// `gFlashMemoryPresent` stays FALSE for the session the same way; without
/// the latch, a later `store` would re-read the now-healthy file and
/// overwrite a save this session never loaded.
#[test]
fn a_failed_boot_read_disables_saving_even_after_the_medium_recovers() {
    let temp = TempSave::new("latch");
    // A *directory* at the save path fails the read with an I/O error --
    // the transient-failure stand-in -- without being "not found".
    std::fs::create_dir_all(&temp.path).unwrap();
    let mut slot = temp.slot();
    let saved = slot.load();
    assert_eq!(saved.status, SaveFileStatus::NoFlash);

    // The medium "recovers": a perfectly valid save appears at the path.
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
        .store(&SaveBlock1::default(), &block2)
        .expect_err("a NoFlash session must never write");
    assert!(matches!(err, SaveFileError::NoDataDirectory));
    assert_eq!(
        std::fs::read(&temp.path).unwrap(),
        original,
        "the recovered save must be byte-identical -- this session never loaded it"
    );
}

// -- the exit-write consent gate (issue #214 review) ----------------------

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
    )
    .unwrap();
    let original = std::fs::read(&temp.path).unwrap();

    let outcome = slot
        .store_unless_foreign_save(&SaveBlock1::default(), &block2)
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

    // Nothing on disk: `Empty` is not continuable, so the write proceeds.
    let outcome = slot
        .store_unless_foreign_save(&SaveBlock1::default(), &block2)
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);

    // A corrupted image is not continuable either -- there is no save left
    // to protect, exactly the case upstream's HAS_NO_SAVED_GAME menu
    // already treats as fresh ground.
    let mut image = std::fs::read(&temp.path).unwrap();
    let written_sector = image.len() / 2;
    image[written_sector] ^= 0xFF;
    std::fs::write(&temp.path, &image).unwrap();
    let outcome = slot
        .store_unless_foreign_save(&SaveBlock1::default(), &block2)
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);
}

/// #230 review, second round: a NEW GAME written over a `Corrupt` image
/// must not inherit the previous trainer's deferred bytes. `store_impl`'s
/// pre-write load retains the corrupt image's still-validating sectors as
/// the serialization base, so the `preserve_existing` (new-game) path
/// zeroes that base first, exactly as upstream's `Sav2_ClearSetDefault`
/// memsets `gSaveBlock1/2` before a new game's first save.
#[test]
fn a_new_game_over_a_corrupt_save_carries_no_deferred_bytes() {
    use engine::save::{Sector, SECTOR_SIGNATURE, SECTOR_SIZE};

    // Upstream's sector ids within a slot: SaveBlock2 is sector 0,
    // SaveBlock1 fills sectors 1..=4.
    const SECTOR_ID_SAVEBLOCK2: u16 = 0;
    const SECTOR_ID_SAVEBLOCK1_FIRST: u16 = 1;
    // A SaveBlock2 payload offset no field models yet (play-time region).
    const B2_DEFERRED: usize = 0x10;

    let temp = TempSave::new("newgame-corrupt-deferred");
    let block2 = SaveBlock2 {
        encryption_key: 0xDEAD_BEEF,
        ..SaveBlock2::default()
    };
    {
        // Two saves, so the newest slot (counter 2) has the parity the
        // adopted counter 0 selects after corruption: `Corrupt` resets the
        // counter to 0 and the copy pass still best-effort reads slot
        // `0 % 2` -- the slot the deferred byte must sit in for the
        // pre-write load to retain it.
        let mut slot = temp.slot();
        slot.store(&SaveBlock1::default(), &block2).unwrap(); // counter 1, slot 1
        slot.store(&SaveBlock1::default(), &block2).unwrap(); // counter 2, slot 0
    }

    // Plant a deferred byte in the newest slot's (still checksum-valid)
    // SaveBlock2 sector and break one SaveBlock1 sector's payload in
    // *each* slot: the image resolves `Corrupt`, while its surviving
    // sectors -- the planted one included -- individually validate, which
    // is exactly what the pre-write load copies into the retained base.
    let mut image = std::fs::read(&temp.path).unwrap();
    let mut planted = false;
    let mut damaged = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let start = index * SECTOR_SIZE;
        let sector = Sector::from_bytes(image[start..start + SECTOR_SIZE].try_into().unwrap());
        if sector.signature() != SECTOR_SIGNATURE {
            continue;
        }
        if sector.id() == SECTOR_ID_SAVEBLOCK2 && sector.counter() == 2 {
            let mut payload = block2.to_bytes();
            payload[B2_DEFERRED] = 0x5A;
            let replacement = Sector::write(SECTOR_ID_SAVEBLOCK2, &payload, sector.counter());
            image[start..start + SECTOR_SIZE].copy_from_slice(replacement.as_bytes());
            planted = true;
        } else if sector.id() == SECTOR_ID_SAVEBLOCK1_FIRST {
            image[start] ^= 0xFF; // break the payload checksum, not the footer
            damaged += 1;
        }
    }
    assert!(
        planted && damaged == 2,
        "the fixture must plant one sector and damage one per slot"
    );
    std::fs::write(&temp.path, &image).unwrap();

    // A fresh session boots against the corrupt image -- the menu offers
    // NEW GAME only -- and writes its new game over it.
    let mut slot = temp.slot();
    assert_eq!(slot.load().status, SaveFileStatus::Corrupt);
    let outcome = slot
        .store_unless_foreign_save(&SaveBlock1::default(), &SaveBlock2::default())
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);

    // The new game went to slot 1 (adopted counter 0 -> written counter 1);
    // the wrecked old slot 0 remains, so the image reloads as the
    // best-effort `Error` -- continuable -- with the *new game's* blocks.
    let saved = temp.slot().load();
    assert!(saved.status.menu_shows_continue());
    assert_eq!(
        saved.block2.encryption_key, 0,
        "the loaded save is the new game, not the old trainer's"
    );
    // And the new game's own SaveBlock2 sector (counter 1) carries a
    // zeroed deferred byte -- not the previous trainer's 0x5A.
    let image = std::fs::read(&temp.path).unwrap();
    let mut checked = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let start = index * SECTOR_SIZE;
        let sector = Sector::from_bytes(image[start..start + SECTOR_SIZE].try_into().unwrap());
        if sector.signature() == SECTOR_SIGNATURE
            && sector.id() == SECTOR_ID_SAVEBLOCK2
            && sector.counter() == 1
        {
            assert_eq!(
                sector.data()[B2_DEFERRED],
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

/// The save path really takes the inter-process lock (issue #214 review):
/// `SaveFile::lock` is what creates the sibling `.lock` file, so its
/// existence after a store proves the guard line ran -- deleting
/// `store_impl`'s `file.lock()?` fails this test deterministically, where
/// a thread-interleaving probe could only fail it probabilistically. The
/// exclusion semantics themselves are pinned at the `SaveFile::lock`
/// layer (`engine`'s own lock test).
#[test]
fn storing_takes_the_inter_process_lock() {
    let temp = TempSave::new("lock-taken");
    let mut slot = temp.slot();
    slot.store(&SaveBlock1::default(), &SaveBlock2::default())
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

/// The stale-session check (issue #214 review): when two sessions load the
/// same save and both write on exit, the second writer must refuse rather
/// than overwrite the first writer's newer persisted progress with its own
/// stale in-memory copy. The counter this session loaded is its identity;
/// a disk that rotated past it belongs to someone else's progress now.
#[test]
fn a_session_never_overwrites_progress_saved_after_its_own_load() {
    let temp = TempSave::new("stale-session");
    let block2 = SaveBlock2::default();

    // A prior save both sessions will load.
    let mut writer = temp.slot();
    writer
        .store(
            &SaveBlock1 {
                money: 100,
                ..SaveBlock1::default()
            },
            &block2,
        )
        .unwrap();

    // Both sessions boot-load the same image.
    let mut session_a = temp.slot();
    let mut session_b = temp.slot();
    assert_eq!(session_a.load().status, SaveFileStatus::Ok);
    assert_eq!(session_b.load().status, SaveFileStatus::Ok);

    // Session B exits first: its write advances the rotation.
    assert_eq!(
        session_b
            .store(
                &SaveBlock1 {
                    money: 200,
                    ..SaveBlock1::default()
                },
                &block2,
            )
            .unwrap(),
        super::StoreOutcome::Written
    );
    let after_b = std::fs::read(&temp.path).unwrap();

    // Session A exits second: its identity is stale, so it must refuse and
    // leave B's progress byte-identical.
    assert_eq!(
        session_a
            .store(
                &SaveBlock1 {
                    money: 100,
                    ..SaveBlock1::default()
                },
                &block2,
            )
            .unwrap(),
        super::StoreOutcome::RefusedStaleSession
    );
    assert_eq!(std::fs::read(&temp.path).unwrap(), after_b);
    assert_eq!(
        temp.slot().load().block1.money,
        200,
        "the surviving save must be B's, the newest persisted progress"
    );

    // B itself can keep saving: its identity tracked its own write.
    assert_eq!(
        session_b
            .store(
                &SaveBlock1 {
                    money: 300,
                    ..SaveBlock1::default()
                },
                &block2,
            )
            .unwrap(),
        super::StoreOutcome::Written
    );
}

/// #230 review round three: the stale-session check must be *directional*.
/// A disk counter that fell *behind* the session's identity is not another
/// instance's newer save -- it is `SaveStore::load`'s corruption fallback
/// to the older intact slot after the newest slot was damaged (a
/// continuable `Error`). The session still holding the valid state must be
/// allowed to write -- healing the image -- not be refused as stale; the
/// old `!=` comparison conflated the two and silently discarded the whole
/// session's progress.
#[test]
fn a_damaged_newest_slot_does_not_refuse_the_sessions_exit_write() {
    use engine::save::{Sector, SECTOR_SIGNATURE, SECTOR_SIZE};

    let temp = TempSave::new("damaged-newest-slot");
    let block2 = SaveBlock2 {
        encryption_key: 0xBEEF_CAFE,
        ..SaveBlock2::default()
    };
    let mut slot = temp.slot();
    slot.store(&SaveBlock1::default(), &block2).unwrap(); // counter 1, slot 1
    slot.store(&SaveBlock1::default(), &block2).unwrap(); // counter 2, slot 0
    assert_eq!(slot.load().status, SaveFileStatus::Ok); // session identity: 2

    // Damage one counter-2 sector's payload on disk mid-session: the
    // pre-write load falls back to the intact counter-1 slot and reports
    // the continuable `Error` -- a counter *behind* the session's identity.
    let mut image = std::fs::read(&temp.path).unwrap();
    let mut damaged = false;
    for index in 0..image.len() / SECTOR_SIZE {
        let start = index * SECTOR_SIZE;
        let sector = Sector::from_bytes(image[start..start + SECTOR_SIZE].try_into().unwrap());
        if sector.signature() == SECTOR_SIGNATURE && sector.counter() == 2 && !damaged {
            image[start] ^= 0xFF; // break the payload checksum, not the footer
            damaged = true;
        }
    }
    assert!(damaged, "the fixture must damage one newest-slot sector");
    std::fs::write(&temp.path, &image).unwrap();

    // The exit write must proceed and heal the image with this session's
    // state, not refuse it as stale.
    let outcome = slot
        .store(
            &SaveBlock1 {
                money: 777,
                ..SaveBlock1::default()
            },
            &block2,
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

/// #230 review round four: when the newest slot is damaged mid-session, the
/// healing write must carry the *session's* deferred lineage forward -- the
/// payload the boot load decoded -- not the older intact slot's, which the
/// pre-write load's fallback adopted as its serialization base. Without the
/// restore, play time/options/Pokédex silently roll back a generation while
/// every checksum stays green.
#[test]
fn healing_a_damaged_newest_slot_keeps_the_sessions_deferred_lineage() {
    use engine::save::{Sector, SECTOR_SIGNATURE, SECTOR_SIZE};

    const SECTOR_ID_SAVEBLOCK2: u16 = 0;
    // A SaveBlock2 payload offset no field models yet (play-time region).
    const B2_DEFERRED: usize = 0x10;

    let temp = TempSave::new("heal-lineage");
    let block2 = SaveBlock2 {
        encryption_key: 0xFEED_F00D,
        ..SaveBlock2::default()
    };
    {
        let mut slot = temp.slot();
        slot.store(&SaveBlock1::default(), &block2).unwrap(); // counter 1, slot 1
        slot.store(&SaveBlock1::default(), &block2).unwrap(); // counter 2, slot 0
    }

    // Distinct deferred bytes per generation: the older save's SaveBlock2
    // carries 0x11, the newest (the one this session continues) 0x5A.
    let mut image = std::fs::read(&temp.path).unwrap();
    let mut planted = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let start = index * SECTOR_SIZE;
        let sector = Sector::from_bytes(image[start..start + SECTOR_SIZE].try_into().unwrap());
        if sector.signature() == SECTOR_SIGNATURE && sector.id() == SECTOR_ID_SAVEBLOCK2 {
            let mut payload = block2.to_bytes();
            payload[B2_DEFERRED] = if sector.counter() == 2 { 0x5A } else { 0x11 };
            let replacement = Sector::write(SECTOR_ID_SAVEBLOCK2, &payload, sector.counter());
            image[start..start + SECTOR_SIZE].copy_from_slice(replacement.as_bytes());
            planted += 1;
        }
    }
    assert_eq!(planted, 2, "both generations' SaveBlock2 sectors planted");
    std::fs::write(&temp.path, &image).unwrap();

    // The session boots from the newest slot: its lineage carries 0x5A.
    let mut slot = temp.slot();
    assert_eq!(slot.load().status, SaveFileStatus::Ok);

    // The newest slot is damaged on disk mid-session (one non-SaveBlock2
    // sector, so the planted byte itself stays readable to a naive base).
    let mut image = std::fs::read(&temp.path).unwrap();
    let mut damaged = false;
    for index in 0..image.len() / SECTOR_SIZE {
        let start = index * SECTOR_SIZE;
        let sector = Sector::from_bytes(image[start..start + SECTOR_SIZE].try_into().unwrap());
        if sector.signature() == SECTOR_SIGNATURE
            && sector.counter() == 2
            && sector.id() != SECTOR_ID_SAVEBLOCK2
            && !damaged
        {
            image[start] ^= 0xFF; // break the payload checksum, not the footer
            damaged = true;
        }
    }
    assert!(damaged, "the fixture must damage one newest-slot sector");
    std::fs::write(&temp.path, &image).unwrap();

    // The healing exit write proceeds (directional stale check) and must
    // write the session's 0x5A forward, not the older slot's 0x11.
    let outcome = slot
        .store(
            &SaveBlock1 {
                money: 777,
                ..SaveBlock1::default()
            },
            &block2,
        )
        .unwrap();
    assert_eq!(outcome, super::StoreOutcome::Written);

    let image = std::fs::read(&temp.path).unwrap();
    let mut checked = 0;
    for index in 0..image.len() / SECTOR_SIZE {
        let start = index * SECTOR_SIZE;
        let sector = Sector::from_bytes(image[start..start + SECTOR_SIZE].try_into().unwrap());
        if sector.signature() == SECTOR_SIGNATURE
            && sector.id() == SECTOR_ID_SAVEBLOCK2
            && sector.counter() == 2
        {
            assert_eq!(
                sector.data()[B2_DEFERRED],
                0x5A,
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

// -- the wrap-aware session-drift comparison (#230 review round five) -----

/// [`super::counter_is_ahead`] must order across the `u32` wrap for
/// *arbitrary* distances -- the adjacent-pair rule the two-slot resolver
/// keeps (`engine`'s `counter_b_is_newer`) mis-orders `MAX -> 1` as "not
/// ahead", which is exactly the reviewed stale-writer escape.
#[test]
fn counter_is_ahead_orders_across_the_wrap_at_any_distance() {
    // The plain, un-wrapped cases.
    assert!(super::counter_is_ahead(3, 7));
    assert!(!super::counter_is_ahead(7, 3));
    assert!(!super::counter_is_ahead(5, 5));
    // The wrap: one advance, and the multi-advance case the adjacent rule
    // gets wrong.
    assert!(super::counter_is_ahead(u32::MAX, 0));
    assert!(super::counter_is_ahead(u32::MAX, 1));
    assert!(super::counter_is_ahead(u32::MAX - 1, 2));
    assert!(!super::counter_is_ahead(0, u32::MAX));
    assert!(!super::counter_is_ahead(1, u32::MAX));
    // Serial-arithmetic half-space boundary: a forward distance under half
    // the counter space is "ahead"; at or past it, it reads as "behind".
    assert!(super::counter_is_ahead(0, u32::MAX / 2 - 1));
    assert!(!super::counter_is_ahead(0, u32::MAX / 2));
}

/// The stale-session refusal must survive the counter wrap: a session whose
/// identity is `u32::MAX` while the disk has advanced past the wrap (any
/// small counter) is holding *older* progress and must refuse to write.
/// Emulated by pinning the session identity directly -- driving a real file
/// through 2^32 rotations is not a test.
#[test]
fn a_stale_session_is_refused_even_across_the_counter_wrap() {
    let block2 = SaveBlock2::default();
    let temp = TempSave::new("stale-across-wrap");

    // Newest persisted progress: a small post-wrap counter.
    let mut writer = temp.slot();
    writer
        .store(
            &SaveBlock1 {
                money: 200,
                ..SaveBlock1::default()
            },
            &block2,
        )
        .unwrap();
    let newest = std::fs::read(&temp.path).unwrap();

    // A session that booted just before the wrap: identity `u32::MAX`,
    // i.e. the disk's counter is ahead of it across the wrap boundary.
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
