use super::super::{create_new_exclusive, stage_at_first_free_name};
use crate::save::file::staging::StagingArea;
use crate::save::file::tests::{saved_store, sibling_path, TempDir};
use crate::save::file::{SaveFile, SAVE_FILE_NAME};
use crate::save::store::FLASH_IMAGE_LEN;

#[cfg(target_os = "linux")]
use super::super::fill_new_file;
#[cfg(windows)]
use super::super::{
    remove_through_verified_handle, remove_through_verified_handle_with, VerifiedRemoval,
    WindowsFileIdentity,
};
#[cfg(unix)]
use crate::save::file::tests::guessable_pid_staging_path;
#[cfg(any(unix, windows))]
use crate::save::file::SaveFileError;
#[cfg(windows)]
use std::path::Path;

#[test]
fn a_staging_name_collision_is_retried_onto_a_fresh_name() {
    let dir = TempDir::new("staging-retry");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    let occupied = sibling_path(&path, ".tmp.occupied");
    std::fs::write(&occupied, b"someone else's file").unwrap();
    let fresh = sibling_path(&path, ".tmp.fresh");
    let mut attempts = 0;
    file.write_with(
        &store,
        SaveFile::sync_directory_best_effort,
        |bytes| {
            stage_at_first_free_name(
                std::iter::repeat_with(|| {
                    attempts += 1;
                    if attempts == 1 {
                        occupied.clone()
                    } else {
                        fresh.clone()
                    }
                })
                .take(2),
                create_new_exclusive,
                bytes,
            )
        },
        |_| {},
    )
    .expect("a staging-name collision must be retried onto a fresh name");

    assert_eq!(
        attempts, 2,
        "the collision must cost exactly one extra attempt"
    );
    assert_eq!(
        std::fs::read(&occupied).unwrap(),
        b"someone else's file",
        "a colliding staging attempt must leave the file already at that name alone"
    );
    assert!(
        !fresh.exists(),
        "the retried staged file must be renamed into place, not left on disk"
    );
    let reloaded = file.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

#[cfg(target_os = "linux")]
const STAGING_WRITE_FAILURE_CHILD: &str = "POKEEMERALD_RS_STAGING_WRITE_FAILURE_CHILD";

#[cfg(target_os = "linux")]
#[test]
fn a_staged_write_that_fails_after_creating_its_file_removes_it() {
    // The flash image exceeds the child's one-block file limit; SIGXFSZ is
    // ignored so the failure is EFBIG (`FileTooLarge`), not process death.
    const STAGING_FILE_LIMIT_BLOCKS: &str = "1";
    if std::env::var_os(STAGING_WRITE_FAILURE_CHILD).is_some() {
        let dir = TempDir::new("staging-write-failure");
        let staged = dir.join("staged.tmp");
        let err = fill_new_file(create_new_exclusive, &staged, &vec![0u8; FLASH_IMAGE_LEN])
            .expect_err("a staged write over the file-size limit cannot succeed");
        assert_eq!(err.kind(), std::io::ErrorKind::FileTooLarge, "{err:?}");
        assert!(
            !staged.exists(),
            "a staged file whose write failed must be cleaned up, not left beside the save"
        );
        return;
    }

    let exe = std::env::current_exe().expect("the test binary must be locatable");
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(format!(
            r#"trap "" XFSZ; ulimit -f {STAGING_FILE_LIMIT_BLOCKS}; exec "$0" "$1" --exact --nocapture"#
        ))
        .arg(&exe)
        .arg("save::file::staging::tests::creation_and_cleanup::a_staged_write_that_fails_after_creating_its_file_removes_it")
        .env(STAGING_WRITE_FAILURE_CHILD, "1")
        .output()
        .expect("the child test process must be spawnable");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("1 passed"),
        "child status {:?}\nstdout:\n{stdout}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_planted_at_the_guessable_pid_staging_name_is_not_followed() {
    let dir = TempDir::new("staging-symlink");
    let path = dir.join(SAVE_FILE_NAME);
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    std::os::unix::fs::symlink(&bystander, guessable_pid_staging_path(&path)).unwrap();

    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();
    file.write(&store)
        .expect("planting a decoy symlink must not fail the write");

    assert_eq!(
        std::fs::read(&bystander).unwrap(),
        b"not a save file",
        "a symlink planted at a staging name must never redirect the flash \
         image onto the file it points at"
    );
    assert!(
        !std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()),
        "the save path must hold a save image, not a planted symlink"
    );
    let reloaded = file.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

#[cfg(unix)]
#[test]
fn a_symlink_at_the_staging_path_the_write_actually_uses_is_refused() {
    let dir = TempDir::new("staging-symlink-real-name");
    let path = dir.join(SAVE_FILE_NAME);
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    let staging = sibling_path(&path, ".tmp.planted");
    std::os::unix::fs::symlink(&bystander, &staging).unwrap();

    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();
    let err = file
        .write_with(
            &store,
            SaveFile::sync_directory_best_effort,
            |bytes| {
                stage_at_first_free_name(
                    std::iter::once(staging.clone()),
                    create_new_exclusive,
                    bytes,
                )
            },
            |_| {},
        )
        .expect_err("staging onto a planted symlink must never succeed");

    assert!(
        matches!(err, SaveFileError::Write { .. }),
        "a refused staged write must surface as a write failure: {err:?}"
    );
    assert_eq!(
        std::fs::read(&bystander).unwrap(),
        b"not a save file",
        "the exclusive open must not follow the symlink onto its target"
    );
    assert!(
        std::fs::symlink_metadata(&staging)
            .expect("the planted symlink must survive")
            .file_type()
            .is_symlink(),
        "a refused staging attempt must not delete the path another caller created"
    );
    assert!(
        !path.exists(),
        "a write that never got staged must not create the save file"
    );
}

#[cfg(unix)]
#[test]
fn cleaning_up_a_failed_staged_write_leaves_the_entry_that_replaced_it_alone() {
    let dir = TempDir::new("staging-cleanup-ownership");
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    let staging = dir.join("staged.tmp");
    let mut staged = stage_at_first_free_name(
        std::iter::once(staging.clone()),
        create_new_exclusive,
        &vec![0u8; FLASH_IMAGE_LEN],
    )
    .expect("the exclusive staging write must succeed");
    std::fs::remove_file(&staging).unwrap();
    std::os::unix::fs::symlink(&bystander, &staging).unwrap();

    let err = staged.remove_after(std::io::Error::other("the rename failed"));

    assert_eq!(
        err.kind(),
        std::io::ErrorKind::Other,
        "cleanup that removed nothing must surface the failure it was given: {err:?}"
    );
    assert!(
        std::fs::symlink_metadata(&staging)
            .expect("the planted symlink must survive")
            .file_type()
            .is_symlink(),
        "cleanup must never remove an entry that replaced this call's staging file"
    );
    assert_eq!(
        std::fs::read(&bystander).unwrap(),
        b"not a save file",
        "cleanup must not reach the symlink's target either"
    );
}

// Ownership is checked before the unlink, so a swapped-in directory survives.
// A check-to-unlink race remains; no seam forces that interleaving here.
#[cfg(unix)]
#[test]
fn a_directory_that_replaces_the_staging_entry_survives_cleanup() {
    let dir = TempDir::new("staging-cleanup-directory-swap");

    let staging = dir.join("staged.tmp");
    let mut staged = stage_at_first_free_name(
        std::iter::once(staging.clone()),
        create_new_exclusive,
        &vec![0u8; FLASH_IMAGE_LEN],
    )
    .expect("the exclusive staging write must succeed");
    std::fs::remove_file(&staging).unwrap();
    std::fs::create_dir(&staging).unwrap();

    let err = staged.remove_after(std::io::Error::other("the rename failed"));

    assert_eq!(
        err.kind(),
        std::io::ErrorKind::Other,
        "a directory is never a regular file, so cleanup must never attempt to unlink \
         it and must surface the failure it was given unmodified: {err:?}"
    );
    assert!(
        std::fs::symlink_metadata(&staging)
            .expect("the planted directory must survive")
            .file_type()
            .is_dir(),
        "cleanup must never remove a directory that replaced this call's staging file -- \
         std::fs::remove_file cannot unlink one"
    );
}

// Junctions need no symlink privilege, unlike directory symlinks.
#[cfg(windows)]
fn junction(link: &Path, target: &Path) {
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("cmd runs mklink");
    assert!(
        status.success(),
        "mklink /J {} {}: {status}",
        link.display(),
        target.display()
    );
}

// The hold is released before cleanup reopens the file, so deletion must
// verify identity on its own handle, not trust the retargeted path.
#[cfg(windows)]
#[test]
fn cleanup_leaves_a_file_reached_through_a_retargeted_ancestor_alone() {
    let dir = TempDir::new("staging-cleanup-ancestor-swap");
    let target = dir.join("a");
    let elsewhere = dir.join("b");
    std::fs::create_dir(&target).expect("the link's first target");
    std::fs::create_dir(&elsewhere).expect("the link's second target");
    let victim = elsewhere.join("staged.tmp");
    std::fs::write(&victim, b"someone else's file").expect("the unrelated file exists");

    let link = dir.join("link");
    junction(&link, &target);
    let staging = link.join("staged.tmp");

    let mut staged = stage_at_first_free_name(
        std::iter::once(staging.clone()),
        create_new_exclusive,
        &vec![0u8; FLASH_IMAGE_LEN],
    )
    .expect("the exclusive staging write must succeed");

    std::fs::remove_dir(&link).expect("the peer drops the ancestor junction");
    junction(&link, &elsewhere);

    let err = staged.remove_after(std::io::Error::other("the rename failed"));

    assert_eq!(err.kind(), std::io::ErrorKind::Other, "{err:?}");
    assert_eq!(
        std::fs::read(&victim).expect("the unrelated file survives"),
        b"someone else's file",
        "cleanup must never remove a file the path only reaches through a retargeted ancestor"
    );
}

// A retarget to an empty directory orphans the staged image; cleanup must
// report that, not return only the rename error.
#[cfg(windows)]
#[test]
fn cleanup_reports_a_staged_image_an_ancestor_retarget_orphaned() {
    let dir = TempDir::new("staging-cleanup-ancestor-empty");
    let target = dir.join("a");
    let empty = dir.join("b");
    std::fs::create_dir(&target).expect("the link's first target");
    std::fs::create_dir(&empty).expect("the link's second target");

    let link = dir.join("link");
    junction(&link, &target);
    let staging = link.join("staged.tmp");

    let mut staged = stage_at_first_free_name(
        std::iter::once(staging.clone()),
        create_new_exclusive,
        &vec![0u8; FLASH_IMAGE_LEN],
    )
    .expect("the exclusive staging write must succeed");

    std::fs::remove_dir(&link).expect("the peer drops the ancestor junction");
    junction(&link, &empty);

    let err = staged.remove_after(std::io::Error::other("the rename failed"));

    assert_eq!(err.kind(), std::io::ErrorKind::Other, "{err:?}");
    assert!(
        err.to_string()
            .contains("additionally failed to remove the abandoned staging file"),
        "an orphaned staged image must be reported, not dropped: {err}"
    );
    assert!(
        target.join("staged.tmp").exists(),
        "the orphan really is still there, under the original target"
    );
}

// The retarget lands after the handle is identity-checked; the delete must
// stay bound to that handle, never re-resolve the path.
#[cfg(windows)]
#[test]
fn deletion_survives_an_ancestor_retargeted_between_the_check_and_the_delete() {
    let dir = TempDir::new("staging-cleanup-race-window");
    let target = dir.join("a");
    let elsewhere = dir.join("b");
    std::fs::create_dir(&target).expect("the link's first target");
    std::fs::create_dir(&elsewhere).expect("the link's second target");
    let victim = elsewhere.join("staged.tmp");
    std::fs::write(&victim, b"someone else's file").expect("the unrelated file exists");

    let link = dir.join("link");
    junction(&link, &target);
    let staging = link.join("staged.tmp");
    let created = target.join("staged.tmp");

    let file = create_new_exclusive(&staging).expect("the exclusive staging write must succeed");
    let identity = WindowsFileIdentity::of(&file).expect("identity reads back off the handle");
    drop(file);

    remove_through_verified_handle_with(&staging, identity, |_handle| {
        std::fs::remove_dir(&link).expect("the peer drops the ancestor junction");
        junction(&link, &elsewhere);
    })
    .expect("a retarget after the check must not stop the handle-bound delete");

    assert!(
        !created.exists(),
        "the delete must remove the object identity was checked against, through its handle"
    );
    assert_eq!(
        std::fs::read(&victim).expect("the unrelated file survives"),
        b"someone else's file",
        "a retarget landing after the check must never redirect the delete onto it"
    );
}

// Opening a directory needs `FILE_FLAG_BACKUP_SEMANTICS`; without it the
// verifying open fails as access denied.
#[cfg(windows)]
#[test]
fn cleanup_leaves_a_directory_that_took_the_staging_name_alone() {
    let dir = TempDir::new("staging-cleanup-directory-swap");
    let staging = dir.join("staged.tmp");
    let file = create_new_exclusive(&staging).expect("the exclusive staging create succeeds");
    let identity = WindowsFileIdentity::of(&file).expect("identity reads back off the handle");
    drop(file);
    std::fs::remove_file(&staging).expect("the peer removes the staged file");
    std::fs::create_dir(&staging).expect("the peer plants a directory at its name");

    let removal = remove_through_verified_handle(&staging, identity)
        .expect("a directory at the name is identified, not an open failure");

    assert_eq!(removal, VerifiedRemoval::NotOurs);
    assert!(
        staging.is_dir(),
        "the peer's directory must survive cleanup"
    );
}

// A POSIX delete frees the name while a reader holds the file; the fallback
// may stay `Pending` until the reader closes.
#[cfg(windows)]
#[test]
fn cleanup_deletes_a_staged_file_another_process_still_reads() {
    let dir = TempDir::new("staging-cleanup-shared-reader");
    let staging = dir.join("staged.tmp");
    let file = create_new_exclusive(&staging).expect("the exclusive staging create succeeds");
    let identity = WindowsFileIdentity::of(&file).expect("identity reads back off the handle");
    drop(file);
    let reader = std::fs::File::open(&staging).expect("a sharing reader opens the staged file");

    let removal = remove_through_verified_handle(&staging, identity)
        .expect("a sharing reader must not refuse the verified delete");

    match removal {
        VerifiedRemoval::Unlinked => {
            create_new_exclusive(&staging)
                .expect("an unlinked staging name is free for a same-name retry at once");
        }
        VerifiedRemoval::Pending => {}
        VerifiedRemoval::NotOurs => panic!("the verified file was still at its name"),
    }
    drop(reader);
}

// The hold forbids a replacement of the final entry but not a retarget of an
// ancestor, so only the captured identity can tell the impostor reached
// through the retargeted junction from the image this call staged.
#[cfg(windows)]
#[test]
fn an_impostor_reached_through_a_retargeted_ancestor_is_never_promoted() {
    let dir = TempDir::new("staging-promotion-ancestor-swap");
    let target = dir.join("a");
    let elsewhere = dir.join("b");
    std::fs::create_dir(&target).unwrap();
    std::fs::create_dir(&elsewhere).unwrap();
    let link = dir.join("link");
    junction(&link, &target);

    let path = target.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();
    file.write(&store).unwrap();
    let original = std::fs::read(&path).unwrap();
    let staging = link.join("staged.tmp");
    let victim = elsewhere.join("staged.tmp");
    std::fs::write(&victim, b"someone else's file").unwrap();

    let err = file
        .write_with(
            &store,
            SaveFile::sync_directory_best_effort,
            |bytes| {
                stage_at_first_free_name(
                    std::iter::once(staging.clone()),
                    create_new_exclusive,
                    bytes,
                )
            },
            |_| {
                std::fs::remove_dir(&link).unwrap();
                junction(&link, &elsewhere);
            },
        )
        .expect_err("a retargeted staging impostor must not be promoted");

    assert!(
        matches!(&err, SaveFileError::Write { source, .. }
            if source.kind() == std::io::ErrorKind::InvalidData),
        "{err:?}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read(&victim).unwrap(), b"someone else's file");
}

// The identity outlives the released handle, and a regular file at the same
// name is told apart from the staged one by it.
#[cfg(windows)]
#[test]
fn still_ours_compares_the_retained_identity_before_and_after_release() {
    let dir = TempDir::new("staging-still-ours-identity");
    let staging = dir.join("staged.tmp");
    let mut staged = stage_at_first_free_name(
        std::iter::once(staging.clone()),
        create_new_exclusive,
        &vec![0u8; FLASH_IMAGE_LEN],
    )
    .expect("the exclusive staging write must succeed");

    assert!(staged.still_ours().unwrap());
    staged.release_hold();
    assert!(staged.still_ours().unwrap());

    std::fs::rename(&staging, dir.join("moved.tmp")).unwrap();
    std::fs::write(&staging, b"someone else's file").unwrap();
    assert!(!staged.still_ours().unwrap());
}

// The identity is re-checked at the staged-to-rename seam, where a retarget
// after staging would otherwise promote the image into the wrong directory.
#[cfg(unix)]
#[test]
fn a_guarded_write_refuses_to_rename_after_the_parent_is_retargeted() {
    let dir = TempDir::new("staging-guarded-retarget");
    let first = dir.join("a");
    let second = dir.join("b");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&first, &link).unwrap();

    let file = SaveFile::at(link.join(SAVE_FILE_NAME));
    let guard = file.lock().expect("the lock is taken through the link");
    let (store, _, _) = saved_store();
    let err = file
        .write_with(
            &store,
            SaveFile::sync_directory_best_effort,
            |bytes| StagingArea::beside(file.path()).stage(bytes),
            |_| {
                std::fs::remove_file(&link).unwrap();
                std::os::unix::fs::symlink(&second, &link).unwrap();
            },
        )
        .expect_err("the rename must not follow a retargeted ancestor");
    drop(guard);

    assert!(
        matches!(err, SaveFileError::SaveParentRetargeted { .. }),
        "a retarget between staging and rename must name itself: {err:?}"
    );
    assert!(!first.join(SAVE_FILE_NAME).exists());
    assert!(!second.join(SAVE_FILE_NAME).exists());
}

#[cfg(windows)]
#[test]
fn a_guarded_write_refuses_to_rename_after_a_junction_parent_is_retargeted() {
    let dir = TempDir::new("staging-guarded-junction-retarget");
    let first = dir.join("a");
    let second = dir.join("b");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let link = dir.join("link");
    junction(&link, &first);

    let file = SaveFile::at(link.join(SAVE_FILE_NAME));
    let guard = file.lock().expect("the lock is taken through the junction");
    let (store, _, _) = saved_store();
    let err = file
        .write_with(
            &store,
            SaveFile::sync_directory_best_effort,
            |bytes| StagingArea::beside(file.path()).stage(bytes),
            |_| {
                std::fs::remove_dir(&link).unwrap();
                junction(&link, &second);
            },
        )
        .expect_err("the rename must not follow a retargeted junction");
    drop(guard);

    assert!(
        matches!(err, SaveFileError::SaveParentRetargeted { .. }),
        "a retarget between staging and rename must name itself: {err:?}"
    );
    assert!(!second.join(SAVE_FILE_NAME).exists());
}
