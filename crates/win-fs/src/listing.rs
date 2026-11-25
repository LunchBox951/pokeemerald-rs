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
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Foundation::ERROR_NO_MORE_FILES;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdExtdDirectoryInfo, FileIdExtdDirectoryRestartInfo, GetFileInformationByHandleEx,
    };

    // `u64` elements keep the buffer 8-aligned, as the entries require.
    let mut buffer = vec![0u64; 8192];
    let size = u32::try_from(buffer.len() * 8).expect("64 KiB fits in u32");
    let mut class = FileIdExtdDirectoryRestartInfo;
    let mut entries = Vec::new();
    loop {
        // SAFETY: no safe form exists: neither `std` nor an approved
        // `windows-sys` feature lists a directory through its handle by file
        // ID, and this call is the only way to ask. It is sound because
        // `dir` is a live `File`, so `as_raw_handle` is a valid open handle
        // for the whole call, and `buffer` is a live, exclusively borrowed
        // allocation of exactly `size` bytes, 8-aligned as the class
        // requires, that the call may write up to `size` bytes into and keeps
        // no pointer to afterwards. Everything it wrote is read back through
        // the bounds-checked `walk_records`, not trusted.
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
        walk_records(&buffer, &mut entries).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "a directory listing record lies outside the listing buffer",
            )
        })?;
    }
}

/// The `N` bytes of `buffer`, read in memory order, starting at byte `at`;
/// `None` unless all of them lie inside it. Reading through `u64` elements
/// keeps the walk in safe code: no pointer is cast to a record type, so no
/// alignment or length is assumed.
fn field<const N: usize>(buffer: &[u64], at: usize) -> Option<[u8; N]> {
    let mut bytes = [0u8; N];
    for (i, byte) in bytes.iter_mut().enumerate() {
        let index = at.checked_add(i)?;
        *byte = *buffer.get(index / 8)?.to_ne_bytes().get(index % 8)?;
    }
    Some(bytes)
}

/// Appends the entries of the `FILE_ID_EXTD_DIR_INFO` chain the system wrote
/// at the start of `buffer`, other than `.` and `..`. `None` when any record,
/// or the name it claims, does not lie wholly inside `buffer`; entries
/// appended before that point stay. Every offset and length is checked
/// against `buffer` before it is read, and `NextEntryOffset` is zero on the
/// last record, so the walk ends at the end of the chain or the buffer,
/// whichever comes first.
fn walk_records(buffer: &[u64], entries: &mut Vec<DirectoryEntry>) -> Option<()> {
    use std::os::windows::ffi::OsStringExt as _;
    use windows_sys::Win32::Storage::FileSystem::FILE_ID_EXTD_DIR_INFO;

    let mut offset = 0usize;
    loop {
        let at = |field_offset: usize| offset.checked_add(field_offset);
        let next = u32::from_ne_bytes(field(
            buffer,
            at(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, NextEntryOffset))?,
        )?);
        let attributes = u32::from_ne_bytes(field(
            buffer,
            at(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileAttributes))?,
        )?);
        let name_bytes = usize::try_from(u32::from_ne_bytes(field(
            buffer,
            at(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileNameLength))?,
        )?))
        .ok()?;
        let file_id = field(
            buffer,
            at(std::mem::offset_of!(
                FILE_ID_EXTD_DIR_INFO,
                FileId.Identifier
            ))?,
        )?;
        let name_at = at(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileName))?;
        // The name must end inside the buffer before it is read, so a bad
        // length cannot size an allocation.
        if name_at.checked_add(name_bytes)? > buffer.len() * 8 {
            return None;
        }
        let units = (0..name_bytes / 2)
            .map(|unit| field(buffer, name_at + unit * 2).map(u16::from_ne_bytes))
            .collect::<Option<Vec<u16>>>()?;
        let name = std::ffi::OsString::from_wide(&units);
        if name != "." && name != ".." {
            entries.push(DirectoryEntry {
                name,
                file_id,
                attributes,
            });
        }
        if next == 0 {
            return Some(());
        }
        offset = offset.checked_add(usize::try_from(next).ok()?)?;
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
    // SAFETY: no safe form exists: `std` does not expose volume flags, and
    // the approved `windows-sys` features offer this call only as `unsafe`.
    // It is sound because `handle` is a live `File`, so `as_raw_handle` is a
    // valid open handle for the whole call; `flags` is a live `u32` the call
    // writes through `&raw mut flags` and keeps no pointer to; and every
    // other pointer is null with a zero size, which the API defines as "do
    // not return this field".
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
    use super::{list_directory_by_file_id, walk_records, DirectoryEntry};
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

    /// Writes `bytes` at byte `at` of `buffer`, in memory order.
    fn put(buffer: &mut [u64], at: usize, bytes: &[u8]) {
        for (i, byte) in bytes.iter().enumerate() {
            let index = at + i;
            let mut word = buffer[index / 8].to_ne_bytes();
            word[index % 8] = *byte;
            buffer[index / 8] = u64::from_ne_bytes(word);
        }
    }

    /// Writes one `FILE_ID_EXTD_DIR_INFO` record for `name` at `at`, claiming
    /// `name_bytes` of name and `next` as the distance to the next record.
    fn put_record(buffer: &mut [u64], at: usize, name: &str, name_bytes: u32, next: u32, id: u8) {
        use windows_sys::Win32::Storage::FileSystem::FILE_ID_EXTD_DIR_INFO;
        let field = |offset: usize| at + offset;
        put(
            buffer,
            field(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, NextEntryOffset)),
            &next.to_ne_bytes(),
        );
        put(
            buffer,
            field(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileAttributes)),
            &0x10u32.to_ne_bytes(),
        );
        put(
            buffer,
            field(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileNameLength)),
            &name_bytes.to_ne_bytes(),
        );
        put(
            buffer,
            field(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileId)),
            &[id; 16],
        );
        let units: Vec<u8> = name.encode_utf16().flat_map(u16::to_ne_bytes).collect();
        put(
            buffer,
            field(std::mem::offset_of!(FILE_ID_EXTD_DIR_INFO, FileName)),
            &units,
        );
    }

    #[test]
    fn a_record_chain_is_read_without_the_dot_entries() {
        let mut buffer = vec![0u64; 64];
        put_record(&mut buffer, 0, ".", 2, 128, 1);
        put_record(&mut buffer, 128, "..", 4, 128, 2);
        put_record(&mut buffer, 256, "a.txt", 10, 0, 3);
        let mut entries = Vec::new();
        assert_eq!(walk_records(&buffer, &mut entries), Some(()));
        assert_eq!(
            entries,
            [DirectoryEntry {
                name: "a.txt".into(),
                file_id: [3; 16],
                attributes: 0x10,
            }]
        );
    }

    /// The second length ends two bytes past the 128-byte buffer (88 + 42).
    #[test]
    fn a_name_past_the_buffer_is_refused() {
        let mut buffer = vec![0u64; 16];
        put_record(&mut buffer, 0, "a", u32::MAX, 0, 1);
        assert_eq!(walk_records(&buffer, &mut Vec::new()), None);
        let mut buffer = vec![0u64; 16];
        put_record(&mut buffer, 0, "a", 128 - 88 + 2, 0, 1);
        assert_eq!(walk_records(&buffer, &mut Vec::new()), None);
    }

    /// Offset 124 leaves room for the first four bytes of a header only.
    #[test]
    fn a_next_offset_past_the_buffer_is_refused() {
        let mut buffer = vec![0u64; 16];
        put_record(&mut buffer, 0, "a", 2, u32::MAX, 1);
        assert_eq!(walk_records(&buffer, &mut Vec::new()), None);
        let mut buffer = vec![0u64; 16];
        put_record(&mut buffer, 0, "a", 2, 124, 1);
        assert_eq!(walk_records(&buffer, &mut Vec::new()), None);
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
