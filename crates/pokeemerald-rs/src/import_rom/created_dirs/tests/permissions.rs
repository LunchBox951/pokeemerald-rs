//! Taking back a created level whose mode lacks the owner's write bit, which
//! a rename into a quarantine cannot move (it rewrites the level's `..`).
//!
//! `create_directories` restores owner access right after each pin, so rollback
//! meets such a level only when that restore failed (a umask that took the
//! owner's write and search with `/proc` unmounted). Creation then returns the
//! levels made so far, the failing one last and recorded with both pins, and
//! its ancestors untouched. [`failed_restore`] makes that restore fail for
//! real on Linux, so rollback gets exactly what creation returned; the
//! tests here build the same rollback input by taking the owner's write
//! from the last level after it is created.
//!
//! The route is chosen by the held level's mode, not by a failed rename, so
//! it is taken under root too, where mode bits do not bind and a rename
//! would succeed: the tests check that no capture was made rather than that
//! one failed.

use std::cell::Cell;
use std::os::unix::fs::PermissionsExt as _;

use super::*;

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod failed_restore;

/// `a/b`, created, with `b` (the last level, as when its owner-access
/// restore failed) left at `mode`.
fn chain_with_last_level_at(label: &str, mode: u32) -> (TempDir, Vec<CreatedDirectory>, PathBuf) {
    let dir = TempDir::new(label);
    let level = dir.join("a").join("b");
    let created = super::create_directories(&level).unwrap();
    assert_eq!(created.len(), 2);
    fs::set_permissions(&level, fs::Permissions::from_mode(mode)).unwrap();
    (dir, created, level)
}

/// Rolls `created` back, answering the stages the capture hook saw, each
/// with how many levels had reached the take-back by then.
fn rollback_stages(created: &[CreatedDirectory]) -> Vec<(u32, take_back::CaptureStage)> {
    let taken = Cell::new(0);
    let mut stages = Vec::new();
    super::take_back::undo_created_directories_with_capture_hooks(
        created,
        &mut || taken.set(taken.get() + 1),
        &mut |stage, _| stages.push((taken.get(), stage)),
    );
    stages
}

/// Rolls back `a/b` with `b` at `mode`: both are removed, `b` without any
/// capture, with nothing left beside them.
fn assert_unwritable_last_level_is_removed(label: &str, mode: u32) {
    let (dir, created, level) = chain_with_last_level_at(label, mode);
    let stages = rollback_stages(&created);

    assert!(!level.exists(), "the level without owner write was left");
    assert_eq!(entry_names(&dir.path), Vec::<String>::new());
    // `b` was the first level to reach the take-back, and was never
    // captured; `a` was second, through the quarantine.
    assert!(
        stages.iter().all(|&(taken, _)| taken == 2),
        "the level without owner write was moved into a quarantine: {stages:?}"
    );
}

/// Runs `body` in a child that rolls back and prints nothing, and fails
/// unless the parent saw no warning at all.
fn assert_silent(test_path: &str) -> bool {
    if in_rollback_stderr_child() {
        return false;
    }
    let stderr = rerun_alone_for_stderr(test_path);
    assert!(
        stderr.trim().is_empty(),
        "rollback warned about a level it removed:\n{stderr}"
    );
    true
}

#[test]
fn a_last_level_with_no_mode_bits_is_removed_with_its_ancestors_silently() {
    if assert_silent(
        "import_rom::created_dirs::tests::permissions::a_last_level_with_no_mode_bits_is_removed_with_its_ancestors_silently",
    ) {
        return;
    }
    assert_unwritable_last_level_is_removed("unwritable-000", 0o000);
}

#[test]
fn a_last_level_without_owner_write_is_removed_with_its_ancestors_silently() {
    if assert_silent(
        "import_rom::created_dirs::tests::permissions::a_last_level_without_owner_write_is_removed_with_its_ancestors_silently",
    ) {
        return;
    }
    assert_unwritable_last_level_is_removed("unwritable-0500", 0o500);
}

#[test]
fn a_writable_level_still_goes_through_the_quarantine() {
    // Only a missing owner write bit sends a level around the quarantine;
    // a level that keeps it, whatever else it lacks, is still captured.
    let (dir, created, level) = chain_with_last_level_at("writable-0300", 0o300);
    let stages = rollback_stages(&created);

    assert!(!level.exists());
    assert_eq!(entry_names(&dir.path), Vec::<String>::new());
    assert!(
        stages.iter().any(|&(taken, _)| taken == 1),
        "a level with owner write was not captured: {stages:?}"
    );
}

#[test]
fn the_premise_a_directory_without_write_cannot_move_to_another_parent() {
    // Mode bits do not bind root, so the rename would succeed there.
    if rustix::process::geteuid().is_root() {
        eprintln!("skipped: running as root, whose renames ignore the missing write bit");
        return;
    }
    let dir = TempDir::new("unwritable-premise");
    fs::create_dir_all(dir.join("from").join("level")).unwrap();
    fs::create_dir(dir.join("to")).unwrap();
    fs::set_permissions(
        dir.join("from").join("level"),
        fs::Permissions::from_mode(0o500),
    )
    .unwrap();
    let err = fs::rename(dir.join("from").join("level"), dir.join("to").join("level"))
        .expect_err("a directory without write moved to another parent");
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied, "{err}");
}

#[test]
fn a_level_without_owner_write_is_removed_with_no_descriptor_free() {
    // No quarantine is made for it, so rollback needs no descriptor and no
    // entropy; a quarantine would not open here.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::permissions::a_level_without_owner_write_is_removed_with_no_descriptor_free",
        );
        return;
    }
    let dir = TempDir::new("unwritable-pressure");
    let level = dir.join("only");
    let created = super::create_directories(&level).unwrap();
    fs::set_permissions(&level, fs::Permissions::from_mode(0o500)).unwrap();

    let mut fillers = None;
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        if fillers.is_none() {
            fillers = Some(fill_descriptor_table_leaving(0));
        }
    });
    drop(fillers);

    assert!(!level.exists(), "the level was left standing");
    assert_eq!(entry_names(&dir.path), Vec::<String>::new());
}

#[test]
fn a_level_without_owner_write_that_holds_something_is_left_and_stops_the_walk() {
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::permissions::a_level_without_owner_write_that_holds_something_is_left_and_stops_the_walk",
        );
        let warned = warned_path(
            &stderr,
            "warning: ",
            " is left in place: it could not be removed: ",
        );
        assert_names_marked_path(&stderr, warned, "level at: ");
        return;
    }
    let dir = TempDir::new("unwritable-nonempty");
    let level = dir.join("a").join("b");
    let created = super::create_directories(&level).unwrap();
    fs::write(level.join("keep"), b"x").unwrap();
    fs::set_permissions(&level, fs::Permissions::from_mode(0o500)).unwrap();

    let stages = rollback_stages(&created);

    fs::set_permissions(&level, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        level.join("keep").is_file(),
        "the level's contents were lost"
    );
    assert!(
        dir.join("a").is_dir(),
        "the ancestor was taken past the level"
    );
    assert_eq!(entry_names(&dir.join("a")), ["b"]);
    assert!(stages.is_empty(), "the level was captured: {stages:?}");
    mark_path("level at: ", &level);
}

#[test]
fn a_level_without_owner_write_swapped_after_validation_is_left_and_stops_the_walk() {
    // `b` is moved aside within its own parent (which needs no write on `b`)
    // and an empty directory takes its name, after the walk vouched for it.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::permissions::a_level_without_owner_write_swapped_after_validation_is_left_and_stops_the_walk",
        );
        assert!(
            stderr.contains(" is left in place: another entry has taken its name"),
            "{stderr}"
        );
        if cfg!(any(target_os = "linux", target_os = "android")) {
            assert!(
                stderr.contains(" is the directory rollback made for "),
                "{stderr}"
            );
        }
        return;
    }
    let dir = TempDir::new("unwritable-swapped");
    let level = dir.join("a").join("b");
    let moved = dir.join("a").join("moved");
    let created = super::create_directories(&level).unwrap();
    fs::set_permissions(&level, fs::Permissions::from_mode(0o500)).unwrap();

    let taken = Cell::new(0);
    let mut stages = Vec::new();
    super::take_back::undo_created_directories_with_capture_hooks(
        &created,
        &mut || {
            taken.set(taken.get() + 1);
            fs::rename(&level, &moved).unwrap();
            fs::create_dir(&level).unwrap();
        },
        &mut |stage, _| stages.push(stage),
    );

    assert!(level.is_dir(), "the replacement was removed");
    assert!(moved.is_dir(), "the original was removed");
    assert_eq!(taken.get(), 1, "the walk went past the swapped level");
    assert!(stages.is_empty(), "the level was captured: {stages:?}");
    fs::set_permissions(&moved, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_level_without_owner_write_removed_after_validation_is_not_reported_moved() {
    if assert_silent(
        "import_rom::created_dirs::tests::permissions::a_level_without_owner_write_removed_after_validation_is_not_reported_moved",
    ) {
        return;
    }
    let (dir, created, level) = chain_with_last_level_at("unwritable-vanished", 0o500);
    let taken = Cell::new(0);
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        taken.set(taken.get() + 1);
        if taken.get() == 1 {
            fs::remove_dir(&level).unwrap();
        }
    });

    assert_eq!(entry_names(&dir.path), Vec::<String>::new());
}

#[cfg(target_os = "linux")]
#[test]
fn a_level_without_owner_write_on_another_filesystem_is_removed() {
    use std::os::unix::fs::MetadataExt as _;
    // As `quarantine::a_level_on_another_filesystem_is_taken_back_through_a_quarantine_beside_it`,
    // with `more` left without owner write: it is unlinked in place, not
    // renamed (which would fail across filesystems anyway), and `new` goes
    // through the shared quarantine.
    let shm = std::path::Path::new("/dev/shm");
    let dir = TempDir::new("unwritable-cross-device");
    let Ok(shm_meta) = fs::metadata(shm) else {
        panic!("this test needs /dev/shm");
    };
    assert_ne!(
        shm_meta.dev(),
        fs::metadata(&dir.path).unwrap().dev(),
        "this test needs /dev/shm on another filesystem than the temp dir"
    );
    let other = shm.join(format!(
        "pokeemerald-rs-unwritable-cross-device-{}",
        std::process::id()
    ));
    fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(&other, dir.join("m")).unwrap();
    let target = dir.join("new").join("..").join("m").join("more");
    let created = super::create_directories(&target).unwrap();
    fs::set_permissions(other.join("more"), fs::Permissions::from_mode(0o500)).unwrap();

    super::undo_created_directories(&created);

    let on_shm = entry_names(&other);
    let in_base = entry_names(&dir.path);
    fs::remove_dir_all(&other).unwrap();
    assert!(on_shm.is_empty(), "left on /dev/shm: {on_shm:?}");
    assert_eq!(in_base, ["m"], "left in the base: {in_base:?}");
}
