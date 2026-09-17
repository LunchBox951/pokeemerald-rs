use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{
    data_dir_for, default_save_path_from, staging, HostFamily, SaveFile, SaveFileError,
    LOCK_FILE_NAME, SAVE_DIR_NAME, SAVE_FILE_NAME, SAVE_PATH_ENV,
};
use crate::save::block::{SaveBlock1, SaveBlock2};
use crate::save::store::{SaveStatus, SaveStore, FLASH_IMAGE_LEN};

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(*value))
    }
}

/// `save_path` with `suffix` appended, naming an entry beside it.
pub(super) fn sibling_path(save_path: &Path, suffix: impl AsRef<std::ffi::OsStr>) -> PathBuf {
    let mut path = save_path.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

/// The one staging name a party without this process's clock or entropy
/// could still guess: `<save>.tmp.<pid>`. Staging must never render it.
pub(super) fn guessable_pid_staging_path(save_path: &Path) -> PathBuf {
    sibling_path(save_path, format!(".tmp.{}", std::process::id()))
}

pub(super) struct TempDir {
    pub(super) path: PathBuf,
}

impl TempDir {
    pub(super) fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-save-file-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&path));
        std::fs::create_dir_all(&path).expect("scratch directory must be creatable");
        Self { path }
    }

    pub(super) fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.path));
    }
}

#[test]
fn windows_data_dir_prefers_appdata_then_falls_back_to_the_user_profile() {
    let with_appdata = env_of(&[("APPDATA", "C:/Users/May/AppData/Roaming")]);
    assert_eq!(
        data_dir_for(HostFamily::Windows, with_appdata),
        Some(PathBuf::from("C:/Users/May/AppData/Roaming"))
    );

    let profile_only = env_of(&[("USERPROFILE", "C:/Users/May")]);
    assert_eq!(
        data_dir_for(HostFamily::Windows, profile_only),
        Some(
            PathBuf::from("C:/Users/May")
                .join("AppData")
                .join("Roaming")
        )
    );

    assert_eq!(data_dir_for(HostFamily::Windows, env_of(&[])), None);
}

#[test]
fn macos_data_dir_is_the_application_support_directory() {
    assert_eq!(
        data_dir_for(HostFamily::MacOs, env_of(&[("HOME", "/Users/may")])),
        Some(
            PathBuf::from("/Users/may")
                .join("Library")
                .join("Application Support")
        )
    );
    assert_eq!(data_dir_for(HostFamily::MacOs, env_of(&[])), None);
}

#[test]
fn xdg_data_dir_prefers_an_absolute_xdg_data_home() {
    let absolute = env_of(&[("XDG_DATA_HOME", "/srv/data"), ("HOME", "/home/may")]);
    assert_eq!(
        data_dir_for(HostFamily::Xdg, absolute),
        Some(PathBuf::from("/srv/data"))
    );
}

#[test]
fn xdg_data_dir_ignores_a_relative_xdg_data_home_and_uses_home() {
    let relative = env_of(&[("XDG_DATA_HOME", "data"), ("HOME", "/home/may")]);
    assert_eq!(
        data_dir_for(HostFamily::Xdg, relative),
        Some(PathBuf::from("/home/may").join(".local").join("share"))
    );
    assert_eq!(data_dir_for(HostFamily::Xdg, env_of(&[])), None);
}

#[test]
fn an_empty_environment_variable_counts_as_unset() {
    let empty = env_of(&[("HOME", ""), ("XDG_DATA_HOME", "")]);
    assert_eq!(data_dir_for(HostFamily::Xdg, empty), None);
}

#[test]
fn the_save_path_override_wins_over_every_data_directory() {
    let env = env_of(&[(SAVE_PATH_ENV, "/tmp/scratch.sav"), ("HOME", "/home/may")]);
    assert_eq!(
        default_save_path_from(HostFamily::Xdg, env).unwrap(),
        PathBuf::from("/tmp/scratch.sav")
    );
}

#[test]
fn without_an_override_the_save_path_is_the_named_file_under_the_data_directory() {
    let env = env_of(&[("HOME", "/home/may")]);
    assert_eq!(
        default_save_path_from(HostFamily::Xdg, env).unwrap(),
        PathBuf::from("/home/may")
            .join(".local")
            .join("share")
            .join(SAVE_DIR_NAME)
            .join(SAVE_FILE_NAME)
    );
}

#[test]
fn an_empty_override_falls_through_to_the_data_directory() {
    let env = env_of(&[(SAVE_PATH_ENV, ""), ("HOME", "/home/may")]);
    assert!(default_save_path_from(HostFamily::Xdg, env)
        .unwrap()
        .starts_with("/home/may"));
}

#[test]
fn no_data_directory_is_a_named_error_not_a_guessed_path() {
    let err = default_save_path_from(HostFamily::Xdg, env_of(&[])).unwrap_err();
    assert!(matches!(err, SaveFileError::NoDataDirectory));
    assert!(
        err.to_string().contains(SAVE_PATH_ENV),
        "the diagnostic must name the override that fixes it: {err}"
    );
}

pub(super) fn saved_store() -> (SaveStore, SaveBlock1, SaveBlock2) {
    let block2 = SaveBlock2 {
        encryption_key: 0x1234_5678,
        ..SaveBlock2::default()
    };
    let block1 = SaveBlock1 {
        money: 4321,
        ..SaveBlock1::default()
    };
    let mut store = SaveStore::new();
    store.save(&block1, &block2);
    (store, block1, block2)
}

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
fn reading_a_directory_in_the_files_place_is_an_io_error_not_a_panic() {
    let dir = TempDir::new("isdir");
    let path = dir.join(SAVE_FILE_NAME);
    std::fs::create_dir_all(&path).unwrap();

    let file = SaveFile::at(&path);
    assert!(!file.exists(), "a directory is not a save file");
    assert!(
        matches!(file.read(), Err(SaveFileError::Read { .. })),
        "reading a directory must surface as a read failure"
    );
}

/// The container of every level from the filesystem root down to `target`,
/// outermost first, computed independently of [`SaveFile::ancestor_chain`]
/// and [`SaveFile::directory_containing`] via [`Path::ancestors`].
fn expected_ancestor_containers(target: &Path) -> Vec<PathBuf> {
    let mut containers: Vec<PathBuf> = target
        .ancestors()
        .filter_map(|level| level.parent().map(Path::to_path_buf))
        .collect();
    containers.reverse();
    containers
}

#[test]
fn ancestor_chain_lists_every_level_outermost_first() {
    let dir = TempDir::new("ancestor-chain");
    let target = dir.join("one").join("two");

    let chain = SaveFile::ancestor_chain(&target);
    let mut expected: Vec<PathBuf> = target.ancestors().map(Path::to_path_buf).collect();
    expected.reverse();
    assert_eq!(
        chain, expected,
        "the chain must list every level from the filesystem root to the target, \
         outermost first, with nothing skipped or reordered"
    );
}

#[test]
fn locking_a_fresh_multi_level_root_syncs_the_whole_ancestor_chain() {
    let dir = TempDir::new("sync-created");
    let target = dir.join("one").join("two");
    let file = SaveFile::at(target.join(SAVE_FILE_NAME));

    let synced = std::cell::RefCell::new(Vec::new());
    let guard = file
        .lock_with(|path| synced.borrow_mut().push(path.to_path_buf()))
        .expect("a fresh multi-level root must be lockable");
    drop(guard);

    assert!(target.is_dir());
    assert_eq!(
        synced.into_inner(),
        expected_ancestor_containers(&target),
        "on a first save, every level's directory entry in its own container must be \
         synced, outermost first, and nothing else -- otherwise a created directory's \
         entry can be unsynced and vanish after a power loss, or an unrelated \
         directory can be synced unintentionally"
    );
}

#[test]
fn a_first_save_under_an_absolute_path_never_syncs_the_working_directory() {
    let dir = TempDir::new("absolute-no-cwd-sync");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    assert!(dir.path.is_absolute(), "the temp root must be absolute");

    let synced = std::cell::RefCell::new(Vec::new());
    let guard = file
        .lock_with(|path| synced.borrow_mut().push(path.to_path_buf()))
        .expect("a fresh absolute path must be lockable");
    drop(guard);

    assert!(
        !synced.into_inner().contains(&PathBuf::from(".")),
        "a first save under an absolute path must never sync the process's working \
         directory -- the filesystem root has no container to record its own entry in"
    );
}

#[test]
fn directory_containing_has_no_entry_to_record_for_a_level_that_already_exists() {
    for level in [Path::new("/"), Path::new("."), Path::new("..")] {
        assert_eq!(
            SaveFile::directory_containing(level),
            None,
            "{level:?} always exists already, so it has no directory entry that \
             `create_dir_all` could have made and no container to synchronise"
        );
    }
}

#[test]
fn directory_containing_returns_the_parent_for_a_level_create_dir_all_can_make() {
    assert_eq!(
        SaveFile::directory_containing(Path::new("/tmp")),
        Some(Path::new("/"))
    );
    assert_eq!(
        SaveFile::directory_containing(Path::new("sub")),
        Some(Path::new(".")),
        "a bare relative name's entry lives in the working directory"
    );
    assert_eq!(
        SaveFile::directory_containing(Path::new("../saves")),
        Some(Path::new("..")),
        "the entry this level actually adds lives in its literal parent, not in \
         the working directory the whole relative path is resolved against"
    );
    assert_eq!(
        SaveFile::directory_containing(Path::new("./saves")),
        Some(Path::new(".")),
    );
}

#[test]
fn a_locker_that_wins_the_race_syncs_ancestors_an_earlier_contender_left_unsynced() {
    let dir = TempDir::new("race");
    let target = dir.join("one").join("two");
    let path = target.join(SAVE_FILE_NAME);

    // An earlier contender created the hierarchy but was pre-empted before
    // it could sync or lock -- its directories now exist on disk with
    // nobody yet having synced them.
    let first = SaveFile::at(&path);
    first.create_parent_directory().unwrap();

    let second = SaveFile::at(&path);
    let synced = std::cell::RefCell::new(Vec::new());
    let guard = second
        .lock_with(|p| synced.borrow_mut().push(p.to_path_buf()))
        .unwrap();
    drop(guard);

    assert_eq!(
        synced.into_inner(),
        expected_ancestor_containers(&target),
        "a locker must sync every ancestor's container of a first save regardless of \
         who created it on disk, and nothing else -- otherwise an earlier contender's \
         unsynced work can be reported as a successful save"
    );
}

#[test]
fn locking_after_a_successful_first_save_syncs_nothing_more() {
    let dir = TempDir::new("sync-after-first-save");
    let file = SaveFile::at(dir.join("one").join("two").join(SAVE_FILE_NAME));
    let (store, _, _) = saved_store();

    let guard = file.lock().unwrap();
    file.write(&store).unwrap();
    drop(guard);

    let synced = std::cell::RefCell::new(Vec::new());
    let guard = file
        .lock_with(|path| synced.borrow_mut().push(path.to_path_buf()))
        .unwrap();
    drop(guard);

    assert!(
        synced.into_inner().is_empty(),
        "once a save file exists, this is no longer a first save, so ancestors must \
         not be resynced on every subsequent lock"
    );
}

#[test]
fn locking_synchronises_ancestors_only_once_the_lock_is_held() {
    let dir = TempDir::new("sync-order");
    let path = dir.join("nested").join("deeper").join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);

    let synced_while_locked = std::cell::Cell::new(false);
    let lock_path = file.lock_path();
    let guard = file
        .lock_with(|_ancestor_parent| {
            let probe = std::fs::OpenOptions::new()
                .write(true)
                .open(&lock_path)
                .expect("the lock file must already exist while ancestors are synced");
            synced_while_locked.set(matches!(
                probe.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
        })
        .expect("locking must create the missing hierarchy");
    drop(guard);

    assert!(
        synced_while_locked.get(),
        "ancestors must be synced only after this call holds the exclusive lock -- \
         syncing before locking would let a second, concurrent locker report a \
         successful save before either locker had made them durable"
    );
}

#[test]
fn locking_before_any_directory_exists_creates_the_whole_hierarchy() {
    let dir = TempDir::new("lock-mkdir");
    let path = dir.join("nested").join("deeper").join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);

    let guard = file
        .lock()
        .expect("locking must create the missing hierarchy");
    assert!(path.parent().unwrap().is_dir());
    assert!(file.lock_path().exists());

    let (store, _, _) = saved_store();
    file.write(&store).unwrap();
    drop(guard);

    let reloaded = file.read().unwrap().expect("the file was just written");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

/// The longest basename `parent` accepts as a save file, found by growing
/// one byte at a time until the host itself refuses one.
///
/// The ceiling is `PATH_MAX`, not [`staging::MAX_COMPONENT_LEN`], which that
/// module documents as a guess: a FUSE mount negotiates a component limit far
/// above 255, and stopping at the guess would report a length this host still
/// accepts as its longest.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn longest_valid_save_basename(parent: &Path) -> usize {
    (1..4096)
        .take_while(|&len| {
            let candidate = parent.join("n".repeat(len));
            match std::fs::File::create(&candidate) {
                Ok(_) => {
                    std::fs::remove_file(&candidate).unwrap();
                    true
                }
                Err(err) if err.kind() == std::io::ErrorKind::InvalidFilename => false,
                Err(err) => {
                    panic!("unexpected error probing the host's real path limit: {err:?}")
                }
            }
        })
        .last()
        .unwrap_or(0)
}

/// A save at the host's longest valid basename, which overflows the
/// component limit once `.lock` is appended, must still be lockable.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_save_at_the_hosts_longest_valid_basename_is_still_lockable() {
    let dir = TempDir::new("long-basename-lock");
    let basename_len = longest_valid_save_basename(&dir.path);
    let path = dir.join(&"n".repeat(basename_len));
    let file = SaveFile::at(&path);
    std::fs::File::create(&path).expect("the longest valid basename must itself be writable");
    std::fs::remove_file(&path).unwrap();

    // `<save>.lock` is the name that made this save unlockable. A host that
    // still accepts it cannot reproduce the defect, which is the host's to
    // offer and not this test's to demand, so note it and keep asserting the
    // property -- a save this host accepts is lockable either way.
    let derived_sibling = sibling_path(&path, ".lock");
    let boundary_reproduced = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&derived_sibling)
        .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidFilename);
    if !boundary_reproduced {
        drop(std::fs::remove_file(&derived_sibling));
        eprintln!(
            "note: this host accepts a {basename_len}-byte basename's `.lock` \
             sibling, so #1189's boundary is not reproduced here; the \
             lockability assertions below still run"
        );
    }

    let guard = file
        .lock()
        .expect("a save this host accepts must be lockable");
    let (store, _, _) = saved_store();
    file.write(&store).unwrap();
    drop(guard);

    let reloaded = file.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

/// Two distinct saves in one directory deliberately share the directory's
/// one lock. That trade -- made because a volume's aliases cannot be
/// enumerated, so any basename-derived name splits some one save across two
/// locks -- is only sound if the shared lock genuinely excludes and
/// genuinely releases, so both halves are pinned here.
#[test]
fn two_saves_in_one_directory_serialise_on_the_directorys_lock() {
    let dir = TempDir::new("directory-lock-serialises");
    let first = SaveFile::at(dir.join("first.sav"));
    let second = SaveFile::at(dir.join("second.sav"));

    assert_eq!(
        first.lock_path(),
        second.lock_path(),
        "every save in one directory must take one lock"
    );

    let guard = first.lock().expect("the first save must be lockable");

    let probe =
        SaveFile::open_lock_file(&second.lock_path()).expect("the lock file must be openable");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("the directory lock must exclude the second save, got {other:?}"),
    }
    drop(probe);
    drop(guard);

    let released = second
        .lock()
        .expect("the lock must be free once the first guard drops");
    drop(released);
}

/// The lock path must be one fixed name per directory, never derived from
/// the save's basename.
///
/// A volume's aliases cannot be enumerated from `std`, and `K` (U+212A)
/// against `k` also differs in encoded length; a fixed name has no spelling to split.
#[test]
fn the_lock_path_is_one_fixed_name_per_directory() {
    let dir = TempDir::new("lock-path-fixed");
    let spellings = [
        SAVE_FILE_NAME.to_owned(),
        "S".repeat(140),
        "s".repeat(140),
        "\u{c4}".repeat(70),
        "\u{e4}".repeat(70),
        // The straddling pair: 141 encoded bytes against 47, one entry.
        "k".repeat(141),
        "\u{212a}".repeat(47),
    ];

    for spelling in &spellings {
        assert_eq!(
            SaveFile::at(dir.join(spelling)).lock_path(),
            dir.path.join(LOCK_FILE_NAME),
            "every save in one directory must derive one fixed lock path, whatever \
             its basename's spelling or encoded length"
        );
    }
}

/// A save whose basename is not UTF-8 is still a save, and still takes the
/// directory's lock: the lock name never reads the save's own bytes.
#[cfg(unix)]
#[test]
fn a_non_utf8_save_basename_still_takes_the_directorys_lock() {
    use std::os::unix::ffi::OsStrExt;

    let dir = TempDir::new("non-utf8-lock");
    let file = SaveFile::at(dir.path.join(std::ffi::OsStr::from_bytes(b"\x80")));

    assert_eq!(file.lock_path(), dir.path.join(LOCK_FILE_NAME));
    let guard = file
        .lock()
        .expect("a non-UTF-8 save basename must still be lockable");
    drop(guard);
}

#[test]
fn the_save_lock_excludes_a_second_locker_until_dropped() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let dir = TempDir::new("lock");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let first_lock_released = Arc::new(AtomicBool::new(false));

    let guard = file.lock().expect("first lock must succeed");

    let probe = std::fs::OpenOptions::new()
        .write(true)
        .open(file.lock_path())
        .expect("the lock file exists while the guard is held");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("the held lock must exclude a second locker, got {other:?}"),
    }
    drop(probe);

    let contender = {
        let first_lock_released = Arc::clone(&first_lock_released);
        let file = SaveFile::at(&path);
        std::thread::spawn(move || {
            let _guard = file.lock().expect("second lock must eventually succeed");
            first_lock_released.load(Ordering::SeqCst)
        })
    };
    // This gives the contender a chance to block; the nonblocking probe above proves exclusion.
    std::thread::yield_now();
    first_lock_released.store(true, Ordering::SeqCst);
    drop(guard);
    assert!(
        contender.join().expect("contender must not panic"),
        "the second lock() returned while the first guard was still held"
    );
}

/// A bare relative save-file name, unique to this process and thread, that
/// removes itself on drop -- never touching the working directory every thread shares.
struct BareRelativeSave {
    name: PathBuf,
}

impl BareRelativeSave {
    fn unique(label: &str) -> Self {
        Self {
            name: PathBuf::from(format!(
                "pokeemerald-rs-save-file-{label}-{}-{:?}.sav",
                std::process::id(),
                std::thread::current().id()
            )),
        }
    }
}

impl Drop for BareRelativeSave {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.name));
    }
}

#[test]
fn a_bare_relative_save_path_syncs_the_working_directory_after_the_rename() {
    let bare = BareRelativeSave::unique("write");
    let file = SaveFile::at(bare.name.clone());
    let (store, _, _) = saved_store();

    let synced = std::cell::RefCell::new(Vec::new());
    file.write_with(
        &store,
        |path| synced.borrow_mut().push(path.to_path_buf()),
        |bytes| staging::StagingArea::beside(file.path()).stage(bytes),
        |_| {},
    )
    .expect("writing a bare relative save path must succeed");

    assert_eq!(
        synced.into_inner(),
        vec![PathBuf::from(".")],
        "a bare relative save path's directory entry lives in the working directory, and \
         the rename must best-effort sync it exactly once"
    );
}

/// A save configured at the lock path itself must be refused, not locked.
///
/// Locking it would hand out a guard on that save's own data file, and its
/// next write renames a fresh inode over that file: every later locker
/// would open the replacement and exclude nobody, which is exactly the
/// replaced-inode hazard the lock is sited on a sibling to avoid. Refusing
/// fails closed instead.
#[test]
fn a_save_configured_at_the_lock_path_is_refused_rather_than_locked() {
    let dir = TempDir::new("save-at-the-lock-path");
    let file = SaveFile::at(dir.join(LOCK_FILE_NAME));

    match file.lock() {
        Err(SaveFileError::LockPathIsSave { path }) => assert_eq!(path, file.lock_path()),
        other => panic!(
            "a save at the lock path must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
}

/// The refusal must recognise the lock file by the entry it is, not by the
/// name it is spelled with.
///
/// A case-folding or normalising volume resolves byte-different names to
/// one entry, and `std` cannot enumerate those aliases, so comparing
/// basenames bytewise would let an aliased spelling through. A symlink
/// stands in for that aliasing here because it is the one alias a
/// case-sensitive test host also supports.
#[cfg(unix)]
#[test]
fn a_save_that_only_resolves_to_the_lock_path_is_refused_too() {
    let dir = TempDir::new("save-aliasing-the-lock-path");
    let file = SaveFile::at(dir.join("aliased.sav"));

    std::fs::write(dir.join(LOCK_FILE_NAME), b"").unwrap();
    std::os::unix::fs::symlink(dir.join(LOCK_FILE_NAME), file.path()).unwrap();
    assert_ne!(
        file.path().file_name(),
        file.lock_path().file_name(),
        "the save must not be refusable by its name alone, or this proves nothing"
    );

    match file.lock() {
        Err(SaveFileError::LockPathIsSave { .. }) => {}
        other => panic!(
            "a save resolving to the lock file must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
}

/// Windows has no stable by-handle identity, so a hard link to the lock
/// file canonicalises to itself and no path comparison can recognise it.
/// The lock handle therefore denies `FILE_SHARE_DELETE`, which closes the
/// hazard rather than detecting it: while the guard is held, nothing can
/// rename over that entry or unlink it, so an aliasing save's publishing
/// rename fails loudly instead of quietly replacing the locked inode.
///
/// That denial must not cost the contention path it protects: a second
/// locker must still open the very same file and block on it.
#[cfg(windows)]
#[test]
fn a_held_lock_refuses_replacement_while_still_admitting_a_second_locker() {
    let dir = TempDir::new("windows-lock-sharing");
    let first = SaveFile::at(dir.join("first.sav"));
    let second = SaveFile::at(dir.join("second.sav"));

    let guard = first.lock().expect("the first save must be lockable");

    let probe = SaveFile::open_lock_file(&second.lock_path())
        .expect("denying delete sharing must still admit a second locker's open");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("the held lock must exclude the second locker, got {other:?}"),
    }
    drop(probe);

    let replacement = dir.join("replacement");
    std::fs::write(&replacement, b"").unwrap();
    assert!(
        std::fs::rename(&replacement, first.lock_path()).is_err(),
        "renaming over a held lock must fail: replacing that inode would leave this \
         guard holding an unlinked file and exclude nobody afterwards"
    );
    assert!(
        std::fs::remove_file(first.lock_path()).is_err(),
        "unlinking a held lock must fail for the same reason"
    );

    drop(guard);
}

/// A symlink planted in the fixed lock slot must be refused.
///
/// Opening follows it, so every locker lands on whatever it points at at
/// that moment. Let the target be replaced by its own directory's save --
/// an ordinary rename there -- and a locker from before the rename holds
/// the old inode while one from after holds the new, so two processes run
/// the read-modify-write cycle this lock exists to serialise.
#[cfg(unix)]
#[test]
fn a_symlinked_lock_slot_is_refused() {
    let dir = TempDir::new("symlinked-lock-slot");
    let elsewhere = TempDir::new("symlinked-lock-slot-target");
    let target = elsewhere.join("someone-elses.sav");
    std::fs::write(&target, b"").unwrap();

    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    std::os::unix::fs::symlink(&target, file.lock_path()).unwrap();

    match file.lock() {
        Err(SaveFileError::LockPathIsAlias { path }) => assert_eq!(path, file.lock_path()),
        other => panic!(
            "a symlinked lock slot must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
}

/// A dangling symlink in the lock slot must be refused before anything
/// opens it.
///
/// The open carries `create`, so following one would create its target
/// outside the save directory -- a side effect no later check can undo,
/// which is why the refusal has to come first. The same ordering is what
/// keeps a slot pointing at a FIFO from parking the open until some reader
/// turns up.
#[cfg(unix)]
#[test]
fn a_dangling_symlinked_lock_slot_is_refused_without_creating_its_target() {
    let dir = TempDir::new("dangling-lock-slot");
    let elsewhere = TempDir::new("dangling-lock-slot-target");
    let target = elsewhere.join("never-created.sav");

    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    std::os::unix::fs::symlink(&target, file.lock_path()).unwrap();

    match file.lock() {
        Err(SaveFileError::LockPathIsAlias { path }) => assert_eq!(path, file.lock_path()),
        other => panic!(
            "a dangling symlinked lock slot must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
    assert!(
        !target.exists(),
        "the refusal must precede the open, which would otherwise have created {} \
         outside the save directory",
        target.display()
    );
}

/// A lock slot that is not a plain file must be refused rather than
/// opened: it cannot carry a lock, and opening a FIFO there would block.
/// A directory stands in, being the one such entry `std` can create.
#[test]
fn a_lock_slot_that_is_not_a_plain_file_is_refused() {
    let dir = TempDir::new("non-file-lock-slot");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    std::fs::create_dir(file.lock_path()).unwrap();

    match file.lock() {
        Err(SaveFileError::LockPathNotAPlainFile { path }) => assert_eq!(path, file.lock_path()),
        other => panic!(
            "a lock slot that is not a plain file must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
}

/// One lock file serves every save in a directory, so it is created under
/// whichever umask its first saver had. A later saver who cannot write
/// that file must still be able to lock it -- otherwise a second user in a
/// shared directory, or the same user after a run under different
/// privileges, is shut out of a perfectly valid save for good.
///
/// A privileged runner bypasses the mode outright, so it is CI's
/// unprivileged runners that actually drive the fallback here; the
/// property asserted holds either way.
#[cfg(unix)]
#[test]
fn a_lock_file_this_user_cannot_write_is_still_lockable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new("read-only-lock-file");
    let first = SaveFile::at(dir.join("first.sav"));
    let second = SaveFile::at(dir.join("second.sav"));

    std::fs::write(first.lock_path(), b"").unwrap();
    std::fs::set_permissions(first.lock_path(), std::fs::Permissions::from_mode(0o444)).unwrap();

    let guard = first
        .lock()
        .expect("a lock file this user cannot write must still be lockable");

    let probe = SaveFile::open_lock_file(&second.lock_path())
        .expect("a second saver must still open the shared lock");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("a read-only lock handle must still exclude a second saver, got {other:?}"),
    }
    drop(probe);
    drop(guard);
}

/// A hard link in the lock slot must still be accepted.
///
/// Unlike a symlink it is a name of its own: renaming any of the inode's
/// other names leaves this slot naming the inode this guard holds, so a
/// later locker still lands on it and is still excluded. Hard-linking
/// backup tools snapshot directories this way, and refusing them would
/// cost availability for a hazard that is not there.
#[cfg(unix)]
#[test]
fn a_hard_linked_lock_slot_is_still_accepted() {
    let dir = TempDir::new("hard-linked-lock-slot");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));

    std::fs::write(file.lock_path(), b"").unwrap();
    std::fs::hard_link(file.lock_path(), dir.join("backup-snapshot")).unwrap();

    let guard = file
        .lock()
        .expect("a second name for the lock file's own inode must not refuse the lock");
    drop(guard);
}

/// An ordinary lock file an earlier run left behind must still be usable:
/// the slot check must reject aliases, not every pre-existing file.
#[cfg(unix)]
#[test]
fn an_ordinary_pre_existing_lock_file_is_still_accepted() {
    let dir = TempDir::new("pre-existing-lock-slot");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    std::fs::write(file.lock_path(), b"left by an earlier run").unwrap();

    let guard = file
        .lock()
        .expect("an ordinary lock file from an earlier run must be reusable");
    drop(guard);
}

/// The refusal must compare filesystem identity, not canonical path text.
///
/// `canonicalize` is not a canonical *entry*: on a case-folding Linux
/// directory it is `realpath`, which keeps the caller's own spelling, so a
/// save configured as `.EMERALD.LOCK` names the same entry as
/// `.emerald.lock` yet canonicalises to a different string -- and a
/// text comparison would admit it, after which its write renames a fresh
/// inode over the file every locker holds.
///
/// A hard link reproduces exactly that shape -- one inode, two canonical
/// paths -- on any Unix host, so the property is pinned without needing a
/// case-folding volume to test on.
#[cfg(unix)]
#[test]
fn a_save_sharing_the_lock_files_inode_is_refused_despite_a_different_canonical_path() {
    let dir = TempDir::new("save-hard-linked-to-the-lock");
    let file = SaveFile::at(dir.join("linked.sav"));

    std::fs::write(dir.join(LOCK_FILE_NAME), b"").unwrap();
    std::fs::hard_link(dir.join(LOCK_FILE_NAME), file.path()).unwrap();
    assert_ne!(
        std::fs::canonicalize(file.path()).unwrap(),
        std::fs::canonicalize(file.lock_path()).unwrap(),
        "the two names must canonicalise differently, or this does not exercise the \
         boundary a path-text comparison misses"
    );

    match file.lock() {
        Err(SaveFileError::LockPathIsSave { .. }) => {}
        other => panic!(
            "a save on the lock file's own inode must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }
}

/// The refusal must catch only the save that *is* the lock file. An
/// ordinary save sharing the directory holds that lock rather than being
/// refused by it, before and after it exists on disk.
#[test]
fn an_ordinary_save_beside_the_lock_file_still_locks() {
    let dir = TempDir::new("ordinary-save-beside-lock");
    let file = SaveFile::at(dir.join(SAVE_FILE_NAME));
    let (store, _, _) = saved_store();

    let guard = file
        .lock()
        .expect("a save that does not exist yet must be lockable");
    file.write(&store).unwrap();
    drop(guard);

    let guard = file
        .lock()
        .expect("an existing ordinary save must still be lockable");
    drop(guard);
}

/// `_POSIX_NAME_MAX` is 14; a host at that limit still writes short saves.
#[test]
fn the_lock_file_name_fits_the_posix_minimum_component_limit() {
    assert!(
        LOCK_FILE_NAME.len() <= 14,
        "{LOCK_FILE_NAME:?} exceeds 14 bytes"
    );
}

/// A `umask 077` creator leaves `0600`, which no later owner can even read.
#[cfg(unix)]
#[test]
fn a_freshly_created_lock_file_is_lockable_by_every_owner() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new("lock-mode");
    let file = SaveFile::at(dir.path.join("a.sav"));
    let guard = file.lock().expect("a guard");
    let mode = std::fs::metadata(dir.path.join(LOCK_FILE_NAME))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o666,
        0o666,
        "lock mode {mode:o} shuts later owners out"
    );
    let leftovers: Vec<_> = std::fs::read_dir(&dir.path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name != LOCK_FILE_NAME)
        .collect();
    assert!(
        leftovers.is_empty(),
        "locking left {leftovers:?} beside the save"
    );
    drop(guard);
}

/// Only the creator widens; an existing slot keeps its owner's mode.
#[cfg(unix)]
#[test]
fn a_pre_existing_lock_file_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new("lock-mode-kept");
    let lock_path = dir.path.join(LOCK_FILE_NAME);
    std::fs::write(&lock_path, b"").unwrap();
    std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let file = SaveFile::at(dir.path.join("a.sav"));
    let guard = file.lock().expect("a guard");
    let mode = std::fs::metadata(&lock_path).unwrap().permissions().mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "an existing slot's mode was rewritten to {mode:o}"
    );
    drop(guard);
}

/// Staging must never unlink an entry it did not create.
#[cfg(unix)]
#[test]
fn a_file_already_bearing_the_lock_staging_name_survives_a_lock() {
    let dir = TempDir::new("lock-stage-collision");
    let taken = SaveFile::staging_name();
    let bystander = dir.path.join(&taken);
    std::fs::write(&bystander, b"a save that happens to bear the staging name").unwrap();
    let lock_path = dir.path.join(LOCK_FILE_NAME);
    let mut draws = vec![taken, SaveFile::staging_name()].into_iter();

    let created = SaveFile::create_lock_file_with(&lock_path, || draws.next().unwrap())
        .expect("a second draw finds a free staging name");

    assert!(created.is_some(), "the slot was created");
    assert_eq!(
        std::fs::read(&bystander).unwrap(),
        b"a save that happens to bear the staging name"
    );
    assert!(lock_path.exists());
    assert!(
        draws.next().is_none(),
        "the taken name was drawn and skipped"
    );
}

/// A pool of taken staging names exhausts creation rather than unlinking one.
#[cfg(unix)]
#[test]
fn creation_gives_up_when_every_drawn_staging_name_is_taken() {
    let dir = TempDir::new("lock-stage-exhausted");
    let taken = SaveFile::staging_name();
    let leftover = dir.path.join(&taken);
    std::fs::write(&leftover, b"leftover").unwrap();
    let lock_path = dir.path.join(LOCK_FILE_NAME);
    let mut draws = 0usize;

    let error = SaveFile::create_lock_file_with(&lock_path, || {
        draws += 1;
        taken.clone()
    })
    .expect_err("no free staging name");

    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(draws, usize::from(u8::MAX) + 1);
    assert_eq!(std::fs::read(&leftover).unwrap(), b"leftover");
    assert!(!lock_path.exists());
}

/// Leftover staging names never stand between a saver and an existing lock.
#[cfg(unix)]
#[test]
fn an_existing_lock_is_opened_even_when_every_staging_name_is_taken() {
    let dir = TempDir::new("lock-stage-existing");
    let lock_path = dir.path.join(LOCK_FILE_NAME);
    std::fs::write(&lock_path, b"").unwrap();

    let opened = SaveFile::open_lock_file_with(&lock_path, |_| {
        panic!("an existing slot is opened without staging")
    })
    .expect("the existing lock is opened without staging");

    drop(opened);
}

/// Leftover staging names beside an absent slot are skipped, never unlinked.
#[cfg(unix)]
#[test]
fn leftover_staging_names_beside_the_lock_slot_still_admit_a_first_lock() {
    let dir = TempDir::new("lock-staging-names-taken");
    let leftovers: Vec<String> = (0..16).map(|_| SaveFile::staging_name()).collect();
    for leftover in &leftovers {
        std::fs::write(dir.path.join(leftover), b"leftover").unwrap();
    }
    let lock_path = dir.path.join(LOCK_FILE_NAME);
    let mut draws = leftovers
        .iter()
        .cloned()
        .chain(std::iter::once(SaveFile::staging_name()));

    let file = SaveFile::open_lock_file_with(&lock_path, |path| {
        SaveFile::create_lock_file_with(path, || draws.next().unwrap())
    })
    .expect("a first save finds a free staging name");

    file.lock().expect("the created slot locks");
    for leftover in &leftovers {
        assert_eq!(
            std::fs::read(dir.path.join(leftover)).unwrap(),
            b"leftover",
            "{leftover:?} was disturbed"
        );
    }
}

/// The name staging draws is `.lk` plus ten hex digits, no longer than the
/// lock name and never a pid name.
#[cfg(unix)]
#[test]
fn the_drawn_lock_staging_name_is_no_longer_than_the_lock_name() {
    let pid_name = format!(".lk{}", std::process::id());
    for _ in 0..64 {
        let name = SaveFile::staging_name();
        let digits = name.strip_prefix(".lk").expect("a .lk staging name");
        assert_eq!(digits.len(), 10, "{name} is not ten digits wide");
        assert_eq!(name.len(), LOCK_FILE_NAME.len());
        assert!(
            digits.bytes().all(|b| b.is_ascii_hexdigit()),
            "{name} is not hexadecimal"
        );
        assert_ne!(name, pid_name);
    }
}

/// Staging cleanup unlinks only the inode it created.
#[cfg(unix)]
#[test]
fn lock_staging_cleanup_spares_an_entry_it_did_not_create() {
    let dir = TempDir::new("lock-stage-not-mine");
    let staged = dir.path.join(SaveFile::staging_name());
    let mine = std::fs::File::create(&staged).unwrap();
    std::fs::remove_file(&staged).unwrap();
    std::fs::write(&staged, b"a stranger's file under the staging name").unwrap();
    SaveFile::remove_only_own_staging(&staged, &mine);
    assert_eq!(
        std::fs::read(&staged).expect("the stranger's file survives"),
        b"a stranger's file under the staging name"
    );
}

/// A directory exactly `total_len` bytes long, from components under 255 bytes.
#[cfg(target_os = "linux")]
fn directory_of_length(root: &Path, total_len: usize) -> PathBuf {
    let mut path = root.to_path_buf();
    let mut remaining = total_len - path.as_os_str().len();
    assert!(remaining >= 2, "the scratch root must leave room to nest");
    let mut first = remaining % 201;
    if first < 2 {
        first += 201;
    }
    path.push("d".repeat(first - 1));
    std::fs::create_dir(&path).unwrap();
    remaining -= first;
    while remaining > 0 {
        path.push("d".repeat(200));
        std::fs::create_dir(&path).unwrap();
        remaining -= 201;
    }
    assert_eq!(path.as_os_str().len(), total_len);
    path
}

/// A directory leaving no room for the fixed lock name must fail closed.
///
/// The name is 13 bytes where the save's own basename may be one, so a
/// host-valid save path can sit inside `PATH_MAX` while its lock path does
/// not. Locking some other entry instead would hand this save a second lock
/// identity that every ordinary spelling of the directory ignores, so the
/// refusal is the whole point: no guard, and nothing created.
#[cfg(target_os = "linux")]
#[test]
fn a_directory_with_no_room_for_the_lock_name_is_refused_rather_than_locked() {
    const LONGEST_PATH: usize = 4095;
    let temp = TempDir::new("deep-directory");
    let parent = directory_of_length(&temp.path, LONGEST_PATH - LOCK_FILE_NAME.len());
    let save_path = parent.join("a");
    std::fs::write(&save_path, [0u8; 1]).expect("the host accepts this save path");
    assert!(save_path.as_os_str().len() <= LONGEST_PATH);

    let file = SaveFile::at(&save_path);
    match file.lock() {
        Err(SaveFileError::Lock { path, source }) => {
            assert_eq!(
                path,
                file.lock_path(),
                "the refusal must name the lock this save cannot have"
            );
            assert_eq!(source.kind(), std::io::ErrorKind::InvalidFilename);
        }
        other => panic!(
            "a directory with no room for the lock name must be refused, got {:?}",
            other.map(|_| "a guard")
        ),
    }

    let entries: Vec<_> = std::fs::read_dir(&parent)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name != "a")
        .collect();
    assert!(
        entries.is_empty(),
        "a refused lock must create nothing: {entries:?}"
    );
}

/// Two spellings of one save directory must contend for one lock.
///
/// A short symlinked spelling of a directory spelled near `PATH_MAX` reaches
/// the same entries, so both spellings must land on the one `.emerald.lock`
/// inode. Locking anything else when the name does not fit -- the directory's
/// own inode, say -- gives the long spelling a second identity the short one
/// never takes, and two processes then run `SaveSlot::store`'s
/// read-modify-write cycle at once, one overwriting the other's progress.
#[cfg(target_os = "linux")]
#[test]
fn two_spellings_of_one_save_directory_contend_for_one_lock() {
    const LONGEST_PATH: usize = 4095;
    let deep_root = TempDir::new("aliased-directory-deep");
    let short_root = TempDir::new("aliased-directory-short");
    let deep = directory_of_length(&deep_root.path, LONGEST_PATH - LOCK_FILE_NAME.len());
    let short = short_root.join("d");
    std::os::unix::fs::symlink(&deep, &short).unwrap();

    let long_spelling = SaveFile::at(deep.join("a"));
    let short_spelling = SaveFile::at(short.join("a"));
    std::fs::write(long_spelling.path(), [0u8; 1]).expect("the host accepts the long save path");
    assert!(
        short_spelling.path().exists(),
        "both spellings must reach the one save"
    );
    assert!(
        std::fs::File::create(deep.join(LOCK_FILE_NAME))
            .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidFilename),
        "the long spelling must have no room for the lock name the short spelling \
         has room for, or the two spellings were never at risk of splitting"
    );

    let guard = short_spelling
        .lock()
        .expect("the short spelling reaches the lock name");
    let probe = short_spelling
        .open_lock_slot()
        .expect("the one lock slot opens");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("the one lock slot must exclude a second locker, got {other:?}"),
    }
    drop(probe);

    assert!(
        matches!(long_spelling.lock(), Err(SaveFileError::Lock { .. })),
        "the long spelling must fail closed rather than take a second identity \
         while the short spelling holds the directory's one lock"
    );
    drop(guard);
}
