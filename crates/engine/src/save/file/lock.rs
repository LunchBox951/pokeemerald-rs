//! Vets, opens, and identifies this directory's one lock slot.
//!
//! [`SaveFile::lock`] is the public entry; everything else here, including
//! the lock slot's path, backs its validation, identity, staging, and
//! platform-sharing rules. The parent-directory and ancestor-sync machinery
//! this module shares with [`SaveFile::write`] stays in the parent module.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use super::open::{
    open_refused_a_symlink, refuse_an_unusable_entry, refuse_an_unusable_open,
    refuse_unusable_opens, UnusableEntry,
};
use super::{SaveFile, SaveFileError, SaveFileGuard, LOCK_FILE_NAME};

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
        self.lock_observing(sync_directory, || {})
    }

    /// As [`SaveFile::lock_with`], running `between_resolutions` after the
    /// parent's identity is recorded and again after the slot is locked and
    /// vetted, the two points a retarget could split acquisition across.
    pub(super) fn lock_observing(
        &self,
        sync_directory: impl FnMut(&Path),
        mut between_resolutions: impl FnMut(),
    ) -> Result<SaveFileGuard, SaveFileError> {
        let first_save = !self.exists();
        let parent = self.create_parent_directory()?;

        let path = self.lock_path();
        let lock_error = |source| SaveFileError::Lock {
            path: path.clone(),
            source,
        };
        let parent_or_here = parent.unwrap_or_else(|| Path::new("."));
        let identity = LockedDirectory::identity_at(parent_or_here).map_err(lock_error)?;
        between_resolutions();
        let file = self.open_lock_slot()?;
        file.lock().map_err(lock_error)?;
        // The identity and the slot came from two resolutions of the path,
        // so the directory is bound to the slot actually locked: every
        // later check also requires the parent's slot entry to be that file.
        let directory =
            LockedDirectory::bind(parent_or_here, identity, &file).map_err(lock_error)?;
        if !Self::names_its_own_entry(&path, &file)? {
            return Err(SaveFileError::LockPathIsAlias { path });
        }
        if self.resolves_to(&path, &file)? {
            return Err(SaveFileError::LockPathIsSave { path });
        }
        between_resolutions();
        self.refuse_a_retargeted_parent(Some(&directory))?;
        if first_save {
            if let Some(parent) = parent {
                Self::sync_ancestor_chain(parent, sync_directory);
            }
        }
        self.refuse_a_retargeted_parent(Some(&directory))?;
        let directory = self.held_directory.publish(directory, &self.path)?;
        Ok(SaveFileGuard {
            _lock_file: file,
            _directory: directory,
        })
    }

    /// Fails closed unless this save path's parent still resolves to the
    /// directory `held` pinned and its lock slot is still the file `held`
    /// locked; `None` means no guard is live, so nothing is promised and
    /// nothing is checked.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::SaveParentRetargeted`] if the parent names another
    /// directory or its slot is not the locked file; [`SaveFileError::Lock`]
    /// if either could not be inspected.
    pub(super) fn refuse_a_retargeted_parent(
        &self,
        held: Option<&LockedDirectory>,
    ) -> Result<(), SaveFileError> {
        let Some(held) = held else {
            return Ok(());
        };
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        match held.is_named_by(parent, &parent.join(LOCK_FILE_NAME)) {
            Ok(true) => Ok(()),
            Ok(false) => Err(SaveFileError::SaveParentRetargeted {
                path: self.path.clone(),
            }),
            Err(source) => Err(SaveFileError::Lock {
                path: self.lock_path(),
                source,
            }),
        }
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

/// The parent directory a [`SaveFileGuard`] locked, recorded by identity so
/// the path's parent can later be compared against it: the lock slot is a
/// file, and pins no ancestor symlink or junction.
///
/// # Identity contract
///
/// On Unix the identity is the directory's `(dev, ino)` and no handle is held
/// (see the field note); a nonblocking one would need the `libc` crate, which
/// the engine does not name. So the guarantee is bounded: a path naming any directory that is *distinct from the locked one while
/// the locked one still exists* fails closed. A filesystem may recycle an
/// inode number only after the directory has been removed, and removing the
/// locked directory requires first taking the held lock slot's entry out of
/// it, by unlink or by renaming it away. Either already defeats
/// directory-based exclusion on Unix (a later locker creates a new slot),
/// which is a documented limitation of advisory locks outside this guard's
/// contract; this check does not claim to survive it. Windows keeps a handle
/// without `FILE_SHARE_DELETE`, so while that handle is retained the
/// directory cannot be removed or renamed; the retention is best-effort
/// (`pin` yields `None` if the open fails), so it narrows rather than closes
/// the same window.
#[derive(Debug)]
pub(super) struct LockedDirectory {
    /// Windows keeps the directory open so its identity cannot be reused
    /// while the guard lives. Unix holds no handle: opening the path again
    /// could block on a FIFO swapped in for it, or be refused for a directory
    /// without list permission, and the `stat` identity needs neither.
    #[cfg(windows)]
    _pin: Option<std::fs::File>,
    identity: DirectoryIdentity,
    /// The lock file this guard actually holds. The directory identity and
    /// the slot are read through separate resolutions of the path, so an
    /// ancestor retargeted between them and back again could otherwise
    /// record one directory while locking another's slot.
    slot: SlotIdentity,
}

#[cfg(unix)]
type DirectoryIdentity = (u64, u64);
#[cfg(windows)]
type DirectoryIdentity = super::open::WindowsFileIdentity;
#[cfg(unix)]
type SlotIdentity = (u64, u64);
#[cfg(windows)]
type SlotIdentity = super::open::WindowsFileIdentity;

impl LockedDirectory {
    /// Binds `identity`, which [`Self::identity_at`] read from `directory`
    /// (following symlinks and, on Windows, junctions), to the lock file
    /// `slot` the guard holds; Windows also pins `directory`, best-effort.
    fn bind(
        #[cfg_attr(not(windows), allow(unused_variables))] directory: &Path,
        identity: DirectoryIdentity,
        slot: &std::fs::File,
    ) -> std::io::Result<Self> {
        Ok(Self {
            #[cfg(windows)]
            _pin: Self::pin(directory),
            identity,
            slot: Self::slot_identity_of(slot)?,
        })
    }

    /// Whether `directory` resolves to the directory this one recorded and
    /// its lock slot `slot` is still the file this guard locked.
    fn is_named_by(&self, directory: &Path, slot: &Path) -> std::io::Result<bool> {
        Ok(Self::identity_at(directory)? == self.identity
            && Self::slot_identity_at(slot)? == Some(self.slot))
    }

    #[cfg(unix)]
    fn slot_identity_of(slot: &std::fs::File) -> std::io::Result<SlotIdentity> {
        use std::os::unix::fs::MetadataExt as _;

        let metadata = slot.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }

    /// The unfollowed entry at `slot`, so a symlink there never matches; a
    /// missing slot is no match rather than an error.
    #[cfg(unix)]
    fn slot_identity_at(slot: &Path) -> std::io::Result<Option<SlotIdentity>> {
        use std::os::unix::fs::MetadataExt as _;

        match std::fs::symlink_metadata(slot) {
            Ok(metadata) => Ok(Some((metadata.dev(), metadata.ino()))),
            Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(other) => Err(other),
        }
    }

    #[cfg(windows)]
    fn slot_identity_of(slot: &std::fs::File) -> std::io::Result<SlotIdentity> {
        super::open::WindowsFileIdentity::of(slot)
    }

    /// Opens the slot entry itself (`FILE_FLAG_OPEN_REPARSE_POINT`) for
    /// metadata only, sharing everything so the held handle's denial of
    /// delete sharing does not refuse it; a missing slot is no match.
    #[cfg(windows)]
    fn slot_identity_at(slot: &Path) -> std::io::Result<Option<SlotIdentity>> {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        let handle = match std::fs::OpenOptions::new()
            .access_mode(0)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(slot)
        {
            Ok(handle) => handle,
            Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(other) => return Err(other),
        };
        super::open::WindowsFileIdentity::of(&handle).map(Some)
    }

    /// Unix reads the identity by `stat`, which neither needs list
    /// permission nor can block on a FIFO the path was retargeted to.
    #[cfg(unix)]
    fn identity_at(directory: &Path) -> std::io::Result<DirectoryIdentity> {
        use std::os::unix::fs::MetadataExt as _;

        let metadata = std::fs::metadata(directory)?;
        Self::refuse_a_non_directory(&metadata)?;
        Ok((metadata.dev(), metadata.ino()))
    }

    #[cfg(windows)]
    fn identity_at(directory: &Path) -> std::io::Result<DirectoryIdentity> {
        let handle = Self::open_handle(directory)?;
        super::open::WindowsFileIdentity::of(&handle)
    }

    #[cfg(windows)]
    fn pin(directory: &Path) -> Option<std::fs::File> {
        Self::open_handle(directory).ok()
    }

    /// Metadata-only access with backup semantics, the way Windows opens a
    /// directory; no `OPEN_REPARSE_POINT`, so a junction is followed to its
    /// target, and no `FILE_SHARE_DELETE`, so the directory cannot be
    /// renamed or removed while it is held.
    #[cfg(windows)]
    fn open_handle(directory: &Path) -> std::io::Result<std::fs::File> {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        let handle = std::fs::OpenOptions::new()
            .access_mode(0)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(directory)?;
        Self::refuse_a_non_directory(&handle.metadata()?)?;
        Ok(handle)
    }

    fn refuse_a_non_directory(metadata: &std::fs::Metadata) -> std::io::Result<()> {
        if metadata.is_dir() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                "the save's parent is not a directory",
            ))
        }
    }
}

/// The [`LockedDirectory`] of the guard currently live on a [`SaveFile`] or
/// any of its clones, held weakly so the lock outlives nothing but its guard.
#[derive(Debug, Clone, Default)]
pub(super) struct HeldDirectory(Arc<Mutex<Weak<LockedDirectory>>>);

impl HeldDirectory {
    /// The directory a live guard pinned, if there is one; holding the
    /// returned handle keeps it live for as long as the caller's operation.
    pub(super) fn current(&self) -> Option<Arc<LockedDirectory>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .upgrade()
    }

    /// Records `directory` as the live guard's, or joins the live guard's
    /// when it is the same directory. The mutex is never held across
    /// blocking I/O.
    ///
    /// # Errors
    ///
    /// [`SaveFileError::SaveParentRetargeted`] if a guard on another
    /// directory is still live on this `SaveFile`: its context is never
    /// overwritten.
    fn publish(
        &self,
        directory: LockedDirectory,
        save: &Path,
    ) -> Result<Arc<LockedDirectory>, SaveFileError> {
        let mut live = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = live.upgrade() {
            return if (existing.identity, existing.slot) == (directory.identity, directory.slot) {
                Ok(existing)
            } else {
                Err(SaveFileError::SaveParentRetargeted {
                    path: save.to_path_buf(),
                })
            };
        }
        let directory = Arc::new(directory);
        *live = Arc::downgrade(&directory);
        Ok(directory)
    }
}
