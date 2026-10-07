use super::*;

#[test]
fn reading_a_path_with_no_file_reports_no_save_rather_than_an_error() {
    let dir = TempDir::new("missing");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    assert!(!file.exists());
    assert!(file.read().unwrap().is_none());
}

#[test]
fn a_written_image_reads_back_byte_identical() {
    let dir = TempDir::new("roundtrip");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    let (store, _, _) = saved_store();

    file.write(&store).unwrap();
    assert!(file.exists());

    let reloaded = file.read().unwrap().expect("the file was just written");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

#[test]
fn a_written_save_reloads_through_the_stores_own_validation() {
    let dir = TempDir::new("reload");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    let (store, block1, block2) = saved_store();
    file.write(&store).unwrap();

    let mut reloaded = file.read().unwrap().expect("the file was just written");
    let outcome = reloaded.load();

    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(outcome.block2.encryption_key, block2.encryption_key);
    assert_eq!(outcome.block1.money, block1.money);
    assert_eq!(reloaded.save_counter(), store.save_counter());
    assert_eq!(reloaded.last_written_sector(), store.last_written_sector());
}

#[test]
fn writing_creates_the_parent_directory() {
    let dir = TempDir::new("mkdir");
    let file = SaveFile::at(dir.join("nested").join("deeper").join(SAVE_FILE_NAME));
    let (store, _, _) = saved_store();

    file.write(&store).unwrap();
    assert!(file.exists());
}

#[test]
fn writing_leaves_no_temporary_file_behind() {
    let dir = TempDir::new("atomic");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    file.write(&store).unwrap();

    let leftover_staging_names: Vec<_> = std::fs::read_dir(&dir.path)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .filter(|name| {
            name.to_string_lossy()
                .starts_with(&format!("{SAVE_FILE_NAME}.tmp."))
        })
        .collect();
    assert!(
        leftover_staging_names.is_empty(),
        "the staged temporary must be renamed away, not left on disk: {leftover_staging_names:?}"
    );
}

#[test]
fn a_write_that_cannot_be_staged_leaves_the_previous_save_byte_identical() {
    let dir = TempDir::new("staging");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);

    let (first, _, block2) = saved_store();
    file.write(&first).unwrap();
    let original = std::fs::read(&path).expect("the first save must be readable");

    // Every staging attempt is pointed at the same pre-existing directory, so
    // every attempt -- and the retry bound -- collides, forcing the failure
    // this test exercises without depending on the real staging name's shape.
    let unstageable = guessable_pid_staging_path(&path);
    std::fs::create_dir_all(&unstageable).unwrap();

    let mut second = first.clone();
    second.save(
        &SaveBlock1 {
            money: 777_777,
            ..SaveBlock1::default()
        },
        &block2,
    );
    let err = file
        .write_with(
            &second,
            SaveFile::sync_directory_best_effort,
            |bytes| {
                staging::stage_at_first_free_name(
                    std::iter::once(unstageable.clone()),
                    staging::create_new_exclusive,
                    bytes,
                )
            },
            |_| {},
        )
        .expect_err("staging into a directory cannot succeed");
    assert!(
        matches!(err, SaveFileError::Write { .. }),
        "a failed staged write must surface as a write failure: {err:?}"
    );
    assert_eq!(
        std::fs::read(&path).expect("the previous save must still be there"),
        original,
        "a write that never got staged must not touch the image already on \
         disk -- writing straight to the destination would lose both \
         rotating slots at once"
    );
}

#[test]
fn a_staged_write_that_cannot_be_renamed_removes_its_staged_file() {
    let dir = TempDir::new("staging-rename-failure");
    let path = dir.join(SAVE_FILE_NAME);
    // A directory at the save path stages fine but can never be renamed onto,
    // so the failure lands after the staged file exists.
    std::fs::create_dir_all(&path).unwrap();
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    let staged = sibling_path(&path, ".tmp.staged");
    let err = file
        .write_with(
            &store,
            SaveFile::sync_directory_best_effort,
            |bytes| {
                staging::stage_at_first_free_name(
                    std::iter::once(staged.clone()),
                    staging::create_new_exclusive,
                    bytes,
                )
            },
            |_| {},
        )
        .expect_err("renaming onto a directory cannot succeed");

    assert!(
        matches!(err, SaveFileError::Write { .. }),
        "a failed rename must surface as a write failure: {err:?}"
    );
    assert!(
        !staged.exists(),
        "a staged file whose rename failed must be cleaned up, not left beside the save"
    );
}

#[cfg(unix)]
#[test]
fn a_staging_file_replaced_before_the_rename_is_never_promoted() {
    let dir = TempDir::new("staging-replaced-before-rename");
    let path = dir.join(SAVE_FILE_NAME);
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    let file = SaveFile::at(&path);
    let (first, _, block2) = saved_store();
    file.write(&first).unwrap();
    let original = std::fs::read(&path).expect("the first save must be readable");

    let mut second = first.clone();
    second.save(
        &SaveBlock1 {
            money: 424_242,
            ..SaveBlock1::default()
        },
        &block2,
    );

    // The window the exclusive create alone cannot defend: the staged file is
    // unlinked and a symlink takes its name after the create proved the name
    // was free, so only an ownership check standing between the write and the
    // rename can keep the replacement off the save path.
    let staging_name = sibling_path(&path, ".tmp.replaced");
    let err = file
        .write_with(
            &second,
            SaveFile::sync_directory_best_effort,
            |bytes| {
                staging::stage_at_first_free_name(
                    std::iter::once(staging_name.clone()),
                    staging::create_new_exclusive,
                    bytes,
                )
            },
            |staged| {
                std::fs::remove_file(staged).unwrap();
                std::os::unix::fs::symlink(&bystander, staged).unwrap();
            },
        )
        .expect_err("a write that lost its staged file must never report success");

    assert!(
        matches!(&err, SaveFileError::Write { source, .. }
            if source.kind() == std::io::ErrorKind::InvalidData),
        "a replaced staging file must fail closed, not pass for the staged image: {err:?}"
    );
    assert!(
        !std::fs::symlink_metadata(&path)
            .expect("the previous save must still be there")
            .file_type()
            .is_symlink(),
        "the save path must never become the planted symlink"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "a refused promotion must leave the image already on disk untouched"
    );
    assert_eq!(
        std::fs::read(&bystander).unwrap(),
        b"not a save file",
        "a refused promotion must not reach the symlink's target"
    );
    assert!(
        std::fs::symlink_metadata(&staging_name)
            .expect("the planted symlink must survive")
            .file_type()
            .is_symlink(),
        "a refused promotion must not delete the entry that replaced ours"
    );
}

#[cfg(unix)]
#[test]
fn a_staging_file_swapped_for_another_regular_file_is_never_promoted() {
    let dir = TempDir::new("staging-swapped-before-rename");
    let path = dir.join(SAVE_FILE_NAME);

    let file = SaveFile::at(&path);
    let (first, _, block2) = saved_store();
    file.write(&first).unwrap();
    let original = std::fs::read(&path).expect("the first save must be readable");

    let mut second = first.clone();
    second.save(
        &SaveBlock1 {
            money: 515_151,
            ..SaveBlock1::default()
        },
        &block2,
    );

    // A regular file walks straight past the "not a symlink, not a
    // directory" test, so nothing but the staged handle's own device and
    // inode can tell this impostor from the image this call wrote.
    let staging_name = sibling_path(&path, ".tmp.swapped");
    let err = file
        .write_with(
            &second,
            SaveFile::sync_directory_best_effort,
            |bytes| {
                staging::stage_at_first_free_name(
                    std::iter::once(staging_name.clone()),
                    staging::create_new_exclusive,
                    bytes,
                )
            },
            |staged| {
                std::fs::remove_file(staged).unwrap();
                std::fs::write(staged, b"someone else's file").unwrap();
            },
        )
        .expect_err("a write that lost its staged file must never report success");

    assert!(
        matches!(&err, SaveFileError::Write { source, .. }
            if source.kind() == std::io::ErrorKind::InvalidData),
        "a swapped staging file must fail closed, not pass for the staged image: {err:?}"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "a refused promotion must leave the image already on disk untouched"
    );
    assert_eq!(
        std::fs::read(&staging_name).unwrap(),
        b"someone else's file",
        "a refused promotion must not delete the entry that replaced ours"
    );
}

#[test]
fn overwriting_an_existing_save_replaces_it_whole() {
    let dir = TempDir::new("overwrite");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    let (mut store, _, block2) = saved_store();
    file.write(&store).unwrap();

    let second = SaveBlock1 {
        money: 999_999,
        ..SaveBlock1::default()
    };
    store.save(&second, &block2);
    file.write(&store).unwrap();

    let mut reloaded = file.read().unwrap().unwrap();
    let outcome = reloaded.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(outcome.block1.money, 999_999);
}

#[test]
fn a_file_of_the_wrong_length_is_rejected_by_length_not_silently_padded() {
    let dir = TempDir::new("truncated");
    let path = dir.join(SAVE_FILE_NAME);
    std::fs::write(&path, vec![0u8; FLASH_IMAGE_LEN - 1]).unwrap();

    let err = SaveFile::at(&path).read().unwrap_err();
    match err {
        SaveFileError::BadLength { expected, got, .. } => {
            assert_eq!(expected, FLASH_IMAGE_LEN);
            assert_eq!(got, u64::try_from(FLASH_IMAGE_LEN - 1).unwrap());
        }
        other => panic!("expected a length rejection, got {other:?}"),
    }
}

#[test]
fn an_oversized_file_is_rejected_after_a_bounded_read() {
    let dir = TempDir::new("oversized");
    let path = dir.join(SAVE_FILE_NAME);
    let actual_len = FLASH_IMAGE_LEN + 4096;
    std::fs::write(&path, vec![0u8; actual_len]).unwrap();

    let err = SaveFile::at(&path).read().unwrap_err();
    match err {
        SaveFileError::BadLength { expected, got, .. } => {
            assert_eq!(expected, FLASH_IMAGE_LEN);
            assert_eq!(
                got,
                u64::try_from(actual_len).unwrap(),
                "the bounded probe cap must not leak into the reported length"
            );
        }
        other => panic!("expected a length rejection, got {other:?}"),
    }
}

#[test]
fn an_oversized_files_rejection_does_not_misstate_its_length() {
    let dir = TempDir::new("oversized-message");
    let path = dir.join(SAVE_FILE_NAME);
    let actual_len = FLASH_IMAGE_LEN + 4096;
    std::fs::write(&path, vec![0u8; actual_len]).unwrap();

    let message = SaveFile::at(&path).read().unwrap_err().to_string();
    let bounded_probe_len = FLASH_IMAGE_LEN + 1;
    assert!(
        !message.contains(&format!("is {bounded_probe_len} bytes")),
        "the rejection states a length the {actual_len}-byte file does not have: {message}"
    );
    assert!(
        message.contains(&format!("is {actual_len} bytes")),
        "the rejection must state the file's true length: {message}"
    );
}

#[test]
fn reading_a_directory_in_the_files_place_is_refused_before_any_open() {
    let dir = TempDir::new("isdir");
    let path = dir.join(SAVE_FILE_NAME);
    std::fs::create_dir_all(&path).unwrap();

    let file = SaveFile::at(&path);
    assert!(!file.exists(), "a directory is not a save file");
    match file.read() {
        Err(SaveFileError::SavePathNotAPlainFile { path: refused }) => {
            assert_eq!(refused, path);
        }
        other => panic!("expected a not-a-plain-file refusal, got {other:?}"),
    }
}

/// A symlinked save path must be refused rather than followed, matching the
/// lock slot's policy: each locker (or reader) would otherwise chase
/// whatever the link happened to lead to at the time.
#[cfg(unix)]
#[test]
fn reading_a_symlinked_save_path_is_refused_rather_than_followed() {
    let dir = TempDir::new("read-symlink");
    let target = dir.join("real-save");
    let (store, _, _) = saved_store();
    SaveFile::at(&target).write(&store).unwrap();

    let path = dir.join(SAVE_FILE_NAME);
    std::os::unix::fs::symlink(&target, &path).unwrap();

    match SaveFile::at(&path).read() {
        Err(SaveFileError::SavePathIsAlias { path: refused }) => {
            assert_eq!(refused, path);
        }
        other => panic!("expected an alias refusal, got {other:?}"),
    }
}

/// A FIFO in the save's place must be refused rather than opened: a
/// read-only open of one waits for a writer that a stale entry never gets,
/// so boot would hang there instead of reporting a save-file error.
#[cfg(unix)]
#[test]
fn reading_a_fifo_in_the_files_place_fails_rather_than_waiting_for_a_writer() {
    let dir = TempDir::new("read-fifo");
    let path = dir.join(SAVE_FILE_NAME);
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .expect("mkfifo must run")
            .success(),
        "mkfifo must create the FIFO this test stands on"
    );

    let (outcomes, outcome) = std::sync::mpsc::channel();
    let probe = path.clone();
    std::thread::spawn(move || {
        drop(outcomes.send(SaveFile::at(&probe).read().map(|store| store.is_some())));
    });

    let read = outcome
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("reading a FIFO must return rather than wait for a writer that never comes");
    match read {
        Err(SaveFileError::SavePathNotAPlainFile { path: refused }) => {
            assert_eq!(refused, path);
        }
        other => panic!("a FIFO is not a save image, so reading one must fail: {other:?}"),
    }
}

/// The save path's refusal must survive an entry that changes underneath it:
/// a party that swaps a symlink in after the inspection but before the open
/// must not get that link followed and its target accepted as save data.
#[cfg(unix)]
#[test]
fn a_save_path_swapped_after_inspection_is_still_not_followed() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let dir = TempDir::new("read-swap");
    let target = dir.join("link-target");
    let (store, _, _) = saved_store();
    SaveFile::at(&target).write(&store).unwrap();
    let path = dir.join(SAVE_FILE_NAME);
    let stop = Arc::new(AtomicBool::new(false));
    let swapper = {
        let (stop, path, target) = (Arc::clone(&stop), path.clone(), target.clone());
        let alias_stage = dir.join("stage-alias");
        let plain_stage = dir.join("stage-plain");
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                drop(std::fs::remove_file(&alias_stage));
                if std::os::unix::fs::symlink(&target, &alias_stage).is_ok() {
                    drop(std::fs::rename(&alias_stage, &path));
                }
                if std::fs::write(&plain_stage, [0u8; 10]).is_ok() {
                    drop(std::fs::rename(&plain_stage, &path));
                }
            }
        })
    };
    let file = SaveFile::at(&path);
    let mut followed = false;
    for _ in 0..200_000 {
        if matches!(file.read(), Ok(Some(_))) {
            followed = true;
            break;
        }
    }
    stop.store(true, Ordering::Relaxed);
    swapper.join().unwrap();
    assert!(!followed, "a symlink swapped in after the inspection was followed and its target accepted as save data, so the refusal at {} is bypassable", path.display());
}
