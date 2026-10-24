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
/// Cleanup never deletes by the directory's name alone, because a name can
/// be swapped between a check and the delete. Every path a test derives
/// through [`TempDir::join`] is recorded; drop removes only those entries
/// and then calls the non-recursive `remove_dir`, which refuses a
/// non-empty directory. A stranger that swaps itself in at the claimed name
/// therefore survives, and so does anything the fixture left unrecorded
/// (the directory is then left behind rather than guessed at). On unix drop
/// also checks the name still holds the claimed inode before touching it.
/// The residual is a same-user swap that recreates the exact recorded
/// names. Off unix there is no identity to check, so drop removes nothing
/// and leaves the directory.
pub(crate) struct TempDir {
    path: PathBuf,
    /// The claimed directory's identity, so cleanup can tell it from
    /// whatever occupies the name afterwards.
    #[cfg(unix)]
    identity: (u64, u64),
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
            #[cfg(unix)]
            identity: identity_of(&path).expect("the directory just created"),
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
}

/// Device and inode of the directory itself (never a link's target), or
/// `None` when the name holds anything else.
#[cfg(unix)]
fn identity_of(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt as _;
    let meta = std::fs::symlink_metadata(path).ok()?;
    meta.is_dir().then(|| (meta.dev(), meta.ino()))
}

impl Drop for TempDir {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            if identity_of(&self.path) != Some(self.identity) {
                return;
            }
            let entries = std::mem::take(&mut *self.entries.lock().expect("the entry list"));
            for name in entries {
                let entry = self.path.join(name);
                // `symlink_metadata` so a link is unlinked, never followed.
                let _ = match std::fs::symlink_metadata(&entry) {
                    Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&entry),
                    Ok(_) => std::fs::remove_file(&entry),
                    Err(err) => Err(err),
                };
            }
            // Non-recursive: a non-empty directory is left alone.
            let _ = std::fs::remove_dir(&self.path);
        }
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

    #[cfg(unix)]
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

    #[cfg(unix)]
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

    #[cfg(unix)]
    #[test]
    fn dropping_a_claim_spares_a_stranger_swapped_in_during_cleanup() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        // A same-user peer keeps swapping a populated stranger in and out of
        // the claimed name while the claim is dropped. Bounded: the race
        // window is a few instructions wide, so this raises the odds of a
        // by-name recursive delete failing rather than guaranteeing them.
        let root = TempDir::new("race-root");
        for round in 0..2_000u32 {
            let claimed = TempDir::new_in(root.path(), "probe");
            let path = claimed.path().to_path_buf();
            let stash = root.join(&format!("stash-{round}"));
            let stranger = root.join(&format!("stranger-{round}"));
            std::fs::create_dir(&stranger).unwrap();
            std::fs::write(stranger.join("sentinel"), b"not yours").unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let attacker = {
                let (path, stash, stranger, stop) =
                    (path.clone(), stash.clone(), stranger.clone(), stop.clone());
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        if std::fs::rename(&path, &stash).is_err() {
                            continue;
                        }
                        let _ = std::fs::rename(&stranger, &path);
                        let _ = std::fs::rename(&path, &stranger);
                        let _ = std::fs::rename(&stash, &path);
                    }
                })
            };
            std::thread::yield_now();
            drop(claimed);
            stop.store(true, Ordering::Relaxed);
            attacker.join().unwrap();
            let survived = [&stranger, &path, &stash].iter().any(|dir| {
                std::fs::read(dir.join("sentinel")).ok().as_deref() == Some(&b"not yours"[..])
            });
            assert!(
                survived,
                "round {round}: cleanup deleted the stranger's directory"
            );
            for dir in [&stranger, &path, &stash] {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
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
