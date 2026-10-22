//! Windows handle identity and handle-bound delete, shared by `rom-import`,
//! `engine`, and `xtask`.
//!
//! The whole crate is compiled only on Windows (`#![cfg(windows)]`); on every
//! other target it is empty, so dependants name it unconditionally or under
//! `cfg(windows)` and keep their own non-Windows arm.
//!
//! Public surface:
//!
//! - [`WindowsFileIdentity`]: the volume and file ID a handle exposes,
//!   comparable across two opens of what ought to be the same object.
//! - [`open_for_verified_delete`], [`remove_through_open_handle`],
//!   [`remove_through_verified_handle`], and
//!   [`remove_through_verified_handle_with`]: verify a path still reaches an
//!   identity and delete through that same handle, reporting a
//!   [`VerifiedRemoval`].
//! - [`delete_through_handle`]: the delete alone, for a handle the caller has
//!   already verified.
//!
//! What a missing path or a pending delete means is the caller's decision:
//! these primitives report `NotFound` as an error and a pending delete as
//! [`VerifiedRemoval::Pending`], and each dependant maps them at its own
//! boundary. The `windows-sys` use is bounded in the dependency ledger
//! (`README.md`).
#![cfg(windows)]

use std::path::Path;

/// The identity Windows reads off an open handle: volume serial number and
/// file ID, the one per-object identity it exposes to a handle regardless of
/// which path was used to open it (`std`'s equivalent,
/// `MetadataExt::volume_serial_number`/`file_index`, is still gated on the
/// unstable `windows_by_handle`, rust-lang/rust#63010). Callers compare it
/// across two opens of what ought to be the same object.
///
/// The 128-bit ID `GetFileInformationByHandleEx(FileIdInfo)` returns is
/// read first: `GetFileInformationByHandle`'s 64-bit file index is not
/// guaranteed unique on `ReFS` (Dev Drive's file system), whose documented
/// unique identifier is the 128-bit one. Only a file system that rejects
/// `FileIdInfo` falls back to the 64-bit index, zero-extended, which is how
/// NTFS fills the 128-bit form anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsFileIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

impl WindowsFileIdentity {
    /// Reads the identity of the object `handle` refers to: the 128-bit
    /// `FileIdInfo` form, or the 64-bit index where the file system rejects
    /// that class ([`rejects_the_class`]). Any other failure surfaces.
    ///
    /// # Errors
    ///
    /// Any query failure other than the file system rejecting `FileIdInfo`.
    ///
    /// # Panics
    ///
    /// Never in practice: `FILE_ID_INFO` is far smaller than `u32::MAX`.
    pub fn of(handle: &std::fs::File) -> std::io::Result<Self> {
        use std::os::windows::io::AsRawHandle as _;
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
            .is_some_and(rejects_the_class);
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

/// Whether `code` means a failed `FileIdInfo` query rejected that class.
/// CIFS/SMB may reject `FileIdInfo` while `GetFileInformationByHandle` still answers.
fn rejects_the_class(code: u32) -> bool {
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_FUNCTION, ERROR_INVALID_LEVEL, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    };

    matches!(
        code,
        ERROR_NOT_SUPPORTED
            | ERROR_INVALID_PARAMETER
            | ERROR_INVALID_FUNCTION
            | ERROR_INVALID_LEVEL
    )
}

#[cfg(test)]
mod windows_file_identity_tests {
    use super::rejects_the_class;
    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_INVALID_FUNCTION, ERROR_INVALID_HANDLE, ERROR_INVALID_LEVEL,
        ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    };

    /// A share whose server refuses `FileIdInfo` with `ERROR_INVALID_LEVEL`
    /// falls back to the 64-bit index instead of failing every caller there.
    #[test]
    fn an_smb_share_that_refuses_the_class_falls_back() {
        assert!(rejects_the_class(ERROR_INVALID_LEVEL));
    }

    #[test]
    fn the_other_unsupported_class_codes_fall_back() {
        for code in [
            ERROR_NOT_SUPPORTED,
            ERROR_INVALID_PARAMETER,
            ERROR_INVALID_FUNCTION,
        ] {
            assert!(rejects_the_class(code), "code {code} should fall back");
        }
    }

    #[test]
    fn a_real_query_failure_still_propagates() {
        for code in [ERROR_ACCESS_DENIED, ERROR_INVALID_HANDLE] {
            assert!(!rejects_the_class(code), "code {code} should propagate");
        }
    }
}

/// Opens `path` for [`remove_through_verified_handle`]'s identity check and
/// delete: `DELETE | FILE_READ_ATTRIBUTES` access,
/// `FILE_FLAG_OPEN_REPARSE_POINT` so a reparse point planted at the final
/// component is not followed underneath the check, and
/// `FILE_FLAG_BACKUP_SEMANTICS` so a directory that took the name opens and
/// fails the identity check instead of failing the open as access denied.
///
/// The share mode admits every other opener. A deny-all open would fail as
/// a sharing violation whenever a scanner or indexer still held the freshly
/// written file, leaving it behind; sharing cannot redirect the check or the
/// delete, since both go through this handle.
///
/// # Errors
///
/// The open failing, including `NotFound` for a path that reaches nothing.
pub fn open_for_verified_delete(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    std::fs::OpenOptions::new()
        .access_mode(DELETE | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

/// What [`remove_through_verified_handle`] did about `path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedRemoval {
    /// `path` no longer reached the verified object, so nothing was touched.
    NotOurs,
    /// The object was deleted with POSIX semantics: its name left the
    /// directory when the verifying handle closed, even if another process
    /// still holds the object open.
    Unlinked,
    /// The volume refused POSIX semantics, so the object was only marked
    /// delete-on-close: its name stays in the directory, refusing a
    /// same-name create, until every other handle to it closes.
    Pending,
}

/// Deletes the object `handle` refers to through `handle` itself rather
/// than a second lookup of its path.
///
/// POSIX semantics (`FileDispositionInfoEx`) is tried first, so the name is
/// gone once `handle` closes whatever other opener remains. A volume or
/// Windows release without it answers `ERROR_NOT_SUPPORTED`,
/// `ERROR_INVALID_PARAMETER`, or `ERROR_INVALID_FUNCTION`, and falls back to
/// the plain `FileDispositionInfo` delete-on-close, reported as
/// [`VerifiedRemoval::Pending`] because another handle can hold the name.
///
/// # Errors
///
/// Either disposition call failing for a reason other than the POSIX
/// semantics fallback codes.
pub fn delete_through_handle(handle: &std::fs::File) -> std::io::Result<VerifiedRemoval> {
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FileDispositionInfo, FileDispositionInfoEx, FILE_DISPOSITION_FLAG_DELETE,
        FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX,
    };

    let posix = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    // SAFETY: `FileDispositionInfoEx` takes a `FILE_DISPOSITION_INFO_EX`, which `posix` is.
    match unsafe { set_file_information(handle, FileDispositionInfoEx, &posix) } {
        Ok(()) => return Ok(VerifiedRemoval::Unlinked),
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
    unsafe { set_file_information(handle, FileDispositionInfo, &legacy) }?;
    Ok(VerifiedRemoval::Pending)
}

/// `SetFileInformationByHandle` on `handle` with `info` as the buffer.
///
/// # Safety
///
/// `T` must be the structure `class` documents as its buffer.
unsafe fn set_file_information<T>(
    handle: &std::fs::File,
    class: windows_sys::Win32::Storage::FileSystem::FILE_INFO_BY_HANDLE_CLASS,
    info: &T,
) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::SetFileInformationByHandle;

    let size = u32::try_from(std::mem::size_of::<T>())
        .expect("a file-information structure is far smaller than u32::MAX");
    // SAFETY: `handle` stays open for the call, the caller guarantees `T` matches `class`, and `size` is `T`'s exact size.
    let ok = unsafe {
        SetFileInformationByHandle(
            handle.as_raw_handle(),
            class,
            std::ptr::from_ref(info).cast(),
            size,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Removes the file `identity` names if `path` still reaches it, deleting
/// through the same handle the identity check opens so no re-lookup of
/// `path` occurs between the check and the delete. A path that now reaches a
/// different object is left alone ([`VerifiedRemoval::NotOurs`]). A path
/// that reaches nothing at all is an error (`NotFound`), not `NotOurs`; a
/// caller that treats a vanished path as already clean opens through
/// [`open_for_verified_delete`] and maps `NotFound` itself before
/// [`remove_through_open_handle`].
///
/// # Errors
///
/// The open, identity query, or delete failing, including `NotFound`.
pub fn remove_through_verified_handle(
    path: &Path,
    identity: WindowsFileIdentity,
) -> std::io::Result<VerifiedRemoval> {
    remove_through_verified_handle_with(path, identity, |_handle| {})
}

/// [`remove_through_verified_handle`] with a hook run between the identity
/// check and the delete, so a test can land a retarget in exactly that
/// window without a real race: a test's only other way to reach it would be
/// two threads racing the OS, which no cheap seam makes deterministic.
///
/// # Errors
///
/// As [`remove_through_verified_handle`].
pub fn remove_through_verified_handle_with(
    path: &Path,
    identity: WindowsFileIdentity,
    before_delete: impl FnOnce(&std::fs::File),
) -> std::io::Result<VerifiedRemoval> {
    let handle = open_for_verified_delete(path)?;
    remove_through_open_handle(&handle, identity, before_delete)
}

/// The identity check and delete of [`remove_through_verified_handle_with`]
/// for a `handle` already opened by [`open_for_verified_delete`]: deletes
/// the object it refers to only if its identity is `identity`.
///
/// # Errors
///
/// The identity query or delete failing.
pub fn remove_through_open_handle(
    handle: &std::fs::File,
    identity: WindowsFileIdentity,
    before_delete: impl FnOnce(&std::fs::File),
) -> std::io::Result<VerifiedRemoval> {
    if WindowsFileIdentity::of(handle)? != identity {
        return Ok(VerifiedRemoval::NotOurs);
    }
    before_delete(handle);
    delete_through_handle(handle)
}
