use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{
    data_dir_for, default_save_path_from, staging, HostFamily, SaveFile, SaveFileError,
    SAVE_DIR_NAME, SAVE_FILE_NAME, SAVE_PATH_ENV,
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
    let guard = file
        .lock_with(|_ancestor_parent| {
            let probe = std::fs::OpenOptions::new()
                .write(true)
                .open(sibling_path(&path, ".lock"))
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
    assert!(sibling_path(&path, ".lock").exists());

    let (store, _, _) = saved_store();
    file.write(&store).unwrap();
    drop(guard);

    let reloaded = file.read().unwrap().expect("the file was just written");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

/// The longest basename `parent` accepts as a save file, found by growing
/// one byte at a time until the host refuses it.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn longest_valid_save_basename(parent: &Path) -> usize {
    (1..=staging::MAX_COMPONENT_LEN)
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

    assert!(
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(sibling_path(&path, ".lock"))
            .is_err_and(|err| err.kind() == std::io::ErrorKind::InvalidFilename),
        "test setup must actually push the naive lock sibling over the host's component \
         limit, or this does not exercise the fallback"
    );

    let guard = file
        .lock()
        .expect("a save this host accepts must be lockable");
    let (store, _, _) = saved_store();
    file.write(&store).unwrap();
    drop(guard);

    let reloaded = file.read().unwrap().expect("the save must be readable");
    assert_eq!(reloaded.flash_image(), store.flash_image());
}

/// A host limit under the naive `<save>.lock` name must fall back to the
/// shared sidecar, and that fallback must actually be locked.
#[test]
fn a_host_component_limit_below_the_naive_lock_name_falls_back_to_the_shared_lock_name() {
    const HOST_NAME_MAX: usize = 143;
    let dir = TempDir::new("lock-host-limit");
    let path = dir.join(&"s".repeat(140));
    let file = SaveFile::at(&path);

    let attempted = std::cell::RefCell::new(Vec::new());
    let refuse_long_components = |candidate: &Path| -> std::io::Result<std::fs::File> {
        attempted.borrow_mut().push(candidate.to_path_buf());
        let component_len = candidate
            .file_name()
            .map_or(0, |name| name.as_encoded_bytes().len());
        if component_len > HOST_NAME_MAX {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
        }
        SaveFile::open_lock_file(candidate)
    };

    let guard = file
        .lock_with_open(|_| {}, refuse_long_components)
        .expect("a host limit under the naive lock name must still be lockable");

    assert_eq!(
        attempted.into_inner(),
        vec![sibling_path(&path, ".lock"), file.shared_lock_path()],
        "the shared fallback must be tried only after the naive name is refused"
    );

    let probe = std::fs::OpenOptions::new()
        .write(true)
        .open(file.shared_lock_path())
        .expect("the shared lock file exists while the guard is held");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => panic!("the shared lock must actually exclude a second locker, got {other:?}"),
    }
    drop(probe);
    drop(guard);
}

/// Two distinct over-limit saves in one directory deliberately share the
/// fallback sidecar. That trade -- made because a host's aliases cannot be
/// enumerated, so no basename-derived name can keep one save on one lock --
/// is only sound if the shared lock genuinely excludes and genuinely
/// releases, so both halves are pinned here.
#[test]
fn two_over_limit_saves_in_one_directory_serialise_on_the_shared_lock() {
    const HOST_NAME_MAX: usize = 143;
    let dir = TempDir::new("shared-fallback-lock");
    let first = SaveFile::at(dir.join(&format!("{}a", "s".repeat(139))));
    let second = SaveFile::at(dir.join(&format!("{}b", "s".repeat(139))));

    let refuse_long_components = |candidate: &Path| -> std::io::Result<std::fs::File> {
        let component_len = candidate
            .file_name()
            .map_or(0, |name| name.as_encoded_bytes().len());
        if component_len > HOST_NAME_MAX {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
        }
        SaveFile::open_lock_file(candidate)
    };

    assert_eq!(
        first.shared_lock_path(),
        second.shared_lock_path(),
        "every over-limit save in one directory must fall back to one sidecar"
    );

    let guard = first
        .lock_with_open(|_| {}, refuse_long_components)
        .expect("the first over-limit save must be lockable");

    let probe = SaveFile::open_lock_file(&second.shared_lock_path())
        .expect("the shared lock file must be openable");
    match probe.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => {}
        other => {
            panic!("the shared fallback must exclude the second over-limit save, got {other:?}")
        }
    }
    drop(probe);
    drop(guard);

    let released = second
        .lock_with_open(|_| {}, refuse_long_components)
        .expect("the shared lock must be free once the first guard drops");
    drop(released);
}

/// A real, host-safe stand-in file for `candidate`, keyed by a hash of its
/// raw bytes so two distinct candidates -- even non-UTF-8 ones no host need
/// accept as a dirent -- get two distinct backing files without touching
/// the raw name itself.
fn non_utf8_safe_backing(dir: &Path) -> impl Fn(&Path) -> std::io::Result<std::fs::File> {
    use std::hash::Hasher;

    let dir = dir.to_path_buf();
    move |candidate: &Path| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write(candidate.as_os_str().as_encoded_bytes());
        SaveFile::open_lock_file(&dir.join(format!("{:016x}", hasher.finish())))
    }
}

/// Two saves whose basenames are distinct non-UTF-8 byte strings are two
/// distinct saves, so each must lock its own sidecar.
#[cfg(unix)]
#[test]
fn distinct_non_utf8_save_basenames_do_not_share_one_lock() {
    use std::os::unix::ffi::OsStrExt;

    let dir = TempDir::new("non-utf8-lock");
    let first = SaveFile::at(dir.path.join(std::ffi::OsStr::from_bytes(b"\x80")));
    let second = SaveFile::at(dir.path.join(std::ffi::OsStr::from_bytes(b"\x81")));
    assert_ne!(first.path(), second.path(), "the two saves must differ");

    let held = first
        .lock_with_open(|_| {}, non_utf8_safe_backing(&dir.path))
        .expect("the first save must be lockable");
    let (locked, waited) = std::sync::mpsc::channel();
    let other = second.clone();
    let backing_dir = dir.path.clone();
    std::thread::spawn(move || {
        let guard = other
            .lock_with_open(|_| {}, non_utf8_safe_backing(&backing_dir))
            .expect("the second save must be lockable");
        let _sent = locked.send(());
        drop(guard);
    });
    let unrelated_save_locks_independently = waited
        .recv_timeout(std::time::Duration::from_secs(5))
        .is_ok();
    drop(held);

    assert!(
        unrelated_save_locks_independently,
        "{:?} waited on the lock held for the unrelated save {:?}: both saves resolve to \
         one sidecar",
        second.path(),
        first.path()
    );
}

/// The entry a case-folding host -- macOS and Windows by default -- resolves
/// `candidate` to: one directory entry serves every ASCII case variant.
fn case_folded_entry(candidate: &Path) -> PathBuf {
    let name = candidate.file_name().unwrap_or_default().to_string_lossy();
    candidate.with_file_name(name.to_lowercase())
}

/// The sidecar `file` ends up locking on a host that both refuses an
/// over-limit component and folds the case of its directory entries.
fn sidecar_locked_on_a_case_folding_host(file: &SaveFile) -> PathBuf {
    const HOST_NAME_MAX: usize = 143;

    let locked = std::cell::RefCell::new(PathBuf::new());
    let guard = file
        .lock_with_open(
            |_| {},
            |candidate: &Path| {
                let component_len = candidate
                    .file_name()
                    .map_or(0, |name| name.as_encoded_bytes().len());
                if component_len > HOST_NAME_MAX {
                    return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
                }
                let entry = case_folded_entry(candidate);
                let opened = SaveFile::open_lock_file(&entry)?;
                *locked.borrow_mut() = entry;
                Ok(opened)
            },
        )
        .expect("an over-limit save must be lockable");
    drop(guard);
    locked.into_inner()
}

/// Two case variants of one over-limit save name one file on a case-folding
/// host, so their locks must land on one sidecar and exclude one another.
#[test]
fn case_variants_of_one_over_limit_save_lock_one_sidecar_on_a_case_folding_host() {
    let dir = TempDir::new("case-folding-lock");
    let upper = SaveFile::at(dir.join(&"S".repeat(140)));
    let lower = SaveFile::at(dir.join(&"s".repeat(140)));

    assert_eq!(
        sidecar_locked_on_a_case_folding_host(&upper),
        sidecar_locked_on_a_case_folding_host(&lower),
        "a case-folding host resolves {:?} and {:?} to one save, so a second locker \
         must not walk away with a sidecar of its own",
        upper.path().file_name().unwrap_or_default(),
        lower.path().file_name().unwrap_or_default(),
    );
}

/// A case-folding host folds non-ASCII case too, so `Ä` and `ä` name one
/// save there. The fallback's first shape hashed the ASCII-lowercased
/// basename, which leaves those two spellings byte-different: each locked a
/// sidecar of its own and both ran the read-modify-write cycle this lock
/// exists to serialise, losing whichever save wrote second.
#[test]
fn non_ascii_case_variants_of_one_over_limit_save_lock_one_sidecar_on_a_case_folding_host() {
    let dir = TempDir::new("non-ascii-case-folding-lock");
    // Seventy two-byte chars: 140 bytes, over the limit once `.lock` lands.
    let upper = SaveFile::at(dir.join(&"\u{c4}".repeat(70)));
    let lower = SaveFile::at(dir.join(&"\u{e4}".repeat(70)));

    assert_eq!(
        sidecar_locked_on_a_case_folding_host(&upper),
        sidecar_locked_on_a_case_folding_host(&lower),
        "a case-folding host resolves {:?} and {:?} to one save, so a second locker \
         must not walk away with a sidecar of its own",
        upper.path().file_name().unwrap_or_default(),
        lower.path().file_name().unwrap_or_default(),
    );
}

/// The fallback name must not be derived from the basename at all, and must
/// be a fixed literal.
///
/// Not derived: the aliases a volume folds together cannot be enumerated
/// from `std`. ASCII folding misses `Ä`/`ä`; bucketing the non-ASCII names
/// apart misses folds that cross the ASCII boundary, such as `K` (U+212A)
/// against `k`. Any basename-derived name therefore splits some one save
/// across two locks.
///
/// A fixed literal: the name is cross-process, cross-build state. Its first
/// shape hashed with `DefaultHasher`, whose algorithm `std` declines to
/// promise across releases, so two binaries built on different toolchains
/// could derive two sidecars for one save and both enter the guarded cycle.
#[test]
fn the_fallback_lock_name_is_a_fixed_literal_independent_of_the_basename() {
    let dir = TempDir::new("fallback-name-independence");
    let spellings = [
        "S".repeat(140),
        "s".repeat(140),
        "\u{c4}".repeat(70),
        "\u{e4}".repeat(70),
        "\u{212a}".repeat(47),
        "k".repeat(141),
    ];

    for spelling in &spellings {
        assert_eq!(
            SaveFile::at(dir.join(spelling)).shared_lock_path(),
            dir.path.join(".lock.shared"),
            "every over-limit save in one directory must fall back to one fixed \
             sidecar, whatever its basename and whatever toolchain built this"
        );
    }
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
        .open(sibling_path(&path, ".lock"))
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

/// An ordinary lock path is always some basename with the literal `.lock`
/// suffix appended, so it always ends in that suffix; a fallback name that
/// also ended in `.lock` (its first, reported shape was `.<hash>.lock`)
/// could always be some short save's own lock name. The fallback must use
/// a namespace no ordinary lock path can reach.
#[test]
fn an_over_limit_saves_shared_lock_namespace_cannot_be_an_ordinary_locks_name() {
    let dir = TempDir::new("shared-lock-namespace-shape");
    let long = SaveFile::at(dir.join(&"s".repeat(140)));

    let fallback_name = long
        .shared_lock_path()
        .file_name()
        .expect("the hashed lock path names a file")
        .as_encoded_bytes()
        .to_vec();

    assert!(
        !fallback_name.ends_with(b".lock"),
        "a fallback name ending in `.lock` could always be produced by some short save's \
         own basename plus the ordinary `.lock` suffix, got {:?}",
        String::from_utf8_lossy(&fallback_name)
    );
}

/// The over-limit save's fallback is named `.lock.<tag>`; reproduces the
/// exact adversarial short save reported against the fallback's first
/// shape, `.<tag>.lock` (a short save literally named `.<tag>`), keyed to
/// the live tag so the guard tracks whatever it is. Those are two distinct
/// saves, so holding the over-limit save's lock must leave the short save
/// free.
#[test]
fn an_over_limit_saves_shared_lock_does_not_collide_with_a_short_saves_own_lock() {
    const HOST_NAME_MAX: usize = 143;
    let dir = TempDir::new("shared-lock-namespace");
    let long = SaveFile::at(dir.join(&"s".repeat(140)));

    let fallback = long.shared_lock_path();
    let tag = fallback
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(".lock."))
        .expect("the fallback name carries its tag after the `.lock.` namespace");

    // The short save whose ordinary `<save>.lock` would have been the long
    // save's fallback, had the fallback kept its first reported shape.
    let short = SaveFile::at(dir.join(&format!(".{tag}")));
    assert_ne!(
        short.lock_path(),
        fallback,
        "the fallback namespace must never equal an ordinary lock name"
    );
    assert!(
        short
            .lock_path()
            .file_name()
            .is_some_and(|name| name.as_encoded_bytes().len() <= HOST_NAME_MAX),
        "the colliding short save must be one this host accepts outright"
    );

    let refuse_long_components = |candidate: &Path| -> std::io::Result<std::fs::File> {
        let component_len = candidate
            .file_name()
            .map_or(0, |name| name.as_encoded_bytes().len());
        if component_len > HOST_NAME_MAX {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
        }
        SaveFile::open_lock_file(candidate)
    };

    let guard = long
        .lock_with_open(|_| {}, refuse_long_components)
        .expect("the over-limit save must be lockable");

    let probe = SaveFile::open_lock_file(&short.lock_path())
        .expect("the short save's own lock file must be openable");
    match probe.try_lock() {
        Ok(()) => {}
        other => panic!(
            "locking {} must not block the unrelated save {}, got {other:?}",
            long.path().display(),
            short.path().display()
        ),
    }
    drop(probe);
    drop(guard);
}
