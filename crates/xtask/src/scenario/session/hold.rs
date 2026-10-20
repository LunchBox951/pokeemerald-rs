//! The Unix and Windows halves of the scratch hold: opening and pinning the
//! directory, restoring its mode under a restrictive umask, and emptying it
//! through the descriptor.

use std::path::Path;

/// Sets the directory at `path` to 0o700 if its mode differs, without ever
/// following a symlink, and only on a directory this session could have
/// created: Linux pins the entry with `O_PATH | O_NOFOLLOW | O_DIRECTORY`
/// (which needs no permission bits) and changes the mode of that pinned
/// inode. Other Unixes never change the mode (see their variant below).
///
/// `mkdir(0o700)` can only yield a subset of 0o700 (plus an inherited
/// setgid bit), so an entry with any other bit is left untouched and refused.
/// Emptiness can be read only once owner read is back, so a non-empty entry
/// has its original mode put back and is refused. The owner is not compared
/// (no `geteuid` among the approved `rustix` features).
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) fn restore_private_mode(path: &Path) -> std::io::Result<()> {
    use std::os::fd::AsRawFd as _;
    let pinned = rustix::fs::open(
        path,
        rustix::fs::OFlags::PATH
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let stat = rustix::fs::fstat(&pinned)?;
    if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::Directory {
        return Err(not_created_here());
    }
    let mode = stat.st_mode & 0o7777;
    if mode & 0o777 == 0o700 {
        return Ok(());
    }
    if mode & 0o5077 != 0 {
        return Err(not_created_here());
    }
    // `fchmod` refuses an `O_PATH` descriptor, but its `/proc` entry names
    // the pinned inode itself, so neither change can reach a replacement.
    let pinned_path = format!("/proc/self/fd/{}", pinned.as_raw_fd());
    let chmod = |mode| {
        rustix::fs::chmodat(
            rustix::fs::CWD,
            pinned_path.as_str(),
            mode,
            rustix::fs::AtFlags::empty(),
        )
    };
    chmod(rustix::fs::Mode::RWXU)?;
    let readable = rustix::fs::open(
        pinned_path.as_str(),
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    if !is_empty(&readable)? {
        chmod(rustix::fs::Mode::from_raw_mode(mode))?;
        return Err(not_created_here());
    }
    Ok(())
}

/// Beyond Linux there is no permission-free directory pin through the
/// approved `rustix` surface, so nothing here changes a mode: a directory that
/// already has owner rwx (and no bits beyond 0o700 and setgid) proceeds to the
/// claim, and one the umask left without owner read or search is refused
/// rather than restored by pathname (#2475).
#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
pub(super) fn restore_private_mode(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let found = std::fs::symlink_metadata(path)?;
    let mode = found.permissions().mode() & 0o7777;
    if !found.is_dir() || mode & 0o5077 != 0 {
        return Err(not_created_here());
    }
    if mode & 0o777 != 0o700 {
        return Err(std::io::Error::other(format!(
            "the scratch directory was created mode {mode:o}: the process umask withheld owner \
             read or search, and this platform cannot restore the mode safely (#2475)"
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn not_created_here() -> std::io::Error {
    std::io::Error::other("the scratch entry is not a directory this session created")
}

#[cfg(unix)]
pub(super) fn open_hold(path: &Path) -> std::io::Result<std::fs::File> {
    rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map(std::fs::File::from)
    .map_err(Into::into)
}

#[cfg(windows)]
pub(super) fn open_hold(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    // Read and write sharing only: nothing may rename or delete the directory
    // while it is held, yet the engine can still open files inside it.
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
pub(super) fn open_hold(_path: &Path) -> std::io::Result<std::fs::File> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// Empties `dir` through its descriptor: every entry is unlinked relative to
/// its held parent and subdirectories are opened `NOFOLLOW`, so no pathname
/// is ever re-resolved and a symlink is unlinked, not followed. Each entry
/// must still be the inode the listing named, or cleanup stops; for a
/// non-directory that is a check-then-unlink window (there is no unlink by
/// descriptor), bounded by the hold pinning the parent.
#[cfg(unix)]
pub(super) fn remove_contents(
    dir: &std::fs::File,
    before_remove: &mut dyn FnMut(&std::ffi::CStr),
) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;
    let dev = dir.metadata()?.dev();
    let mut names = Vec::new();
    for entry in rustix::fs::Dir::read_from(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_owned();
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            names.push((name, entry.file_type(), entry.ino()));
        }
    }
    for (name, kind, ino) in names {
        // `DT_UNKNOWN` filesystems report no type; ask without following links.
        let kind = if kind == rustix::fs::FileType::Unknown {
            rustix::fs::FileType::from_raw_mode(
                rustix::fs::statat(dir, name.as_c_str(), rustix::fs::AtFlags::SYMLINK_NOFOLLOW)?
                    .st_mode,
            )
        } else {
            kind
        };
        if kind == rustix::fs::FileType::Directory {
            // A test seam: the instant between the listing and the open.
            before_remove(name.as_c_str());
            let open = || {
                rustix::fs::openat(
                    dir,
                    name.as_c_str(),
                    rustix::fs::OFlags::RDONLY
                        | rustix::fs::OFlags::DIRECTORY
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )
            };
            let child = match open() {
                Ok(child) => child,
                Err(rustix::io::Errno::ACCESS) => {
                    // An engine created under a restrictive umask leaves a
                    // directory its owner cannot read; restore owner access to
                    // that exact listed inode and open it through its pin, so
                    // the name is never resolved again.
                    restore_owner_access(dir, &name, dev, ino)?
                }
                Err(error) => return Err(error.into()),
            };
            let child = std::fs::File::from(child);
            require_entry(&child, dev, ino)?;
            remove_contents(&child, before_remove)?;
            rustix::fs::unlinkat(dir, name.as_c_str(), rustix::fs::AtFlags::REMOVEDIR)?;
        } else {
            before_remove(name.as_c_str());
            let found =
                rustix::fs::statat(dir, name.as_c_str(), rustix::fs::AtFlags::SYMLINK_NOFOLLOW)?;
            #[allow(
                clippy::unnecessary_cast,
                clippy::cast_sign_loss,
                reason = "`st_dev` is `u64` on Linux and `i32` on macOS"
            )]
            if (found.st_dev as u64, found.st_ino as u64) != (dev, ino) {
                return Err(std::io::Error::other(
                    "a scratch entry was replaced during cleanup",
                ));
            }
            rustix::fs::unlinkat(dir, name.as_c_str(), rustix::fs::AtFlags::empty())?;
        }
    }
    Ok(())
}

/// Restores owner rwx on the listed subdirectory `name` of `dir` after
/// confirming, through a path-only descriptor, that it is still the inode the
/// listing named, so a foreign directory swapped in is never chmod-ed, then
/// opens it for reading through that same pin while the pin is still held,
/// so the returned descriptor is the verified inode and never a replacement
/// installed at `name` after the chmod (even one reusing its inode number).
/// Linux only: elsewhere there is no way to change the mode of the verified
/// inode itself (a path-based chmod could hit a replacement), so recovery is
/// refused and the open's `EACCES` stands.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) fn restore_owner_access(
    dir: &std::fs::File,
    name: &std::ffi::CStr,
    dev: u64,
    ino: u64,
) -> std::io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::AsRawFd as _;
    use std::os::unix::fs::PermissionsExt as _;
    let pinned = rustix::fs::openat(
        dir,
        name,
        rustix::fs::OFlags::PATH
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let pinned = std::fs::File::from(pinned);
    require_entry(&pinned, dev, ino)?;
    // Both the `chmod` and the open follow the `/proc` link to the pinned
    // inode itself; `NOFOLLOW` would refuse that link, and is not needed.
    let pinned_path = format!("/proc/self/fd/{}", pinned.as_raw_fd());
    std::fs::set_permissions(&pinned_path, std::fs::Permissions::from_mode(0o700))?;
    let readable = rustix::fs::open(
        pinned_path.as_str(),
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    drop(pinned);
    Ok(readable)
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
fn restore_owner_access(
    _dir: &std::fs::File,
    _name: &std::ffi::CStr,
    _dev: u64,
    _ino: u64,
) -> std::io::Result<std::os::fd::OwnedFd> {
    Err(std::io::ErrorKind::PermissionDenied.into())
}

/// Fails unless `child` is the inode its parent's listing named, so a
/// directory swapped in after the listing is never descended into.
#[cfg(unix)]
fn require_entry(child: &std::fs::File, dev: u64, ino: u64) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;
    let found = child.metadata()?;
    if (found.dev(), found.ino()) != (dev, ino) {
        return Err(std::io::Error::other(
            "a scratch subdirectory was replaced during cleanup",
        ));
    }
    Ok(())
}

/// Whether the directory `dir` has no entries but `.` and `..`.
#[cfg(unix)]
pub(super) fn is_empty(dir: impl std::os::fd::AsFd) -> std::io::Result<bool> {
    for entry in rustix::fs::Dir::read_from(dir)? {
        let entry = entry?;
        if entry.file_name().to_bytes() != b"." && entry.file_name().to_bytes() != b".." {
            return Ok(false);
        }
    }
    Ok(true)
}
