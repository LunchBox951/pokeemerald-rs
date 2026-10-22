//! Private, exclusively claimed temporary directories for test fixtures.
//!
//! The shared temporary directory is writable by other users and processes, so
//! a fixture must never write to, or remove, a predictable name directly inside
//! it. Each test thread claims one fresh root with `create_dir` (retrying under
//! a new name when the name is taken, never `create_dir_all` over an existing
//! path) and places everything below it. The root is removed when the thread
//! ends, and only because this module created it.

use std::cell::RefCell;
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A directory this module created, with the identity it had when claimed.
/// Dropping it removes the tree only while that same directory still sits at
/// the path; a substituted tree is left alone.
struct Owned {
    path: PathBuf,
    #[cfg(unix)]
    identity: (u64, u64),
}

#[cfg(unix)]
fn identity_of(metadata: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt as _;
    (metadata.dev(), metadata.ino())
}

impl Owned {
    fn claim(path: PathBuf) -> io::Result<Self> {
        #[cfg(unix)]
        let identity = identity_of(&std::fs::symlink_metadata(&path)?);
        Ok(Self {
            path,
            #[cfg(unix)]
            identity,
        })
    }

    fn still_ours(&self) -> bool {
        #[cfg(unix)]
        {
            std::fs::symlink_metadata(&self.path)
                .is_ok_and(|metadata| identity_of(&metadata) == self.identity)
        }
        #[cfg(not(unix))]
        {
            true
        }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        if self.still_ours() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Creates a new directory under `parent` with `create_dir`, retrying under a
/// fresh name while the name is taken, and leaves its owner able to list,
/// write and enter it whatever the process umask stripped.
fn claim_in(parent: &Path, tag: &str) -> io::Result<PathBuf> {
    let mut builder = DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let mut last = None;
    for attempt in 0..64 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let path = parent.join(format!("{tag}-{}-{attempt}-{nanos:x}", std::process::id()));
        match builder.create(&path) {
            Ok(()) => {
                #[cfg(unix)]
                File::open(&path)?
                    .set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
                return Ok(path);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => last = Some(err),
            Err(err) => return Err(err),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("no private directory name was free")))
}

thread_local! {
    static ROOT: RefCell<Option<Owned>> = const { RefCell::new(None) };
}

/// The calling test thread's private root, claimed on first use.
pub(crate) fn private_root() -> PathBuf {
    ROOT.with(|root| {
        let mut root = root.borrow_mut();
        if root.is_none() {
            let path = claim_in(&std::env::temp_dir(), "pokeemerald-rs-xtask-test")
                .expect("claim a private test directory");
            *root = Some(Owned::claim(path).expect("identify the private test directory"));
        }
        root.as_ref().map(|owned| owned.path.clone()).unwrap()
    })
}

/// A path under the private root. Nothing exists at it yet; `name` is a plain
/// file name, so two calls with one name give one path within a test.
pub(crate) fn private_path(name: &str) -> PathBuf {
    private_root().join(name)
}

/// A freshly created directory under the private root, named after `tag`.
pub(crate) fn private_dir(tag: &str) -> PathBuf {
    claim_in(&private_root(), tag).expect("claim a private fixture directory")
}

/// Creates `path` with `create_new`, so an existing entry (a planted symlink
/// included) is refused instead of followed, and writes `bytes` to it.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file: File = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_dirs_are_distinct_and_exclusively_created() {
        let a = private_dir("x");
        let b = private_dir("x");
        assert_ne!(a, b);
        assert!(a.starts_with(private_root()) && a.is_dir() && b.is_dir());
    }

    #[test]
    fn a_taken_name_is_refused_by_create_dir_and_left_intact() {
        let taken = private_dir("taken");
        std::fs::write(taken.join("keep.txt"), b"keep me").unwrap();
        assert!(DirBuilder::new().create(&taken).is_err());
        assert_eq!(std::fs::read(taken.join("keep.txt")).unwrap(), b"keep me");
    }

    #[cfg(unix)]
    #[test]
    fn write_new_does_not_follow_a_planted_symlink() {
        let victim = private_path("victim");
        std::fs::write(&victim, b"precious").unwrap();
        let planted = private_path("planted");
        std::os::unix::fs::symlink(&victim, &planted).unwrap();
        assert!(write_new(&planted, b"clobber").is_err());
        assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
    }

    #[test]
    fn write_new_refuses_an_existing_file() {
        let path = private_path("existing");
        write_new(&path, b"first").unwrap();
        assert!(write_new(&path, b"second").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
    }

    #[cfg(unix)]
    #[test]
    fn the_private_root_stays_owner_accessible_under_a_stripping_umask() {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(private_dir("mode"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o700, 0o700, "mode {mode:o}");
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_does_not_delete_a_tree_substituted_at_the_root_path() {
        let (root, moved) = std::thread::spawn(|| {
            let root = private_root();
            let moved = root.with_extension("aside");
            std::fs::rename(&root, &moved).unwrap();
            std::fs::create_dir(&root).unwrap();
            std::fs::write(root.join("foreign.txt"), b"not yours").unwrap();
            (root, moved)
        })
        .join()
        .unwrap();
        let survived = std::fs::read(root.join("foreign.txt")).ok();
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&moved);
        assert_eq!(survived.as_deref(), Some(&b"not yours"[..]));
    }

    #[test]
    fn the_private_root_dies_with_its_thread() {
        let root = std::thread::spawn(|| {
            let dir = private_dir("scoped");
            std::fs::write(dir.join("f"), b"x").unwrap();
            private_root()
        })
        .join()
        .unwrap();
        assert!(!root.exists());
    }
}
