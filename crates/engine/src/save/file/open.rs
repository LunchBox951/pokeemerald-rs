//! Decides whether an entry at a save or lock path is a plain file this
//! module may open, and opens it so a swap after that decision cannot
//! change the answer.
//!
//! [`SaveFile::read`](super::SaveFile::read) and the lock slot share this
//! one policy: a pre-open check for a fast, typed refusal, open flags that
//! neither follow a symlink nor block on a FIFO, and a check of the opened
//! handle itself. On Windows, [`SaveFile::read`] departs from that shared
//! policy in one way: [`open_verified_for_read`] lets a non-symlink reparse
//! point (a cloud-files placeholder, an app-execution alias, a deduplicated
//! file) through to its filter instead of refusing it, verified by
//! [`WindowsFileIdentity`] rather than by [`refuse_an_unusable_kind`]'s
//! blanket rule. The lock slot keeps that blanket rule; see
//! [`open_verified_for_read`]'s own docs for why the save path does not.

use std::path::Path;

/// What is wrong with an entry [`refuse_an_unusable_entry`] inspected, for
/// the caller to fold into whichever [`SaveFileError`](super::SaveFileError) variant fits its own
/// path (a save path or a lock slot).
#[derive(Debug)]
pub(super) enum UnusableEntry {
    /// The entry could not be inspected at all.
    Inspect(std::io::Error),
    /// The entry is a symlink instead of naming the file it opens.
    IsAlias,
    /// The entry exists and is not a symlink, but is not a plain file
    /// either -- a directory, socket, device, FIFO, or another kind of
    /// Windows reparse point.
    NotAPlainFile,
}

/// Refuses a symlinked or non-plain-file entry before anything opens it,
/// since the open would follow the link or block on a FIFO. A missing entry
/// is fine: the caller's own open reports that in its own way.
///
/// This is the one policy [`SaveFile::read`](super::SaveFile::read) and the lock slot's
/// `open_lock_slot` share for "what counts as a plain file here", so the two
/// can never quietly disagree.
///
/// # Errors
///
/// [`UnusableEntry::IsAlias`] if `path` is a symlink; [`UnusableEntry::NotAPlainFile`]
/// if it is anything else that is not a plain file; [`UnusableEntry::Inspect`]
/// if it could not be inspected.
pub(super) fn refuse_an_unusable_entry(path: &Path) -> Result<(), UnusableEntry> {
    let entry = match std::fs::symlink_metadata(path) {
        Ok(entry) => entry,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(UnusableEntry::Inspect(source)),
    };
    refuse_an_unusable_kind(&entry)
}

/// The one classification both [`refuse_an_unusable_entry`] and
/// [`refuse_an_unusable_open`] apply to what they inspected -- every
/// caller's policy except [`open_verified_for_read`]'s (Windows only; see
/// that function).
///
/// On Windows `std` calls only a name-surrogate reparse point (a symlink or
/// junction) a symlink, so a cloud-files placeholder, an app-execution
/// alias, or a deduplicated file reports as a plain file. Opened with
/// [`refuse_unusable_opens`]'s flags, such an entry would be read as its
/// raw reparse object, not the data its filter serves, so any reparse point
/// that is not a symlink is refused too.
fn refuse_an_unusable_kind(metadata: &std::fs::Metadata) -> Result<(), UnusableEntry> {
    if metadata.is_symlink() {
        Err(UnusableEntry::IsAlias)
    } else if metadata.is_file() && !is_reparse_point(metadata) {
        Ok(())
    } else {
        Err(UnusableEntry::NotAPlainFile)
    }
}

/// Whether `metadata` carries Windows's reparse-point attribute; never true
/// elsewhere.
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_attributes() & open_flags::FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

/// `open(2)`/`CreateFileW` flag and errno values `std` has no portable name
/// for, needed so the open itself -- not just the [`refuse_an_unusable_entry`]
/// check before it -- refuses a symlinked final component and never blocks
/// on a FIFO's other end.
#[cfg(unix)]
mod open_flags {
    // Linux's generic `<asm-generic/fcntl.h>`: `O_NOFOLLOW` is `0x20000`,
    // unmodified outside the arm/aarch64/powerpc/powerpc64/m68k override below.
    #[cfg(all(
        target_os = "linux",
        not(any(
            target_arch = "arm",
            target_arch = "aarch64",
            target_arch = "powerpc",
            target_arch = "powerpc64",
            target_arch = "m68k"
        ))
    ))]
    pub(super) const O_NOFOLLOW: i32 = 0x0002_0000;
    // `<asm/fcntl.h>` puts `O_NOFOLLOW` at `0x8000` on these: arm, aarch64,
    // and m68k swap it with `O_LARGEFILE`; powerpc has `O_DIRECT` at `0x20000`.
    #[cfg(all(
        target_os = "linux",
        any(
            target_arch = "arm",
            target_arch = "aarch64",
            target_arch = "powerpc",
            target_arch = "powerpc64",
            target_arch = "m68k"
        )
    ))]
    pub(super) const O_NOFOLLOW: i32 = 0x0000_8000;
    // Generic Linux `<asm-generic/fcntl.h>` and `<asm-generic/errno.h>`;
    // MIPS and SPARC use their own `<asm/fcntl.h>` and `<asm/errno.h>`.
    #[cfg(all(
        target_os = "linux",
        not(any(
            target_arch = "mips",
            target_arch = "mips32r6",
            target_arch = "mips64",
            target_arch = "mips64r6",
            target_arch = "sparc",
            target_arch = "sparc64"
        ))
    ))]
    pub(super) const O_NONBLOCK: i32 = 0x0000_0800;
    #[cfg(all(
        target_os = "linux",
        not(any(
            target_arch = "mips",
            target_arch = "mips32r6",
            target_arch = "mips64",
            target_arch = "mips64r6",
            target_arch = "sparc",
            target_arch = "sparc64"
        ))
    ))]
    pub(super) const ELOOP: i32 = 40;
    // MIPS's `<asm/fcntl.h>` and `<asm/errno.h>`, r6 variants included.
    #[cfg(all(
        target_os = "linux",
        any(
            target_arch = "mips",
            target_arch = "mips32r6",
            target_arch = "mips64",
            target_arch = "mips64r6"
        )
    ))]
    pub(super) const O_NONBLOCK: i32 = 0x0000_0080;
    #[cfg(all(
        target_os = "linux",
        any(
            target_arch = "mips",
            target_arch = "mips32r6",
            target_arch = "mips64",
            target_arch = "mips64r6"
        )
    ))]
    pub(super) const ELOOP: i32 = 90;
    // SPARC's `<asm/fcntl.h>` and `<asm/errno.h>`.
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "sparc", target_arch = "sparc64")
    ))]
    pub(super) const O_NONBLOCK: i32 = 0x0000_4000;
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "sparc", target_arch = "sparc64")
    ))]
    pub(super) const ELOOP: i32 = 62;

    // Pins x86_64 to the generic branch and aarch64 to the override branch
    // so a misrouted arch fails compilation instead of using the wrong flag.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const _: () = assert!(O_NOFOLLOW == 0x0002_0000);
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    const _: () = assert!(O_NOFOLLOW == 0x0000_8000);
    // Pins x86_64 to the generic branch and mips64 and sparc64 to their
    // override branches so a misrouted arch fails compilation instead of
    // using the wrong flag.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const _: () = assert!(O_NONBLOCK == 0x0000_0800 && ELOOP == 40);
    #[cfg(all(target_os = "linux", target_arch = "mips64"))]
    const _: () = assert!(O_NONBLOCK == 0x0000_0080 && ELOOP == 90);
    #[cfg(all(target_os = "linux", target_arch = "sparc64"))]
    const _: () = assert!(O_NONBLOCK == 0x0000_4000 && ELOOP == 62);

    // macOS's and the BSDs' shared `<sys/fcntl.h>` and `<sys/errno.h>`.
    #[cfg(not(target_os = "linux"))]
    pub(super) const O_NOFOLLOW: i32 = 0x0000_0100;
    #[cfg(not(target_os = "linux"))]
    pub(super) const O_NONBLOCK: i32 = 0x0000_0004;
    #[cfg(not(target_os = "linux"))]
    pub(super) const ELOOP: i32 = 62;
}

/// Windows's `<winbase.h>` and `<winnt.h>`.
#[cfg(windows)]
mod open_flags {
    /// Opens a reparse point (a symlink, among others) itself rather than
    /// following it.
    pub(super) const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    /// Marks an entry as a reparse point of any tag.
    pub(super) const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
}

/// Adds the flags that close the window between [`refuse_an_unusable_entry`]
/// and the open it precedes: an entry swapped for a symlink in that window
/// can no longer be followed, and a FIFO swapped in cannot block the open.
pub(super) fn refuse_unusable_opens(options: &mut std::fs::OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(open_flags::O_NOFOLLOW | open_flags::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(open_flags::FILE_FLAG_OPEN_REPARSE_POINT);
    }
}

/// Whether `error` is [`refuse_unusable_opens`]'s flags refusing to follow a
/// symlinked final component.
pub(super) fn open_refused_a_symlink(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(open_flags::ELOOP)
    }
    #[cfg(not(unix))]
    {
        // Windows opens the reparse point itself instead of failing the
        // open; `refuse_an_unusable_open` catches it from the handle.
        let _ = error;
        false
    }
}

/// Refuses an opened handle that is not a plain file after all: the one
/// case [`refuse_unusable_opens`]'s flags cannot themselves refuse on
/// Windows (a reparse point, opened rather than followed), or anything
/// [`refuse_an_unusable_entry`] could have missed because the entry changed
/// between that check and this open.
pub(super) fn refuse_an_unusable_open(file: &std::fs::File) -> Result<(), UnusableEntry> {
    let opened = file.metadata().map_err(UnusableEntry::Inspect)?;
    refuse_an_unusable_kind(&opened)
}

/// The identity Windows reads off an open handle: volume serial number and
/// file ID, the one per-object identity it exposes to a handle regardless of
/// which path was used to open it (`std`'s equivalent,
/// `MetadataExt::volume_serial_number`/`file_index`, is still gated on the
/// unstable `windows_by_handle`, rust-lang/rust#63010). Shared by
/// [`open_verified_for_read`] and the staged-save cleanup delete in
/// `staging` (issue #1132), each comparing it across two opens of what
/// ought to be the same object.
///
/// The 128-bit ID `GetFileInformationByHandleEx(FileIdInfo)` returns is
/// read first: `GetFileInformationByHandle`'s 64-bit file index is not
/// guaranteed unique on `ReFS` (Dev Drive's file system), whose documented
/// unique identifier is the 128-bit one. Only a file system that rejects
/// `FileIdInfo` falls back to the 64-bit index, zero-extended, which is how
/// NTFS fills the 128-bit form anyway.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WindowsFileIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

#[cfg(windows)]
impl WindowsFileIdentity {
    /// Reads the identity of the object `handle` refers to: the 128-bit
    /// `FileIdInfo` form, or the 64-bit index where the file system rejects
    /// that class (`ERROR_INVALID_PARAMETER`, `ERROR_NOT_SUPPORTED`, or
    /// `ERROR_INVALID_FUNCTION`). Any other failure surfaces.
    pub(super) fn of(handle: &std::fs::File) -> std::io::Result<Self> {
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::Foundation::{
            ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            FileIdInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
            BY_HANDLE_FILE_INFORMATION, FILE_ID_INFO,
        };

        let mut wide = FILE_ID_INFO::default();
        let size = u32::try_from(std::mem::size_of::<FILE_ID_INFO>())
            .expect("FILE_ID_INFO is far smaller than u32::MAX");
        // SAFETY: `handle` stays open for this call, `FileIdInfo` takes a `FILE_ID_INFO`, and `size` is that structure's exact size.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                handle.as_raw_handle(),
                FileIdInfo,
                std::ptr::from_mut(&mut wide).cast(),
                size,
            )
        };
        if ok != 0 {
            return Ok(Self {
                volume_serial_number: wide.VolumeSerialNumber,
                file_id: wide.FileId.Identifier,
            });
        }
        let refused = std::io::Error::last_os_error();
        let unsupported = refused
            .raw_os_error()
            .and_then(|code| u32::try_from(code).ok())
            .is_some_and(|code| {
                matches!(
                    code,
                    ERROR_NOT_SUPPORTED | ERROR_INVALID_PARAMETER | ERROR_INVALID_FUNCTION
                )
            });
        if !unsupported {
            return Err(refused);
        }

        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `handle` stays open for this call, and `info` is a correctly-sized out parameter the API fills in place.
        let ok = unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &raw mut info) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
        let mut file_id = [0u8; 16];
        file_id[..8].copy_from_slice(&index.to_le_bytes());
        Ok(Self {
            volume_serial_number: u64::from(info.dwVolumeSerialNumber),
            file_id,
        })
    }
}

/// What [`open_verified_for_read`] found wrong with the save path: either of
/// [`UnusableEntry`]'s cases, or the one extra case only a verified read can
/// detect.
#[cfg(windows)]
#[derive(Debug)]
pub(super) enum VerifiedReadError {
    /// As [`UnusableEntry`] classifies it.
    Unusable(UnusableEntry),
    /// [`reopen_through_filter`]'s handle names a different object than the
    /// one [`open_verified_for_read`] verified. `ReOpenFile` resolves
    /// against that object directly rather than a fresh lookup of `path`,
    /// so this should never happen; kept as a fail-closed check rather than
    /// trusted on documentation alone.
    Retargeted,
}

/// [`refuse_an_unusable_entry`]'s rule, less [`refuse_an_unusable_kind`]'s
/// blanket refusal of a Windows reparse point: [`open_verified_for_read`]
/// verifies one of those itself instead of refusing it here. A symlink or
/// junction is still refused, via the same `is_symlink` distinction.
#[cfg(windows)]
fn refuse_an_unusable_entry_allowing_reparse_points(path: &Path) -> Result<(), UnusableEntry> {
    let entry = match std::fs::symlink_metadata(path) {
        Ok(entry) => entry,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(UnusableEntry::Inspect(source)),
    };
    if entry.is_symlink() {
        Err(UnusableEntry::IsAlias)
    } else if entry.is_file() {
        Ok(())
    } else {
        Err(UnusableEntry::NotAPlainFile)
    }
}

/// Opens `path` for [`SaveFile::read`](super::SaveFile::read), admitting a
/// non-symlink Windows reparse point through its filter rather than
/// refusing it the way [`refuse_an_unusable_kind`] (still the lock slot's
/// policy, which has no filter to serve through) does. A symlink or
/// junction is refused from a handle that never followed it, and nothing
/// past that point re-resolves `path`, so nothing landed at that name
/// afterward can change what gets read.
///
/// `Ok(None)` if `path` names nothing.
#[cfg(windows)]
pub(super) fn open_verified_for_read(
    path: &Path,
) -> Result<Option<std::fs::File>, VerifiedReadError> {
    open_verified_for_read_with(path, |_precheck| {})
}

/// As [`open_verified_for_read`], with `before_hydrating_reopen` run on the
/// verified pre-open handle just before [`reopen_through_filter`], so a
/// test can land a filesystem change in that window without a real race
/// (mirrors `staging`'s `remove_through_verified_handle_with`). Never
/// called for an entry [`is_reparse_point`] says needs no reopen at all.
#[cfg(windows)]
pub(super) fn open_verified_for_read_with(
    path: &Path,
    before_hydrating_reopen: impl FnOnce(&std::fs::File),
) -> Result<Option<std::fs::File>, VerifiedReadError> {
    use std::os::windows::fs::OpenOptionsExt as _;

    refuse_an_unusable_entry_allowing_reparse_points(path).map_err(VerifiedReadError::Unusable)?;

    let mut precheck_options = std::fs::OpenOptions::new();
    precheck_options
        .read(true)
        .custom_flags(open_flags::FILE_FLAG_OPEN_REPARSE_POINT);
    let precheck = match precheck_options.open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(VerifiedReadError::Unusable(UnusableEntry::Inspect(source))),
    };
    let kind = precheck
        .metadata()
        .map_err(|source| VerifiedReadError::Unusable(UnusableEntry::Inspect(source)))?;
    if kind.is_symlink() {
        return Err(VerifiedReadError::Unusable(UnusableEntry::IsAlias));
    }
    if !kind.is_file() {
        return Err(VerifiedReadError::Unusable(UnusableEntry::NotAPlainFile));
    }
    if !is_reparse_point(&kind) {
        // `FILE_FLAG_OPEN_REPARSE_POINT` changes nothing for an entry that
        // is not a reparse point, so `precheck` already holds the real
        // data; reopening `path` for it would only add a lookup a race
        // could redirect, for no benefit.
        return Ok(Some(precheck));
    }

    let precheck_identity = WindowsFileIdentity::of(&precheck)
        .map_err(|source| VerifiedReadError::Unusable(UnusableEntry::Inspect(source)))?;
    before_hydrating_reopen(&precheck);
    let data = reopen_through_filter(&precheck)
        .map_err(|source| VerifiedReadError::Unusable(UnusableEntry::Inspect(source)))?;
    let data_identity = WindowsFileIdentity::of(&data)
        .map_err(|source| VerifiedReadError::Unusable(UnusableEntry::Inspect(source)))?;
    if data_identity != precheck_identity {
        return Err(VerifiedReadError::Retargeted);
    }
    Ok(Some(data))
}

/// Reopens the object `handle` already names, without
/// `FILE_FLAG_OPEN_REPARSE_POINT`, so a filter -- `OneDrive`'s cloud-files
/// driver among others -- serves the real data a reparse point like a
/// placeholder stands in for. `ReOpenFile` resolves against `handle`'s own
/// object, not a fresh lookup of its path, so a path swapped in after
/// `handle` was opened has nothing to redirect.
#[cfg(windows)]
fn reopen_through_filter(handle: &std::fs::File) -> std::io::Result<std::fs::File> {
    use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _};
    use windows_sys::Win32::Foundation::{GENERIC_READ, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        ReOpenFile, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    // SAFETY: `handle` stays open and valid for this call; `ReOpenFile` returns either a handle
    // this function becomes the sole owner of, or `INVALID_HANDLE_VALUE`, checked below.
    let reopened = unsafe {
        ReOpenFile(
            handle.as_raw_handle(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            0,
        )
    };
    if reopened == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `reopened` is the valid handle `ReOpenFile` just returned, owned by no one else.
    Ok(unsafe { std::fs::File::from_raw_handle(reopened) })
}
