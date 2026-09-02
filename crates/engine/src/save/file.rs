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
    /// The parent directory is synchronised after the rename when the host
    /// permits opening directories. That final synchronisation is best effort.
    /// If this is the first save under a path whose ancestors did not yet
    /// exist, every newly created ancestor's own parent is also synchronised,
    /// outermost first, so the directory itself survives a power loss after a
    /// reported success.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::CreateDirectory`] if the parent directory could not
    /// be created; [`SaveFileError::Write`] if the temporary file could not
    /// be written, synced, or renamed into place.
    pub fn write(&self, store: &SaveStore) -> Result<(), SaveFileError> {
        let parent = self.ensure_parent_directory()?;

        let staging_path = self.staging_path_for_process();
        let write_error = |source: std::io::Error| SaveFileError::Write {
            path: self.path.clone(),
            source,
        };
        Self::write_and_sync(&staging_path, store.flash_image()).map_err(write_error)?;
        if let Err(source) = std::fs::rename(&staging_path, &self.path) {
            drop(std::fs::remove_file(&staging_path));
            return Err(write_error(source));
        }
        if let Some(parent) = parent {
            Self::sync_directory_best_effort(parent);
        }
        Ok(())
    }

    fn write_and_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write as _;

        let file = std::fs::File::create(path)?;
        let mut staged = std::io::BufWriter::new(file);
        staged.write_all(bytes)?;
        staged.flush()?;
        staged.get_ref().sync_all()
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
    /// Production locks before it reads or writes, so a first save's parent
    /// directory is usually created here rather than in [`SaveFile::write`].
    /// Every ancestor this call creates is best-effort synchronised, outermost
    /// first, but only once this lock is held: creating the directory has to
    /// happen first, since the lock file itself lives there, but a second
    /// locker's own scan would otherwise see those same ancestors as
    /// pre-existing and skip synchronising them. Deferring the synchronising
    /// until after locking means that second locker still blocks on
    /// [`std::fs::File::lock`] until the first locker's synchronising -- and
    /// with it the entire read-modify-write cycle -- has completed, so nobody
    /// can report success before the ancestors this call created are durable.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::CreateDirectory`] if the parent directory could not
    /// be created; [`SaveFileError::Lock`] if the lock file could not be
    /// created or locked.
    pub fn lock(&self) -> Result<SaveFileGuard, SaveFileError> {
        self.lock_with(Self::sync_directory_best_effort)
    }

    /// As [`SaveFile::lock`], but synchronising each newly created ancestor's
    /// parent through `sync_directory` instead of
    /// [`SaveFile::sync_directory_best_effort`]. Kept separate so tests can
    /// observe both which directories creation reports as new and that the
    /// synchronising happens while the lock is held, without depending on a
    /// real fsync's unobservable effect.
    fn lock_with(
        &self,
        mut sync_directory: impl FnMut(&Path),
    ) -> Result<SaveFileGuard, SaveFileError> {
        let created = self.create_parent_directory()?;
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
        for level in &created {
            sync_directory(Self::directory_containing(level));
        }
        Ok(SaveFileGuard { _lock_file: file })
    }

    /// Creates the save file's parent directory (and any missing ancestors),
    /// then immediately best-effort synchronises every level this call
    /// created, outermost first. [`SaveFile::write`] uses this directly: it
    /// has no lock of its own to defer synchronising into, so on the rare
    /// direct, unlocked first write, this is the best durability available.
    /// [`SaveFile::lock`] instead calls [`SaveFile::create_parent_directory`]
    /// and defers synchronising until the lock is held, closing the race
    /// this immediate version cannot: production always locks before it
    /// writes, so this path only ever runs when the lock already created
    /// (and synchronised) every level.
    fn ensure_parent_directory(&self) -> Result<Option<&Path>, SaveFileError> {
        let created = self.create_parent_directory()?;
        for level in &created {
            Self::sync_directory_best_effort(Self::directory_containing(level));
        }
        Ok(self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty()))
    }

    /// Creates the save file's parent directory (and any missing ancestors)
    /// and reports which levels this call created, outermost first, without
    /// synchronising anything -- callers choose when synchronising is safe.
    fn create_parent_directory(&self) -> Result<Vec<PathBuf>, SaveFileError> {
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        let Some(parent) = parent else {
            return Ok(Vec::new());
        };
        let created = Self::directories_to_create(parent);
        std::fs::create_dir_all(parent).map_err(|source| SaveFileError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;
        Ok(created)
    }

    /// The directory levels `create_dir_all(dir)` would need to create,
    /// outermost first: every ancestor from the first missing level down to
    /// `dir` itself. Empty when `dir` already exists.
    ///
    /// This walks the same path `create_dir_all` will, so a concurrent
    /// creator can make the answer stale by the time creation runs. That
    /// only ever widens which parents this call itself synchronises -- never
    /// narrows it -- for [`SaveFile::write`]'s unlocked, immediate use.
    /// [`SaveFile::lock`] additionally defers its synchronising until the
    /// lock is held, so a concurrent locker that observes fewer levels as
    /// missing here still cannot report success before the first locker's
    /// synchronising has happened.
    fn directories_to_create(dir: &Path) -> Vec<PathBuf> {
        let mut missing = Vec::new();
        let mut current = Some(dir);
        while let Some(path) = current.filter(|path| !path.as_os_str().is_empty()) {
            if path.is_dir() {
                break;
            }
            missing.push(path.to_path_buf());
            current = path.parent();
        }
        missing.reverse();
        missing
    }

    /// The directory that `create_dir_all` records `level`'s entry in: its
    /// parent, or `.` for a bare relative level with no parent component.
    fn directory_containing(level: &Path) -> &Path {
        level
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    }

    fn lock_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(".lock");
        PathBuf::from(name)
    }

    fn staging_path_for_process(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(format!(".tmp.{}", std::process::id()));
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
