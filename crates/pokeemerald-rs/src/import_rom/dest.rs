//! The pack's destination directory, opened once and held for the whole
//! import.
//!
//! Every operation here names its file by *basename* inside this
//! directory. Nothing takes a full path, so nothing re-walks the directory
//! components the player's `$POKEEMERALD_PACK` ran through — see
//! [`super`]'s docs for why re-walking them is the hole this closes, and
//! for what stays open off Unix.

use std::collections::hash_map::RandomState;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

// `import_rom::TEMP_PREFIX`'s own doc comment states what this is for.
use super::TEMP_PREFIX;

/// Build a fresh temporary pack name from `sequence`, this destination's own
/// naming counter (`(oop-boundaries)`: scoped to the one [`Dest`] doing the
/// import, not a process-wide global).
///
/// The file [`Dest::create_new`] makes from this name is created
/// exclusively, so the name has one job: be one nothing else already holds.
/// A process id alone is not that. It repeats across PID namespaces sharing
/// one mounted directory, it is recycled after a kill that left a stale
/// temporary file behind, and `/proc` hands it to anyone on the machine --
/// so in a pack directory another account can write to,
/// `.pokeemerald-rs-import.<pid>.tmp` is a name an attacker can pre-create
/// as a link to a file of the player's. The clock's nanoseconds and this
/// destination's own counter go in with it: no pre-created name matches
/// one, and covering a second of them is a billion files.
///
/// `sequence` alone only disambiguates repeat calls on the *same* `Dest` --
/// every import opens a fresh one and calls this exactly once
/// (`import_rom::import_to_with`), so in production `sequence` is always
/// its own instance's first value. `salt` is what still tells two
/// same-nanosecond calls apart across *separate* `Dest` instances (two
/// import attempts racing in one process, or a test driving several): a
/// freshly constructed [`RandomState`] reseeds per call from the same
/// source `HashMap`'s own DoS-resistant randomization uses, so hashing
/// nothing through it and taking the digest is a `std`-only source of a
/// fresh, unpredictable 64 bits with no counter of its own to reset.
///
/// A collision that happens anyway is a refused import naming the path,
/// never a write through someone else's link, and the next run picks a
/// different name.
///
/// What is deliberately *not* in it is the pack's own name. A 240-byte
/// basename is valid on every filesystem this ships to, and prefixing a
/// temporary name with the whole of it pushed past the 255-byte limit for
/// one component: `ENAMETOOLONG` on a name the player never typed, leaving
/// a perfectly valid destination impossible to import to. A fixed prefix
/// and a bounded run of hex numbers is bounded whatever the pack is
/// called, and the destination is not what makes the name unique anyway.
fn next_temp_name(sequence: &AtomicU64) -> OsString {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let sequence = sequence.fetch_add(1, Ordering::Relaxed);
    let salt = RandomState::new().build_hasher().finish();
    OsString::from(format!(
        "{TEMP_PREFIX}.{}.{nanos:x}.{sequence:x}.{salt:x}.tmp",
        std::process::id()
    ))
}

/// Get `path`'s own directory entries onto the storage device, or give up.
///
/// [`Dest::publish`] syncs the destination through the handle it already
/// holds; this is for the directories *above* it, which the import creates
/// but never pins — a newly created directory hangs from a name in its
/// parent, and that name is durable only once the parent is synced.
///
/// Best-effort for the reason [`Dest::publish`]'s sync is: the pack's bytes
/// are on the disk either way, and not every platform will open a directory
/// at all. A failure here is a weaker guarantee, not a failed import.
#[cfg(unix)]
pub(super) fn sync_directory(path: &Path) {
    if let Ok(dir) = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    ) {
        let _ = rustix::fs::fsync(&dir);
    }
}

/// Get `path`'s own directory entries onto the storage device, or give up.
///
/// The path-based spelling of the Unix arm above, and the same best-effort
/// contract. Windows will not open a directory as a file, so this is a
/// no-op there — exactly as it is for [`Dest::publish`]'s own sync.
#[cfg(not(unix))]
pub(super) fn sync_directory(path: &Path) {
    let _ = File::open(path).and_then(|dir| dir.sync_all());
}

/// The destination directory, pinned open.
///
/// On Unix this is a descriptor for the directory itself: the kernel
/// resolved the path once, when it was opened, and every later
/// `openat`/`renameat`/`unlinkat` starts from the directory that
/// descriptor names. Redirecting a symlink the path ran through afterwards
/// changes nothing this handle can reach.
#[cfg(unix)]
pub(super) struct Dest {
    /// The open directory. Held for the life of the import.
    dir: std::os::fd::OwnedFd,
    /// This destination's own temporary-name counter -- [`Self::temp_name`].
    temp_sequence: AtomicU64,
}

#[cfg(unix)]
impl Dest {
    /// Open `dir` and keep it open.
    ///
    /// `O_DIRECTORY` is what makes the handle worth holding: a `dir` that
    /// is not a directory fails here rather than at the first create.
    ///
    /// # Errors
    ///
    /// Whatever `open(2)` reports: the directory is gone, is not a
    /// directory, or is not searchable by this user.
    pub(super) fn open(dir: &Path) -> io::Result<Self> {
        let dir = rustix::fs::open(
            dir,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        Ok(Self {
            dir,
            temp_sequence: AtomicU64::new(0),
        })
    }

    /// The name of the temporary file this import's pack is built in,
    /// beside the pack -- see [`next_temp_name`] for why it looks the way
    /// it does.
    pub(super) fn temp_name(&self) -> OsString {
        next_temp_name(&self.temp_sequence)
    }

    /// Whether `name`, inside this directory, is the open file `rom`.
    ///
    /// A device and inode pair names the file itself, so a hard link under
    /// another name is still the same file — the comparison `std` cannot
    /// make. `statat` without `AT_SYMLINK_NOFOLLOW` follows a link at
    /// `name`, which is the question being asked: not which name the
    /// destination took, but which file a write through it would land on.
    ///
    /// Both sides are handles, not paths. `fstat` on the descriptor the
    /// importer reads from is what makes the answer keep: a ROM path
    /// another account can redirect would otherwise be one file when it is
    /// checked and another when it is read. `rom_path` is unused here for
    /// exactly that reason; it stays in the signature because the
    /// non-Unix arm has no descriptor to ask.
    ///
    /// `false` when either side cannot be read. A destination that does
    /// not exist yet is not the ROM.
    pub(super) fn is_same_file_as(&self, name: &OsStr, rom: &File, _rom_path: &Path) -> bool {
        let (Ok(here), Ok(there)) = (
            rustix::fs::statat(&self.dir, name, rustix::fs::AtFlags::empty()),
            rustix::fs::fstat(rom),
        ) else {
            return false;
        };
        here.st_dev == there.st_dev && here.st_ino == there.st_ino
    }

    /// Create `name` inside this directory, refusing a name already taken.
    ///
    /// `O_EXCL` is the only open that cannot be redirected: it refuses a
    /// symlink sitting at `name`, even a dangling one, so a link planted
    /// in a pack directory another account can write to becomes a refused
    /// import rather than a write through it.
    ///
    /// The mode is `File::create`'s own 0o666; the kernel masks it with
    /// the player's umask either way.
    ///
    /// # Errors
    ///
    /// Whatever `openat(2)` reports, including
    /// [`AlreadyExists`](io::ErrorKind::AlreadyExists) for a taken name.
    pub(super) fn create_new(&self, name: &OsStr) -> io::Result<File> {
        let file = rustix::fs::openat(
            &self.dir,
            name,
            rustix::fs::OFlags::WRONLY
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::EXCL
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::RUSR
                | rustix::fs::Mode::WUSR
                | rustix::fs::Mode::RGRP
                | rustix::fs::Mode::WGRP
                | rustix::fs::Mode::ROTH
                | rustix::fs::Mode::WOTH,
        )?;
        Ok(File::from(file))
    }

    /// Rename `from` onto `to`, both inside this directory.
    ///
    /// Same directory on both sides of one `renameat`, so the publication
    /// is atomic: a reader sees the old pack or the new one, never a
    /// partial file.
    ///
    /// The directory is `fsync`ed afterwards, which is what makes the
    /// rename itself outlive a power loss — the pinned descriptor is
    /// already the handle to sync, so it costs no second open. Best-effort:
    /// the pack's bytes are durable before this is reached (the caller
    /// syncs the temporary file), so a directory entry left in the page
    /// cache is not worth failing an otherwise finished import over.
    ///
    /// # Errors
    ///
    /// Whatever `renameat(2)` reports.
    pub(super) fn publish(&self, from: &OsStr, to: &OsStr) -> io::Result<()> {
        rustix::fs::renameat(&self.dir, from, &self.dir, to)?;
        let _ = rustix::fs::fsync(&self.dir);
        Ok(())
    }

    /// Remove `name` from this directory, ignoring a failure.
    ///
    /// Only ever called with a name [`Self::create_new`] created, and the
    /// removal is name-scoped and never follows a final-component link, so
    /// in a directory only the player writes it removes exactly the file
    /// the import made. An account that can write the directory can have
    /// swapped the entry, and then this removes that account's own entry —
    /// see [`super`]'s docs for why that grants it nothing. A removal that
    /// fails leaves litter but must not replace the diagnosis the caller
    /// is already returning.
    ///
    /// Answers whether the name is gone afterwards, so a caller whose
    /// diagnostic mentions the temporary file can say which of the two
    /// happened instead of asserting the tidy one. Already absent counts as
    /// gone: the caller's question is "is there a file left behind", not
    /// "did this call do the removing".
    pub(super) fn discard(&self, name: &OsStr) -> bool {
        match rustix::fs::unlinkat(&self.dir, name, rustix::fs::AtFlags::empty()) {
            Ok(()) => true,
            Err(err) => err == rustix::io::Errno::NOENT,
        }
    }
}

/// The destination directory, addressed by path.
///
/// Off Unix there is no descriptor to pin: `std` exposes no `openat` on
/// any platform, and `rustix` is Unix-only. Every operation re-resolves
/// the path, exactly as this module's callers did before the Unix handle
/// existed. [`super`]'s docs state what that leaves trusted.
#[cfg(not(unix))]
pub(super) struct Dest {
    /// The directory path, re-resolved on every operation.
    dir: std::path::PathBuf,
    /// This destination's own temporary-name counter -- [`Self::temp_name`].
    temp_sequence: AtomicU64,
}

#[cfg(not(unix))]
impl Dest {
    /// Take `dir` as the destination.
    ///
    /// # Errors
    ///
    /// If `dir` is not a directory. Nothing is pinned, so this is a
    /// question about the path now, not a guarantee about it later.
    pub(super) fn open(dir: &Path) -> io::Result<Self> {
        if !dir.is_dir() {
            return Err(io::Error::other(format!(
                "`{}` is not a directory",
                dir.display()
            )));
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            temp_sequence: AtomicU64::new(0),
        })
    }

    /// The name of the temporary file this import's pack is built in,
    /// beside the pack -- see [`next_temp_name`] for why it looks the way
    /// it does.
    pub(super) fn temp_name(&self) -> OsString {
        next_temp_name(&self.temp_sequence)
    }

    /// Whether `name`, inside this directory, is the open file `rom`.
    ///
    /// The path-level answer: canonical paths, since Windows exposes no
    /// stable file identity through `std` (`rom_import::overwrites_rom`).
    /// The open handle is therefore no use here, and the window the Unix
    /// arm closes stays open — [`super`]'s docs state what that leaves
    /// trusted.
    pub(super) fn is_same_file_as(&self, name: &OsStr, _rom: &File, rom_path: &Path) -> bool {
        rom_import::overwrites_rom(rom_path, &self.dir.join(name))
    }

    /// Create `name` inside this directory, refusing a name already taken.
    ///
    /// # Errors
    ///
    /// Whatever the create reports. `CREATE_NEW` refuses an existing name,
    /// so a taken one is
    /// [`AlreadyExists`](io::ErrorKind::AlreadyExists).
    pub(super) fn create_new(&self, name: &OsStr) -> io::Result<File> {
        File::options()
            .write(true)
            .create_new(true)
            .open(self.dir.join(name))
    }

    /// Rename `from` onto `to`, both inside this directory.
    ///
    /// The directory sync that follows the rename on Unix has no portable
    /// equivalent — Windows will not open a directory as a file — so the
    /// rename's own durability is left to the platform here. The pack's
    /// bytes are already synced either way.
    ///
    /// # Errors
    ///
    /// Whatever the rename reports.
    pub(super) fn publish(&self, from: &OsStr, to: &OsStr) -> io::Result<()> {
        std::fs::rename(self.dir.join(from), self.dir.join(to))?;
        let _ = File::open(&self.dir).and_then(|dir| dir.sync_all());
        Ok(())
    }

    /// Remove `name` from this directory, ignoring a failure.
    ///
    /// Answers whether the name is gone afterwards, as the Unix arm does
    /// and for the same reason.
    pub(super) fn discard(&self, name: &OsStr) -> bool {
        match std::fs::remove_file(self.dir.join(name)) {
            Ok(()) => true,
            Err(err) => err.kind() == io::ErrorKind::NotFound,
        }
    }
}
