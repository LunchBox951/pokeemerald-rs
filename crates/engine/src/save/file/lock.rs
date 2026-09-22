//! Vets, opens, and identifies this directory's one lock slot.
//!
//! [`SaveFile::lock`] is the public entry; everything else here, including
//! the lock slot's path, backs its validation, identity, staging, and
//! platform-sharing rules. The parent-directory and ancestor-sync machinery
//! this module shares with [`SaveFile::write`] stays in the parent module.

use std::path::{Path, PathBuf};

use super::{
    open_refused_a_symlink, refuse_an_unusable_entry, refuse_an_unusable_open,
    refuse_unusable_opens, SaveFile, SaveFileError, SaveFileGuard, UnusableEntry, LOCK_FILE_NAME,
};

impl SaveFile {
    /// Acquires an advisory inter-process lock for this save path.
    ///
    /// The lock is [`LOCK_FILE_NAME`], one fixed file per directory on every
    /// host: a name derived from the save's basename cannot be made
    /// alias-safe, and [`SaveFile::write`] replaces the save's own inode by
    /// rename. A save that resolves to the lock file is refused, and a
    /// directory leaving no room for the name is refused rather than locked
    /// on some other entry, which would exclude nobody.
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
    pub(super) fn lock_with(
        &self,
        sync_directory: impl FnMut(&Path),
    ) -> Result<SaveFileGuard, SaveFileError> {
        let first_save = !self.exists();
        let parent = self.create_parent_directory()?;

        let path = self.lock_path();
        let file = self.open_lock_slot()?;
        file.lock().map_err(|source| SaveFileError::Lock {
            path: path.clone(),
            source,
        })?;
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

    /// Opens this directory's vetted lock slot, without locking it. The
    /// pre-open [`Self::refuse_an_unusable_slot`] check and the open itself
    /// both refuse a symlink or non-plain file, so a slot swapped between
    /// the two cannot slip through either.
    ///
    /// # Errors
    ///
    /// As [`SaveFile::lock`], less [`SaveFileError::CreateDirectory`] and the
    /// identity refusals that need the locked handle.
    pub(super) fn open_lock_slot(&self) -> Result<std::fs::File, SaveFileError> {
        let path = self.lock_path();
        Self::refuse_an_unusable_slot(&path)?;
        let file = Self::open_lock_file(&path).map_err(|source| {
            if open_refused_a_symlink(&source) {
                SaveFileError::LockPathIsAlias { path: path.clone() }
            } else {
                SaveFileError::Lock {
                    path: path.clone(),
                    source,
                }
            }
        })?;
        refuse_an_unusable_open(&file).map_err(|unusable| match unusable {
            UnusableEntry::Inspect(source) => SaveFileError::Lock {
                path: path.clone(),
                source,
            },
            UnusableEntry::IsAlias => SaveFileError::LockPathIsAlias { path: path.clone() },
            UnusableEntry::NotAPlainFile => {
                SaveFileError::LockPathNotAPlainFile { path: path.clone() }
            }
        })?;
        Ok(file)
    }

    /// Refuses a symlinked or non-plain-file lock slot before anything opens
    /// it, since the open would follow the link or block on a FIFO. Shares
    /// [`refuse_an_unusable_entry`]'s policy with [`SaveFile::read`], so the
    /// lock slot and the save path always agree on what a plain file is.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::LockPathIsAlias`] if the slot is a symlink;
    /// [`SaveFileError::LockPathNotAPlainFile`] if it is anything else that
    /// is not a plain file; [`SaveFileError::Lock`] if it could not be
    /// inspected.
    fn refuse_an_unusable_slot(lock: &Path) -> Result<(), SaveFileError> {
        refuse_an_unusable_entry(lock).map_err(|unusable| match unusable {
            UnusableEntry::Inspect(source) => SaveFileError::Lock {
                path: lock.to_path_buf(),
                source,
            },
            UnusableEntry::IsAlias => SaveFileError::LockPathIsAlias {
                path: lock.to_path_buf(),
            },
            UnusableEntry::NotAPlainFile => SaveFileError::LockPathNotAPlainFile {
                path: lock.to_path_buf(),
            },
        })
    }

    /// Whether the unfollowed entry at `lock` is the inode `file` holds. A
    /// hard link passes: it is a name of its own, and backup tools make them.
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

    /// As the `unix` [`SaveFile::names_its_own_entry`], refusing any
    /// symlinked slot, since Windows has no stable by-handle identity.
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

    /// Whether this save path is the inode of the open lock file `file` at
    /// `lock`, by `st_dev`/`st_ino` rather than path text, since
    /// `canonicalize` keeps a case-folding directory's spelling. A missing
    /// save is not the lock file.
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

    /// As the `unix` [`SaveFile::resolves_to`], comparing canonical paths:
    /// Windows `canonicalize` reports the entry's stored spelling, and
    /// [`SaveFile::deny_delete_sharing`] covers the hard link it misses.
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

    /// Opens this directory's lock file, creating it when the slot is free.
    pub(super) fn open_lock_file(path: &Path) -> std::io::Result<std::fs::File> {
        Self::open_lock_file_with(path, Self::create_lock_file)
    }

    /// As [`SaveFile::open_lock_file`], creating an absent slot through `create`.
    pub(super) fn open_lock_file_with(
        path: &Path,
        create: impl FnOnce(&Path) -> std::io::Result<Option<std::fs::File>>,
    ) -> std::io::Result<std::fs::File> {
        // An existing slot is opened first, so staging happens only for a
        // first save and leftover staging names can never block a valid lock.
        match Self::open_existing_lock(path) {
            Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => {}
            result => return result,
        }
        // Creation is exclusive and never follows a symlink, so a slot
        // swapped in after the vetting cannot make this create a file elsewhere.
        match create(path)? {
            Some(file) => Ok(file),
            None => Self::open_existing_lock(path),
        }
    }

    /// Opens the slot read-write, then read-only when this user cannot write
    /// it, so a run under `sudo` never shuts the owner out; a denial is
    /// retried briefly for a creator that publishes before widening. NFS
    /// emulates `flock(2)` as a write lock, so the read-only open fails there.
    fn open_existing_lock(path: &Path) -> std::io::Result<std::fs::File> {
        const RETRIES: u32 = 5;
        let mut attempt = 0;
        loop {
            let mut existing = std::fs::OpenOptions::new();
            existing.read(true).write(true);
            Self::deny_delete_sharing(&mut existing);
            refuse_unusable_opens(&mut existing);
            let denied = match existing.open(path) {
                Err(denied) if denied.kind() == std::io::ErrorKind::PermissionDenied => denied,
                result => return result,
            };
            let mut read_only = std::fs::OpenOptions::new();
            read_only.read(true);
            Self::deny_delete_sharing(&mut read_only);
            refuse_unusable_opens(&mut read_only);
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

    /// Creates the lock slot, or returns `None` when it already exists. The
    /// file is staged at `0666` and published by `link(2)`, so a `umask 077`
    /// creator never exposes a `0600` lock; without hard links it is created
    /// in place and widened after.
    #[cfg(unix)]
    fn create_lock_file(path: &Path) -> std::io::Result<Option<std::fs::File>> {
        Self::create_lock_file_with(path, Self::staging_name)
    }

    /// As the `unix` [`SaveFile::create_lock_file`], drawing staging names
    /// from `draw`.
    #[cfg(unix)]
    pub(super) fn create_lock_file_with(
        path: &Path,
        mut draw: impl FnMut() -> String,
    ) -> std::io::Result<Option<std::fs::File>> {
        use std::os::unix::fs::PermissionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create_new(true);
        // The staging name is unguessable, so nothing can be renamed onto it
        // on purpose, and a taken name just means the next draw.
        let (staged, file) = 'stage: {
            for _ in 0..=u8::MAX {
                let candidate = path.with_file_name(draw());
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
        Self::remove_only_own_staging(&staged, &file);
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

    /// A 10-hex-digit name from this process's random hasher keys: as long
    /// as the lock name, so it fits wherever that fits, and not guessable.
    #[cfg(unix)]
    pub(super) fn staging_name() -> String {
        use std::hash::{BuildHasher, Hasher};
        let draw = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish();
        format!(".lk{:010x}", draw & 0xFF_FFFF_FFFF)
    }

    /// Unlinks the staging entry only while it is still the inode `file` holds.
    #[cfg(unix)]
    pub(super) fn remove_only_own_staging(staged: &Path, file: &std::fs::File) {
        use std::os::unix::fs::MetadataExt;
        let same = match (std::fs::symlink_metadata(staged), file.metadata()) {
            (Ok(entry), Ok(held)) => entry.dev() == held.dev() && entry.ino() == held.ino(),
            _ => false,
        };
        if same {
            let _ = std::fs::remove_file(staged);
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

    /// Denies `FILE_SHARE_DELETE` on the lock handle, so no rename or unlink
    /// can replace the held entry; Windows has no stable by-handle identity.
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

    /// This directory's one lock file: fixed, so it never depends on the
    /// save's basename, and within the POSIX minimum component limit.
    pub(super) fn lock_path(&self) -> PathBuf {
        self.path.with_file_name(LOCK_FILE_NAME)
    }
}

#[cfg(test)]
mod tests;
