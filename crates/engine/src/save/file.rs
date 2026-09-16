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

/// The one lock file [`SaveFile::lock`] uses in any save directory. See
/// [`SaveFile::lock_path`]. At most `_POSIX_NAME_MAX` (14) bytes, so it fits
/// every host whose save the staging code can already narrow a name onto.
pub const LOCK_FILE_NAME: &str = ".emerald.lock";

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
    /// The configured save path is the directory's own lock file.
    LockPathIsSave {
        /// The lock path the save path resolves to.
        path: PathBuf,
    },
    /// The lock slot leads somewhere else instead of being a file itself.
    LockPathIsAlias {
        /// The lock path that does not name the file it opens.
        path: PathBuf,
    },
    /// The lock slot is occupied by something other than a plain file.
    LockPathNotAPlainFile {
        /// The lock path that is not a plain file.
        path: PathBuf,
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
            Self::LockPathIsAlias { path } => write!(
                f,
                "save file: the lock slot {} does not name the file it opens -- each \
                 locker would follow it to whatever it led to at the time, so they \
                 would not exclude one another",
                path.display()
            ),
            Self::LockPathNotAPlainFile { path } => write!(
                f,
                "save file: the lock slot {} is not a plain file -- a directory, socket, \
                 or device there cannot carry a lock, and opening a FIFO would wait for \
                 a reader that never comes",
                path.display()
            ),
            Self::LockPathIsSave { path } => write!(
                f,
                "save file: this save resolves to {}, the lock file every save in that \
                 directory holds -- writing it would replace the inode they lock",
                path.display()
            ),
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
            Self::NoDataDirectory
            | Self::LockPathIsSave { .. }
            | Self::LockPathIsAlias { .. }
            | Self::LockPathNotAPlainFile { .. }
            | Self::BadLength { .. } => None,
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
    /// The lock lives on [`LOCK_FILE_NAME`], one fixed file per directory,
    /// never on the save file itself: [`SaveFile::write`] replaces the save's
    /// inode by rename, and a lock on a replaced inode would silently stop
    /// excluding anyone who opened the path afterwards.
    ///
    /// The name is fixed rather than derived from the save's, so no spelling
    /// can split it. A case-folding or normalising volume resolves several
    /// byte-different names to one save, and `std` cannot enumerate those
    /// aliases; deriving the lock from the basename therefore handed one save
    /// two locks. Every save in a directory now serialises on one lock, which
    /// only delays unrelated saves, where two locks for one save lose data. A
    /// short fixed name also always fits the component limit, so no save this
    /// host accepts is unlockable.
    ///
    /// A save that resolves to the lock file itself is refused, not locked:
    /// its rename would replace the very inode every locker holds.
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
    /// created, locked, or resolved; [`SaveFileError::LockPathIsSave`] if
    /// this save path resolves to the lock file;
    /// [`SaveFileError::LockPathIsAlias`] if the lock slot is a symlink or
    /// does not name the file it opens; [`SaveFileError::LockPathNotAPlainFile`]
    /// if something other than a plain file already occupies the slot.
    pub fn lock(&self) -> Result<SaveFileGuard, SaveFileError> {
        self.lock_with(Self::sync_directory_best_effort)
    }

    /// As [`SaveFile::lock`], synchronising through the given `sync_directory`
    /// rather than always [`SaveFile::sync_directory_best_effort`].
    fn lock_with(&self, sync_directory: impl FnMut(&Path)) -> Result<SaveFileGuard, SaveFileError> {
        let first_save = !self.exists();
        let parent = self.create_parent_directory()?;

        let path = self.lock_path();
        Self::refuse_an_unusable_slot(&path)?;
        let lock_error = |source| SaveFileError::Lock {
            path: path.clone(),
            source,
        };
        let file = Self::open_lock_file(&path).map_err(lock_error)?;
        file.lock().map_err(lock_error)?;
        if !Self::names_its_own_entry(&path, &file)? {
            return Err(SaveFileError::LockPathIsAlias { path });
        }
        if self.resolves_to(&path, &file)? {
            return Err(SaveFileError::LockPathIsSave { path });
        }
        if first_save {
            if let Some(parent) = parent {
                Self::sync_ancestor_chain(parent, sync_directory);
            }
        }
        Ok(SaveFileGuard { _lock_file: file })
    }

    /// Refuses a lock slot that is not this directory's own plain file,
    /// before anything opens it.
    ///
    /// Opening follows a symlink, so [`SaveFile::names_its_own_entry`] is
    /// too late to prevent its side effects: a dangling symlink would have
    /// the open create its target outside this directory, and one pointing
    /// at a FIFO would have the open block until some reader appeared,
    /// neither of which any later check can undo. Anything that is not a
    /// plain file is refused for the same reason -- it cannot carry a lock,
    /// and opening it can block or disturb it.
    ///
    /// An alias planted between this check and the open is still caught
    /// afterwards, by identity. What this forestalls is one already sitting
    /// in a directory the user controls, not a race.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::LockPathIsAlias`] if the slot is a symlink;
    /// [`SaveFileError::LockPathNotAPlainFile`] if it is anything else that
    /// is not a plain file; [`SaveFileError::Lock`] if it could not be
    /// inspected.
    fn refuse_an_unusable_slot(lock: &Path) -> Result<(), SaveFileError> {
        let slot = match std::fs::symlink_metadata(lock) {
            Ok(slot) => slot,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(source) => {
                return Err(SaveFileError::Lock {
                    path: lock.to_path_buf(),
                    source,
                })
            }
        };
        if slot.is_symlink() {
            return Err(SaveFileError::LockPathIsAlias {
                path: lock.to_path_buf(),
            });
        }
        if slot.is_file() {
            Ok(())
        } else {
            Err(SaveFileError::LockPathNotAPlainFile {
                path: lock.to_path_buf(),
            })
        }
    }

    /// Whether the entry at `lock` is the very file `file` was opened on.
    ///
    /// Opening follows a symlink, so a symlink planted in the fixed lock
    /// slot silently redirects every locker to whatever it points at *at
    /// that moment*. Two lockers that open it either side of the target's
    /// own rename then hold two different inodes and exclude nobody, so the
    /// slot must name the inode it opens rather than merely lead to one.
    /// Comparing the unfollowed entry against the open handle says exactly
    /// that, and catches an entry swapped since the open besides.
    ///
    /// A hard link is deliberately *not* refused. It is a name of its own,
    /// so the slot keeps naming the inode this guard holds however the
    /// link's other names are renamed, and a later locker opening the slot
    /// still lands on that same inode. Refusing it would break the
    /// hard-linking backup tools that snapshot a save directory, for a
    /// hazard it does not carry.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::Lock`] if either the entry or the handle could not
    /// be inspected.
    #[cfg(unix)]
    fn names_its_own_entry(lock: &Path, file: &std::fs::File) -> Result<bool, SaveFileError> {
        use std::os::unix::fs::MetadataExt;

        let lock_error = |source| SaveFileError::Lock {
            path: lock.to_path_buf(),
            source,
        };
        let held = file.metadata().map_err(lock_error)?;
        let slot = std::fs::symlink_metadata(lock).map_err(lock_error)?;
        Ok((slot.dev(), slot.ino()) == (held.dev(), held.ino()))
    }

    /// As the `unix` [`SaveFile::names_its_own_entry`], rejecting a
    /// symlinked slot outright.
    ///
    /// Windows exposes no *stable* by-handle identity, so the entry cannot
    /// be compared against the open handle; refusing a slot that is a
    /// symlink at all covers the same ground more bluntly. Creating one
    /// there demands a privilege an ordinary user does not hold, and
    /// [`SaveFile::deny_delete_sharing`] already stops an aliased target
    /// from being replaced while any guard holds it -- the step that would
    /// otherwise split two lockers across two inodes.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::Lock`] if the entry could not be inspected.
    #[cfg(not(unix))]
    fn names_its_own_entry(lock: &Path, _file: &std::fs::File) -> Result<bool, SaveFileError> {
        let slot = std::fs::symlink_metadata(lock).map_err(|source| SaveFileError::Lock {
            path: lock.to_path_buf(),
            source,
        })?;
        Ok(!slot.is_symlink())
    }

    /// Whether this save path names the same directory entry as the lock
    /// file `file`, opened at `lock`.
    ///
    /// Compares filesystem identity, not path text. A volume's aliases --
    /// ASCII and non-ASCII case folding, Unicode normalisation, symlinks,
    /// hard links -- cannot be enumerated from `std`, and canonical text
    /// does not stand in for identity: on a case-folding Linux directory
    /// `canonicalize` is `realpath`, which keeps the caller's own spelling,
    /// so `.EMERALD.LOCK` and `.emerald.lock` name one entry yet compare
    /// unequal. `st_dev` and `st_ino`, taken from the open lock handle
    /// rather than from its path, answer the question directly.
    ///
    /// Callers open the lock file first, so a save path aliasing it resolves
    /// to a file that exists by the time this runs.
    ///
    /// A save that does not exist is not the lock file. Any other failure to
    /// inspect it is reported rather than assumed distinct.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::Lock`] if either file could not be inspected.
    #[cfg(unix)]
    fn resolves_to(&self, lock: &Path, file: &std::fs::File) -> Result<bool, SaveFileError> {
        use std::os::unix::fs::MetadataExt;

        let held = file.metadata().map_err(|source| SaveFileError::Lock {
            path: lock.to_path_buf(),
            source,
        })?;
        let Some(save) = self.metadata_following_links()? else {
            return Ok(false);
        };
        Ok((save.dev(), save.ino()) == (held.dev(), held.ino()))
    }

    /// As the `unix` [`SaveFile::resolves_to`], comparing canonical paths.
    ///
    /// Windows exposes no *stable* by-handle identity accessor, but its
    /// `canonicalize` resolves through `GetFinalPathNameByHandle`, which
    /// reports the entry's own stored spelling rather than the caller's.
    /// One entry under two spellings therefore canonicalises to one path,
    /// so this recognises the aliases a Windows volume folds together.
    ///
    /// It does not recognise a hard link, whose own name canonicalises to
    /// itself; this is the early, friendly error for a direct alias, and
    /// [`SaveFile::deny_delete_sharing`] is what makes the refusal complete
    /// by stopping any alias from replacing the held entry at all.
    #[cfg(not(unix))]
    fn resolves_to(&self, lock: &Path, _file: &std::fs::File) -> Result<bool, SaveFileError> {
        let Some(_) = self.metadata_following_links()? else {
            return Ok(false);
        };
        let save = std::fs::canonicalize(&self.path).map_err(|source| SaveFileError::Lock {
            path: self.path.clone(),
            source,
        })?;
        let lock = std::fs::canonicalize(lock).map_err(|source| SaveFileError::Lock {
            path: lock.to_path_buf(),
            source,
        })?;
        Ok(save == lock)
    }

    /// This save's metadata, following a final symlink, or `None` when no
    /// file is there yet.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::Lock`] if the save exists but could not be inspected.
    fn metadata_following_links(&self) -> Result<Option<std::fs::Metadata>, SaveFileError> {
        match std::fs::metadata(&self.path) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(SaveFileError::Lock {
                path: self.path.clone(),
                source,
            }),
        }
    }

    /// Opens this directory's lock file, falling back to a read-only handle
    /// when the entry exists but this user may not write it.
    ///
    /// One lock file now serves every save in a directory, so whoever
    /// creates it does so under their own umask -- commonly `0644`. Without
    /// this fallback a second user saving to a shared directory, or the same
    /// user after one run under different privileges, would be shut out of
    /// their own perfectly valid save for good, which the former per-save
    /// sidecars never did.
    ///
    /// Locking wants an open file, not a writable one: `File::lock` is
    /// `flock(2)` on Unix and `LockFileEx` on Windows, both of which take a
    /// read-only handle, and these contents are never read or written.
    fn open_lock_file(path: &Path) -> std::io::Result<std::fs::File> {
        // An existing slot is opened first, so staging happens only for a
        // first save and leftover staging names can never block a valid lock.
        match Self::open_existing_lock(path) {
            Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => {}
            result => return result,
        }
        // Creation is exclusive and never follows a symlink, so a slot
        // swapped in after the vetting cannot make this create a file elsewhere.
        match Self::create_lock_file(path)? {
            Some(file) => Ok(file),
            None => Self::open_existing_lock(path),
        }
    }

    /// Opens the slot read-write, then read-only when this user cannot write
    /// it. A denial is retried briefly, since a creator on a filesystem
    /// without hard links publishes the slot before widening it.
    fn open_existing_lock(path: &Path) -> std::io::Result<std::fs::File> {
        const RETRIES: u32 = 5;
        let mut attempt = 0;
        loop {
            let mut existing = std::fs::OpenOptions::new();
            existing.read(true).write(true);
            Self::deny_delete_sharing(&mut existing);
            let denied = match existing.open(path) {
                Err(denied) if denied.kind() == std::io::ErrorKind::PermissionDenied => denied,
                result => return result,
            };
            let mut read_only = std::fs::OpenOptions::new();
            read_only.read(true);
            Self::deny_delete_sharing(&mut read_only);
            match read_only.open(path) {
                Err(still) if still.kind() == std::io::ErrorKind::PermissionDenied => {
                    if attempt == RETRIES {
                        return Err(denied);
                    }
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                result => return result,
            }
        }
    }

    /// Opens the lock file denying `FILE_SHARE_DELETE`, so that while any
    /// process holds it no rename can replace that entry and no unlink can
    /// remove it.
    ///
    /// This, not [`SaveFile::resolves_to`], is what makes the Windows
    /// refusal complete: with no stable by-handle identity to compare,
    /// canonical paths cannot recognise a hard link to the lock file, whose
    /// own name canonicalises to itself. Denying delete sharing closes the
    /// hazard instead of detecting it -- an aliasing save's publishing
    /// rename fails with a sharing violation, a reported error, rather than
    /// silently replacing the inode every locker holds.
    ///
    /// Read and write sharing stay permitted, so a second process still
    /// opens this same file to contend for the lock; only delete is denied.
    /// Creates the lock slot, or returns `None` when it already exists.
    ///
    /// A lock created under `umask 077` is `0600`, which no later owner can
    /// open; the file is staged beside the slot at `0666` and published with
    /// `link(2)`, which is atomic and never follows a symlink, so no process
    /// can observe a fresh lock before its mode is final. A pre-existing slot
    /// keeps whatever mode its owner chose. A filesystem without hard links
    /// falls back to creating the slot in place and widening it afterwards.
    #[cfg(unix)]
    fn create_lock_file(path: &Path) -> std::io::Result<Option<std::fs::File>> {
        use std::os::unix::fs::PermissionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create_new(true);
        // Never unlink an entry this call did not create: a taken staging
        // name, even by another thread of this process, just means the next.
        let pid = std::process::id();
        let (staged, file) = 'stage: {
            for attempt in 0..16u8 {
                let name = match attempt {
                    0 => format!(".lk{pid}"),
                    n => format!(".lk{pid}{n:x}"),
                };
                let candidate = path.with_file_name(name);
                match options.open(&candidate) {
                    Ok(file) => break 'stage (candidate, file),
                    Err(exists) if exists.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(other) => return Err(other),
                }
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "every staging name beside the lock slot is taken",
            ));
        };
        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o666));
        let linked = std::fs::hard_link(&staged, path);
        let _ = std::fs::remove_file(&staged);
        match linked {
            Ok(()) => Ok(Some(file)),
            Err(exists) if exists.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
            Err(_) => {
                let created = options.open(path);
                match created {
                    Ok(file) => {
                        let _ = file.set_permissions(std::fs::Permissions::from_mode(0o666));
                        Ok(Some(file))
                    }
                    Err(exists) if exists.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
                    Err(other) => Err(other),
                }
            }
        }
    }

    /// Windows has no umask; ACLs inherit from the directory, so the slot is
    /// created in place.
    #[cfg(not(unix))]
    fn create_lock_file(path: &Path) -> std::io::Result<Option<std::fs::File>> {
        let mut fresh = std::fs::OpenOptions::new();
        fresh.read(true).write(true).create_new(true);
        Self::deny_delete_sharing(&mut fresh);
        match fresh.open(path) {
            Ok(file) => Ok(Some(file)),
            Err(exists) if exists.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
            Err(other) => Err(other),
        }
    }

    #[cfg(windows)]
    fn deny_delete_sharing(options: &mut std::fs::OpenOptions) {
        use std::os::windows::fs::OpenOptionsExt;

        /// `FILE_SHARE_READ | FILE_SHARE_WRITE`, omitting the
        /// `FILE_SHARE_DELETE` that `std` would otherwise add.
        const SHARE_READ_AND_WRITE_BUT_NOT_DELETE: u32 = 0x1 | 0x2;

        options.share_mode(SHARE_READ_AND_WRITE_BUT_NOT_DELETE);
    }

    /// Nothing to deny: a Unix rename cannot replace the inode behind an
    /// open handle, and [`SaveFile::resolves_to`] compares that inode
    /// directly.
    #[cfg(not(windows))]
    fn deny_delete_sharing(_options: &mut std::fs::OpenOptions) {}

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

    /// This directory's one lock file: fixed, so it never depends on the
    /// save's basename, and within the POSIX minimum component limit.
    fn lock_path(&self) -> PathBuf {
        self.path.with_file_name(LOCK_FILE_NAME)
    }
}

/// Holds a [`SaveFile::lock`] until dropped.
#[derive(Debug)]
pub struct SaveFileGuard {
    _lock_file: std::fs::File,
}

#[cfg(test)]
mod tests;
