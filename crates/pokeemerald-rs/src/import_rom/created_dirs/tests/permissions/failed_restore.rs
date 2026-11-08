//! Rollback of what `create_directories` returns when a level's
//! owner-access restore fails for real. Under a umask that took the owner's
//! search, `fchmodat(level, ".")` is refused and the setter falls back to
//! chmodding the level's `/proc/self/fd` link, which `/proc` resolves in
//! the thread-group leader's descriptor table. A test thread that unshared
//! its own table before reopening the level holds it at a number that table
//! never had, so the fallback finds nothing, as with `/proc` unmounted.
//! Root, which may lack `unshare(2)` in a container, instead hands the
//! level to another user and gives up the capabilities that override
//! ownership and mode bits, so both chmods are refused.

use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use rustix::thread::{CapabilitySet, CapabilitySets, UnshareFlags};

use super::super::{entry_names, fs, TempDir};

/// `nobody`, which root hands the level to.
const NOBODY: u32 = 65_534;

/// Fails the owner-access restore of `level`, about to be reopened on this
/// thread with its owner's search gone, and answers the capabilities to
/// put back; `Err` when this environment refuses the unshare.
///
/// Must run on a thread other than the thread-group leader, which alone
/// keeps the table `/proc/self/fd` reads.
fn fail_restore_of(level: &Path) -> rustix::io::Result<CapabilitySets> {
    let held = rustix::thread::capabilities(None).unwrap();
    let mut bound_by_modes = held;
    bound_by_modes.effective -=
        CapabilitySet::DAC_OVERRIDE | CapabilitySet::DAC_READ_SEARCH | CapabilitySet::FOWNER;
    if rustix::process::geteuid().is_root() {
        std::os::unix::fs::chown(level, Some(NOBODY), None).unwrap();
        rustix::thread::set_capabilities(None, bound_by_modes).unwrap();
    } else {
        rustix::thread::set_capabilities(None, bound_by_modes).unwrap();
        // SAFETY: only this thread's table is unshared, and no descriptor
        // crosses threads after it: the caller runs creation and rollback
        // here and returns nothing that holds one. The leader's copies of
        // what was open before stay open, unowned, until this child
        // process exits.
        unsafe { rustix::thread::unshare_unsafe(UnshareFlags::FILES) }?;
    }
    Ok(held)
}

/// Creates `a/b` under `mask`, a umask taking the owner's write and
/// search, with `b`'s owner-access restore failing for real, and rolls back
/// what creation returned: the error, `b` recorded last with both pins and
/// still at the masked mode, and `a` with its access restored. Both are
/// removed, `b` without any capture, with nothing left beside them.
fn assert_failed_restore_is_rolled_back(label: &'static str, mask: u32) {
    std::thread::spawn(move || {
        let dir = TempDir::new(label);
        let level = dir.join("a").join("b");
        // The umask is process-wide; this runs alone in its child process.
        let previous = rustix::process::umask(rustix::fs::Mode::from_raw_mode(mask));
        let mut reopens = 0;
        let mut induced = None;
        let result = super::super::create_directories_with_hooks(
            &level,
            super::super::MAX_PINNED_LEVELS,
            &mut || {},
            &mut || {
                reopens += 1;
                if reopens == 2 {
                    induced = Some(fail_restore_of(&level));
                }
                None
            },
            &mut || None,
        );
        match induced.unwrap() {
            Ok(held) => rustix::thread::set_capabilities(None, held).unwrap(),
            Err(refused) => {
                // Stdout: the parent fails on any stderr.
                println!("skipped: unshare(CLONE_FILES) refused here ({refused})");
                rustix::process::umask(previous);
                return;
            }
        }
        let Err((created, source)) = result else {
            panic!("creation succeeded with the owner-access restore failing");
        };

        assert_eq!(
            source.kind(),
            std::io::ErrorKind::PermissionDenied,
            "{source}"
        );
        assert_eq!(
            created.len(),
            2,
            "creation did not stop at the failing level"
        );
        let failed = &created[1];
        assert_eq!(failed.path, level);
        assert!(
            failed.parent.is_some() && failed.own.is_some(),
            "the failing level was not recorded with both pins"
        );
        let mode = |path: &std::path::Path| fs::metadata(path).unwrap().mode() & 0o777;
        assert_eq!(
            mode(&level),
            0o777 & !mask,
            "the failing level's owner access was restored"
        );
        assert_eq!(
            mode(&dir.join("a")) & 0o700,
            0o700,
            "the ancestor was not restored"
        );

        // Put back before rollback: what a mask that takes the owner's read
        // does to the quarantine made for `a` is not what this covers.
        rustix::process::umask(previous);
        let stages = super::rollback_stages(&created);

        assert!(!level.exists(), "the level whose restore failed was left");
        assert_eq!(entry_names(&dir.path), Vec::<String>::new());
        // `b` was the first level to reach the take-back, and was never
        // captured; `a` was second, through the quarantine.
        assert!(
            stages.iter().all(|&(taken, _)| taken == 2),
            "the level without owner write was moved into a quarantine: {stages:?}"
        );
    })
    .join()
    .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}

#[test]
fn a_level_left_at_no_mode_bits_by_a_failed_restore_is_removed_with_its_ancestors_silently() {
    if super::assert_silent(
        "import_rom::created_dirs::tests::permissions::failed_restore::a_level_left_at_no_mode_bits_by_a_failed_restore_is_removed_with_its_ancestors_silently",
    ) {
        return;
    }
    assert_failed_restore_is_rolled_back("restore-failed-000", 0o777);
}

#[test]
fn a_level_left_owner_read_only_by_a_failed_restore_is_removed_with_its_ancestors_silently() {
    if super::assert_silent(
        "import_rom::created_dirs::tests::permissions::failed_restore::a_level_left_owner_read_only_by_a_failed_restore_is_removed_with_its_ancestors_silently",
    ) {
        return;
    }
    assert_failed_restore_is_rolled_back("restore-failed-0400", 0o377);
}
