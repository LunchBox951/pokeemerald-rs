//! Save-file persistence (S-5, ladders to I-6): the on-disk home of a
//! [`SaveStore`]'s flash image.
//!
//! Behavioural counterpart `(behavioral-fidelity)` of the *storage medium*
//! upstream's save code writes through, not of any one upstream function.
//! `pokeemerald/src/save.c` talks to AGB flash via `agb_flash`'s
//! `ProgramFlashSectorAndVerify`/`ReadFlash`; a native port has no flash, so
//! the whole [`SaveStore`] image ([`store::FLASH_IMAGE_LEN`] bytes — every
//! sector of both rotating slots) is persisted as one ordinary file. The
//! sector format, checksums, slot rotation, and corrupt-slot fallback are
//! untouched: they already live in [`super::sector`]/[`super::store`], and
//! this module only moves that byte image between memory and disk.
//!
//! # What this module is *not*
//!
//! It is not a second validation layer. A file that reads back as the right
//! length is handed straight to [`SaveStore::from_flash_image`], and whether
//! its contents are usable is [`SaveStore::load`]'s existing
//! `GetSaveValidStatus`/`CopySaveSlotData` decision (issues #130/#138) —
//! never re-derived here. The only failures this module reports are the ones
//! a file system can produce and flash cannot: a missing/unreadable/
//! unwritable path, an unresolvable per-user data directory, and a file
//! whose length is not the flash image's ([`SaveFileError`]).
//!
//! # Where the save file lives
//!
//! [`default_save_path`], in priority order:
//!
//! 1. `$POKEEMERALD_RS_SAVE` ([`SAVE_PATH_ENV`]), used verbatim as the full
//!    file path when set and non-empty. This is the seam headless tests and
//!    CI use to redirect the save into a scratch directory, and it is the
//!    only reason this crate needs no `dirs`-style dependency
//!    `(minimal-deps)`.
//! 2. Otherwise `<per-user data dir>/pokeemerald-rs/pokeemerald.sav`, with
//!    the data directory derived from the host family's own conventional
//!    environment variables ([`data_dir_for`]) — `std::env` only, no
//!    platform APIs:
//!
//!    | family | data directory |
//!    |--------|----------------|
//!    | Windows | `%APPDATA%`, else `%USERPROFILE%\AppData\Roaming` |
//!    | macOS | `$HOME/Library/Application Support` |
//!    | other (Linux/BSD) | `$XDG_DATA_HOME` if absolute, else `$HOME/.local/share` |
//!
//!    The XDG rule ("relative paths are invalid and must be ignored") is the
//!    Base Directory Specification's own, which is why a relative
//!    `$XDG_DATA_HOME` falls through to `$HOME` rather than being joined.
//!
//! # Writes are atomic
//!
//! [`SaveFile::write`] writes a sibling temporary file, **`sync_all`s it to
//! the storage device**, and only then renames it over the destination, so
//! an interrupted write can never leave a truncated image behind. Both
//! halves are load-bearing: the rename is what makes the swap atomic against
//! the process dying mid-write, and the `sync_all` before it is what keeps a
//! *power* loss from renaming a file whose bytes never reached the disk. The
//! parent directory is `sync_all`ed afterwards too, best-effort — that one
//! is what makes the rename itself durable, but not every platform lets a
//! directory be opened as a file, so its failure is ignored rather than
//! reported.
//!
//! This has no upstream counterpart — it is the file-system analogue of the
//! guarantee upstream gets from writing whole 4 KiB flash sectors and
//! verifying each one (`ProgramFlashSectorAndVerify`), and it matters more
//! here than there: a half-written *file* would fail the length check and
//! lose **both** rotating slots at once, where a torn flash write costs at
//! most the one sector being programmed — defeating the very fallback
//! [`SaveStore::load`] exists to provide.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::store::{self, SaveStore};

/// The environment variable that overrides [`default_save_path`] with a full
/// file path (module docs).
pub const SAVE_PATH_ENV: &str = "POKEEMERALD_RS_SAVE";

/// The per-user data subdirectory the default save path lives in.
pub const SAVE_DIR_NAME: &str = "pokeemerald-rs";

/// The default save file's own name.
pub const SAVE_FILE_NAME: &str = "pokeemerald.sav";

/// Why a save-file read or write failed.
///
/// Concrete per-crate enum `(oop-boundaries)` — no `anyhow`. Every variant
/// is a *file system* failure; nothing here describes save *contents*, which
/// [`super::store::SaveStatus`] already owns (module docs).
#[derive(Debug)]
pub enum SaveFileError {
    /// No per-user data directory could be derived from the environment —
    /// none of the variables in the module docs' table were set. The
    /// [`SAVE_PATH_ENV`] override always avoids this.
    NoDataDirectory,
    /// Creating the save file's parent directory failed.
    CreateDirectory {
        /// The directory that could not be created.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },
    /// Reading the save file failed (for a reason other than it not
    /// existing, which [`SaveFile::read`] reports as `Ok(None)`).
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
    /// The file exists but is not a flash image: its length is not
    /// [`store::FLASH_IMAGE_LEN`].
    BadLength {
        /// The file whose length was wrong.
        path: PathBuf,
        /// [`store::FLASH_IMAGE_LEN`].
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
            | Self::Write { source, .. } => Some(source),
            Self::NoDataDirectory | Self::BadLength { .. } => None,
        }
    }
}

/// Which set of environment variables names the per-user data directory
/// (module docs' table).
///
/// An explicit value rather than a `#[cfg]` fork so all three derivations
/// compile — and are unit-tested — on every host `(behavioral-fidelity)`:
/// this port ships Linux, macOS, and Windows binaries from one workspace,
/// and a path rule that only its own platform ever exercises is a path rule
/// nobody checks.
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

/// The per-user data directory `family` conventionally uses, looked up
/// through `env` (`std::env::var_os` in production, a fixture in tests), or
/// `None` if none of that family's variables are set usefully.
#[must_use]
pub fn data_dir_for(family: HostFamily, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let non_empty = |name: &str| {
        env(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    match family {
        HostFamily::Windows => non_empty("APPDATA")
            .or_else(|| non_empty("USERPROFILE").map(|home| home.join("AppData").join("Roaming"))),
        HostFamily::MacOs => {
            non_empty("HOME").map(|home| home.join("Library").join("Application Support"))
        }
        HostFamily::Xdg => non_empty("XDG_DATA_HOME")
            .filter(|dir| dir.is_absolute())
            .or_else(|| non_empty("HOME").map(|home| home.join(".local").join("share"))),
    }
}

/// The save file's path for this host (module docs' priority order).
///
/// # Errors
///
/// [`SaveFileError::NoDataDirectory`] if [`SAVE_PATH_ENV`] is unset and no
/// per-user data directory can be derived.
pub fn default_save_path() -> Result<PathBuf, SaveFileError> {
    default_save_path_from(HostFamily::host(), |name: &str| std::env::var_os(name))
}

/// [`default_save_path`]'s whole body, parameterized on the host family and
/// the environment lookup so every branch is testable on any host (see
/// [`HostFamily`]).
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

/// One save file on disk: a path, plus the read/write of a [`SaveStore`]'s
/// flash image through it (module docs).
///
/// Owns no save state of its own `(oop-boundaries)` — it is deliberately a
/// path and two methods, so the store stays the single home of everything
/// about save *contents*.
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

    /// Whether a file exists at [`SaveFile::path`]. Advisory only —
    /// [`SaveFile::read`] reports the authoritative answer without the
    /// time-of-check race.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// Read the flash image back into a [`SaveStore`], or `Ok(None)` if no
    /// file exists yet.
    ///
    /// `Ok(None)` is the "never saved" case, and it is deliberately *not* an
    /// error: it is this port's counterpart of erased flash, and
    /// [`SaveStore::new`]'s all-`0xFF` buffer is what a caller should use in
    /// its place (which then resolves to [`store::SaveStatus::Empty`], the
    /// same status upstream's `GetSaveValidStatus` returns for a cart that
    /// has never been written).
    ///
    /// The returned store's counters are zero until [`SaveStore::load`] is
    /// called — see [`SaveStore::from_flash_image`].
    ///
    /// # Errors
    ///
    /// [`SaveFileError::Read`] for any I/O failure other than "not found";
    /// [`SaveFileError::BadLength`] if the file is not
    /// [`store::FLASH_IMAGE_LEN`] bytes.
    pub fn read(&self) -> Result<Option<SaveStore>, SaveFileError> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(SaveFileError::Read {
                    path: self.path.clone(),
                    source,
                })
            }
        };
        SaveStore::from_flash_image(&bytes)
            .map(Some)
            .ok_or_else(|| SaveFileError::BadLength {
                path: self.path.clone(),
                expected: store::FLASH_IMAGE_LEN,
                got: bytes.len(),
            })
    }

    /// Write `store`'s flash image out, creating the parent directory if
    /// needed.
    ///
    /// Atomic and durable: the bytes go to a sibling temporary file that is
    /// flushed and `sync_all`ed before being renamed over the destination
    /// (module docs' "Writes are atomic").
    ///
    /// # Errors
    ///
    /// [`SaveFileError::CreateDirectory`] if the parent directory could not
    /// be created; [`SaveFileError::Write`] if the temporary file could not
    /// be written, synced, or renamed into place.
    pub fn write(&self, store: &SaveStore) -> Result<(), SaveFileError> {
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

        let temporary = self.temporary_path();
        let write_error = |source: std::io::Error| SaveFileError::Write {
            path: self.path.clone(),
            source,
        };
        Self::stage(&temporary, store.flash_image()).map_err(write_error)?;
        std::fs::rename(&temporary, &self.path).map_err(|source| {
            // The rename is the only step that can leave the temporary file
            // behind; a best-effort cleanup keeps a failed save from
            // littering the data directory. Its own failure is not worth
            // reporting over the rename failure that caused it.
            drop(std::fs::remove_file(&temporary));
            write_error(source)
        })?;
        if let Some(parent) = parent {
            // The staged bytes are already durable; this is what makes the
            // *rename* durable too. Best-effort by design (module docs):
            // Windows will not open a directory as a file, and a save that
            // is on disk but whose directory entry is only in the page cache
            // is not worth failing an otherwise complete write over.
            drop(std::fs::File::open(parent).and_then(|dir| dir.sync_all()));
        }
        Ok(())
    }

    /// Write `bytes` to `path` and get them onto the storage device: the
    /// staging half of [`SaveFile::write`]'s durability guarantee.
    ///
    /// `flush` alone only empties the user-space buffer into the kernel;
    /// `sync_all` is what the rename below it needs to be able to assume.
    fn stage(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write as _;

        let file = std::fs::File::create(path)?;
        let mut staged = std::io::BufWriter::new(file);
        staged.write_all(bytes)?;
        staged.flush()?;
        staged.get_ref().sync_all()
    }

    /// The sibling path [`SaveFile::write`] stages bytes at.
    ///
    /// A fixed `.tmp` suffix rather than a randomized name: two concurrent
    /// writers to one save file would be a bug in the caller (this port runs
    /// one game at a time), and a predictable name is one a crashed run
    /// leaves recoverable rather than scattering scratch files.
    fn temporary_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(".tmp");
        PathBuf::from(name)
    }
}

#[cfg(test)]
mod tests;
