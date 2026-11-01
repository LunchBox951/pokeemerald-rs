//! The cry fixture's temporary root: claimed exclusively, never pre-deleted,
//! and removed only while it is still the directory that was claimed.

use std::path::{Path, PathBuf};

/// The open handle that pins the claimed directory for the fixture's whole
/// life. A directory that is open cannot have its inode reused, so comparing
/// the handle's identity with the one at a path is a lifetime identity, not
/// the (dev, ino) pair of a directory that may since have been removed and
/// recreated. Unix opens the directory itself; Windows opens it through
/// `win-fs` without following a reparse point. Elsewhere no identity is
/// readable, so no handle is kept and cleanup leaks the directory rather than
/// risk deleting a replacement.
type Held = std::fs::File;

/// Opens the directory at `path` as the pin, refusing a symlink or any entry
/// that is not a directory.
fn pin(path: &Path) -> Option<Held> {
    #[cfg(unix)]
    {
        let held = std::fs::File::open(path).ok()?;
        // The open follows a final symlink, so a link swapped in for the
        // claimed directory is caught by comparing against the path's own
        // (non-following) metadata.
        (is_held_at(&held, path) && is_empty(&held, path)).then_some(held)
    }
    #[cfg(windows)]
    {
        let held = win_fs::open_for_verified_delete(path).ok()?;
        (is_held_at(&held, path) && is_empty(&held, path)).then_some(held)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        None
    }
}

/// Whether `path` is, right now, a real directory that is the very object
/// `held` pins.
fn is_held_at(held: &Held, path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        match (held.metadata(), std::fs::symlink_metadata(path)) {
            (Ok(pinned), Ok(found)) => {
                found.is_dir() && (pinned.dev(), pinned.ino()) == (found.dev(), found.ino())
            }
            _ => false,
        }
    }
    #[cfg(windows)]
    {
        use win_fs::WindowsFileIdentity;
        win_fs::open_for_verified_delete(path).is_ok_and(|fresh| {
            WindowsFileIdentity::of(&fresh)
                .and_then(|found| WindowsFileIdentity::of(held).map(|pinned| found == pinned))
                .unwrap_or(false)
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (held, path);
        false
    }
}

/// The `/proc/self/fd` entry of the directory `held` pins, with a trailing
/// slash so the kernel follows the descriptor link to the inode itself.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn through_proc(held: &Held) -> PathBuf {
    use std::os::fd::AsRawFd as _;
    PathBuf::from(format!("/proc/self/fd/{}/", held.as_raw_fd()))
}

/// Whether the pinned directory holds nothing. Linux reads it through the
/// handle; elsewhere it reads `path` and requires `path` to still be the
/// pinned directory afterwards.
fn is_empty(held: &Held, path: &Path) -> bool {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    if let Ok(mut entries) = std::fs::read_dir(through_proc(held)) {
        return entries.next().is_none();
    }
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
        && is_held_at(held, path)
}

/// Deletes the contents of the pinned directory through its `/proc/self/fd`
/// entry, so every deletion reaches that inode and none re-resolves the
/// fixture's pathname. `None` means `/proc` could not be opened and nothing
/// was touched; `Some` carries the outcome of the deletions.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn clear_through_handle(held: &Held) -> Option<std::io::Result<()>> {
    let through_proc = through_proc(held);
    let entries = std::fs::read_dir(&through_proc).ok()?;
    Some(entries.into_iter().try_for_each(|entry| {
        let entry = entry?;
        let child = through_proc.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(&child)
        } else {
            std::fs::remove_file(&child)
        }
    }))
}

/// Eight bytes a neighbour cannot predict: a `SipHash` with keys std seeds from
/// the operating system's random source.
fn unpredictable() -> u64 {
    use std::hash::{BuildHasher as _, Hasher as _};
    std::hash::RandomState::new().build_hasher().finish()
}

/// A fixture tree this process created itself. The root is claimed with an
/// exclusive `create_dir` under an unpredictable name and is never
/// pre-deleted; cleanup deletes it only while it is still the directory that
/// was claimed.
pub(super) struct Fixture {
    pub(super) root: PathBuf,
    /// The pin on the claimed directory; `None` when no identity is readable
    /// (cleanup then removes nothing).
    held: Option<Held>,
}

impl Fixture {
    /// Claims a fresh private directory under the system temp directory.
    pub(super) fn claim(name: &str) -> Self {
        Self::claim_in(&std::env::temp_dir(), || {
            format!(
                "cry-fixture-{name}-{}-{:016x}",
                std::process::id(),
                unpredictable()
            )
        })
    }

    /// Claims the first candidate directory under `base` that `create_dir`
    /// creates. An occupied candidate (left by another process, or by an
    /// earlier run) is skipped and never touched.
    fn claim_in(base: &Path, mut candidate: impl FnMut() -> String) -> Self {
        loop {
            let root = base.join(candidate());
            match Self::create_private_dir(&root) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("claim {}: {error}", root.display()),
            }
            // Cleanup is armed only by this proof that the entry at `root` is
            // an empty real directory, not a symlink or a replacement.
            assert!(
                std::fs::symlink_metadata(&root).unwrap().is_dir(),
                "{} is not a directory",
                root.display()
            );
            assert!(
                std::fs::read_dir(&root).unwrap().next().is_none(),
                "{} was not empty when claimed",
                root.display()
            );
            // `pin` re-checks emptiness through the handle it opens, so a
            // directory swapped in before the open is never armed. Residual:
            // no std call creates and opens atomically, so an *empty* foreign
            // directory swapped in then is pinned (and later emptied/removed).
            return Self {
                held: pin(&root),
                root,
            };
        }
    }

    fn create_private_dir(path: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            std::fs::DirBuilder::new().mode(0o700).create(path)
        }
        #[cfg(not(unix))]
        std::fs::create_dir(path)
    }

    /// Deletes the claimed tree, but only what is still the claimed directory.
    fn remove_owned_root(&self) {
        self.remove_owned_root_with(|| {});
    }

    /// [`Self::remove_owned_root`] with a hook run between the last identity
    /// check and the removal, where a test lands a swap deterministically.
    ///
    /// Nothing is renamed or moved back, so a file created at the root's name
    /// meanwhile is never overwritten.
    ///
    /// Residuals, by platform:
    /// * Linux: contents go through the handle's `/proc/self/fd` entry; only
    ///   the emptied root is removed by path (`remove_dir`, which fails on a
    ///   populated directory or a symlink). A swap between the identity check
    ///   and that `rmdir` can remove an empty foreign directory. A failure
    ///   while clearing leaves the tree rather than retrying by path.
    /// * Elsewhere, and Linux without `/proc`: std has no handle-relative
    ///   removal, so the identity is re-checked immediately before
    ///   `remove_dir_all(root)`, and a populated swap landing in that instant
    ///   is deleted. Closing it needs `openat`-style removal (the `rustix`
    ///   code behind the `scenes` feature), a dependency decision.
    fn remove_owned_root_with(&self, before_remove: impl FnOnce()) {
        let Some(held) = &self.held else {
            return;
        };
        #[cfg(any(target_os = "linux", target_os = "android"))]
        if let Some(cleared) = clear_through_handle(held) {
            if cleared.is_ok() && is_held_at(held, &self.root) {
                before_remove();
                let _ = std::fs::remove_dir(&self.root);
            }
            return;
        }
        if is_held_at(held, &self.root) {
            before_remove();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.remove_owned_root();
    }
}

#[test]
fn an_occupied_candidate_is_skipped_and_survives() {
    let base = Fixture::claim("occupied-base");
    let occupant = base.root.join("taken");
    std::fs::create_dir(&occupant).unwrap();
    std::fs::write(occupant.join("sentinel"), b"keep").unwrap();
    let mut names = ["taken", "free"].into_iter();
    let root = {
        let fixture = Fixture::claim_in(&base.root, || names.next().unwrap().to_owned());
        assert_eq!(fixture.root, base.root.join("free"));
        fixture.root.clone()
    };
    assert!(!root.exists(), "the claimed root is cleaned up");
    assert_eq!(std::fs::read(occupant.join("sentinel")).unwrap(), b"keep");
}

#[test]
fn a_root_replaced_before_drop_is_not_deleted() {
    let base = Fixture::claim("replaced-base");
    let fixture = Fixture::claim_in(&base.root, || "root".to_owned());
    let root = fixture.root.clone();
    std::fs::rename(&root, base.root.join("moved")).unwrap();
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("sentinel"), b"keep").unwrap();
    drop(fixture);
    assert_eq!(std::fs::read(root.join("sentinel")).unwrap(), b"keep");
}

/// `rmdir` plus `mkdir` can hand the freed inode to the new directory, so a
/// (dev, ino) pair recorded at claim time proves nothing about a recreated
/// root; only a handle held open since the claim keeps the inode from reuse.
#[cfg(unix)]
#[test]
fn a_root_recreated_under_its_name_is_not_deleted() {
    let base = Fixture::claim("recreated-base");
    for attempt in 0..200 {
        let dir = base.root.join(format!("a{attempt}"));
        std::fs::create_dir(&dir).unwrap();
        let fixture = Fixture::claim_in(&dir, || "root".to_owned());
        let root = fixture.root.clone();
        std::fs::remove_dir(&root).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("sentinel"), b"keep").unwrap();
        drop(fixture);
        assert_eq!(
            std::fs::read(root.join("sentinel")).unwrap(),
            b"keep",
            "attempt {attempt}: a recreated root was deleted"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_root_replaced_by_a_symlink_before_drop_is_not_followed() {
    let base = Fixture::claim("symlinked-base");
    let fixture = Fixture::claim_in(&base.root, || "root".to_owned());
    let root = fixture.root.clone();
    let target = base.root.join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("sentinel"), b"keep").unwrap();
    std::fs::remove_dir(&root).unwrap();
    std::os::unix::fs::symlink(&target, &root).unwrap();
    drop(fixture);
    assert_eq!(std::fs::read(target.join("sentinel")).unwrap(), b"keep");
}

/// A swapper thread cycles a foreign directory in and out of the root's name
/// while `Drop` runs. The foreign tree must never be deleted.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_root_replaced_during_cleanup_is_not_deleted() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    let base = Fixture::claim("race-base");
    // Without a readable `/proc` cleanup falls back to the documented
    // path-based removal, which this race would rightly defeat.
    if base
        .held
        .as_ref()
        .is_none_or(|held| std::fs::read_dir(through_proc(held)).is_err())
    {
        return;
    }
    for attempt in 0..2000 {
        let dir = base.root.join(format!("a{attempt}"));
        std::fs::create_dir(&dir).unwrap();
        let fixture = Fixture::claim_in(&dir, || "root".to_owned());
        std::fs::write(fixture.root.join("owned"), b"x").unwrap();
        let root = fixture.root.clone();
        let aside = dir.join("aside");
        let replacement = dir.join("replacement");
        std::fs::create_dir(&replacement).unwrap();
        std::fs::write(replacement.join("sentinel"), b"keep").unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let cycles = Arc::new(AtomicUsize::new(0));
        let swapper = {
            let (stop, cycles) = (stop.clone(), cycles.clone());
            let (root, aside, replacement) = (root.clone(), aside.clone(), replacement.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    cycles.fetch_add(1, Ordering::Relaxed);
                    let _ = std::fs::rename(&root, &aside);
                    let _ = std::fs::rename(&replacement, &root);
                    std::hint::spin_loop();
                    let _ = std::fs::rename(&root, &replacement);
                    let _ = std::fs::rename(&aside, &root);
                }
            })
        };
        while cycles.load(Ordering::Relaxed) < 3 {
            std::hint::spin_loop();
        }
        drop(fixture);
        stop.store(true, Ordering::Relaxed);
        swapper.join().unwrap();
        // The sentinel may sit at any name under `dir`, including a private
        // cleanup name; only its deletion is a failure.
        let survived = std::fs::read_dir(&dir)
            .unwrap()
            .any(|entry| entry.unwrap().path().join("sentinel").exists());
        assert!(survived, "attempt {attempt}: cleanup deleted the sentinel");
    }
}

/// A foreign tree swapped in after the last identity check is not walked:
/// the removal acts on an empty root only, so a populated swap survives.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_tree_swapped_in_after_the_identity_check_survives() {
    let base = Fixture::claim("late-swap-base");
    let fixture = Fixture::claim_in(&base.root, || "root".to_owned());
    std::fs::write(fixture.root.join("owned"), b"x").unwrap();
    let root = fixture.root.clone();
    fixture.remove_owned_root_with(|| {
        std::fs::rename(&root, base.root.join("moved")).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("sentinel"), b"keep").unwrap();
    });
    assert_eq!(std::fs::read(root.join("sentinel")).unwrap(), b"keep");
}

/// Restoring nothing means a file created at the root's name while cleanup
/// runs is never overwritten.
#[cfg(unix)]
#[test]
fn a_file_created_at_the_root_name_during_cleanup_survives() {
    let base = Fixture::claim("late-file-base");
    let fixture = Fixture::claim_in(&base.root, || "root".to_owned());
    let root = fixture.root.clone();
    std::fs::rename(&root, base.root.join("moved")).unwrap();
    std::fs::write(&root, b"keep").unwrap();
    drop(fixture);
    assert_eq!(std::fs::read(&root).unwrap(), b"keep");
}

/// A populated directory found at the claimed name (swapped in before the
/// pin was opened) is never armed for cleanup.
#[cfg(unix)]
#[test]
fn a_populated_directory_is_never_pinned() {
    let base = Fixture::claim("populated-base");
    let populated = base.root.join("populated");
    std::fs::create_dir(&populated).unwrap();
    std::fs::write(populated.join("sentinel"), b"keep").unwrap();
    assert!(pin(&populated).is_none());
    let empty = base.root.join("empty");
    std::fs::create_dir(&empty).unwrap();
    assert!(pin(&empty).is_some());
}
