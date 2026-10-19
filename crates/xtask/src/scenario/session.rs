//! The media one scenario run owns: a private copy of the source pack and a
//! durable save file in a scratch directory of its own, so a scenario never
//! reads the checkout's pack in place, an installed pack, or a player's
//! save, and a restart reopens the very same pack and save (`main_menu.c:1067`
//! continues from the medium; `overworld.c:1705-1737`).
//!
//! Selection is explicit and fail-closed: a pack that cannot be prepared or a
//! save that cannot be opened is a [`ScenarioError::Setup`], never a run
//! against some other source. Nothing here touches the process environment.

use std::path::{Path, PathBuf};

use pokeemerald_rs::App;

use super::ScenarioError;

const PACK_FILE_NAME: &str = "pokeemerald.pack";
const SAVE_FILE_NAME: &str = "pokeemerald.sav";

/// Where a scenario's pack comes from. Either way the run reads a private
/// copy, never the source in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PackSelection {
    /// Copy an already-built pack file (the checkout-extracted pack for
    /// [`Self::checkout`]).
    Prepared(PathBuf),
    /// Import the supported ROM at this path into a fresh pack.
    ImportedRom(PathBuf),
}

impl PackSelection {
    /// The checkout's own extracted pack, `cargo xtask extract`'s output.
    pub(super) fn checkout() -> Self {
        Self::Prepared(assets::AssetPack::repo_pack_path())
    }

    /// Writes the pack as `name` inside `scratch`'s held directory.
    fn prepare(&self, scratch: &Scratch, name: &str) -> Result<(), String> {
        match self {
            Self::Prepared(source) => {
                let mut source_file = std::fs::File::open(source)
                    .map_err(|error| format!("source pack {}: {error}", source.display()))?;
                scratch
                    .write_new(name, |file| std::io::copy(&mut source_file, file).map(drop))
                    .map_err(|error| format!("source pack {}: {error}", source.display()))
            }
            Self::ImportedRom(rom) => {
                let bytes = rom_import::import_to_bytes(rom)
                    .map_err(|error| format!("ROM import of {}: {error}", rom.display()))?;
                scratch
                    .write_new(name, |file| std::io::Write::write_all(file, &bytes))
                    .map_err(|error| format!("ROM import of {}: {error}", rom.display()))
            }
        }
    }
}

/// A directory created exclusively for one session and removed with it.
///
/// The created directory stays open for the session's life. Every write and
/// the cleanup go through that hold, or first prove the pathname still names
/// the held directory; a path that was renamed away or replaced fails closed
/// and is never written into or deleted.
///
/// Unix writes and removes relative to the held descriptor. Windows compares a
/// fresh open of the pathname with the held handle's file identity (sharing
/// without `FILE_SHARE_DELETE` also keeps the directory from being renamed
/// away), but Windows has no handle-relative create here, so a swap between
/// that check and the following pathname operation remains possible there.
///
/// Residuals on every platform: the engine reopens the pack and save by
/// pathname after [`Session::start`]'s check, so a swap after that check is not
/// prevented; ownership starts at the open that follows `create_dir`, which
/// adopts only an empty (Unix: mode 0700) directory under an unguessable name,
/// so only an empty private replacement staged in that instant could be taken
/// (the owner is not compared: that needs `geteuid`, which the approved
/// `rustix` features do not include), and under a umask that withholds owner
/// bits the mode is restored in that same instant without following a link
/// (Linux: through an `O_PATH` pin; other Unixes: `AT_SYMLINK_NOFOLLOW`); and
/// Unix removes the (emptied) root with a pathname `rmdir` after the check, so
/// a replacement that wins that instant loses only if it is empty, never its
/// contents.
#[derive(Debug)]
struct Scratch {
    path: PathBuf,
    /// `None` only once Windows cleanup has released the hold to delete.
    hold: Option<std::fs::File>,
}

impl Scratch {
    fn create_in(base: &Path) -> std::io::Result<Self> {
        // The exclusive `create_dir` is the only arbiter of uniqueness: the
        // first caller to claim a name owns it. The random component keeps the
        // name unguessable, so a swap cannot be staged ahead of creation.
        loop {
            let path = base.join(format!(
                "pokeemerald-scenario-{}-{:016x}",
                std::process::id(),
                unpredictable()
            ));
            match create_private_dir(&path) {
                Ok(()) => return Self::claim(path),
                // Taken by another session, or left by an earlier process.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// Pins the just-created directory and refuses anything it cannot show it
    /// created: the opened directory must be empty and, on Unix, mode 0700. A
    /// replacement that lands between `create_dir` and this open is therefore
    /// adopted only if it is itself an empty private directory.
    fn claim(path: PathBuf) -> std::io::Result<Self> {
        let hold = open_hold(&path)?;
        let mut scratch = Self {
            path,
            hold: Some(hold),
        };
        let proven = scratch
            .require_path(&scratch.path)
            .and_then(|()| scratch.require_fresh());
        if let Err(error) = proven {
            // Never let `Drop` clean up a directory that was not proven ours.
            scratch.hold = None;
            return Err(error);
        }
        Ok(scratch)
    }

    /// Fails unless the held directory is empty and (Unix) mode 0700.
    fn require_fresh(&self) -> std::io::Result<()> {
        let hold = self.hold()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            // mkdir inherits a setgid parent's setgid bit, so a directory this
            // session just created may be 0o2700; setuid and sticky never are.
            let mode = hold.metadata()?.permissions().mode() & 0o5777;
            if mode != 0o700 {
                return Err(not_fresh(&self.path));
            }
            if !is_empty(hold)? {
                return Err(not_fresh(&self.path));
            }
        }
        #[cfg(not(unix))]
        {
            let _ = hold;
            if std::fs::read_dir(&self.path)?.next().is_some() {
                return Err(not_fresh(&self.path));
            }
        }
        Ok(())
    }

    fn hold(&self) -> std::io::Result<&std::fs::File> {
        self.hold.as_ref().ok_or_else(|| {
            std::io::Error::other("the scratch directory's hold was already released")
        })
    }

    /// Fails unless `path` still names the held directory.
    fn require_path(&self, path: &Path) -> std::io::Result<()> {
        let hold = self.hold()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let pinned = hold.metadata()?;
            let found = std::fs::symlink_metadata(path)?;
            if !found.is_dir() || (pinned.dev(), pinned.ino()) != (found.dev(), found.ino()) {
                return Err(not_the_held_directory(path));
            }
        }
        #[cfg(windows)]
        {
            use crate::windows_file_identity::WindowsFileIdentity;
            let fresh = open_hold(path)?;
            if WindowsFileIdentity::of(&fresh)? != WindowsFileIdentity::of(hold)? {
                return Err(not_the_held_directory(path));
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (path, hold);
            return Err(std::io::ErrorKind::Unsupported.into());
        }
        #[cfg(any(unix, windows))]
        Ok(())
    }

    /// Creates `name` exclusively inside the held directory and fills it.
    fn write_new(
        &self,
        name: &str,
        populate: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.require_path(&self.path)?;
        let mut file = self.create_new(name)?;
        populate(&mut file)
    }

    #[cfg(unix)]
    fn create_new(&self, name: &str) -> std::io::Result<std::fs::File> {
        let file = rustix::fs::openat(
            self.hold()?,
            name,
            rustix::fs::OFlags::WRONLY
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::EXCL
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )?;
        // The umask may have withheld owner read, which the engine needs to
        // reopen the pack; set the mode through the new descriptor.
        rustix::fs::fchmod(&file, rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR)?;
        Ok(std::fs::File::from(file))
    }

    #[cfg(not(unix))]
    fn create_new(&self, name: &str) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path.join(name))
    }

    /// Removes the scratch tree at `path` only if it is the held directory.
    /// Returns an error, leaving whatever `path` names untouched, otherwise.
    fn remove_all_at(&mut self, path: &Path) -> std::io::Result<()> {
        self.require_path(path)?;
        #[cfg(unix)]
        {
            remove_contents(self.hold()?)?;
            // `rmdir` only removes an empty directory, so a replacement that
            // wins the instant after the check can lose at most itself if it
            // is empty; it never loses content.
            std::fs::remove_dir(path)
        }
        #[cfg(not(unix))]
        {
            // The hold denies deletion, so Windows releases it for the delete;
            // the identity was verified just above (check-then-act residual).
            self.hold = None;
            std::fs::remove_dir_all(path)
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let path = self.path.clone();
        let _ = self.remove_all_at(&path);
    }
}

fn not_fresh(path: &Path) -> std::io::Error {
    std::io::Error::other(format!(
        "the directory at {} is not the empty private directory this session created",
        path.display()
    ))
}

/// Eight bytes the sandbox of a swapper cannot predict: `/dev/urandom` on
/// Unix, else (and if it cannot be read) the clock and a process-wide counter.
fn unpredictable() -> u64 {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    #[cfg(unix)]
    {
        use std::io::Read as _;
        let mut bytes = [0u8; 8];
        if std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .is_ok()
        {
            return u64::from_le_bytes(bytes);
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    #[expect(clippy::cast_possible_truncation, reason = "a hash input, not a count")]
    let low = nanos as u64;
    low ^ COUNTER
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .rotate_left(48)
}

fn not_the_held_directory(path: &Path) -> std::io::Error {
    std::io::Error::other(format!(
        "the directory at {} no longer matches the session's held scratch directory",
        path.display()
    ))
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        std::fs::DirBuilder::new().mode(0o700).create(path)?;
        // A umask that strips owner bits (0o444 leaves 0o300) would make the
        // directory unopenable for the hold and fail the 0o700 claim, so set
        // the mode the umask withheld. The claim that follows still verifies
        // the opened directory.
        restore_private_mode(path)
    }
    #[cfg(not(unix))]
    std::fs::create_dir(path)
}

/// Sets the directory at `path` to 0o700 if its mode differs, without ever
/// following a symlink, and only on a directory this session could have
/// created: Linux pins the entry with `O_PATH | O_NOFOLLOW | O_DIRECTORY`
/// (which needs no permission bits) and changes the mode of that pinned
/// inode; other Unixes use `fchmodat(AT_SYMLINK_NOFOLLOW)`.
///
/// `mkdir(0o700)` can only yield a subset of 0o700 (plus an inherited
/// setgid bit), so an entry with any other bit is left untouched and refused.
/// Emptiness can be read only once owner read is back, so a non-empty entry
/// has its original mode put back and is refused. The owner is not compared
/// (no `geteuid` among the approved `rustix` features).
#[cfg(any(target_os = "linux", target_os = "android"))]
fn restore_private_mode(path: &Path) -> std::io::Result<()> {
    use std::os::fd::AsRawFd as _;
    let pinned = rustix::fs::open(
        path,
        rustix::fs::OFlags::PATH
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let stat = rustix::fs::fstat(&pinned)?;
    if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::Directory {
        return Err(not_created_here());
    }
    let mode = stat.st_mode & 0o7777;
    if mode & 0o777 == 0o700 {
        return Ok(());
    }
    if mode & 0o5077 != 0 {
        return Err(not_created_here());
    }
    // `fchmod` refuses an `O_PATH` descriptor, but its `/proc` entry names
    // the pinned inode itself, so neither change can reach a replacement.
    let pinned_path = format!("/proc/self/fd/{}", pinned.as_raw_fd());
    let chmod = |mode| {
        rustix::fs::chmodat(
            rustix::fs::CWD,
            pinned_path.as_str(),
            mode,
            rustix::fs::AtFlags::empty(),
        )
    };
    chmod(rustix::fs::Mode::RWXU)?;
    let readable = rustix::fs::open(
        pinned_path.as_str(),
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    if !is_empty(&readable)? {
        chmod(rustix::fs::Mode::from_raw_mode(mode))?;
        return Err(not_created_here());
    }
    Ok(())
}

/// Residual beyond Linux: the check and the no-follow `chmod` resolve the
/// pathname separately, so a directory swapped in between them can have its
/// mode set before the identity check below refuses it.
#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
fn restore_private_mode(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let found = std::fs::symlink_metadata(path)?;
    let mode = found.permissions().mode() & 0o7777;
    if !found.is_dir() {
        return Err(not_created_here());
    }
    if mode & 0o777 == 0o700 {
        return Ok(());
    }
    if mode & 0o5077 != 0 {
        return Err(not_created_here());
    }
    // Never follows a link: a symlink swapped in after the check has its own
    // mode changed, and the `NOFOLLOW` open below refuses it.
    rustix::fs::chmodat(
        rustix::fs::CWD,
        path,
        rustix::fs::Mode::RWXU,
        rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
    )?;
    let opened = open_hold(path)?;
    let pinned = opened.metadata()?;
    if (pinned.dev(), pinned.ino()) != (found.dev(), found.ino()) {
        return Err(not_created_here());
    }
    if !is_empty(&opened)? {
        rustix::fs::fchmod(&opened, rustix::fs::Mode::from_raw_mode(mode))?;
        return Err(not_created_here());
    }
    Ok(())
}

#[cfg(unix)]
fn not_created_here() -> std::io::Error {
    std::io::Error::other("the scratch entry is not a directory this session created")
}

#[cfg(unix)]
fn open_hold(path: &Path) -> std::io::Result<std::fs::File> {
    rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map(std::fs::File::from)
    .map_err(Into::into)
}

#[cfg(windows)]
fn open_hold(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    // Read and write sharing only: nothing may rename or delete the directory
    // while it is held, yet the engine can still open files inside it.
    const SHARE_READ_WRITE: u32 = 0x1 | 0x2;
    // Real access, not zero: Windows checks sharing only against handles that
    // hold access, so a zero-access hold would not deny rename or delete.
    const FILE_LIST_DIRECTORY: u32 = 0x1;
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    let hold = std::fs::OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(SHARE_READ_WRITE)
        .open(path)?;
    let found = hold.metadata()?;
    if !found.is_dir() || found.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(std::io::Error::other("the scratch directory was replaced"));
    }
    Ok(hold)
}

#[cfg(not(any(unix, windows)))]
fn open_hold(_path: &Path) -> std::io::Result<std::fs::File> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// Empties `dir` through its descriptor: every entry is unlinked relative to
/// its held parent and subdirectories are opened `NOFOLLOW`, so no pathname
/// is ever re-resolved and a symlink is unlinked, not followed.
#[cfg(unix)]
fn remove_contents(dir: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;
    let dev = dir.metadata()?.dev();
    let mut names = Vec::new();
    for entry in rustix::fs::Dir::read_from(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_owned();
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            names.push((name, entry.file_type(), entry.ino()));
        }
    }
    for (name, kind, ino) in names {
        // `DT_UNKNOWN` filesystems report no type; ask without following links.
        let kind = if kind == rustix::fs::FileType::Unknown {
            rustix::fs::FileType::from_raw_mode(
                rustix::fs::statat(dir, name.as_c_str(), rustix::fs::AtFlags::SYMLINK_NOFOLLOW)?
                    .st_mode,
            )
        } else {
            kind
        };
        if kind == rustix::fs::FileType::Directory {
            let child = rustix::fs::openat(
                dir,
                name.as_c_str(),
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::DIRECTORY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )?;
            let child = std::fs::File::from(child);
            require_entry(&child, dev, ino)?;
            remove_contents(&child)?;
            rustix::fs::unlinkat(dir, name.as_c_str(), rustix::fs::AtFlags::REMOVEDIR)?;
        } else {
            rustix::fs::unlinkat(dir, name.as_c_str(), rustix::fs::AtFlags::empty())?;
        }
    }
    Ok(())
}

/// Fails unless `child` is the inode its parent's listing named, so a
/// directory swapped in after the listing is never descended into.
#[cfg(unix)]
fn require_entry(child: &std::fs::File, dev: u64, ino: u64) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;
    let found = child.metadata()?;
    if (found.dev(), found.ino()) != (dev, ino) {
        return Err(std::io::Error::other(
            "a scratch subdirectory was replaced during cleanup",
        ));
    }
    Ok(())
}

/// Whether the directory `dir` has no entries but `.` and `..`.
#[cfg(unix)]
fn is_empty(dir: impl std::os::fd::AsFd) -> std::io::Result<bool> {
    for entry in rustix::fs::Dir::read_from(dir)? {
        let entry = entry?;
        if entry.file_name().to_bytes() != b"." && entry.file_name().to_bytes() != b".." {
            return Ok(false);
        }
    }
    Ok(true)
}

/// One scenario's owned pack and save media.
#[derive(Debug)]
pub(super) struct Session {
    pack: PathBuf,
    save: PathBuf,
    scratch: Scratch,
}

impl Session {
    /// Prepare the media for `selection`.
    ///
    /// # Errors
    ///
    /// [`ScenarioError::Setup`] if the scratch directory cannot be created
    /// or the pack cannot be prepared.
    pub(super) fn new(selection: &PackSelection) -> Result<Self, ScenarioError> {
        Self::new_in(selection, &std::env::temp_dir())
    }

    fn new_in(selection: &PackSelection, base: &Path) -> Result<Self, ScenarioError> {
        let scratch = Scratch::create_in(base)
            .map_err(|error| ScenarioError::Setup(format!("scratch directory: {error}")))?;
        selection
            .prepare(&scratch, PACK_FILE_NAME)
            .map_err(ScenarioError::Setup)?;
        let pack = scratch.path.join(PACK_FILE_NAME);
        let save = scratch.path.join("save").join(SAVE_FILE_NAME);
        Ok(Self {
            pack,
            save,
            scratch,
        })
    }

    /// Construct the production [`App`] on this session's pack and save.
    /// Dropping it and calling this again restarts onto the same save.
    ///
    /// # Errors
    ///
    /// [`ScenarioError::Start`] if the pack does not load or the save
    /// medium cannot be opened.
    pub(super) fn start(&self) -> Result<App, ScenarioError> {
        // The engine reopens the pack and save by pathname, so refuse to start
        // on a path that no longer names the directory this session created.
        self.scratch
            .require_path(&self.scratch.path)
            .map_err(|error| ScenarioError::Start(format!("scratch directory: {error}")))?;
        App::new_headless_real_at(&self.pack, &self.save)
            .map_err(|error| ScenarioError::Start(error.to_string()))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod restart_tests;
