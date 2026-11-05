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
///
/// `handle` stays open on the claimed directory for the fixture's lifetime:
/// an inode (Unix) or file ID (Windows) is not recycled while something
/// holds it open, so a directory that is removed and recreated at the same
/// name cannot inherit the claimed identity and be mistaken for it on drop.
#[derive(Debug)]
pub(crate) struct OwnedDir {
    path: PathBuf,
    identity: Identity,
    handle: Option<std::fs::File>,
}

/// What `mkdir` created: a real (non-symlink) directory plus, where the
/// platform can name one, a per-object token that survives a rename and is
/// never shared by two live directories.
#[derive(Debug, PartialEq, Eq)]
struct Identity {
    token: Option<Token>,
}

/// Unix: device and inode.
#[cfg(unix)]
type Token = (u64, u64);

/// Windows: the volume and file ID read off a handle opened without
/// following a reparse point.
#[cfg(windows)]
type Token = win_fs::WindowsFileIdentity;

/// Any other target has no identity this helper can read, so no token is ever
/// produced and [`OwnedDir`] fails closed: it removes nothing on drop.
#[cfg(not(any(unix, windows)))]
type Token = ();

impl Identity {
    /// The identity of the real directory at `path`: `Ok(None)` when the
    /// path is absent, a symlink or not a directory, which says it is not
    /// the claimed directory; `Err` when the read itself failed, which says
    /// nothing about what is there.
    fn of(path: &Path) -> io::Result<Option<Self>> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if !metadata.file_type().is_dir() {
            return Ok(None);
        }
        #[cfg(unix)]
        let token = {
            use std::os::unix::fs::MetadataExt as _;
            Some((metadata.dev(), metadata.ino()))
        };
        #[cfg(windows)]
        let token = Some(
            win_fs::open_for_verified_delete(path)
                .and_then(|handle| win_fs::WindowsFileIdentity::of(&handle))?,
        );
        #[cfg(not(any(unix, windows)))]
        let token: Option<Token> = None;
        Ok(Some(Self { token }))
    }

    /// The identity of the directory `handle` is open on, or `None` where the
    /// platform cannot read one.
    fn of_handle(handle: &std::fs::File) -> Option<Self> {
        #[cfg(unix)]
        let token = {
            use std::os::unix::fs::MetadataExt as _;
            let metadata = handle.metadata().ok()?;
            Some((metadata.dev(), metadata.ino()))
        };
        #[cfg(windows)]
        let token = Some(win_fs::WindowsFileIdentity::of(handle).ok()?);
        #[cfg(not(any(unix, windows)))]
        let token: Option<Token> = {
            let _ = handle;
            None
        };
        Some(Self { token })
    }

    /// Whether this identity can vouch for ownership at all.
    fn is_verifiable(&self) -> bool {
        self.token.is_some()
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
        #[cfg(unix)]
        let builder = {
            let mut builder = std::fs::DirBuilder::new();
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = std::fs::DirBuilder::new();
        builder.create(path)?;
        Self::claim_after_mkdir(path)
    }

    /// The checks that follow `mkdir` at `path`: the identity reads, the
    /// pinning handle, and the emptiness `mkdir` guarantees.
    fn claim_after_mkdir(path: &Path) -> io::Result<Self> {
        let replaced = || {
            io::Error::other(format!(
                "{} was replaced right after it was created",
                path.display()
            ))
        };
        let identity = Identity::of(path)?.ok_or_else(replaced)?;
        // The handle pins the identity for the fixture's lifetime. Its own
        // identity must match the path's: a symlink swapped in between the
        // two reads would open its target instead.
        let handle = Self::open_claimed(path)?;
        if identity.is_verifiable() && Identity::of_handle(&handle).as_ref() != Some(&identity) {
            return Err(replaced());
        }
        // `mkdir` made an empty directory. A same-user process can rename a
        // populated directory over the name before the reads above, and that
        // directory would then be filled and deleted as the fixture's own;
        // anything already inside it says it is not the one `mkdir` made.
        if std::fs::read_dir(path)?.next().is_some() {
            return Err(replaced());
        }
        Ok(Self {
            path: path.to_path_buf(),
            identity,
            handle: Some(handle),
        })
    }

    /// Opens the claimed directory to hold it for the fixture's lifetime.
    fn open_claimed(path: &Path) -> io::Result<std::fs::File> {
        #[cfg(windows)]
        {
            win_fs::open_for_verified_delete(path)
        }
        #[cfg(not(windows))]
        {
            std::fs::File::open(path)
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for OwnedDir {
    /// Removes the tree only while the path still names the directory `mkdir`
    /// created; a replacement (symlink or another directory) is left alone.
    /// On a target with no readable identity (neither Unix nor Windows) the
    /// directory cannot be told from a replacement, so nothing is removed.
    ///
    /// The held handle keeps the claimed identity from being recycled, so a
    /// directory removed and recreated at the name is told apart too. On
    /// Windows the handle is closed before the removal, since the directory
    /// cannot be removed while it is open.
    ///
    /// A removal that fails leaves the tree behind, so it panics unless the
    /// thread is already unwinding: a passing test must not leave its
    /// fixture on disk unreported.
    ///
    /// A path whose identity cannot be read at all (the parent lost search
    /// permission, say, or on Windows the verifying open failed) is not a
    /// replacement: nothing is removed, and that is reported the same way,
    /// so the tree is never left behind silently.
    ///
    /// Still exposed: a same-user process that swaps the path between this
    /// check and the removal, one that swaps an empty directory in between
    /// `mkdir` and the claim's identity reads, and fixture writes that go
    /// through `path()` after a swap. POSIX has no atomic create-and-open for
    /// a directory, and a same-user process can already do anything this
    /// one can; a test fixture in a private temp name accepts those, and the
    /// checks bound the damage to a swap landing inside those windows.
    fn drop(&mut self) {
        let mut outcome = Ok(());
        if self.identity.is_verifiable() {
            match Identity::of(&self.path) {
                Err(error) => outcome = Err(error),
                Ok(Some(identity)) if identity == self.identity => {
                    #[cfg(windows)]
                    drop(self.handle.take());
                    outcome = std::fs::remove_dir_all(&self.path);
                }
                Ok(_) => {}
            }
        }
        drop(self.handle.take());
        if let Err(error) = outcome {
            assert!(
                std::thread::panicking(),
                "removing fixture {} failed: {error}",
                self.path.display()
            );
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

    #[cfg(any(unix, windows))]
    #[test]
    fn two_live_directories_have_different_identities() {
        let base = private_base("identity");
        let first = OwnedDir::create_in(base.path(), "fx").unwrap();
        let second = OwnedDir::create_in(base.path(), "fx").unwrap();
        assert!(first.identity.is_verifiable());
        assert_ne!(
            Identity::of(first.path()).unwrap(),
            Identity::of(second.path()).unwrap()
        );
        assert_eq!(
            Identity::of(first.path()).unwrap().as_ref(),
            Some(&first.identity)
        );
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

    /// Removing the claimed directory frees its name; a directory recreated
    /// there would take the next free identity, the claimed one included
    /// unless the fixture still holds it.
    #[test]
    fn drop_removes_nothing_after_the_claimed_directory_is_removed_and_recreated() {
        let base = private_base("recreate");
        let claimed = OwnedDir::create_in(base.path(), "fx").unwrap();
        let path = claimed.path().to_path_buf();
        std::fs::remove_dir(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("sentinel"), "precious").unwrap();
        drop(claimed);
        assert_eq!(
            std::fs::read(path.join("sentinel")).ok().as_deref(),
            Some(&b"precious"[..]),
            "drop deleted a directory it never claimed"
        );
    }

    /// A populated directory renamed over the name between `mkdir` and the
    /// identity reads is refused: `mkdir` made an empty one.
    #[test]
    fn a_populated_directory_swapped_in_before_the_identity_read_is_refused() {
        let base = private_base("pre-read-swap");
        let candidate = OwnedDir::candidate(base.path(), "fx", 0);
        let victim = base.path().join("victim");
        std::fs::create_dir(&victim).unwrap();
        std::fs::write(victim.join("sentinel"), "precious").unwrap();
        std::fs::create_dir(&candidate).unwrap();
        // The state `claim` would see after a swap that beat its reads.
        std::fs::remove_dir(&candidate).unwrap();
        std::fs::rename(&victim, &candidate).unwrap();

        let claimed = OwnedDir::claim_after_mkdir(&candidate);

        assert!(claimed.is_err(), "a populated replacement was claimed");
        assert_eq!(
            std::fs::read(candidate.join("sentinel")).unwrap(),
            b"precious"
        );
    }

    /// A tree the fixture cannot remove is reported, not left behind
    /// silently. Root can remove a read-only child, so this pins only that
    /// the fixture is either gone or its removal failure surfaced.
    #[cfg(unix)]
    #[test]
    fn a_cleanup_failure_on_drop_is_surfaced_not_left_behind() {
        use std::os::unix::fs::PermissionsExt as _;
        let base = private_base("cleanup");
        let claimed = OwnedDir::create_in(base.path(), "fx").unwrap();
        let path = claimed.path().to_path_buf();
        let child = path.join("locked");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(child.join("f"), "x").unwrap();
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o500)).unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(claimed)));
        let removed = !path.exists();
        if !removed {
            std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::remove_dir_all(&path).unwrap();
        }
        assert!(
            removed || outcome.is_err(),
            "drop left {} behind without reporting it",
            path.display()
        );
    }

    /// An owned directory whose identity cannot be read (the parent lost
    /// search permission) is reported on drop, not skipped as a replacement.
    /// Root reads it regardless, so this pins only that the fixture is
    /// either gone or its failure surfaced.
    #[cfg(unix)]
    #[test]
    fn an_unverifiable_owned_directory_on_drop_is_surfaced_not_left_behind() {
        use std::os::unix::fs::PermissionsExt as _;
        let base = private_base("unverifiable");
        let claimed = OwnedDir::create_in(base.path(), "fx").unwrap();
        let path = claimed.path().to_path_buf();
        std::fs::set_permissions(base.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(claimed)));
        std::fs::set_permissions(base.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let removed = !path.exists();
        assert!(
            removed || outcome.is_err(),
            "drop left {} behind without reporting it",
            path.display()
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
