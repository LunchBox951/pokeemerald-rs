//! Private, exclusively claimed temporary directories for test fixtures.
//!
//! The shared temporary directory is writable by other users and processes, so
//! a fixture must never write to, or remove, a predictable name directly inside
//! it. Each test thread claims one fresh root with `create_dir` (retrying under
//! a new name when the name is taken, never `create_dir_all` over an existing
//! path) and places everything below it. The root is removed when the thread
//! ends, and only because this module created it.
//!
//! Once claimed, the root can be renamed or replaced only by its own user (or
//! root): `/tmp` is sticky, and the macOS and Windows temporary directories
//! are per-user. The identity check before cleanup therefore guards against
//! accidents, not an adversary; it is not atomic with the removal, and the
//! paths handed out are plain pathnames. A same-user process able to swap the
//! root in those instants could already delete or write anything the swap
//! would reach.

use std::cell::RefCell;
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A directory this module created, held open from the moment it was claimed.
/// The handle is the identity: dropping removes the tree only while the path
/// still names the directory the handle refers to, so a tree substituted at
/// the path is left alone. Where no identity can be read back, nothing is
/// ever removed.
struct Owned {
    path: PathBuf,
    #[cfg(any(unix, windows))]
    handle: File,
}

impl Owned {
    #[cfg(any(unix, windows))]
    fn new(path: PathBuf, handle: File) -> Self {
        Self { path, handle }
    }

    #[cfg(not(any(unix, windows)))]
    fn new(path: PathBuf, _handle: ()) -> Self {
        Self { path }
    }

    #[cfg(unix)]
    fn still_ours(&self) -> bool {
        use std::os::unix::fs::MetadataExt as _;
        let identity = |metadata: std::fs::Metadata| (metadata.dev(), metadata.ino());
        match (
            self.handle.metadata(),
            std::fs::symlink_metadata(&self.path),
        ) {
            (Ok(held), Ok(found)) => found.is_dir() && identity(held) == identity(found),
            _ => false,
        }
    }

    #[cfg(windows)]
    fn still_ours(&self) -> bool {
        use win_fs::WindowsFileIdentity;
        open_directory(&self.path).is_ok_and(|fresh| {
            WindowsFileIdentity::of(&fresh)
                .and_then(|found| WindowsFileIdentity::of(&self.handle).map(|held| found == held))
                .unwrap_or(false)
        })
    }

    #[cfg(not(any(unix, windows)))]
    fn still_ours(&self) -> bool {
        false
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        if self.still_ours() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// What `claim_in` keeps of the directory it created, beyond its path.
#[cfg(any(unix, windows))]
type Handle = File;
#[cfg(not(any(unix, windows)))]
type Handle = ();

/// Opens the directory `path` without following a symlink or reparse point
/// at it and restores owner rwx through that handle, whatever the process
/// umask stripped from `mkdir(0700)`. Linux pins it with `O_PATH`, which asks
/// for no permission bits, so a umask that left the directory without owner
/// read (mode 0300 under umask 0444) cannot lock the claim out.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn pin_directory(path: &Path) -> io::Result<Handle> {
    let pinned = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::PATH
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    restore_owner_rwx_with(&pinned, chmod_through_proc)?;
    Ok(pinned)
}

/// Leaves `pinned` alone when the umask kept owner rwx, the usual case, so
/// a Linux without a mounted `/proc` can still claim directories; otherwise
/// changes the mode through `chmod_pinned`.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn restore_owner_rwx_with(
    pinned: &File,
    chmod_pinned: impl FnOnce(&File) -> io::Result<()>,
) -> io::Result<()> {
    let owner = rustix::fs::Mode::RWXU.bits();
    if rustix::fs::fstat(pinned)?.st_mode & owner == owner {
        return Ok(());
    }
    chmod_pinned(pinned)
}

/// Sets mode 0700 through the pinned descriptor's `/proc` entry, which names
/// the inode itself and not the path; an `O_PATH` descriptor cannot `fchmod`.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn chmod_through_proc(pinned: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd as _;
    let through_proc = format!("/proc/self/fd/{}", pinned.as_raw_fd());
    rustix::fs::chmodat(
        rustix::fs::CWD,
        through_proc.as_str(),
        rustix::fs::Mode::RWXU,
        rustix::fs::AtFlags::empty(),
    )?;
    Ok(())
}

/// Opens the directory `path` without following a symlink and restores owner
/// rwx through that descriptor. Beyond Linux there is no permission-free pin
/// through the approved `rustix` surface, so a umask that withheld owner read
/// from the new directory makes this open fail rather than fall back to
/// changing the mode by pathname.
#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
fn pin_directory(path: &Path) -> io::Result<Handle> {
    let held = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    rustix::fs::fchmod(&held, rustix::fs::Mode::RWXU)?;
    Ok(held)
}

/// Opens `path` asking for no access and sharing everything, without
/// following a final reparse point, and refuses anything but a plain
/// directory.
#[cfg(windows)]
fn open_directory(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    const FILE_SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let held = OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_ALL)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let found = held.metadata()?;
    if !found.is_dir() || found.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("the claimed directory was replaced"));
    }
    Ok(held)
}

#[cfg(windows)]
fn pin_directory(path: &Path) -> io::Result<Handle> {
    open_directory(path)
}

#[cfg(not(any(unix, windows)))]
fn pin_directory(_path: &Path) -> io::Result<Handle> {
    Ok(())
}

/// Creates a new directory under `parent` with `create_dir`, retrying under a
/// fresh name while the name is taken, and pins it open straight away. The
/// handle, not the returned path, is what identifies the directory from then
/// on; a refused pin removes nothing, since the path may name another's tree.
fn claim_in(parent: &Path, tag: &str) -> io::Result<(PathBuf, Handle)> {
    #[cfg(unix)]
    let builder = {
        let mut builder = DirBuilder::new();
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder
    };
    #[cfg(not(unix))]
    let builder = DirBuilder::new();
    let mut last = None;
    for attempt in 0..64 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let path = parent.join(format!("{tag}-{}-{attempt}-{nanos:x}", std::process::id()));
        match builder.create(&path) {
            Ok(()) => return pin_directory(&path).map(|handle| (path, handle)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => last = Some(err),
            Err(err) => return Err(err),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("no private directory name was free")))
}

// The only `unsafe` here is inside `std`'s `thread_local!` expansion (a const
// initializer, a plain `RefCell<Option<_>>` with no destructor requirement of
// its own beyond `Owned::drop`); this module writes none of its own.
thread_local! {
    static ROOT: RefCell<Option<Owned>> = const { RefCell::new(None) };
}

/// The calling test thread's private root, claimed on first use.
pub(crate) fn private_root() -> PathBuf {
    ROOT.with(|root| {
        let mut root = root.borrow_mut();
        if root.is_none() {
            let (path, handle) = claim_in(&std::env::temp_dir(), "pokeemerald-rs-xtask-test")
                .expect("claim a private test directory");
            *root = Some(Owned::new(path, handle));
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
    claim_in(&private_root(), tag)
        .expect("claim a private fixture directory")
        .0
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

    #[cfg(unix)]
    #[test]
    fn pinning_refuses_a_symlink_swapped_in_for_the_new_directory() {
        let target = private_dir("target");
        std::fs::write(target.join("keep.txt"), b"keep me").unwrap();
        let planted = private_path("planted-dir");
        std::os::unix::fs::symlink(&target, &planted).unwrap();
        assert!(pin_directory(&planted).is_err());
        assert_eq!(std::fs::read(target.join("keep.txt")).unwrap(), b"keep me");
    }

    #[test]
    fn a_tree_substituted_before_registration_is_not_removed_by_the_handle_holder() {
        let (path, handle) = claim_in(&private_root(), "swap").unwrap();
        let moved = path.with_extension("aside");
        std::fs::rename(&path, &moved).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("foreign.txt"), b"not yours").unwrap();
        drop(Owned::new(path.clone(), handle));
        assert_eq!(
            std::fs::read(path.join("foreign.txt")).unwrap(),
            b"not yours"
        );
    }

    /// A claim whose umask kept owner rwx never needs `/proc`: the fallback
    /// that would change the mode through it is not called.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn an_owner_accessible_directory_is_pinned_without_procfs() {
        let dir = private_dir("no-proc");
        let pinned = File::from(
            rustix::fs::open(
                &dir,
                rustix::fs::OFlags::PATH | rustix::fs::OFlags::DIRECTORY,
                rustix::fs::Mode::empty(),
            )
            .unwrap(),
        );
        restore_owner_rwx_with(&pinned, |_| Err(io::Error::other("no /proc here"))).unwrap();
    }

    /// Set in the child [`a_claim_under_an_owner_stripping_umask_stays_owner_accessible`]
    /// re-runs itself as, so only that child changes its own umask.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    const UMASK_CHILD: &str = "POKEEMERALD_TEST_SUPPORT_UMASK_CHILD";

    /// `mkdir(0700)` under umask 0444 leaves 0300, which a plain read-only
    /// open cannot enter; the claim must still succeed and restore rwx.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn a_claim_under_an_owner_stripping_umask_stays_owner_accessible() {
        use std::os::unix::fs::PermissionsExt as _;
        let name = "test_support::tests::\
                    a_claim_under_an_owner_stripping_umask_stays_owner_accessible";
        if std::env::var_os(UMASK_CHILD).is_none() {
            let output = std::process::Command::new("/bin/sh")
                .args(["-c", "umask 0444 && exec \"$0\" \"$@\""])
                .arg(std::env::current_exe().unwrap())
                .args(["--exact", name, "--test-threads=1"])
                .env(UMASK_CHILD, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains("1 passed"),
                "umask 0444 child failed:\n{stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let dir = private_dir("umask");
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "mode {mode:o}");
        std::fs::write(dir.join("f"), b"x").unwrap();
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
