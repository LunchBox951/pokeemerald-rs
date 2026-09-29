use std::path::Path;

/// Pins the directory used for payload writes. Creating and opening a directory
/// are separate operations; the claim starts at the successful open.
pub(super) struct StagedDirClaim {
    hold: Option<std::fs::File>,
}

impl StagedDirClaim {
    pub(super) fn require_path(&self, path: &Path) -> std::io::Result<()> {
        let hold = self.hold.as_ref().ok_or_else(|| unpinned_directory(path))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let identity = hold.metadata()?;
            let found = std::fs::symlink_metadata(path)?;
            if !found.is_dir() || (identity.dev(), identity.ino()) != (found.dev(), found.ino()) {
                return Err(std::io::Error::other(format!(
                    "the directory at {} no longer matches the capture's held directory",
                    path.display()
                )));
            }
        }
        #[cfg(not(unix))]
        let _ = hold;
        Ok(())
    }

    #[cfg(unix)]
    pub(super) fn write_payload(
        &self,
        staged: &Path,
        name: &str,
        bytes: &[u8],
    ) -> std::io::Result<()> {
        use std::io::Write as _;
        let hold = self
            .hold
            .as_ref()
            .ok_or_else(|| unpinned_directory(staged))?;
        let fd = rustix::fs::openat(
            hold,
            name,
            rustix::fs::OFlags::WRONLY
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::EXCL
                | rustix::fs::OFlags::NOFOLLOW,
            // `File::create`'s own 0o666; the process umask narrows it.
            rustix::fs::Mode::RUSR
                | rustix::fs::Mode::WUSR
                | rustix::fs::Mode::RGRP
                | rustix::fs::Mode::WGRP
                | rustix::fs::Mode::ROTH
                | rustix::fs::Mode::WOTH,
        )?;
        std::fs::File::from(fd).write_all(bytes)
    }

    #[cfg(not(unix))]
    pub(super) fn write_payload(
        &self,
        staged: &Path,
        name: &str,
        bytes: &[u8],
    ) -> std::io::Result<()> {
        use std::io::Write as _;
        self.require_path(staged)?;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staged.join(name))?
            .write_all(bytes)
    }

    // Unix carries the handle through rename. Windows must release its sharing
    // restriction first; preserving identity across that gap is issue #1345.
    #[cfg_attr(
        not(windows),
        expect(
            clippy::unused_self,
            reason = "only Windows releases its hold before rename"
        )
    )]
    pub(super) fn release_hold(&mut self) {
        #[cfg(windows)]
        self.hold.take();
    }
}

fn unpinned_directory(path: &Path) -> std::io::Error {
    std::io::Error::other(format!(
        "no handle pins the capture directory at {}; publication was refused",
        path.display()
    ))
}

/// `O_SEARCH` on Apple targets, which have no `O_PATH`: authorization to
/// search a directory, without the read access `O_RDONLY` demands. The
/// platform spells it `O_EXEC | O_DIRECTORY` (`libc`'s Apple `O_EXEC`,
/// `0x4000_0000`); `rustix` 1.1 exposes no flag for it, so the value travels
/// through `OFlags`' externally defined bits. [`claim_staged_dir`] always adds
/// `DIRECTORY`.
#[cfg(target_vendor = "apple")]
const APPLE_SEARCH: rustix::fs::OFlags = rustix::fs::OFlags::from_bits_retain(0x4000_0000);

#[cfg(unix)]
fn open_held_dir(path: &Path, access: rustix::fs::OFlags) -> std::io::Result<std::fs::File> {
    rustix::fs::open(
        path,
        access | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    )
    .map(std::fs::File::from)
    .map_err(Into::into)
}

#[cfg(unix)]
pub(super) fn claim_staged_dir(path: &Path) -> std::io::Result<StagedDirClaim> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let access = rustix::fs::OFlags::PATH;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let access = rustix::fs::OFlags::RDONLY;
    let held = open_held_dir(path, access);
    // A `0444` umask leaves the capture's own staging directory at mode `0333`:
    // writable and searchable, but unreadable, so `O_RDONLY` is refused. Search
    // access is all the hold is used for -- `openat` for payload creation and
    // `fstat` for identity.
    #[cfg(target_vendor = "apple")]
    let held = match held {
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            open_held_dir(path, APPLE_SEARCH)
        }
        held => held,
    };
    // A claim without a hold can never write a payload, so a directory that
    // cannot be held fails the claim instead of reporting a dead success.
    let claim = StagedDirClaim { hold: Some(held?) };
    claim.require_path(path)?;
    Ok(claim)
}

#[cfg(windows)]
pub(super) fn claim_staged_dir(path: &Path) -> std::io::Result<StagedDirClaim> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let hold = std::fs::OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(0)
        .open(path)?;
    let found = hold.metadata()?;
    if !found.is_dir() || found.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(std::io::Error::other("the capture directory was replaced"));
    }
    Ok(StagedDirClaim { hold: Some(hold) })
}

#[cfg(not(any(unix, windows)))]
pub(super) fn claim_staged_dir(_path: &Path) -> std::io::Result<StagedDirClaim> {
    Err(std::io::ErrorKind::Unsupported.into())
}

#[cfg(windows)]
pub(super) fn claim_promoted_dir(path: &Path) -> std::io::Result<StagedDirClaim> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        match claim_staged_dir(path) {
            // Concurrent directory operations can briefly deny the exclusive
            // open without replacing the directory. Other failures are final.
            Err(error)
                if matches!(error.raw_os_error(), Some(32 | 33))
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::yield_now();
            }
            result => return result,
        }
    }
}

/// Pins the output directory for a whole publication.
///
/// On Unix the pointer is staged and promoted relative to this held
/// directory ([`Self::stage_pointer`], `staging::StagedFile`), so the
/// pointer can only land in the directory the claim verified, whatever the
/// pathname names by then. Windows has no fd-relative rename through the
/// approved surface, so there the pathname stays in the loop: the hold keeps
/// the directory from being renamed away, [`Self::require_path`] compares a
/// fresh open of the path with the held handle, and a swap in the gap between
/// that check and the pointer's rename remains possible (check-then-act).
pub(super) struct OutputDirClaim {
    hold: std::fs::File,
}

impl OutputDirClaim {
    /// Fails unless `path` still resolves to the held directory.
    pub(super) fn require_path(&self, path: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let identity = self.hold.metadata()?;
            let found = std::fs::metadata(path)?;
            if !found.is_dir() || (identity.dev(), identity.ino()) != (found.dev(), found.ino()) {
                return Err(no_longer_the_held_directory(path));
            }
        }
        #[cfg(windows)]
        {
            let fresh = open_output_dir(path)?;
            if !fresh.metadata()?.is_dir()
                || WindowsFileIdentity::of(&fresh)? != WindowsFileIdentity::of(&self.hold)?
            {
                return Err(no_longer_the_held_directory(path));
            }
        }
        #[cfg(not(any(unix, windows)))]
        let _ = path;
        Ok(())
    }

    /// Creates a staged pointer named after `path`'s final component inside
    /// the held directory.
    #[cfg(unix)]
    pub(super) fn stage_pointer(
        &self,
        path: &Path,
        bytes: &[u8],
    ) -> std::io::Result<super::staging::StagedFile> {
        super::staging::stage_in(&self.hold, path, bytes)
    }

    #[cfg(not(unix))]
    #[expect(
        clippy::unused_self,
        reason = "one signature for every platform; only the unix arm stages through the hold"
    )]
    pub(super) fn stage_pointer(
        &self,
        path: &Path,
        bytes: &[u8],
    ) -> std::io::Result<super::staging::StagedFile> {
        super::staging::stage(path, bytes)
    }
}

fn no_longer_the_held_directory(path: &Path) -> std::io::Error {
    std::io::Error::other(format!(
        "the output directory at {} no longer matches the capture's held directory",
        path.display()
    ))
}

/// Unlike the staging claim this follows a symlinked `output_dir`, which
/// callers may pass.
#[cfg(unix)]
pub(super) fn claim_output_dir(path: &Path) -> std::io::Result<OutputDirClaim> {
    // `O_PATH` directory descriptors serve `openat`, `fstatat`, and
    // `renameat` as their directory argument, so no read access is needed.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let access = rustix::fs::OFlags::PATH;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let access = rustix::fs::OFlags::RDONLY;
    let open = |access| {
        rustix::fs::open(
            path,
            access | rustix::fs::OFlags::DIRECTORY,
            rustix::fs::Mode::empty(),
        )
        .map(std::fs::File::from)
        .map_err(std::io::Error::from)
    };
    let hold = open(access);
    // A search-only output directory (mode `0333`) refuses `O_RDONLY`; see
    // [`claim_staged_dir`].
    #[cfg(target_vendor = "apple")]
    let hold = match hold {
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => open(APPLE_SEARCH),
        hold => hold,
    };
    Ok(OutputDirClaim { hold: hold? })
}

/// Opens `path` following any symlink or junction, without requesting access
/// and without `FILE_SHARE_DELETE`, so an open handle keeps the directory
/// from being renamed or deleted.
#[cfg(windows)]
fn open_output_dir(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    std::fs::OpenOptions::new()
        .access_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(path)
}

#[cfg(windows)]
pub(super) fn claim_output_dir(path: &Path) -> std::io::Result<OutputDirClaim> {
    open_output_dir(path).map(|hold| OutputDirClaim { hold })
}

#[cfg(not(any(unix, windows)))]
pub(super) fn claim_output_dir(_path: &Path) -> std::io::Result<OutputDirClaim> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// The volume and file ID of the object a handle refers to, read the way
/// `rom-import`'s `WindowsFileIdentity` does: the 128-bit `FileIdInfo`, or the
/// zero-extended 64-bit index where the file system rejects that class.
#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct WindowsFileIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

#[cfg(windows)]
impl WindowsFileIdentity {
    fn of(handle: &std::fs::File) -> std::io::Result<Self> {
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

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
pub(super) fn rename_without_replacement(source: &Path, destination: &Path) -> std::io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(Into::into)
}

#[cfg(windows)]
pub(super) fn rename_without_replacement(source: &Path, destination: &Path) -> std::io::Result<()> {
    // Modern Windows rename can replace empty directories too. This refusal is
    // not atomic with rename; closing that window belongs to #1345.
    match std::fs::symlink_metadata(destination) {
        Ok(_) => return Err(std::io::ErrorKind::AlreadyExists.into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    std::fs::rename(source, destination)
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    windows
)))]
pub(super) fn rename_without_replacement(
    _source: &Path,
    _destination: &Path,
) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic directory promotion without replacement is unavailable on this target",
    ))
}
