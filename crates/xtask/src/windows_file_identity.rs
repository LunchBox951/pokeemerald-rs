//! The handle identity query shared by `record-snapshot` and the extract
//! publish path (F-3: `xtask extract` publishes its pack only after the staged
//! file's identity is verified; #1886).

/// The volume and file ID of the object a handle refers to, read the way
/// `rom-import`'s `WindowsFileIdentity` does: the 128-bit `FileIdInfo`, or the
/// zero-extended 64-bit index where the file system rejects that class.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowsFileIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

impl WindowsFileIdentity {
    /// Reads the volume and file ID of the object `handle` refers to: the
    /// 128-bit `FileIdInfo`, falling back to the zero-extended 64-bit index
    /// only when the file system rejects that class ([`rejects_the_class`]).
    /// Any other query error propagates.
    pub(crate) fn of(handle: &std::fs::File) -> std::io::Result<Self> {
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

/// Whether `code`, from a failed `FileIdInfo` query, means the file system
/// does not serve that class rather than that the query failed: not supported,
/// invalid parameter (FAT, pre-Windows 8), invalid function, or invalid level
/// (a CIFS/SMB server that does not know the class, where
/// `GetFileInformationByHandle` still answers).
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
mod tests {
    use super::rejects_the_class;
    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_INVALID_FUNCTION, ERROR_INVALID_HANDLE, ERROR_INVALID_LEVEL,
        ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    };

    /// A share whose server refuses `FileIdInfo` with `ERROR_INVALID_LEVEL`
    /// falls back to the 64-bit index instead of failing every extract there.
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
