//! Test-only fixture directories the test itself created.
//!
//! A fixture that builds a predictable path under the shared temp directory,
//! then writes into it or recursively deletes it, acts on whatever already
//! sits at that name: an unrelated populated directory is deleted, and a
//! planted symlink redirects the writes. [`OwnedDir`] instead claims a name
//! with `mkdir`, which refuses an existing entry of any kind (including a
//! symlink, dangling or not), retries the next name, and removes only the
//! tree it claimed.

use std::io;
use std::path::{Path, PathBuf};

/// Names tried before giving up; a run that finds all of them occupied is
/// being attacked or the temp directory is badly polluted.
const ATTEMPTS: u64 = 64;

/// A directory this process exclusively created, removed on drop.
#[derive(Debug)]
pub(crate) struct OwnedDir {
    path: PathBuf,
    identity: Identity,
}

/// What `mkdir` created: on Unix its device and inode, elsewhere only the
/// fact that it was a real (non-symlink) directory.
#[derive(Debug, PartialEq, Eq)]
struct Identity(Option<(u64, u64)>);

impl Identity {
    /// The identity of the real directory at `path`, or `None` when `path` is
    /// absent, a symlink or not a directory.
    fn of(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        if !metadata.file_type().is_dir() {
            return None;
        }
        #[cfg(unix)]
        let ids = {
            use std::os::unix::fs::MetadataExt as _;
            Some((metadata.dev(), metadata.ino()))
        };
        #[cfg(not(unix))]
        let ids = None;
        Some(Self(ids))
    }
}

impl OwnedDir {
    /// Claims a fresh private directory directly under the system temp directory.
    pub(crate) fn create(prefix: &str) -> io::Result<Self> {
        Self::create_in(&std::env::temp_dir(), prefix)
    }

    /// Claims a fresh private directory named for `prefix` under `base`. Names
    /// held by a concurrent sibling fixture or a leftover are skipped, so no
    /// shared counter is needed.
    pub(crate) fn create_in(base: &Path, prefix: &str) -> io::Result<Self> {
        Self::create_in_from(base, prefix, 0)
    }

    /// Tries `ATTEMPTS` names from attempt number `first`, skipping each name
    /// that is already occupied.
    fn create_in_from(base: &Path, prefix: &str, first: u64) -> io::Result<Self> {
        for attempt in first..first + ATTEMPTS {
            match Self::claim(&Self::candidate(base, prefix, attempt)) {
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                other => return other,
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "every {prefix} fixture name under {} is occupied",
                base.display()
            ),
        ))
    }

    fn candidate(base: &Path, prefix: &str, attempt: u64) -> PathBuf {
        base.join(format!("{prefix}-{}-{attempt}", std::process::id()))
    }

    /// Creates `path` with a single `mkdir`, failing with `AlreadyExists` when
    /// anything (file, directory or symlink) is already there.
    fn claim(path: &Path) -> io::Result<Self> {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(path)?;
        let identity = Identity::of(path).ok_or_else(|| {
            io::Error::other(format!(
                "{} was replaced right after it was created",
                path.display()
            ))
        })?;
        Ok(Self {
            path: path.to_path_buf(),
            identity,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for OwnedDir {
    /// Removes the tree only while the path still names the directory `mkdir`
    /// created; a replacement (symlink or another directory) is left alone.
    ///
    /// Still exposed: a same-user process that swaps the path between this
    /// check and the removal, and fixture writes that go through `path()`
    /// after a swap. A test fixture in a private temp name accepts both; the
    /// check bounds the damage to a swap landing inside that window.
    fn drop(&mut self) {
        if Identity::of(&self.path).as_ref() == Some(&self.identity) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn private_base(tag: &str) -> OwnedDir {
        OwnedDir::create(&format!("pokeemerald-rs-owned-fixture-base-{tag}")).unwrap()
    }

    #[test]
    fn claiming_a_populated_preexisting_directory_is_refused_and_leaves_it_intact() {
        let base = private_base("populated");
        let occupied = OwnedDir::candidate(base.path(), "fx", 0);
        std::fs::create_dir(&occupied).unwrap();
        std::fs::write(occupied.join("sentinel"), "precious").unwrap();

        let error = OwnedDir::claim(&occupied).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            std::fs::read(occupied.join("sentinel")).unwrap(),
            b"precious"
        );
    }

    #[test]
    fn create_skips_occupied_names_and_leaves_them_intact() {
        let base = private_base("skips");
        for attempt in 0..3 {
            let occupied = OwnedDir::candidate(base.path(), "fx", attempt);
            std::fs::create_dir(&occupied).unwrap();
            std::fs::write(occupied.join("sentinel"), "precious").unwrap();
        }

        let claimed = OwnedDir::create_in_from(base.path(), "fx", 0).unwrap();

        assert_eq!(claimed.path(), OwnedDir::candidate(base.path(), "fx", 3));
        let claimed_path = claimed.path().to_path_buf();
        drop(claimed);
        assert!(
            !claimed_path.exists(),
            "the claimed tree is removed on drop"
        );
        for attempt in 0..3 {
            let occupied = OwnedDir::candidate(base.path(), "fx", attempt);
            assert_eq!(
                std::fs::read(occupied.join("sentinel")).unwrap(),
                b"precious"
            );
        }
    }

    #[test]
    fn create_fails_rather_than_reusing_when_every_name_is_occupied() {
        let base = private_base("exhausted");
        for attempt in 0..ATTEMPTS {
            std::fs::create_dir(OwnedDir::candidate(base.path(), "fx", attempt)).unwrap();
        }

        let error = OwnedDir::create_in_from(base.path(), "fx", 0).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        for attempt in 0..ATTEMPTS {
            assert!(OwnedDir::candidate(base.path(), "fx", attempt).is_dir());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_planted_directory_symlink_is_never_followed_or_removed() {
        let base = private_base("dir-link");
        let victim = private_base("dir-link-victim");
        std::fs::write(victim.path().join("sentinel"), "precious").unwrap();
        let link = OwnedDir::candidate(base.path(), "fx", 0);
        std::os::unix::fs::symlink(victim.path(), &link).unwrap();

        let claimed = OwnedDir::create_in_from(base.path(), "fx", 0).unwrap();
        std::fs::write(claimed.path().join("charmap.txt"), "fixture").unwrap();
        drop(claimed);

        assert_eq!(
            std::fs::read(victim.path().join("sentinel")).unwrap(),
            b"precious"
        );
        assert!(!victim.path().join("charmap.txt").exists());
        assert!(link.is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn a_planted_dangling_symlink_is_not_created_through() {
        let base = private_base("dangling");
        let target = base.path().join("elsewhere");
        let link = OwnedDir::candidate(base.path(), "fx", 0);
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let error = OwnedDir::claim(&link).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(!target.exists(), "mkdir created the symlink's target");
    }

    #[cfg(unix)]
    #[test]
    fn a_claimed_directory_is_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt as _;
        let base = private_base("mode");
        let claimed = OwnedDir::create_in(base.path(), "fx").unwrap();
        let mode = std::fs::metadata(claimed.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "mode {mode:o}");
    }

    #[test]
    fn drop_removes_only_the_claimed_tree_after_a_post_claim_replacement() {
        let base = private_base("swap");
        let claimed = OwnedDir::create_in(base.path(), "fx").unwrap();
        std::fs::write(claimed.path().join("charmap.txt"), "fixture").unwrap();
        let path = claimed.path().to_path_buf();
        let aside = base.path().join("aside");
        std::fs::rename(&path, &aside).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("sentinel"), "precious").unwrap();
        drop(claimed);
        assert_eq!(
            std::fs::read(path.join("sentinel")).ok().as_deref(),
            Some(&b"precious"[..]),
            "drop deleted the replacement it never claimed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn drop_does_not_follow_a_symlink_swapped_in_after_the_claim() {
        let base = private_base("swap-link");
        let victim = private_base("swap-link-victim");
        std::fs::write(victim.path().join("sentinel"), "precious").unwrap();
        let claimed = OwnedDir::create_in(base.path(), "fx").unwrap();
        let path = claimed.path().to_path_buf();
        std::fs::remove_dir(&path).unwrap();
        std::os::unix::fs::symlink(victim.path(), &path).unwrap();
        drop(claimed);
        assert_eq!(
            std::fs::read(victim.path().join("sentinel")).unwrap(),
            b"precious"
        );
        assert!(path.is_symlink());
    }
}
