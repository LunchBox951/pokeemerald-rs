//! Rollback of a level its owner could not list as it stood, so the listing
//! granted the owner's read and search bits, and whose mode then could not
//! be put back. The setter is a stand-in that grants for real and then fails
//! the put-back with `EIO`. Root gives up, on the rolling-back thread, the
//! capabilities that override mode bits, so it too is refused the listing.

use std::cell::Cell;
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use rustix::fs::Mode;
use rustix::thread::CapabilitySet;

use super::super::{entry_names, fs, in_rollback_stderr_child, rerun_alone_for_stderr, TempDir};

/// Creates `new` at `0o300`, holding `keep` when `populated`, and rolls it
/// back with the put-back of its mode failing. Answers the directory, the
/// level, and how many times the mode was set.
fn roll_back_with_put_back_failing(
    label: &'static str,
    populated: bool,
) -> (TempDir, PathBuf, u32) {
    std::thread::spawn(move || {
        let dir = TempDir::new(label);
        let level = dir.join("new");
        let created = super::super::create_directories(&level).unwrap();
        if populated {
            fs::write(level.join("keep"), b"x").unwrap();
        }
        fs::set_permissions(&level, fs::Permissions::from_mode(0o300)).unwrap();

        let held = rustix::thread::capabilities(None).unwrap();
        let mut bound_by_modes = held;
        bound_by_modes.effective -= CapabilitySet::DAC_OVERRIDE | CapabilitySet::DAC_READ_SEARCH;
        rustix::thread::set_capabilities(None, bound_by_modes).unwrap();
        let calls = Cell::new(0);
        let set_mode = |own: &OwnedFd, mode: Mode| -> std::io::Result<()> {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                crate::import_rom::dest::set_held_directory_mode(own, mode)
            } else {
                Err(rustix::io::Errno::IO.into())
            }
        };
        super::super::take_back::undo_created_directories_setting_modes_with(&created, &set_mode);
        rustix::thread::set_capabilities(None, held).unwrap();
        (dir, level, calls.get())
    })
    .join()
    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

#[test]
fn a_populated_level_whose_mode_cannot_be_put_back_is_reported_unmoved() {
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::permissions::mode_put_back::a_populated_level_whose_mode_cannot_be_put_back_is_reported_unmoved",
        );
        assert!(
            stderr.contains(
                "/new is left in place: it holds entries, so rollback did not move it into a cleanup directory"
            ),
            "stderr:\n{stderr}"
        );
        assert!(
            stderr.contains(
                "/new keeps the owner read and search bits rollback granted to list it; its mode could not be put back to 0o300: "
            ),
            "the failed put-back was not reported; stderr:\n{stderr}"
        );
        return;
    }
    let (dir, level, calls) = roll_back_with_put_back_failing("mode-left-populated", true);
    let mode = fs::metadata(&level).unwrap().permissions().mode() & 0o7777;
    fs::set_permissions(&level, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(calls, 2, "the bits were not granted and then put back");
    assert_eq!(mode, 0o700, "the level is not at the mode the report says");
    assert!(level.join("keep").is_file());
    assert_eq!(entry_names(&dir.path), ["new"], "the level was moved");
}

#[test]
fn an_empty_level_whose_mode_cannot_be_put_back_is_removed_silently() {
    // The level is gone, so the mode it was left at concerns nothing.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::permissions::mode_put_back::an_empty_level_whose_mode_cannot_be_put_back_is_removed_silently",
        );
        assert!(stderr.trim().is_empty(), "rollback warned:\n{stderr}");
        return;
    }
    let (dir, level, calls) = roll_back_with_put_back_failing("mode-left-empty", false);

    assert_eq!(calls, 2, "the bits were not granted and then put back");
    assert!(!level.exists(), "the empty level was left");
    assert_eq!(entry_names(&dir.path), Vec::<String>::new());
}
