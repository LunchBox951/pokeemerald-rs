//! The cry fixture's temporary root: claimed exclusively, never pre-deleted,
//! and removed only while it is still the directory that was claimed.

use std::path::{Path, PathBuf};

/// Identity of a claimed directory: what cleanup compares before it deletes.
/// Unix: device and inode. Windows: the file ID `win-fs` reads through a
/// handle, which survives the creation-time reuse NTFS name tunnelling can
/// cause. Elsewhere none is available, so cleanup leaks the directory rather
/// than risk deleting a replacement.
#[cfg(unix)]
type Identity = (u64, u64);
#[cfg(windows)]
type Identity = win_fs::WindowsFileIdentity;
#[cfg(not(any(unix, windows)))]
type Identity = ();

/// The identity of the real directory (not a link) at `path`, if it has one.
fn identity_at(path: &Path) -> Option<Identity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let metadata = std::fs::symlink_metadata(path).ok()?;
        metadata.is_dir().then(|| (metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        let handle = win_fs::open_for_verified_delete(path).ok()?;
        win_fs::WindowsFileIdentity::of(&handle).ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        None
    }
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
    claimed: Option<Identity>,
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
            return Self {
                claimed: identity_at(&root),
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
    ///
    /// Checking `root` and then deleting `root` would hand the delete to
    /// whatever a swapper put there in between. So the root is first renamed
    /// to a fresh unpredictable sibling, a single atomic step that takes
    /// whatever stood at the path. The identity is then checked at that
    /// private name, which nobody else knows, and only a match is deleted. A
    /// foreign directory that was moved by mistake is renamed back (or, if its
    /// old name was taken meanwhile, left at the private name); it is never
    /// deleted.
    fn remove_owned_root(&self) {
        let Some(claimed) = self.claimed else {
            return;
        };
        let Some(parent) = self.root.parent() else {
            return;
        };
        let private = parent.join(format!(".cry-fixture-cleanup-{:016x}", unpredictable()));
        if std::fs::rename(&self.root, &private).is_err() {
            return;
        }
        if identity_at(&private) == Some(claimed) {
            let _ = std::fs::remove_dir_all(&private);
        } else {
            let _ = std::fs::rename(&private, &self.root);
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
/// while `Drop` runs. A foreign tree may be left at a private name beside the
/// root, but it must never be deleted.
#[cfg(unix)]
#[test]
fn a_root_replaced_during_cleanup_is_not_deleted() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    let base = Fixture::claim("race-base");
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
