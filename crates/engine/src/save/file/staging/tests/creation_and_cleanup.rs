use super::super::{create_new_exclusive, stage_at_first_free_name};
use crate::save::file::tests::{saved_store, sibling_path, TempDir};
use crate::save::file::{SaveFile, SAVE_FILE_NAME};
use crate::save::store::FLASH_IMAGE_LEN;

// Only the platform-gated tests below reach these, so their imports carry the
// same gate: an import a host compiles out every use of is a `dead_code`
// warning, and warnings are denied.
#[cfg(target_os = "linux")]
use super::super::fill_new_file;
#[cfg(windows)]
use super::super::{
    remove_through_verified_handle, remove_through_verified_handle_with, VerifiedRemoval,
    WindowsFileIdentity,
};
#[cfg(unix)]
use crate::save::file::tests::guessable_pid_staging_path;
#[cfg(unix)]
use crate::save::file::SaveFileError;
#[cfg(windows)]
use std::path::Path;

#[test]
fn a_staging_name_collision_is_retried_onto_a_fresh_name() {
    let dir = TempDir::new("staging-retry");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    // The first attempt lands on an occupied name and the second on a free
    // one, so only a retry that carries on past the collision can write.
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

/// Set in the re-executed child of
/// [`a_staged_write_that_fails_after_creating_its_file_removes_it`].
#[cfg(target_os = "linux")]
const STAGING_WRITE_FAILURE_CHILD: &str = "POKEEMERALD_RS_STAGING_WRITE_FAILURE_CHILD";

#[cfg(target_os = "linux")]
#[test]
fn a_staged_write_that_fails_after_creating_its_file_removes_it() {
    // No in-process API can fail a write to a freshly created file, so the
    // child re-executes this test under a file-size limit that the flash
    // image exceeds, with SIGXFSZ ignored so the failure surfaces as EFBIG.
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
        .arg(r#"trap "" XFSZ; ulimit -f 1; exec "$0" "$1" --exact --nocapture"#)
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

    // A symlink here targets the one name an attacker without this process's
    // clock or entropy could still guess; the real staging name never
    // matches it, and `create_new` refuses a symlink regardless.
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

    // The symlink sits on the very name the staging attempt opens, so only
    // the exclusive `create_new` open can keep the flash image off its target.
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

/// A directory the ownership check can see before the unlink runs is never a
/// regular file, so it is caught by [`StagedSave::still_ours`] and no unlink
/// is even attempted -- `remove_after` only ever calls `std::fs::remove_file`
/// on an entry the check just confirmed is still this call's own staged
/// file. Only a peer that lands the swap in the narrow gap between that
/// check and the unlink -- the same accepted bound the rename window above
/// documents -- could make the unlink itself observe a directory there, and
/// no cheap seam exists to force that exact interleaving deterministically.
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

/// Points the directory junction at `link` at `target`, replacing whatever
/// it named before. Unlike a directory symlink this needs no privilege, so
/// the ancestor-retarget regressions below never have to skip (mirrors
/// `crates/rom-import/src/lib.rs`'s test helper of the same shape).
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

/// Issue #1132's "swapped entry must survive" regression: the peer never
/// touches the staged entry itself, only an *ancestor* the staging path
/// walks through, after [`StagedSave::release_hold`] has already given up
/// the deny-all hold ([`StagedSave::remove_after`] releases it before this
/// call even runs). Only the identity `remove_through_verified_handle`
/// binds to the handle it opens for the delete -- not a fresh trust in
/// whatever the path now resolves to -- can tell the two apart.
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

/// An ancestor retargeted to an empty directory leaves the staging path
/// reaching nothing while the staged image still sits under the junction's
/// original target. Cleanup cannot remove it and must say so, folding the
/// `NotFound` into the returned error the way the non-Windows arm folds its
/// own, rather than returning the rename error alone.
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

/// The ancestor-retarget test above completes its swap before cleanup ever
/// starts, so an implementation that re-resolves `path` for both the check
/// and the delete would already pass it. This exercises the narrower
/// window: `before_delete` retargets the ancestor after
/// `remove_through_verified_handle_with` has already opened and
/// identity-checked its handle, and only then may it delete. A handle-bound
/// delete must still remove exactly the object it opened; it must never
/// reach whatever the retargeted path now resolves to.
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

/// A directory opens only with `FILE_FLAG_BACKUP_SEMANTICS`; without it the
/// verifying open fails as access denied and cleanup reports a staging file
/// that is already gone as left behind.
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

/// A scanner or indexer opening the freshly written staging file must not
/// turn cleanup into a sharing violation, and a POSIX delete must free the
/// name while that reader still holds its handle.
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
