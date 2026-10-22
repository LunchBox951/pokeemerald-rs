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
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A directory this module created; dropping it removes exactly that tree.
struct Owned(PathBuf);

impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn claim_in(parent: &Path, tag: &str) -> io::Result<PathBuf> {
    let mut builder = DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let mut last = None;
    for _ in 0..64 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let path = parent.join(format!(
            "{tag}-{}-{}-{nanos:x}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match builder.create(&path) {
            Ok(()) => return Ok(path),
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
            *root = Some(Owned(path));
        }
        root.as_ref().map(|owned| owned.0.clone()).unwrap()
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
