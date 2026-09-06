//! Persists a [`SaveStore`] flash image as one host file.
//!
//! This module resolves save paths, performs exact-length reads, and provides
//! locking and atomic writes. Save contents and slot validation remain owned by
//! [`SaveStore`].

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

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
        got: usize,
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
        file.take(oversized_image_probe_len as u64)
            .read_to_end(&mut bytes)
            .map_err(|source| SaveFileError::Read {
                path: self.path.clone(),
                source,
            })?;
        if bytes.len() != store::FLASH_IMAGE_LEN {
            return Err(SaveFileError::BadLength {
                path: self.path.clone(),
                expected: store::FLASH_IMAGE_LEN,
                got: bytes.len(),
            });
        }
        SaveStore::from_flash_image(&bytes)
            .map(Some)
            .ok_or_else(|| SaveFileError::BadLength {
                path: self.path.clone(),
                expected: store::FLASH_IMAGE_LEN,
                got: bytes.len(),
            })
    }

    /// Atomically replaces the save file with `store`'s synchronised image.
    ///
    /// The directory holding the save entry -- `.` for a bare relative path --
    /// is best-effort synchronised after the rename; see [`SaveFile::ensure_parent_directory`].
    ///
    /// # Errors
    ///
    /// [`SaveFileError::CreateDirectory`] if the parent directory could not
    /// be created; [`SaveFileError::Write`] if the temporary file could not
    /// be written, synced, or renamed into place.
    pub fn write(&self, store: &SaveStore) -> Result<(), SaveFileError> {
        self.write_with(store, Self::sync_directory_best_effort, || {
            self.staging_path()
        })
    }

    /// As [`SaveFile::write`], synchronising through the given `sync_directory`
    /// and drawing each staging attempt's name from `staging_path`, rather than
    /// always [`SaveFile::sync_directory_best_effort`] and [`SaveFile::staging_path`].
    fn write_with(
        &self,
        store: &SaveStore,
        mut sync_directory: impl FnMut(&Path),
        staging_path: impl FnMut() -> PathBuf,
    ) -> Result<(), SaveFileError> {
        self.ensure_parent_directory()?;

        let write_error = |source: std::io::Error| SaveFileError::Write {
            path: self.path.clone(),
            source,
        };
        let staged = Self::stage(staging_path, store.flash_image()).map_err(write_error)?;
        if let Err(source) = std::fs::rename(&staged, &self.path) {
            drop(std::fs::remove_file(&staged));
            return Err(write_error(source));
        }
        if let Some(containing) = Self::directory_containing(&self.path) {
            sync_directory(containing);
        }
        Ok(())
    }

    /// Bound on retries after a staging-name collision before giving up.
    const MAX_STAGING_ATTEMPTS: u32 = 8;

    /// Stages `bytes` under a fresh name from `next_path` on every attempt,
    /// retrying a name collision up to [`Self::MAX_STAGING_ATTEMPTS`] times;
    /// returns the path actually written.
    fn stage(mut next_path: impl FnMut() -> PathBuf, bytes: &[u8]) -> std::io::Result<PathBuf> {
        let mut last_collision = None;
        for _ in 0..Self::MAX_STAGING_ATTEMPTS {
            let path = next_path();
            match Self::write_and_sync(&path, bytes) {
                Ok(()) => return Ok(path),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    last_collision = Some(err);
                }
                Err(err) => return Err(err),
            }
        }
        Err(last_collision.unwrap_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "exhausted staging attempts",
            )
        }))
    }

    /// Opens `path` exclusively -- refusing an existing file, directory, or
    /// symlink there instead of following or truncating it -- then writes and
    /// syncs `bytes`, removing `path` again on any failure once past that
    /// open so this call never deletes a path a different caller created.
    fn write_and_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write as _;

        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        let result = (|| {
            let mut staged = std::io::BufWriter::new(file);
            staged.write_all(bytes)?;
            staged.flush()?;
            staged.get_ref().sync_all()
        })();
        if result.is_err() {
            drop(std::fs::remove_file(path));
        }
        result
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

    /// A collision-resistant sibling staging name -- unguessable to a planted
    /// symlink and unlikely to be shared by a second writer; [`Self::stage`]
    /// retries the rare exact collision, so this needs resistance, not proof.
    fn staging_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(".tmp.");
        name.push(Self::unique_component());
        PathBuf::from(name)
    }

    /// `std`-only entropy: process id, clock nanoseconds, and a fresh
    /// `RandomState`-derived salt, which alone already differs between two
    /// calls at the same nanosecond.
    fn unique_component() -> String {
        use std::hash::{BuildHasher, Hasher};

        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let salt = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish();
        format!("{pid:x}.{nanos:x}.{salt:x}")
    }
}

/// Holds a [`SaveFile::lock`] until dropped.
#[derive(Debug)]
pub struct SaveFileGuard {
    _lock_file: std::fs::File,
}

#[cfg(test)]
mod tests;
