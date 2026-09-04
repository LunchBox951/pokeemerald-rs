use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{
    data_dir_for, default_save_path_from, HostFamily, SaveFile, SaveFileError, SAVE_DIR_NAME,
    SAVE_FILE_NAME, SAVE_PATH_ENV,
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

fn expected_sibling_path(save_path: &Path, suffix: impl AsRef<std::ffi::OsStr>) -> PathBuf {
    let mut path = save_path.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

fn expected_staging_path(save_path: &Path) -> PathBuf {
    expected_sibling_path(save_path, format!(".tmp.{}", std::process::id()))
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-save-file-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&path));
        std::fs::create_dir_all(&path).expect("scratch directory must be creatable");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
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

fn saved_store() -> (SaveStore, SaveBlock1, SaveBlock2) {
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

    assert!(
        !expected_staging_path(&path).exists(),
        "the staged temporary must be renamed away, not left on disk"
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

    std::fs::create_dir_all(expected_staging_path(&path)).unwrap();

    let mut second = first.clone();
    second.save(
        &SaveBlock1 {
            money: 777_777,
            ..SaveBlock1::default()
        },
        &block2,
    );
    let err = file
        .write(&second)
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
            assert_eq!(got, FLASH_IMAGE_LEN - 1);
        }
        other => panic!("expected a length rejection, got {other:?}"),
    }
}

#[test]
fn an_oversized_file_is_rejected_after_a_bounded_read() {
    let dir = TempDir::new("oversized");
    let path = dir.join(SAVE_FILE_NAME);
    std::fs::write(&path, vec![0u8; FLASH_IMAGE_LEN + 4096]).unwrap();

    let err = SaveFile::at(&path).read().unwrap_err();
    match err {
        SaveFileError::BadLength { expected, got, .. } => {
            assert_eq!(expected, FLASH_IMAGE_LEN);
            assert_eq!(got, FLASH_IMAGE_LEN + 1);
        }
        other => panic!("expected a length rejection, got {other:?}"),
    }
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
                .open(expected_sibling_path(&path, ".lock"))
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
    assert!(expected_sibling_path(&path, ".lock").exists());

    let (store, _, _) = saved_store();
    file.write(&store).unwrap();
    drop(guard);

    let reloaded = file.read().unwrap().expect("the file was just written");
    assert_eq!(reloaded.flash_image(), store.flash_image());
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
        .open(expected_sibling_path(&path, ".lock"))
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
