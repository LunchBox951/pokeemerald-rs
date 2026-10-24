//! Scratch directories for the crate's unit tests.

use std::path::{Path, PathBuf};

/// How many names a claim tries before it gives up.
const CLAIM_ATTEMPTS: u32 = 1_000;

/// A fresh directory this process created, removed on drop.
///
/// A predictable name is only safe to use once `create_dir` has made it:
/// `create_dir` fails on any existing entry, including a populated
/// directory or a symlink, so the name is retried rather than reused, and
/// no candidate is ever deleted or written through before it is claimed.
/// Only a directory this value created is removed, so a stranger that
/// holds a candidate name keeps its contents. On unix cleanup also checks
/// the name still holds the claimed directory's inode, so a stranger that
/// replaces the claim before drop keeps its contents too.
pub(crate) struct TempDir {
    path: PathBuf,
    /// The claimed directory's identity, so cleanup can tell it from
    /// whatever occupies the name afterwards.
    #[cfg(unix)]
    identity: (u64, u64),
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
        #[cfg(unix)]
        {
            let identity = identity_of(&path).expect("the directory just created");
            Self { path, identity }
        }
        #[cfg(not(unix))]
        {
            Self { path }
        }
    }

    /// Whether the name still holds the directory this value created.
    /// Non-unix targets have no cheap identity to compare, so they trust
    /// the name.
    fn still_owns_path(&self) -> bool {
        #[cfg(unix)]
        {
            identity_of(&self.path) == Some(self.identity)
        }
        #[cfg(not(unix))]
        {
            true
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn join(&self, name: &str) -> PathBuf {
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
        if self.still_owns_path() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TempDir;

    /// The name the first claim under `root` would take for `label`.
    fn first_candidate(root: &std::path::Path, label: &str) -> std::path::PathBuf {
        root.join(format!("rom-import-{label}-{}-0", std::process::id()))
    }

    #[test]
    fn a_claim_leaves_an_occupied_candidate_and_its_contents_alone() {
        let root = TempDir::new("occupied-root");
        let occupied = first_candidate(root.path(), "probe");
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

    #[cfg(unix)]
    #[test]
    fn dropping_a_claim_spares_a_stranger_that_took_its_name() {
        let root = TempDir::new("replaced-root");
        let claimed = TempDir::new_in(root.path(), "probe");
        let path = claimed.path().to_path_buf();
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
    fn a_claim_never_writes_through_a_planted_symlink() {
        let root = TempDir::new("symlink-root");
        let victim = TempDir::new("symlink-victim");
        std::fs::write(victim.join("sentinel"), b"not yours").expect("the sentinel");
        let link = first_candidate(root.path(), "probe");
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
        let link = first_candidate(root.path(), "probe");
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
