use super::shared::area;
use crate::save::block::SaveBlock1;
use crate::save::file::tests::{guessable_pid_staging_path, saved_store, TempDir};
use crate::save::file::{SaveFile, SAVE_FILE_NAME};

const SECOND_WRITER_MONEY: u32 = 222_222;

/// `std` maps `ERROR_SHARING_VIOLATION` to `PermissionDenied` on some Windows
/// versions only, so accept either spelling.
#[cfg(windows)]
fn is_a_sharing_violation(err: &std::io::Error) -> bool {
    const ERROR_SHARING_VIOLATION: i32 = 32;

    err.kind() == std::io::ErrorKind::PermissionDenied
        || err.raw_os_error() == Some(ERROR_SHARING_VIOLATION)
}

/// On Windows the live hold makes replacement impossible; it must be released
/// before the promoting rename.
#[cfg(windows)]
#[test]
fn a_staged_image_cannot_be_opened_or_removed_while_its_hold_lives() {
    let dir = TempDir::new("staging-exclusive-hold");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    let staged_path = std::cell::RefCell::new(std::path::PathBuf::new());
    file.write_with(
        &store,
        SaveFile::sync_directory_best_effort,
        |bytes| area(&file).stage(bytes),
        |staged| {
            staged_path.replace(staged.to_path_buf());
            let opened = std::fs::OpenOptions::new()
                .read(true)
                .open(staged)
                .expect_err("a held staging entry must refuse a second open");
            assert!(
                is_a_sharing_violation(&opened),
                "opening a held staging entry must fail as a sharing violation: {opened:?}"
            );

            let removed = std::fs::remove_file(staged)
                .expect_err("a held staging entry must refuse deletion");
            assert!(
                is_a_sharing_violation(&removed),
                "deleting a held staging entry must fail as a sharing violation: {removed:?}"
            );
            assert!(
                staged.exists(),
                "a refused deletion must leave the staged image where it is"
            );
        },
    )
    .expect("a write whose staged image nobody else could touch must succeed");

    assert!(
        !staged_path.into_inner().exists(),
        "the hold must end in time for the rename, not leave the staged image beside the save"
    );
    let reloaded = file.read().unwrap().expect("the save must be readable");
    assert_eq!(
        reloaded.flash_image(),
        store.flash_image(),
        "the save must hold the bytes written under the hold"
    );
}

#[test]
fn two_save_files_on_one_path_never_share_a_staging_name() {
    let dir = TempDir::new("same-path-staging-names");
    let path = dir.join(SAVE_FILE_NAME);

    let first = SaveFile::at(&path);
    let second = SaveFile::at(&path);
    let first_staging = area(&first).first_name();
    let second_staging = area(&second).first_name();
    let guessable_name = guessable_pid_staging_path(&path);

    assert_ne!(
        first_staging, second_staging,
        "two SaveFile values on the same path must never derive the same staging name"
    );
    assert_ne!(
        first_staging, guessable_name,
        "the staging name must not be the guessable, symlink-plantable <save>.tmp.<pid> form"
    );
    assert_ne!(
        second_staging, guessable_name,
        "the staging name must not be the guessable, symlink-plantable <save>.tmp.<pid> form"
    );

    let (first_store, _, block2) = saved_store();
    first
        .write(&first_store)
        .expect("the first writer must succeed");

    let mut second_store = first_store.clone();
    second_store.save(
        &SaveBlock1 {
            money: SECOND_WRITER_MONEY,
            ..SaveBlock1::default()
        },
        &block2,
    );
    second
        .write(&second_store)
        .expect("the second writer must succeed");

    let reloaded = second.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), second_store.flash_image());
}
