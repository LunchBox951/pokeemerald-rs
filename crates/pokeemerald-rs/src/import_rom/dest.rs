//! The pack's destination directory, opened once and held for the whole
//! import.
//!
//! Every operation here names its file by *basename* inside this
//! directory. Nothing takes a full path, so nothing re-walks the directory
//! components the player's `$POKEEMERALD_PACK` ran through — see
//! [`super`]'s docs for why re-walking them is the hole this closes, and
//! for what stays open off Unix.

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::path::Path;

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
        Ok(Self { dir })
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
    pub(super) fn discard(&self, name: &OsStr) {
        let _ = rustix::fs::unlinkat(&self.dir, name, rustix::fs::AtFlags::empty());
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
        })
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
    pub(super) fn discard(&self, name: &OsStr) {
        let _ = std::fs::remove_file(self.dir.join(name));
    }
}
