//! Persists a [`SaveStore`] flash image as one host file.
//!
//! This module resolves save paths, performs exact-length reads, and provides
//! locking and atomic writes. Save contents and slot validation remain owned by
//! [`SaveStore`]; the sibling entry a write is staged into is owned by the
//! private `staging` submodule.

mod staging;

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use self::staging::{StagedSave, StagingArea};
use super::store::{self, SaveStore};

/// Environment variable containing an explicit save-file path.
pub const SAVE_PATH_ENV: &str = "POKEEMERALD_RS_SAVE";

/// Per-user data subdirectory containing the default save file.
pub const SAVE_DIR_NAME: &str = "pokeemerald-rs";

/// Default save-file name.
pub const SAVE_FILE_NAME: &str = "pokeemerald.sav";

/// File-system or path-resolution failure while accessing a save file.
#[derive(Debug)]
pub enum SaveFileError {
    /// No explicit path or per-user data directory is available.
    NoDataDirectory,
    /// Creating the save file's parent directory failed.
    CreateDirectory {
        /// The directory that could not be created.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },
    /// Reading an existing save file failed.
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },
    /// Writing the save file failed.
    Write {
        /// The file that could not be written.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },
    /// Creating or locking the sibling lock file failed.
    Lock {
        /// The lock file that could not be created or locked.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },
    /// The file length does not match [`store::FLASH_IMAGE_LEN`].
    BadLength {
        /// The file whose length was wrong.
        path: PathBuf,
        /// Required image length.
        expected: usize,
        /// The length actually found.
        got: u64,
    },
}

impl std::fmt::Display for SaveFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDataDirectory => write!(
                f,
                "save file: no per-user data directory in the environment -- \
                 set ${SAVE_PATH_ENV} to choose one explicitly"
            ),
            Self::CreateDirectory { path, source } => {
                write!(f, "save file: creating {} failed: {source}", path.display())
            }
            Self::Read { path, source } => {
                write!(f, "save file: reading {} failed: {source}", path.display())
            }
            Self::Write { path, source } => {
                write!(f, "save file: writing {} failed: {source}", path.display())
            }
            Self::Lock { path, source } => {
                write!(f, "save file: locking {} failed: {source}", path.display())
            }
            Self::BadLength {
                path,
                expected,
                got,
            } => write!(
                f,
                "save file: {} is {got} bytes, not a {expected}-byte save image",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SaveFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateDirectory { source, .. }
            | Self::Read { source, .. }
            | Self::Write { source, .. }
            | Self::Lock { source, .. } => Some(source),
            Self::NoDataDirectory | Self::BadLength { .. } => None,
        }
    }
}

/// Host convention used to resolve a per-user data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFamily {
    /// `%APPDATA%`, else `%USERPROFILE%\AppData\Roaming`.
    Windows,
    /// `$HOME/Library/Application Support`.
    MacOs,
    /// The XDG Base Directory Specification: `$XDG_DATA_HOME` when
    /// absolute, else `$HOME/.local/share`.
    Xdg,
}

impl HostFamily {
    /// The family this binary was compiled for.
    #[must_use]
    pub const fn host() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Xdg
        }
    }
}

/// Resolves `family`'s data directory through `env`, ignoring empty values.
#[must_use]
pub fn data_dir_for(family: HostFamily, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let non_empty_path = |name: &str| {
        env(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    match family {
        HostFamily::Windows => non_empty_path("APPDATA").or_else(|| {
            non_empty_path("USERPROFILE").map(|home| home.join("AppData").join("Roaming"))
        }),
        HostFamily::MacOs => {
            non_empty_path("HOME").map(|home| home.join("Library").join("Application Support"))
        }
        HostFamily::Xdg => non_empty_path("XDG_DATA_HOME")
            .filter(|dir| is_absolute_xdg_path(dir.as_os_str()))
            .or_else(|| non_empty_path("HOME").map(|home| home.join(".local").join("share"))),
    }
}

/// Whether `path` is absolute under the XDG Base Directory Specification's
/// POSIX path rules, independently of the platform running this binary.
fn is_absolute_xdg_path(path: &OsStr) -> bool {
    path.as_encoded_bytes().starts_with(b"/")
}

/// Resolves this host's save-file path.
///
/// # Errors
///
/// [`SaveFileError::NoDataDirectory`] if [`SAVE_PATH_ENV`] is unset and no
/// per-user data directory can be derived.
pub fn default_save_path() -> Result<PathBuf, SaveFileError> {
    default_save_path_from(HostFamily::host(), |name: &str| std::env::var_os(name))
}

/// Resolves a save-file path from an explicit host family and environment.
///
/// # Errors
///
/// As [`default_save_path`].
pub fn default_save_path_from(
    family: HostFamily,
    env: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, SaveFileError> {
    if let Some(explicit) = env(SAVE_PATH_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(explicit));
    }
    data_dir_for(family, env)
        .map(|dir| dir.join(SAVE_DIR_NAME).join(SAVE_FILE_NAME))
        .ok_or(SaveFileError::NoDataDirectory)
}

/// Path-backed persistence for a [`SaveStore`] flash image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFile {
    path: PathBuf,
}

impl SaveFile {
    /// A save file at an explicit `path`.
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// A save file at [`default_save_path`].
    ///
    /// # Errors
    ///
    /// As [`default_save_path`].
    pub fn default_location() -> Result<Self, SaveFileError> {
        Ok(Self::at(default_save_path()?))
    }

    /// Where this save file lives.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the path currently names a file.
    ///
    /// This advisory check may become stale before a later operation.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// Reads an exact-length flash image, or returns `Ok(None)` when absent.
    ///
    /// Call [`SaveStore::load`] on the returned store to reconstruct its
    /// counters and validate its contents.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::Read`] for any I/O failure other than "not found";
    /// [`SaveFileError::BadLength`] if the file is not
    /// [`store::FLASH_IMAGE_LEN`] bytes.
    pub fn read(&self) -> Result<Option<SaveStore>, SaveFileError> {
        use std::io::Read as _;

        let file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(SaveFileError::Read {
                    path: self.path.clone(),
                    source,
                })
            }
        };
        let oversized_image_probe_len = store::FLASH_IMAGE_LEN + 1;
        let mut bytes = Vec::with_capacity(oversized_image_probe_len);
        // Borrow so the handle survives for `observed_length`'s metadata query.
        (&file)
            .take(oversized_image_probe_len as u64)
            .read_to_end(&mut bytes)
            .map_err(|source| SaveFileError::Read {
                path: self.path.clone(),
                source,
            })?;
        if bytes.len() != store::FLASH_IMAGE_LEN {
            return Err(SaveFileError::BadLength {
                path: self.path.clone(),
                expected: store::FLASH_IMAGE_LEN,
                got: self.observed_length(&file, &bytes)?,
            });
        }
        SaveStore::from_flash_image(&bytes)
            .map(Some)
            .ok_or_else(|| SaveFileError::BadLength {
                path: self.path.clone(),
                expected: store::FLASH_IMAGE_LEN,
                got: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            })
    }

    /// The real length behind a bounded probe read's `bytes.len()`: exact
    /// already if the probe finished short, else asked of `file`'s
    /// metadata, since the probe cap is identical for every oversized file.
    fn observed_length(&self, file: &std::fs::File, bytes: &[u8]) -> Result<u64, SaveFileError> {
        let probed = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if bytes.len() < store::FLASH_IMAGE_LEN {
            return Ok(probed);
        }
        let metadata_len = file
            .metadata()
            .map_err(|source| SaveFileError::Read {
                path: self.path.clone(),
                source,
            })?
            .len();
        // A shrink between the read and this call must not under-report
        // what was actually read.
        Ok(metadata_len.max(probed))
    }

    /// Atomically replaces the save file with `store`'s synchronised image.
    ///
    /// The directory holding the save entry -- `.` for a bare relative path --
    /// is best-effort synchronised after the rename; see the private
    /// `SaveFile::ensure_parent_directory`.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::CreateDirectory`] if the parent directory could not
    /// be created; [`SaveFileError::Write`] if the temporary file could not
    /// be written, synced, or renamed into place, or if something replaced
    /// it between those two steps.
    pub fn write(&self, store: &SaveStore) -> Result<(), SaveFileError> {
        self.write_with(
            store,
            Self::sync_directory_best_effort,
            |bytes| StagingArea::beside(&self.path).stage(bytes),
            |_| {},
        )
    }

    /// As [`SaveFile::write`], synchronising through the given `sync_directory`
    /// and staging through `stage`, rather than always
    /// [`SaveFile::sync_directory_best_effort`] and
    /// [`StagingArea::stage`].
    ///
    /// `before_rename` runs on the staged path once it holds the image and
    /// before anything promotes it, which is the only point from which the
    /// window this call has to defend can be occupied on purpose.
    fn write_with(
        &self,
        store: &SaveStore,
        mut sync_directory: impl FnMut(&Path),
        stage: impl FnOnce(&[u8]) -> std::io::Result<StagedSave>,
        before_rename: impl FnOnce(&Path),
    ) -> Result<(), SaveFileError> {
        self.ensure_parent_directory()?;

        let write_error = |source: std::io::Error| SaveFileError::Write {
            path: self.path.clone(),
            source,
        };
        let mut staged = stage(store.flash_image()).map_err(write_error)?;
        before_rename(&staged.path);
        // Nothing in `std` fuses this check to the rename below, so a
        // replacement landing between the two is still promoted; the
        // exclusive create, the unguessable name, and the hold kept open
        // across the check bound that window rather than close it. On
        // Windows the hold admits nobody at all, so the window is only as
        // wide as its release: from `release_hold` to the rename, and on to
        // the cleanup unlink if that rename fails.
        match staged.still_ours() {
            Ok(true) => {}
            Ok(false) => {
                return Err(write_error(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "the staged image at {} was replaced before it could be renamed into place",
                        staged.path.display()
                    ),
                )))
            }
            Err(unreadable) => return Err(write_error(staged.remove_after(unreadable))),
        }
        staged.release_hold();
        if let Err(source) = std::fs::rename(&staged.path, &self.path) {
            return Err(write_error(staged.remove_after(source)));
        }
        if let Some(containing) = Self::directory_containing(&self.path) {
            sync_directory(containing);
        }
        Ok(())
    }

    fn sync_directory_best_effort(path: &Path) {
        drop(std::fs::File::open(path).and_then(|directory| directory.sync_all()));
    }

    /// Acquires an advisory inter-process lock for this save path.
    ///
    /// The lock lives on a sibling `.lock` file, not the save file itself:
    /// [`SaveFile::write`] replaces the save's inode by rename, and a lock
    /// on a replaced inode would silently stop excluding anyone who opened
    /// the path afterwards.
    ///
    /// Hold the returned guard across the complete read-modify-write cycle.
    ///
    /// On a first save, creates the missing hierarchy and best-effort
    /// synchronises every ancestor while the lock is held.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::CreateDirectory`] if the parent directory could not
    /// be created; [`SaveFileError::Lock`] if the lock file could not be
    /// created or locked.
    pub fn lock(&self) -> Result<SaveFileGuard, SaveFileError> {
        self.lock_with(Self::sync_directory_best_effort)
    }

    /// As [`SaveFile::lock`], synchronising through the given `sync_directory`
    /// rather than always [`SaveFile::sync_directory_best_effort`].
    fn lock_with(&self, sync_directory: impl FnMut(&Path)) -> Result<SaveFileGuard, SaveFileError> {
        let first_save = !self.exists();
        let parent = self.create_parent_directory()?;
        let path = self.lock_path();
        let lock_error = |source: std::io::Error| SaveFileError::Lock {
            path: path.clone(),
            source,
        };
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(lock_error)?;
        file.lock().map_err(lock_error)?;
        if first_save {
            if let Some(parent) = parent {
                Self::sync_ancestor_chain(parent, sync_directory);
            }
        }
        Ok(SaveFileGuard { _lock_file: file })
    }

    /// Creates the save file's parent directory and any missing ancestors;
    /// on a first save, best-effort synchronises every ancestor.
    fn ensure_parent_directory(&self) -> Result<Option<&Path>, SaveFileError> {
        let first_save = !self.exists();
        let parent = self.create_parent_directory()?;
        if first_save {
            if let Some(parent) = parent {
                Self::sync_ancestor_chain(parent, Self::sync_directory_best_effort);
            }
        }
        Ok(parent)
    }

    /// Creates the save file's parent directory and any missing ancestors,
    /// without synchronising anything.
    fn create_parent_directory(&self) -> Result<Option<&Path>, SaveFileError> {
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        if let Some(parent) = parent {
            std::fs::create_dir_all(parent).map_err(|source| SaveFileError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        Ok(parent)
    }

    /// `dir` and every non-empty ancestor above it, outermost first.
    fn ancestor_chain(dir: &Path) -> Vec<PathBuf> {
        let mut levels = Vec::new();
        let mut current = Some(dir);
        while let Some(path) = current.filter(|path| !path.as_os_str().is_empty()) {
            levels.push(path.to_path_buf());
            current = path.parent();
        }
        levels.reverse();
        levels
    }

    /// Best-effort synchronises the containing directory of each level of
    /// `dir`'s ancestor chain, outermost first.
    fn sync_ancestor_chain(dir: &Path, mut sync_directory: impl FnMut(&Path)) {
        for level in Self::ancestor_chain(dir) {
            if let Some(containing) = Self::directory_containing(&level) {
                sync_directory(containing);
            }
        }
    }

    /// The directory holding `level`'s entry: its parent, `.` for a bare
    /// relative name, or `None` for a root, drive prefix, or `.`/`..` level.
    fn directory_containing(level: &Path) -> Option<&Path> {
        use std::path::Component;

        match level.components().next_back() {
            Some(Component::Normal(_)) => Some(
                level
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new(".")),
            ),
            None
            | Some(
                Component::RootDir
                | Component::Prefix(_)
                | Component::CurDir
                | Component::ParentDir,
            ) => None,
        }
    }

    fn lock_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(".lock");
        PathBuf::from(name)
    }
}

/// Holds a [`SaveFile::lock`] until dropped.
#[derive(Debug)]
pub struct SaveFileGuard {
    _lock_file: std::fs::File,
}

#[cfg(test)]
mod tests;
