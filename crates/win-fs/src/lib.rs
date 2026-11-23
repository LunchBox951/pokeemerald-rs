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
//!   [`VerifiedRemoval`]. A volume whose file IDs are not stable (FAT, exFAT)
//!   fails closed: an equal ID there does not prove the original.
//! - [`delete_through_handle`] and
//!   [`delete_through_handle_ignoring_read_only`]: the delete alone, for a
//!   handle the caller has already verified.
//! - [`wide_file_id`], [`list_directory_by_file_id`], and
//!   [`volume_supports_open_by_file_id`]: the strict 128-bit ID of a handle,
//!   a directory's entries listed through its handle with that same ID, and
//!   whether the volume's IDs are stable enough to verify against.
//!
//! What a missing path or a pending delete means is the caller's decision:
//! these primitives report `NotFound` as an error and a pending delete as
//! [`VerifiedRemoval::Pending`], and each dependant maps them at its own
//! boundary. The `windows-sys` use is bounded in the dependency ledger
//! (`README.md`).
#![cfg(windows)]

use std::path::Path;

mod listing;

pub use listing::{list_directory_by_file_id, volume_supports_open_by_file_id, DirectoryEntry};

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
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };

        let refused = match query_file_id_info(handle) {
            Ok(wide) => {
                return Ok(Self {
                    volume_serial_number: wide.VolumeSerialNumber,
                    file_id: wide.FileId.Identifier,
                });
            }
            Err(refused) => refused,
        };
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

/// `GetFileInformationByHandleEx(FileIdInfo)` on `handle`, with no fallback.
///
/// # Panics
///
/// Never in practice: `FILE_ID_INFO` is far smaller than `u32::MAX`.
fn query_file_id_info(
    handle: &std::fs::File,
) -> std::io::Result<windows_sys::Win32::Storage::FileSystem::FILE_ID_INFO> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandleEx, FILE_ID_INFO,
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
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(wide)
    }
}

/// The 128-bit file ID (`FileIdInfo`) of the object `handle` refers to, the
/// same ID a [`list_directory_by_file_id`] listing reports for that entry.
/// Unlike [`WindowsFileIdentity::of`] there is no 64-bit fallback: a caller
/// comparing against a listing needs the one form the listing carries.
///
/// # Errors
///
/// The `FileIdInfo` query failing for any reason.
pub fn wide_file_id(handle: &std::fs::File) -> std::io::Result<[u8; 16]> {
    Ok(query_file_id_info(handle)?.FileId.Identifier)
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
    delete_through_handle_with_flags(handle, 0)
}

/// [`delete_through_handle`], with the POSIX-semantics attempt also passing
/// `FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE`, so an object marked
/// read-only is deleted rather than refused. The legacy fallback has no such
/// flag and still refuses a read-only object.
///
/// # Errors
///
/// As [`delete_through_handle`].
pub fn delete_through_handle_ignoring_read_only(
    handle: &std::fs::File,
) -> std::io::Result<VerifiedRemoval> {
    use windows_sys::Win32::Storage::FileSystem::FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE;

    delete_through_handle_with_flags(handle, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE)
}

/// The delete of [`delete_through_handle`], with `extra_posix_flags` added to
/// the POSIX-semantics attempt's flags.
fn delete_through_handle_with_flags(
    handle: &std::fs::File,
    extra_posix_flags: windows_sys::Win32::Storage::FileSystem::FILE_DISPOSITION_INFO_EX_FLAGS,
) -> std::io::Result<VerifiedRemoval> {
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FileDispositionInfo, FileDispositionInfoEx, FILE_DISPOSITION_FLAG_DELETE,
        FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX,
    };

    let posix = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | extra_posix_flags,
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
/// different object is left alone ([`VerifiedRemoval::NotOurs`]). On a
/// volume whose file IDs are not stable (FAT, exFAT) the identity proves
/// nothing, so nothing is deleted and the call fails
/// ([`remove_through_open_handle`]). A path
/// that reaches nothing at all is an error (`NotFound`), not `NotOurs`; a
/// caller that treats a vanished path as already clean opens through
/// [`open_for_verified_delete`] and maps `NotFound` itself before
/// [`remove_through_open_handle`].
///
/// # Errors
///
/// The open, identity query, or delete failing, including `NotFound`, or
/// [`remove_through_open_handle`]'s refusal on an unstable-ID volume.
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
/// An equal identity proves ownership only where file IDs are stable, so the
/// delete is refused unless the volume answers
/// [`volume_supports_open_by_file_id`] yes. FAT and exFAT IDs follow
/// directory-entry slots, which a replacement created after the original
/// was removed can occupy, so there an equal identity does not tell the
/// original from its replacement and nothing is deleted.
///
/// # Errors
///
/// [`std::io::ErrorKind::Unsupported`] when the volume's file IDs are not
/// stable enough to prove ownership, nothing deleted; otherwise the volume
/// or identity query or the delete failing.
pub fn remove_through_open_handle(
    handle: &std::fs::File,
    identity: WindowsFileIdentity,
    before_delete: impl FnOnce(&std::fs::File),
) -> std::io::Result<VerifiedRemoval> {
    let stable_ids = volume_supports_open_by_file_id(handle)?;
    remove_if_ids_prove_ownership(handle, identity, stable_ids, before_delete)
}

/// [`remove_through_open_handle`] with the volume's answer supplied, so a
/// test on an NTFS volume can stand in for the FAT or exFAT one it cannot
/// create: with `stable_ids` false the delete is refused whatever the
/// identity says.
fn remove_if_ids_prove_ownership(
    handle: &std::fs::File,
    identity: WindowsFileIdentity,
    stable_ids: bool,
    before_delete: impl FnOnce(&std::fs::File),
) -> std::io::Result<VerifiedRemoval> {
    if !stable_ids {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "the volume's file IDs follow directory-entry slots a replacement can reuse, \
             so a matching file ID does not prove the file is the original",
        ));
    }
    if WindowsFileIdentity::of(handle)? != identity {
        return Ok(VerifiedRemoval::NotOurs);
    }
    before_delete(handle);
    delete_through_handle(handle)
}

#[cfg(test)]
mod verified_removal_tests {
    use super::{
        open_for_verified_delete, remove_if_ids_prove_ownership, remove_through_open_handle,
        volume_supports_open_by_file_id, VerifiedRemoval, WindowsFileIdentity,
    };
    use std::path::PathBuf;

    /// A directory private to one test of this process.
    fn private_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("win-fs-removal-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    /// On a volume whose file IDs follow reusable directory slots (FAT,
    /// exFAT), a matching identity is no proof the file is the original, so
    /// the file the path reaches is kept. The matching identity here is the
    /// worst case: a replacement that landed in the freed slot.
    #[test]
    fn a_matching_identity_on_a_reusable_id_volume_deletes_nothing() {
        let dir = private_dir("reusable-ids");
        let path = dir.join("partial.bin");
        std::fs::write(&path, b"x").unwrap();
        let handle = open_for_verified_delete(&path).unwrap();
        let identity = WindowsFileIdentity::of(&handle).unwrap();
        let mut hook_ran = false;
        let removal = remove_if_ids_prove_ownership(&handle, identity, false, |_| hook_ran = true);
        drop(handle);
        let survived = path.exists();
        let _ = std::fs::remove_dir_all(&dir);
        let refused = removal.expect_err("a reusable-ID volume must refuse the delete");
        assert_eq!(refused.kind(), std::io::ErrorKind::Unsupported);
        assert!(!hook_ran, "the refusal comes before the delete window");
        assert!(survived, "the file was deleted on identity alone");
    }

    /// The same check on a volume with stable IDs (the temp directory is
    /// NTFS or `ReFS` on a Windows runner) still deletes the verified file.
    #[test]
    fn a_matching_identity_on_a_stable_id_volume_deletes_the_file() {
        let dir = private_dir("stable-ids");
        let path = dir.join("partial.bin");
        std::fs::write(&path, b"x").unwrap();
        let handle = open_for_verified_delete(&path).unwrap();
        let identity = WindowsFileIdentity::of(&handle).unwrap();
        let removal = remove_if_ids_prove_ownership(&handle, identity, true, |_| {});
        drop(handle);
        let survived = path.exists();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            matches!(
                removal,
                Ok(VerifiedRemoval::Unlinked | VerifiedRemoval::Pending)
            ),
            "{removal:?}"
        );
        assert!(
            !survived,
            "the verified file is gone once its handle closes"
        );
    }

    /// The public entry takes its answer from the volume the file is on: it
    /// refuses exactly when that volume lacks stable IDs, so a runner whose
    /// temp directory is FAT or exFAT sees the refusal and one on NTFS or
    /// `ReFS` sees the delete. It never deletes where the volume says no.
    #[test]
    fn the_public_entry_follows_the_volumes_own_answer() {
        let dir = private_dir("volume-answer");
        let path = dir.join("partial.bin");
        std::fs::write(&path, b"x").unwrap();
        let handle = open_for_verified_delete(&path).unwrap();
        let identity = WindowsFileIdentity::of(&handle).unwrap();
        let stable = volume_supports_open_by_file_id(&handle).unwrap();
        let removal = remove_through_open_handle(&handle, identity, |_| {});
        drop(handle);
        let survived = path.exists();
        let _ = std::fs::remove_dir_all(&dir);
        if stable {
            assert!(removal.is_ok(), "{removal:?}");
            assert!(!survived);
        } else {
            let refused = removal.expect_err("an unstable-ID volume must refuse");
            assert_eq!(refused.kind(), std::io::ErrorKind::Unsupported);
            assert!(survived, "the file was deleted on identity alone");
        }
    }

    /// The public entry asks the real volume, so on the runner's volume it
    /// still removes the file it verified and leaves a different one.
    #[test]
    fn the_public_entry_deletes_the_original_and_spares_a_replacement() {
        let dir = private_dir("public-entry");
        let original = dir.join("original.bin");
        let other = dir.join("other.bin");
        std::fs::write(&original, b"x").unwrap();
        std::fs::write(&other, b"y").unwrap();
        let identity =
            WindowsFileIdentity::of(&open_for_verified_delete(&original).unwrap()).unwrap();
        let other_handle = open_for_verified_delete(&other).unwrap();
        let spared = remove_through_open_handle(&other_handle, identity, |_| {});
        drop(other_handle);
        let other_survived = other.exists();
        let original_handle = open_for_verified_delete(&original).unwrap();
        let removed = remove_through_open_handle(&original_handle, identity, |_| {});
        drop(original_handle);
        let original_survived = original.exists();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(spared, Ok(VerifiedRemoval::NotOurs)), "{spared:?}");
        assert!(other_survived);
        assert!(
            matches!(
                removed,
                Ok(VerifiedRemoval::Unlinked | VerifiedRemoval::Pending)
            ),
            "{removed:?}"
        );
        assert!(!original_survived);
    }
}
