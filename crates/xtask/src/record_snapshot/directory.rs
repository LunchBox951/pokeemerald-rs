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
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
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

#[cfg(unix)]
pub(super) fn claim_staged_dir(path: &Path) -> std::io::Result<StagedDirClaim> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let access = rustix::fs::OFlags::PATH;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let access = rustix::fs::OFlags::RDONLY;
    let hold = match rustix::fs::open(
        path,
        access | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    ) {
        Ok(fd) => Some(std::fs::File::from(fd)),
        Err(rustix::io::Errno::ACCESS) if std::fs::symlink_metadata(path)?.is_dir() => None,
        Err(error) => return Err(error.into()),
    };
    let claim = StagedDirClaim { hold };
    if claim.hold.is_some() {
        claim.require_path(path)?;
    }
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
