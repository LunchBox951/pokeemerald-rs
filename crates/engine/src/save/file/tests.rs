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
    let unstageable = expected_staging_path(&path);
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
            || unstageable.clone(),
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
fn a_staging_name_collision_is_retried_onto_a_fresh_name() {
    let dir = TempDir::new("staging-retry");
    let path = dir.join(SAVE_FILE_NAME);
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    // The first attempt lands on an occupied name and the second on a free
    // one, so only a retry that carries on past the collision can write.
    let occupied = expected_sibling_path(&path, ".tmp.occupied");
    std::fs::write(&occupied, b"someone else's file").unwrap();
    let fresh = expected_sibling_path(&path, ".tmp.fresh");
    let mut attempts = 0;
    file.write_with(
        &store,
        SaveFile::sync_directory_best_effort,
        || {
            attempts += 1;
            if attempts == 1 {
                occupied.clone()
            } else {
                fresh.clone()
            }
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

#[test]
fn a_staged_write_that_cannot_be_renamed_removes_its_staged_file() {
    let dir = TempDir::new("staging-rename-failure");
    let path = dir.join(SAVE_FILE_NAME);
    // A directory at the save path stages fine but can never be renamed onto,
    // so the failure lands after the staged file exists.
    std::fs::create_dir_all(&path).unwrap();
    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();

    let staged = expected_sibling_path(&path, ".tmp.staged");
    let err = file
        .write_with(
            &store,
            SaveFile::sync_directory_best_effort,
            || staged.clone(),
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
        let err = SaveFile::write_and_sync(&staged, &vec![0u8; FLASH_IMAGE_LEN])
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
        .arg("save::file::tests::a_staged_write_that_fails_after_creating_its_file_removes_it")
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
fn a_symlink_planted_at_the_old_deterministic_staging_name_is_not_followed() {
    let dir = TempDir::new("staging-symlink");
    let path = dir.join(SAVE_FILE_NAME);
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    // A symlink here targets the one deterministic name an attacker without
    // this process's clock or entropy could still guess; the real staging
    // name never matches it, and `create_new` refuses a symlink regardless.
    std::os::unix::fs::symlink(&bystander, expected_staging_path(&path)).unwrap();

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
    let staging = expected_sibling_path(&path, ".tmp.planted");
    std::os::unix::fs::symlink(&bystander, &staging).unwrap();

    let file = SaveFile::at(&path);
    let (store, _, _) = saved_store();
    let err = file
        .write_with(
            &store,
            SaveFile::sync_directory_best_effort,
            || staging.clone(),
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
    let staging = expected_sibling_path(&path, ".tmp.replaced");
    let err = file
        .write_with(
            &second,
            SaveFile::sync_directory_best_effort,
            || staging.clone(),
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
        std::fs::symlink_metadata(&staging)
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
    let staging = expected_sibling_path(&path, ".tmp.swapped");
    let err = file
        .write_with(
            &second,
            SaveFile::sync_directory_best_effort,
            || staging.clone(),
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
        std::fs::read(&staging).unwrap(),
        b"someone else's file",
        "a refused promotion must not delete the entry that replaced ours"
    );
}

#[cfg(unix)]
#[test]
fn cleaning_up_a_failed_staged_write_leaves_the_entry_that_replaced_it_alone() {
    let dir = TempDir::new("staging-cleanup-ownership");
    let bystander = dir.join("bystander");
    std::fs::write(&bystander, b"not a save file").unwrap();

    let staging = dir.join("staged.tmp");
    let staged = SaveFile::stage(|| staging.clone(), &vec![0u8; FLASH_IMAGE_LEN])
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

#[test]
fn two_save_files_on_one_path_never_share_a_staging_name() {
    let dir = TempDir::new("same-path-staging-names");
    let path = dir.join(SAVE_FILE_NAME);

    // Two independent `SaveFile` values on the identical path, not clones of
    // one -- each must derive its own staging name. Observed directly: a
    // race between two writers proves nothing when either can finish staging
    // before the other one even starts.
    let first = SaveFile::at(&path);
    let second = SaveFile::at(&path);
    let first_staging = first.staging_path();
    let second_staging = second.staging_path();
    let old_deterministic_name = expected_staging_path(&path);

    assert_ne!(
        first_staging, second_staging,
        "two SaveFile values on the same path must never derive the same staging name"
    );
    assert_ne!(
        first_staging, old_deterministic_name,
        "the staging name must not be the old, symlink-plantable <save>.tmp.<pid> form"
    );
    assert_ne!(
        second_staging, old_deterministic_name,
        "the staging name must not be the old, symlink-plantable <save>.tmp.<pid> form"
    );

    // Distinct staging names also mean two writers on one path can be
    // sequenced without either clobbering the other's in-flight staging file.
    let (first_store, _, block2) = saved_store();
    first
        .write(&first_store)
        .expect("the first writer must succeed");

    let mut second_store = first_store.clone();
    second_store.save(
        &SaveBlock1 {
            money: 222_222,
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
        || file.staging_path(),
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
