//! The pinned scratch directory the ROM build runs in.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::RomBuildError;

/// A private scratch directory, pinned by an open handle and removed when
/// dropped, only while its path still names that directory.
///
/// Holding only the pathname would let a same-user process rename the
/// directory away, plant another at the path, and have the drop delete it and
/// the later build steps run in it. So creation takes an unpredictable name
/// exclusively and opens the directory; [`Scratch::check`] fails unless the
/// path still names the held directory (device and inode on Unix, the file ID
/// on Windows), and the build calls it around every pathname-based stage.
///
/// Limits, since `std` has no `openat`/`unlinkat`: the check is a stage
/// boundary, not atomic with the operations between checks, and covers the
/// scratch root, not a `pokeemerald/` subtree swapped inside it. Residuals,
/// each bounded the way the scenario scratch (`scenario/session.rs`) bounds
/// its own:
/// - A replacement installed between the mkdir and the open is adopted only
///   if it is itself an empty, mode-0700 directory, which holds
///   nothing to lose.
/// - Cleanup refuses on any mismatch and removes nothing. On Linux it empties
///   the directory through `/proc/self/fd/<hold>`, so a swap after the last
///   check cannot redirect the recursive removal; the root itself goes through
///   an identity-checked, empty-only `remove_dir` by pathname (no parent
///   handle exists in `std`), so a swap in that last instant can cost an
///   empty replacement and never content.
/// - Other Unix targets have no path that names the held inode (macOS's
///   `/dev/fd/<n>` does not resolve below a directory), so the children are
///   removed by pathname. The identity is re-checked immediately before each
///   top-level child is removed, and `remove_dir_all` works from descriptors
///   below that child, so a swap can only redirect a single child removal
///   (one name, resolved in the replacement) inside the few instructions
///   between that check and its `unlink`/`rmdir`. Closing it needs
///   `openat`/`unlinkat` on the held descriptor, which `std` lacks and which
///   is the Cargo feature decision #2873 tracks.
///
/// Windows holds the directory without delete sharing, so it cannot be
/// renamed or replaced while held.
pub(super) struct Scratch {
    pub(super) path: PathBuf,
    /// `None` once the hold was released (Windows removal) or never proven ours.
    hold: Option<std::fs::File>,
}

impl Scratch {
    pub(super) fn create(root: &Path) -> Result<Self, RomBuildError> {
        // The exclusive mkdir is the only arbiter of uniqueness; the random
        // component keeps the name from being staged ahead of creation.
        loop {
            let path = root.join(format!(
                "pokeemerald-rs-rom-build-{}-{:016x}",
                std::process::id(),
                unpredictable()
            ));
            let builder = &mut std::fs::DirBuilder::new();
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(builder, 0o700);
            match builder.create(&path) {
                Ok(()) => {
                    return Self::claim(path.clone()).map_err(|err| {
                        // Empty-only, so a replacement loses nothing but itself.
                        let _ = std::fs::remove_dir(&path);
                        RomBuildError::Scratch(format!("{}: {err}", path.display()))
                    });
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
                Err(err) => {
                    return Err(RomBuildError::Scratch(format!("{}: {err}", path.display())));
                }
            }
        }
    }

    /// Pins the just-created directory and refuses anything it cannot show it
    /// created: it must be the entry at `path`, empty, and (Unix) mode 0700.
    /// Cleanup stays disarmed until that holds, so a populated directory
    /// planted at the name is never removed.
    pub(super) fn claim(path: PathBuf) -> std::io::Result<Self> {
        // A umask that strips owner bits (0o400 leaves 0o300) would make the
        // directory unopenable for the hold and fail the 0o700 check, so put
        // back the mode the umask withheld before the hold is taken. A claim
        // that is then refused puts the withheld mode back.
        #[cfg(unix)]
        let restored = restore_private_mode(&path)?;
        let proven = open_hold(&path).and_then(|hold| {
            let mut scratch = Self {
                path,
                hold: Some(hold),
            };
            let shown = scratch
                .require_path(&scratch.path)
                .and_then(|()| scratch.require_fresh());
            if shown.is_err() {
                // Disarm cleanup: the directory was not shown to be ours.
                scratch.hold = None;
            }
            shown.map(|()| scratch)
        });
        #[cfg(unix)]
        if proven.is_err() {
            if let Some(restored) = restored {
                restored.undo();
            }
        }
        proven
    }

    fn hold(&self) -> std::io::Result<&std::fs::File> {
        self.hold.as_ref().ok_or_else(|| {
            std::io::Error::other("the scratch directory's hold was already released")
        })
    }

    fn require_fresh(&self) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            // mkdir inherits a setgid parent's setgid bit, so 0o2700 is ours.
            let mode = self.hold()?.metadata()?.permissions().mode() & 0o5777;
            if mode != 0o700 {
                return Err(not_fresh(&self.path));
            }
        }
        if std::fs::read_dir(&self.path)?.next().is_some() {
            return Err(not_fresh(&self.path));
        }
        Ok(())
    }

    /// Fails unless `path` still names the held directory.
    fn require_path(&self, path: &Path) -> std::io::Result<()> {
        let hold = self.hold()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let pinned = hold.metadata()?;
            let found = std::fs::symlink_metadata(path)?;
            if !found.is_dir() || (pinned.dev(), pinned.ino()) != (found.dev(), found.ino()) {
                return Err(not_the_held_directory(path));
            }
        }
        #[cfg(windows)]
        {
            use win_fs::WindowsFileIdentity;
            let fresh = open_hold(path)?;
            if WindowsFileIdentity::of(&fresh)? != WindowsFileIdentity::of(hold)? {
                return Err(not_the_held_directory(path));
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (path, hold);
            return Err(ErrorKind::Unsupported.into());
        }
        #[cfg(any(unix, windows))]
        Ok(())
    }

    /// Fails unless the scratch path still names the directory created.
    pub(super) fn check(&self) -> Result<(), RomBuildError> {
        self.require_path(&self.path)
            .map_err(|err| RomBuildError::Scratch(format!("{}: {err}", self.path.display())))
    }

    /// The checkout directory, once the scratch root is shown to be ours.
    pub(super) fn tree(&self) -> Result<PathBuf, RomBuildError> {
        self.check()?;
        Ok(self.path.join("pokeemerald"))
    }

    /// Removes the scratch tree only if the path still names the hold;
    /// otherwise leaves whatever is there untouched.
    fn remove_all(&mut self) -> std::io::Result<()> {
        self.require_path(&self.path)?;
        #[cfg(unix)]
        {
            self.empty_through_hold()?;
            // Empty-only: a replacement winning this instant loses at most
            // itself if empty, never its contents.
            self.require_path(&self.path)?;
            std::fs::remove_dir(&self.path)
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileTypeExt as _;
            // The hold denies renaming or replacing the root while it empties;
            // a link is removed, never followed.
            for entry in std::fs::read_dir(&self.path)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_symlink_dir() {
                    std::fs::remove_dir(entry.path())?;
                } else if kind.is_dir() {
                    std::fs::remove_dir_all(entry.path())?;
                } else {
                    std::fs::remove_file(entry.path())?;
                }
            }
            // The hold denies deleting the root itself; release it for an
            // empty-only `remove_dir`.
            self.hold = None;
            std::fs::remove_dir(&self.path)
        }
        #[cfg(not(any(unix, windows)))]
        Err(ErrorKind::Unsupported.into())
    }

    #[cfg(unix)]
    fn empty_through_hold(&self) -> std::io::Result<()> {
        // On Linux the descriptor's `/proc` entry names the held inode itself,
        // so the children are reached through the hold, not the pathname.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let dir = {
            use std::os::fd::AsRawFd as _;
            PathBuf::from(format!("/proc/self/fd/{}", self.hold()?.as_raw_fd()))
        };
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        let dir = self.path.clone();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            // Without a path to the held inode, shrink the swap window to this
            // one child by proving the pathname still names the hold first.
            #[cfg(not(any(target_os = "linux", target_os = "android")))]
            self.require_path(&self.path)?;
            let child = dir.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                std::fs::remove_dir_all(child)?;
            } else {
                std::fs::remove_file(child)?;
            }
        }
        Ok(())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Best effort: a leftover temp directory must not mask the result.
        if self.hold.is_some() {
            let _ = self.remove_all();
        }
    }
}

/// A directory's mode before [`restore_private_mode`] raised it, with the pin
/// that names that inode, so a refused claim can put the mode back on the
/// directory it changed even if the path no longer names it.
#[cfg(unix)]
pub(super) struct ModeRestore {
    pin: std::fs::File,
    original: u32,
}

#[cfg(unix)]
impl ModeRestore {
    /// Best effort: a failure leaves the directory at 0o700, which only the
    /// owner can open.
    fn undo(&self) {
        use std::os::fd::AsRawFd as _;
        use std::os::unix::fs::PermissionsExt as _;
        let via = format!("/proc/self/fd/{}", self.pin.as_raw_fd());
        let _ = std::fs::set_permissions(via, std::fs::Permissions::from_mode(self.original));
    }
}

#[cfg(unix)]
fn restore_private_mode(path: &Path) -> std::io::Result<Option<ModeRestore>> {
    restore_private_mode_with(path, || {})
}

/// Sets the directory at `path` to 0o700 if its mode differs, without ever
/// following a symlink and only on a directory this build could have created.
///
/// `mkdir(0o700)` can only yield a subset of 0o700 (plus an inherited setgid
/// bit), so an entry with any other bit is left untouched and refused. Linux
/// pins the entry with `O_PATH` (which needs no permission bits) and decides
/// everything on the pinned inode: it must be a directory, its own mode must
/// be one `mkdir` could have produced, and it must be the entry at `path`; the
/// change then goes through `/proc/self/fd/<pin>`, which names that inode
/// itself. So a different directory swapped in at `path` after the first look
/// is refused on its own mode and never changed. Elsewhere there is no
/// permission-free pin through `std`, so a directory the umask left without
/// owner read or search is refused rather than restored by pathname.
///
/// Returns what [`ModeRestore::undo`] needs when the mode was changed.
/// `after_first_look` runs between the pathname look and the pin, where a
/// test places the swap.
#[cfg(unix)]
pub(super) fn restore_private_mode_with(
    path: &Path,
    after_first_look: impl FnOnce(),
) -> std::io::Result<Option<ModeRestore>> {
    use std::os::fd::AsRawFd as _;
    use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
    let found = std::fs::symlink_metadata(path)?;
    let mode = found.mode() & 0o7777;
    if !found.is_dir() || mode & 0o5077 != 0 {
        return Err(not_fresh(path));
    }
    if mode & 0o700 == 0o700 {
        return Ok(None);
    }
    let Some(o_path) = O_PATH else {
        return Err(std::io::Error::other(format!(
            "the scratch directory was created mode {mode:o}: the process umask withheld \
             owner read or search, and this platform cannot restore the mode safely"
        )));
    };
    after_first_look();
    let pinned = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(o_path)
        .open(path)?;
    let held = pinned.metadata()?;
    let original = held.mode() & 0o7777;
    if !held.is_dir() || original & 0o5077 != 0 {
        return Err(not_fresh(path));
    }
    let again = std::fs::symlink_metadata(path)?;
    if !again.is_dir() || (held.dev(), held.ino()) != (again.dev(), again.ino()) {
        return Err(not_the_held_directory(path));
    }
    if original & 0o700 == 0o700 {
        return Ok(None);
    }
    let restore = ModeRestore {
        pin: pinned,
        original,
    };
    let via = format!("/proc/self/fd/{}", restore.pin.as_raw_fd());
    std::fs::set_permissions(via, std::fs::Permissions::from_mode(0o700))?;
    Ok(Some(restore))
}

/// `O_PATH` on Linux for the architectures whose value is the generic one;
/// elsewhere there is no pin and the mode is not restored.
#[cfg(unix)]
const O_PATH: Option<i32> = if cfg!(all(
    any(target_os = "linux", target_os = "android"),
    any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "powerpc64",
        target_arch = "s390x"
    )
)) {
    Some(0o1000_0000)
} else {
    None
};

/// Opens `path` as the scratch hold. Unix opens non-blocking so a FIFO swapped
/// in at the name cannot stall the open; the claim then refuses it.
#[cfg(unix)]
fn open_hold(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    if let Some(flag) = super::O_NONBLOCK {
        options.custom_flags(flag);
    }
    options.open(path)
}

#[cfg(windows)]
fn open_hold(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    // Read and write sharing only: nothing may rename or delete the directory
    // while it is held, yet the build can still open files inside it.
    const SHARE_READ_WRITE: u32 = 0x1 | 0x2;
    // Real access, not zero: Windows checks sharing only against handles that
    // hold access, so a zero-access hold would not deny rename or delete.
    const FILE_LIST_DIRECTORY: u32 = 0x1;
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    let hold = std::fs::OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(SHARE_READ_WRITE)
        .open(path)?;
    let found = hold.metadata()?;
    if !found.is_dir() || found.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(std::io::Error::other("the scratch directory was replaced"));
    }
    Ok(hold)
}

#[cfg(not(any(unix, windows)))]
fn open_hold(_path: &Path) -> std::io::Result<std::fs::File> {
    Err(ErrorKind::Unsupported.into())
}

fn not_fresh(path: &Path) -> std::io::Error {
    std::io::Error::other(format!(
        "the directory at {} is not the empty private directory this build created",
        path.display()
    ))
}

fn not_the_held_directory(path: &Path) -> std::io::Error {
    std::io::Error::other(format!(
        "the path {} no longer names the scratch directory this build created",
        path.display()
    ))
}

/// Eight bytes a swapper cannot predict: a `SipHash` with keys std seeds from
/// the operating system's random source, on every platform.
fn unpredictable() -> u64 {
    use std::hash::{BuildHasher as _, Hasher as _};
    std::hash::RandomState::new().build_hasher().finish()
}
