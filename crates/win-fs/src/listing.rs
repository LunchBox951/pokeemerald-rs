//! Listing a directory through its handle by 128-bit file ID, and asking
//! whether the volume's file IDs are stable enough to verify a listing
//! against.

/// One entry of [`list_directory_by_file_id`]: its name, the 128-bit file
/// ID the listing reports (the [`crate::wide_file_id`] form; the 64-bit form
/// is not unique on `ReFS`), and its attribute bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// The entry's name within the listed directory.
    pub name: std::ffi::OsString,
    /// The 128-bit file ID the listing reports for the entry.
    pub file_id: [u8; 16],
    /// The entry's `FILE_ATTRIBUTE_*` bits as the listing reports them.
    pub attributes: u32,
}

impl DirectoryEntry {
    /// Whether the entry is a directory that is not a reparse point, the one
    /// kind a walk may descend into without following a link.
    #[must_use]
    pub fn is_real_directory(&self) -> bool {
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        };

        self.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
            && self.attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
    }
}

/// The entries of the directory `dir` refers to, other than `.` and `..`,
/// listed through `dir` itself (`FileIdExtdDirectoryInfo`, restarted at the
/// first call) rather than a lookup of its path. `dir` must have been opened
/// with `FILE_LIST_DIRECTORY` access.
///
/// # Errors
///
/// A listing call failing for any reason other than reaching the end.
///
/// # Panics
///
/// Never in practice: the 64 KiB listing buffer's size fits in `u32`.
pub fn list_directory_by_file_id(dir: &std::fs::File) -> std::io::Result<Vec<DirectoryEntry>> {
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
            #[expect(
                clippy::cast_ptr_alignment,
                reason = "the buffer is 8-aligned and the system 8-aligns each record"
            )]
            // SAFETY: the call above filled `buffer` with a chain of
            // `FILE_ID_EXTD_DIR_INFO` records, each 8-aligned and ending
            // within it; `NextEntryOffset` is zero on the last one.
            let (next, attributes, file_id, name) = unsafe {
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
                entries.push(DirectoryEntry {
                    name,
                    file_id,
                    attributes,
                });
            }
            if next == 0 {
                break;
            }
            offset += usize::try_from(next).unwrap_or(usize::MAX);
        }
    }
}

/// Whether the volume holding the object `handle` refers to can open a file
/// by its ID (`FILE_SUPPORTS_OPEN_BY_FILE_ID`): only there is a file ID a
/// stable identity rather than a directory-entry position a replacement can
/// reuse. NTFS and `ReFS` answer yes; FAT and exFAT, whose IDs follow
/// directory-entry slots, answer no.
///
/// # Errors
///
/// The volume query failing.
pub fn volume_supports_open_by_file_id(handle: &std::fs::File) -> std::io::Result<bool> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationByHandleW;
    // `Win32_System_SystemServices`, which names this flag, is outside the
    // approved feature set, so its documented value is spelled here.
    const FILE_SUPPORTS_OPEN_BY_FILE_ID: u32 = 0x0100_0000;

    let mut flags = 0u32;
    // SAFETY: `handle` stays open for this call, `flags` is an out parameter, and every other buffer is null with a zero size.
    let ok = unsafe {
        GetVolumeInformationByHandleW(
            handle.as_raw_handle(),
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
    Ok(flags & FILE_SUPPORTS_OPEN_BY_FILE_ID != 0)
}

#[cfg(test)]
mod tests {
    use super::list_directory_by_file_id;
    use crate::{
        delete_through_handle, delete_through_handle_ignoring_read_only, open_for_verified_delete,
        wide_file_id, VerifiedRemoval,
    };
    use std::os::windows::fs::OpenOptionsExt as _;
    use std::path::PathBuf;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;

    /// A directory private to one test of this process.
    fn private_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("win-fs-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    /// Each listed entry carries the 128-bit ID a handle opened by its name
    /// reports, and only a plain directory counts as one to descend into.
    #[test]
    fn a_listing_names_each_entry_by_the_id_its_handle_reports() {
        let dir = private_dir("listing");
        std::fs::write(dir.join("file.txt"), b"x").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let held = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&dir)
            .unwrap();
        let mut checked: Vec<_> = list_directory_by_file_id(&held)
            .unwrap()
            .iter()
            .map(|entry| {
                let opened = open_for_verified_delete(&dir.join(&entry.name)).unwrap();
                (
                    entry.name.clone(),
                    wide_file_id(&opened).unwrap() == entry.file_id,
                    entry.is_real_directory(),
                )
            })
            .collect();
        checked.sort();
        drop(held);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            checked,
            [
                (std::ffi::OsString::from("file.txt"), true, false),
                (std::ffi::OsString::from("sub"), true, true),
            ]
        );
    }

    /// The plain delete refuses a read-only file; the ignoring variant
    /// removes it through the same kind of handle.
    #[test]
    fn a_read_only_file_is_deleted_only_when_the_attribute_is_ignored() {
        let dir = private_dir("read-only");
        let path = dir.join("locked.txt");
        std::fs::write(&path, b"x").unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        let plain = delete_through_handle(&open_for_verified_delete(&path).unwrap());
        let ignoring =
            delete_through_handle_ignoring_read_only(&open_for_verified_delete(&path).unwrap());
        let survived = path.exists();
        if survived {
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            #[expect(
                clippy::permissions_set_readonly_false,
                reason = "Windows has no mode bits; this only clears the read-only attribute"
            )]
            permissions.set_readonly(false);
            let _ = std::fs::set_permissions(&path, permissions);
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert!(plain.is_err(), "{plain:?}");
        assert!(
            matches!(
                ignoring,
                Ok(VerifiedRemoval::Unlinked | VerifiedRemoval::Pending)
            ),
            "{ignoring:?}"
        );
        assert!(
            !survived,
            "the read-only file is gone once its handle closes"
        );
    }
}
