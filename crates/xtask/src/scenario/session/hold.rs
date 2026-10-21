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

/// Empties `dir` (held as `dir`, named by `dir_path`) with a handle-based walk,
/// the Windows counterpart of the Unix `remove_contents`. The volume must
/// support opening by file ID (NTFS and `ReFS` do; FAT and exFAT, whose IDs
/// follow directory-entry slots, do not), or nothing is removed. The entries
/// are listed through the held handle (`FileIdExtdDirectoryInfo`, which
/// carries each entry's full 128-bit file ID; the 64-bit form is not unique on
/// `ReFS`). Each entry is then opened by name under the held directory with
/// `DELETE` access, no delete sharing and without following a reparse point,
/// and is refused unless the opened handle's 128-bit file ID (`FileIdInfo`) is
/// the one the listing named. Deletion goes through that verified handle and
/// subdirectories recurse through their own handles, which keep them from
/// being renamed while they are emptied. The name is resolved once, so a
/// swapped-in entry fails the ID check and is left alone. `OpenFileById` is
/// not used: it sits behind `windows-sys`'s `Win32_Security` feature, outside
/// the approved `Win32_Storage_FileSystem` surface.
#[cfg(windows)]
pub(super) fn remove_contents(
    dir: &std::fs::File,
    dir_path: &Path,
    before_remove: &mut dyn FnMut(&std::ffi::OsStr),
) -> std::io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };
    const DELETE: u32 = 0x0001_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    const FILE_LIST_DIRECTORY: u32 = 0x1;
    const SHARE_READ_WRITE: u32 = 0x1 | 0x2;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    require_unique_file_ids(dir)?;
    for (name, listed_id, attributes) in list_entries(dir)? {
        // A test seam: the instant between the listing and the open.
        before_remove(&name);
        let child_path = dir_path.join(&name);
        let child = std::fs::OpenOptions::new()
            .access_mode(DELETE | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY)
            .share_mode(SHARE_READ_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&child_path)?;
        if file_id(&child)? != listed_id {
            return Err(std::io::Error::other(
                "a scratch entry was replaced during cleanup",
            ));
        }
        let is_real_dir = attributes & FILE_ATTRIBUTE_DIRECTORY != 0
            && attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0;
        if is_real_dir {
            remove_contents(&child, &child_path, before_remove)?;
        }
        mark_for_deletion(&child)?;
        // The entry goes when this handle closes (or, after the legacy
        // fallback, when the last handle to it closes).
        drop(child);
    }
    Ok(())
}

/// Refuses a volume that cannot open a file by its ID: only there is a file ID
/// a stable identity rather than a directory-entry position that a
/// replacement can reuse. The claim runs this before the session writes
/// anything, and cleanup runs it again before it removes anything.
#[cfg(windows)]
pub(super) fn require_unique_file_ids(dir: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationByHandleW;
    const FILE_SUPPORTS_OPEN_BY_FILE_ID: u32 = 0x0100_0000;
    let mut flags = 0u32;
    // SAFETY: `dir` stays open for this call, `flags` is an out parameter, and
    // every other buffer is null with a zero size.
    let ok = unsafe {
        GetVolumeInformationByHandleW(
            dir.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut flags,
            std::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    if flags & FILE_SUPPORTS_OPEN_BY_FILE_ID == 0 {
        return Err(std::io::Error::other(
            "the scratch volume has no stable file IDs to verify cleanup against (FAT or \
             exFAT); point TEMP at an NTFS or ReFS volume",
        ));
    }
    Ok(())
}

/// The entries of `dir` other than `.` and `..` as (name, 128-bit file ID,
/// attributes), listed through the handle itself.
#[cfg(windows)]
fn list_entries(dir: &std::fs::File) -> std::io::Result<Vec<(std::ffi::OsString, [u8; 16], u32)>> {
    use std::os::windows::ffi::OsStringExt as _;
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Foundation::ERROR_NO_MORE_FILES;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdExtdDirectoryInfo, FileIdExtdDirectoryRestartInfo, GetFileInformationByHandleEx,
        FILE_ID_EXTD_DIR_INFO,
    };
    // `u64` elements keep the buffer 8-aligned, as the entries require.
    let mut buffer = vec![0u64; 8192];
    let size = u32::try_from(buffer.len() * 8).expect("64 KiB fits in u32");
    let mut class = FileIdExtdDirectoryRestartInfo;
    let mut entries = Vec::new();
    loop {
        // SAFETY: `dir` stays open for this call and `buffer` is `size` bytes.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                dir.as_raw_handle(),
                class,
                buffer.as_mut_ptr().cast(),
                size,
            )
        };
        if ok == 0 {
            let error = std::io::Error::last_os_error();
            if error
                .raw_os_error()
                .and_then(|code| u32::try_from(code).ok())
                == Some(ERROR_NO_MORE_FILES)
            {
                return Ok(entries);
            }
            return Err(error);
        }
        class = FileIdExtdDirectoryInfo;
        let base = buffer.as_ptr().cast::<u8>();
        let mut offset = 0usize;
        loop {
            #[allow(
                clippy::cast_ptr_alignment,
                reason = "the buffer is 8-aligned and the system 8-aligns each record"
            )]
            // SAFETY: the call above filled `buffer` with a chain of
            // `FILE_ID_EXTD_DIR_INFO` records, each 8-aligned and ending
            // within it; `NextEntryOffset` is zero on the last one.
            let (next, attributes, id, name) = unsafe {
                let record = base.add(offset).cast::<FILE_ID_EXTD_DIR_INFO>();
                let name_at = base
                    .add(offset + std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileName))
                    .cast::<u16>();
                let units = usize::try_from((*record).FileNameLength).unwrap_or(0) / 2;
                (
                    (*record).NextEntryOffset,
                    (*record).FileAttributes,
                    (*record).FileId.Identifier,
                    std::ffi::OsString::from_wide(std::slice::from_raw_parts(name_at, units)),
                )
            };
            if name != "." && name != ".." {
                entries.push((name, id, attributes));
            }
            if next == 0 {
                break;
            }
            offset += usize::try_from(next).unwrap_or(usize::MAX);
        }
    }
}

/// The 128-bit file ID of the object `handle` refers to, the same ID a
/// `FileIdExtdDirectoryInfo` listing reports for that entry.
#[cfg(windows)]
fn file_id(handle: &std::fs::File) -> std::io::Result<[u8; 16]> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandleEx, FILE_ID_INFO,
    };
    let mut info = FILE_ID_INFO::default();
    let size = u32::try_from(std::mem::size_of::<FILE_ID_INFO>())
        .expect("FILE_ID_INFO is far smaller than u32::MAX");
    // SAFETY: `handle` stays open for this call, `FileIdInfo` fills a
    // `FILE_ID_INFO`, and `size` is that structure's exact size.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle.as_raw_handle(),
            FileIdInfo,
            std::ptr::from_mut(&mut info).cast(),
            size,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(info.FileId.Identifier)
}

/// Deletes the object `handle` refers to through `handle` itself. POSIX
/// semantics (`FileDispositionInfoEx`, ignoring the read-only attribute) is
/// tried first, so the name leaves the directory when `handle` closes even if
/// another opener (a scanner, say) still holds the object, and its parent can
/// then be removed. A volume or Windows release without it answers
/// `ERROR_NOT_SUPPORTED`, `ERROR_INVALID_PARAMETER`, or
/// `ERROR_INVALID_FUNCTION`, and falls back to the plain `FileDispositionInfo`
/// delete-on-close, which another open handle can keep pending.
#[cfg(windows)]
fn mark_for_deletion(handle: &std::fs::File) -> std::io::Result<()> {
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FileDispositionInfo, FileDispositionInfoEx, FILE_DISPOSITION_FLAG_DELETE,
        FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX,
    };
    let posix = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    };
    // SAFETY: `FileDispositionInfoEx` takes a `FILE_DISPOSITION_INFO_EX`, which `posix` is.
    match unsafe { set_file_information(handle, FileDispositionInfoEx, &posix) } {
        Ok(()) => return Ok(()),
        Err(refused)
            if refused
                .raw_os_error()
                .and_then(|code| u32::try_from(code).ok())
                .is_some_and(|code| {
                    matches!(
                        code,
                        ERROR_NOT_SUPPORTED | ERROR_INVALID_PARAMETER | ERROR_INVALID_FUNCTION
                    )
                }) => {}
        Err(failed) => return Err(failed),
    }
    let legacy = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: `FileDispositionInfo` takes a `FILE_DISPOSITION_INFO`, which `legacy` is.
    unsafe { set_file_information(handle, FileDispositionInfo, &legacy) }
}

/// `SetFileInformationByHandle` on `handle` with `info` as the buffer.
///
/// # Safety
///
/// `T` must be the structure `class` documents as its buffer.
#[cfg(windows)]
unsafe fn set_file_information<T>(
    handle: &std::fs::File,
    class: windows_sys::Win32::Storage::FileSystem::FILE_INFO_BY_HANDLE_CLASS,
    info: &T,
) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::SetFileInformationByHandle;
    let size = u32::try_from(std::mem::size_of::<T>())
        .expect("a file-information structure is far smaller than u32::MAX");
    // SAFETY: `handle` stays open for the call, the caller guarantees `T`
    // matches `class`, and `size` is `T`'s exact size.
    let ok = unsafe {
        SetFileInformationByHandle(
            handle.as_raw_handle(),
            class,
            std::ptr::from_ref(info).cast(),
            size,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
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
fn restore_owner_access(
    dir: &std::fs::File,
    name: &std::ffi::CStr,
    dev: u64,
    ino: u64,
) -> std::io::Result<std::os::fd::OwnedFd> {
    restore_owner_access_with(dir, name, dev, ino, || {})
}

/// [`restore_owner_access`] with a hook run between the chmod and the
/// reopen, so a test can install a replacement at `name` in exactly that
/// window and check that the returned descriptor is still the pinned inode.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) fn restore_owner_access_with(
    dir: &std::fs::File,
    name: &std::ffi::CStr,
    dev: u64,
    ino: u64,
    after_chmod: impl FnOnce(),
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
    after_chmod();
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
