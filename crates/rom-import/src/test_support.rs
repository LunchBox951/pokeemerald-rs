//! Scratch directories for the crate's unit tests.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

/// How many names a claim tries before it gives up.
const CLAIM_ATTEMPTS: u32 = 1_000;

/// A fresh directory this process created, cleaned up on drop.
///
/// A predictable name is only safe to use once `create_dir` has made it:
/// `create_dir` fails on any existing entry, including a populated
/// directory or a symlink, so the name is retried rather than reused, and
/// no candidate is ever deleted or written through before it is claimed.
///
/// The claim keeps a handle on the directory it created, so cleanup can
/// tell the claimed object from whatever occupies the name afterwards.
/// Every path a test derives through [`TempDir::join`] is recorded; drop
/// removes only those entries and then calls the non-recursive
/// `remove_dir`, which refuses a non-empty directory. A stranger that
/// swaps itself in at the claimed name therefore survives, and so does
/// anything the fixture left unrecorded (the directory is then left behind
/// rather than guessed at).
///
/// How tightly removal is bound to the claimed object depends on the
/// target:
///
/// * Linux: the recorded entries are removed through
///   `/proc/self/fd/<n>/`, the retained handle's own entry, which names
///   the claimed inode however its pathname is swapped. The directory
///   itself can only be removed by name (`rmdir` has no handle form
///   `std` reaches), so the name is re-checked against the handle right
///   before it.
/// * Other Unix: the name is re-checked against the handle (device and
///   inode) right before each removal, and removal is by name.
/// * Windows: the name is re-checked against the handle's
///   `win_fs::WindowsFileIdentity` right before the entries are removed,
///   and the directory is deleted through the retained handle itself.
/// * Anywhere else nothing is removed.
///
/// The residual, by-name steps are the final `remove_dir` on Unix and the
/// entry removals off Linux: a same-user process able to swap the name in
/// the instants between the re-check and the removal can still have an
/// empty directory (Linux) or a directory holding the exact recorded names
/// (elsewhere) removed. Closing that needs `unlinkat`/`openat`, which is a
/// `libc` or `rustix` dependency this std-only crate does not take. The
/// claim's own `create_dir` and the open of its handle are not atomic
/// either: a same-user swap in that gap becomes the claim, because no
/// handle exists yet to compare it with (only a non-directory or a link
/// is refused). Likewise a recorded name is a pathname, not proof of
/// ownership: an entry another same-user process creates at a recorded
/// name before drop is removed with everything below it. The directory
/// is under the per-user (or sticky) temporary directory, so only the
/// same user can make any of these swaps, and nothing outside it is
/// reachable by name.
pub(crate) struct TempDir {
    path: PathBuf,
    /// The claimed directory, held open from just after `create_dir`.
    #[cfg(any(unix, windows))]
    handle: std::fs::File,
    /// First path component of everything handed out by `join`.
    entries: Mutex<Vec<OsString>>,
}

impl TempDir {
    /// Claim a fresh directory under the OS temporary directory.
    pub(crate) fn new(label: &str) -> Self {
        Self::new_in(&std::env::temp_dir(), label)
    }

    /// Claim a fresh directory under `root`.
    ///
    /// Panics rather than reusing a name, so a hostile or crowded `root`
    /// fails the test instead of the fixture.
    pub(crate) fn new_in(root: &Path, label: &str) -> Self {
        let pid = std::process::id();
        for attempt in 0..CLAIM_ATTEMPTS {
            let path = root.join(format!("rom-import-{label}-{pid}-{attempt}"));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self::owning(path),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(err) => panic!("a writable temp dir: {err}"),
            }
        }
        panic!("no unused temp directory name for {label} under pid {pid}");
    }

    fn owning(path: PathBuf) -> Self {
        Self {
            #[cfg(any(unix, windows))]
            handle: open_claim(&path).expect("the directory just created"),
            path,
            entries: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// A path inside the directory. Its first component is recorded so drop
    /// can remove it.
    pub(crate) fn join(&self, name: &str) -> PathBuf {
        if let Some(Component::Normal(first)) = Path::new(name).components().next() {
            let mut entries = self.entries.lock().expect("the entry list");
            if !entries.iter().any(|known| known == first) {
                entries.push(first.to_os_string());
            }
        }
        self.path.join(name)
    }

    /// Whether the claimed name still reaches the directory the handle
    /// holds, rather than a link, another directory, or nothing.
    #[cfg(unix)]
    fn name_is_claim(&self) -> bool {
        use std::os::unix::fs::MetadataExt as _;
        match (
            self.handle.metadata(),
            std::fs::symlink_metadata(&self.path),
        ) {
            (Ok(held), Ok(found)) => {
                found.is_dir() && (held.dev(), held.ino()) == (found.dev(), found.ino())
            }
            _ => false,
        }
    }

    #[cfg(windows)]
    fn name_is_claim(&self) -> bool {
        use win_fs::WindowsFileIdentity;
        win_fs::open_for_verified_delete(&self.path).is_ok_and(|fresh| {
            WindowsFileIdentity::of(&fresh)
                .and_then(|found| WindowsFileIdentity::of(&self.handle).map(|held| found == held))
                .unwrap_or(false)
        })
    }

    /// A spelling of the claimed directory that reaches its inode whatever
    /// the pathname now names, or `None` where no such spelling exists or
    /// `/proc` does not vouch for it.
    #[cfg(target_os = "linux")]
    fn pinned_dir(&self) -> Option<PathBuf> {
        use std::os::fd::AsRawFd as _;
        use std::os::unix::fs::MetadataExt as _;
        let through = PathBuf::from(format!("/proc/self/fd/{}", self.handle.as_raw_fd()));
        let (reached, held) = (
            std::fs::metadata(&through).ok()?,
            self.handle.metadata().ok()?,
        );
        ((reached.dev(), reached.ino()) == (held.dev(), held.ino())).then_some(through)
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    fn pinned_dir(&self) -> Option<PathBuf> {
        None
    }

    /// Removes the recorded entries and the directory, if the name still
    /// reaches the claim. `between_check_and_removal` runs after the first
    /// check and before any removal, so a test can land a swap in exactly
    /// that window; drop passes a no-op.
    #[cfg(unix)]
    fn remove_claimed(&self, between_check_and_removal: impl FnOnce()) {
        if !self.name_is_claim() {
            return;
        }
        between_check_and_removal();
        let entries = std::mem::take(&mut *self.entries.lock().expect("the entry list"));
        let pinned = self.pinned_dir();
        for name in entries {
            let entry = match &pinned {
                Some(dir) => dir.join(name),
                // By name: only as sound as the check made just before it.
                None if self.name_is_claim() => self.path.join(name),
                None => return,
            };
            // `symlink_metadata` so a link is unlinked, never followed.
            let _ = match std::fs::symlink_metadata(&entry) {
                Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&entry),
                Ok(_) => std::fs::remove_file(&entry),
                Err(err) => Err(err),
            };
        }
        // Non-recursive: a non-empty directory is left alone.
        if self.name_is_claim() {
            let _ = std::fs::remove_dir(&self.path);
        }
    }

    #[cfg(windows)]
    fn remove_claimed(&self, between_check_and_removal: impl FnOnce()) {
        if !self.name_is_claim() {
            return;
        }
        between_check_and_removal();
        let entries = std::mem::take(&mut *self.entries.lock().expect("the entry list"));
        for name in entries {
            if !self.name_is_claim() {
                return;
            }
            let entry = self.path.join(name);
            let _ = match std::fs::symlink_metadata(&entry) {
                Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&entry),
                Ok(_) => std::fs::remove_file(&entry),
                Err(err) => Err(err),
            };
        }
        // Through the handle that is the claim, not through the name. A
        // non-empty directory refuses, which leaves it alone.
        let _ = win_fs::delete_through_handle(&self.handle);
    }
}

/// Opens the directory `create_dir` just made and refuses to hold anything
/// else: a name swapped to a link or another directory in the gap between
/// the two calls fails the identity comparison instead of becoming the
/// claim.
#[cfg(unix)]
fn open_claim(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::MetadataExt as _;
    let handle = std::fs::File::open(path)?;
    let (held, found) = (handle.metadata()?, std::fs::symlink_metadata(path)?);
    if !found.is_dir() || (held.dev(), held.ino()) != (found.dev(), found.ino()) {
        return Err(std::io::Error::other("the new directory was replaced"));
    }
    Ok(handle)
}

#[cfg(windows)]
fn open_claim(path: &Path) -> std::io::Result<std::fs::File> {
    let handle = win_fs::open_for_verified_delete(path)?;
    if !handle.metadata()?.is_dir() {
        return Err(std::io::Error::other("the new directory was replaced"));
    }
    Ok(handle)
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Targets with no identity to compare remove nothing.
        #[cfg(any(unix, windows))]
        self.remove_claimed(|| {});
    }
}

#[cfg(test)]
mod tests {
    use super::TempDir;

    /// The name the first claim under `root` would take for `label`.
    /// Recorded in `root`, so the test's own cleanup removes whatever it
    /// plants there.
    fn first_candidate(root: &TempDir, label: &str) -> std::path::PathBuf {
        root.join(&format!("rom-import-{label}-{}-0", std::process::id()))
    }

    #[test]
    fn a_claim_leaves_an_occupied_candidate_and_its_contents_alone() {
        let root = TempDir::new("occupied-root");
        let occupied = first_candidate(&root, "probe");
        std::fs::create_dir(&occupied).expect("the stranger's directory");
        std::fs::write(occupied.join("sentinel"), b"not yours").expect("the sentinel");

        let claimed = TempDir::new_in(root.path(), "probe");
        assert_ne!(claimed.path(), occupied);
        assert!(std::fs::read_dir(claimed.path()).unwrap().next().is_none());
        assert_eq!(
            std::fs::read(occupied.join("sentinel")).unwrap(),
            b"not yours"
        );

        drop(claimed);
        assert_eq!(
            std::fs::read(occupied.join("sentinel")).unwrap(),
            b"not yours",
            "dropping the claim must not reach the stranger's directory"
        );
    }

    #[test]
    fn dropping_a_claim_spares_a_stranger_that_took_its_name() {
        let root = TempDir::new("replaced-root");
        let claimed = TempDir::new_in(root.path(), "probe");
        let path = root.join(
            claimed
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a utf-8 claim name"),
        );
        // Moved, not removed: a freed inode number can be handed straight
        // to the stranger, which would make the two indistinguishable.
        let moved = root.join("moved-claim");
        std::fs::rename(&path, &moved).expect("the claim is moved behind its back");
        std::fs::create_dir(&path).expect("a stranger takes the name");
        std::fs::write(path.join("sentinel"), b"not yours").expect("the sentinel");

        drop(claimed);
        assert_eq!(std::fs::read(path.join("sentinel")).unwrap(), b"not yours");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn drop_removes_what_the_fixture_recorded_and_the_directory() {
        let root = TempDir::new("cleanup-root");
        let claimed = TempDir::new_in(root.path(), "probe");
        let path = claimed.path().to_path_buf();
        std::fs::write(claimed.join("emerald.gba"), b"rom").unwrap();
        std::fs::create_dir(claimed.join("nested")).unwrap();
        std::fs::write(claimed.join("nested").join("inner"), b"x").unwrap();
        drop(claimed);
        assert!(!path.exists());
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn drop_leaves_an_unrecorded_entry_and_its_directory_in_place() {
        let root = TempDir::new("unrecorded-root");
        let claimed = TempDir::new_in(root.path(), "probe");
        // Recorded in `root`, whose own drop sweeps up the kept directory.
        let path = root.join(
            claimed
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .expect("a utf-8 claim name"),
        );
        std::fs::write(path.join("not-via-join"), b"kept").unwrap();
        drop(claimed);
        assert_eq!(std::fs::read(path.join("not-via-join")).unwrap(), b"kept");
    }

    /// The swap lands after cleanup's first identity check and before any
    /// removal, through the seam, so the window is hit on every run.
    #[cfg(any(unix, windows))]
    #[test]
    fn a_swap_between_the_identity_check_and_the_removal_spares_the_stranger() {
        let root = TempDir::new("swap-root");
        let claimed = TempDir::new_in(root.path(), "probe");
        std::fs::write(claimed.join("emerald.gba"), b"mine").unwrap();
        let name = claimed.path().file_name().and_then(|n| n.to_str()).unwrap();
        // Recorded in `root`, so its own drop sweeps up both directories.
        let path = root.join(name);
        let moved = root.join("moved-claim");

        claimed.remove_claimed(|| {
            std::fs::rename(&path, &moved).expect("the claim is moved behind its back");
            std::fs::create_dir(&path).expect("a stranger takes the name");
            std::fs::write(path.join("emerald.gba"), b"not yours").expect("the stranger's file");
        });

        assert_eq!(
            std::fs::read(path.join("emerald.gba")).unwrap(),
            b"not yours",
            "cleanup reached the stranger that took the name"
        );
        // Linux removes through the handle, so the claimed directory's own
        // entry goes even though its name was lost. Elsewhere the re-check
        // fails first and nothing is removed.
        assert_eq!(
            moved.join("emerald.gba").exists(),
            !cfg!(target_os = "linux"),
            "removal must follow the claimed inode on Linux and stop elsewhere"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_claim_never_writes_through_a_planted_symlink() {
        let root = TempDir::new("symlink-root");
        let victim = TempDir::new("symlink-victim");
        std::fs::write(victim.join("sentinel"), b"not yours").expect("the sentinel");
        let link = first_candidate(&root, "probe");
        std::os::unix::fs::symlink(victim.path(), &link).expect("the planted link");

        let claimed = TempDir::new_in(root.path(), "probe");
        std::fs::write(claimed.join("emerald.gba"), b"mine").expect("a fixture write");
        drop(claimed);

        assert_eq!(
            std::fs::read(victim.join("sentinel")).unwrap(),
            b"not yours"
        );
        assert!(!victim.join("emerald.gba").exists());
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the planted link is the stranger's to keep"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_at_a_candidate_is_not_claimed_either() {
        let root = TempDir::new("dangling-root");
        let link = first_candidate(&root, "probe");
        let target = root.join("not-yet-created");
        std::os::unix::fs::symlink(&target, &link).expect("the planted link");

        let claimed = TempDir::new_in(root.path(), "probe");
        assert_ne!(claimed.path(), link);
        assert!(!target.exists(), "create_dir must not follow the link");
    }

    #[test]
    #[should_panic(expected = "no unused temp directory name")]
    fn a_root_with_every_name_taken_fails_closed() {
        let root = TempDir::new("crowded-root");
        for attempt in 0..super::CLAIM_ATTEMPTS {
            let taken = root.join(&format!(
                "rom-import-probe-{}-{attempt}",
                std::process::id()
            ));
            std::fs::create_dir(taken).expect("a taken name");
        }
        let _ = TempDir::new_in(root.path(), "probe");
    }
}
