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
/// `rustix` features do not include); and Unix removes the (emptied) root with a pathname `rmdir` after the check, so
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
            let mode = hold.metadata()?.permissions().mode() & 0o7777;
            if mode != 0o700 {
                return Err(not_fresh(&self.path));
            }
            for entry in rustix::fs::Dir::read_from(hold)? {
                let entry = entry?;
                if entry.file_name().to_bytes() != b"." && entry.file_name().to_bytes() != b".." {
                    return Err(not_fresh(&self.path));
                }
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
        rustix::fs::openat(
            self.hold()?,
            name,
            rustix::fs::OFlags::WRONLY
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::EXCL
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .map(std::fs::File::from)
        .map_err(Into::into)
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
        std::fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    std::fs::create_dir(path)
}

#[cfg(unix)]
fn open_hold(path: &Path) -> std::io::Result<std::fs::File> {
    rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NOFOLLOW,
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
    let hold = std::fs::OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY)
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
    let mut names = Vec::new();
    for entry in rustix::fs::Dir::read_from(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_owned();
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            names.push((name, entry.file_type()));
        }
    }
    for (name, kind) in names {
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
                    | rustix::fs::OFlags::NOFOLLOW,
                rustix::fs::Mode::empty(),
            )?;
            remove_contents(&std::fs::File::from(child))?;
            rustix::fs::unlinkat(dir, name.as_c_str(), rustix::fs::AtFlags::REMOVEDIR)?;
        } else {
            rustix::fs::unlinkat(dir, name.as_c_str(), rustix::fs::AtFlags::empty())?;
        }
    }
    Ok(())
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
mod tests {
    use super::*;

    fn missing(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pokeemerald-no-such-{}-{name}", std::process::id()))
    }

    #[test]
    fn a_missing_source_pack_is_a_setup_error() {
        let error = Session::new(&PackSelection::Prepared(missing("pack")))
            .expect_err("no source pack must never prepare a session");
        assert!(matches!(error, ScenarioError::Setup(_)), "got {error:?}");
    }

    #[test]
    fn a_missing_rom_is_a_setup_error() {
        let error = Session::new(&PackSelection::ImportedRom(missing("rom")))
            .expect_err("no ROM must never prepare a session");
        assert!(matches!(error, ScenarioError::Setup(_)), "got {error:?}");
    }

    #[test]
    fn a_prepared_pack_is_copied_into_an_owned_scratch_directory() {
        let source = std::env::temp_dir().join(format!(
            "pokeemerald-scenario-source-{}.pack",
            std::process::id()
        ));
        std::fs::write(&source, b"not a real pack").unwrap();
        let first = Session::new(&PackSelection::Prepared(source.clone())).unwrap();
        let second = Session::new(&PackSelection::Prepared(source.clone())).unwrap();
        std::fs::remove_file(&source).unwrap();
        assert_ne!(first.pack, source);
        assert_ne!(first.pack, second.pack);
        assert_ne!(first.save, second.save);
        assert_eq!(std::fs::read(&first.pack).unwrap(), b"not a real pack");
        let scratch = first.pack.parent().unwrap().to_path_buf();
        drop(first);
        assert!(
            !scratch.exists(),
            "the scratch directory dies with its session"
        );
    }

    fn foreign_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-scenario-foreign-{}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("keep.txt"), b"keep me").unwrap();
        dir
    }

    #[test]
    fn a_write_into_a_directory_that_is_not_the_held_one_is_refused() {
        let scratch = Scratch::create_in(&std::env::temp_dir()).unwrap();
        let foreign = foreign_dir("write");
        assert!(scratch.require_path(&foreign).is_err());
        assert!(scratch.require_path(&scratch.path).is_ok());
        let _ = std::fs::remove_dir_all(&foreign);
    }

    #[test]
    fn cleanup_aimed_at_a_directory_that_is_not_the_held_one_removes_nothing() {
        let mut scratch = Scratch::create_in(&std::env::temp_dir()).unwrap();
        let foreign = foreign_dir("cleanup");
        assert!(scratch.remove_all_at(&foreign).is_err());
        assert_eq!(std::fs::read(foreign.join("keep.txt")).unwrap(), b"keep me");
        let _ = std::fs::remove_dir_all(&foreign);
    }

    /// A base directory private to one test, so a name vacated by a swap can
    /// never be claimed by another test's scratch directory.
    #[cfg(unix)]
    fn private_base(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "pokeemerald-scenario-base-{}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir(&base).unwrap();
        base
    }

    #[cfg(unix)]
    fn swap_in(scratch: &Scratch, replacement: impl FnOnce(&Path)) -> PathBuf {
        let aside = scratch.path.with_extension("aside");
        std::fs::rename(&scratch.path, &aside).unwrap();
        replacement(&scratch.path);
        aside
    }

    #[cfg(unix)]
    #[test]
    fn a_replaced_scratch_directory_receives_no_pack_write() {
        for symlink in [false, true] {
            let base = private_base("write");
            let scratch = Scratch::create_in(&base).unwrap();
            let target = foreign_dir("write-target");
            let aside = swap_in(&scratch, |path| {
                if symlink {
                    std::os::unix::fs::symlink(&target, path).unwrap();
                } else {
                    std::fs::create_dir(path).unwrap();
                }
            });
            let error = scratch
                .write_new(PACK_FILE_NAME, |_| Ok(()))
                .expect_err("a replaced scratch directory must refuse the write");
            assert!(error.to_string().contains("no longer matches"), "{error}");
            assert!(!scratch.path.join(PACK_FILE_NAME).exists());
            assert!(!target.join(PACK_FILE_NAME).exists());
            assert!(!aside.join(PACK_FILE_NAME).exists());
            let _ = std::fs::remove_dir_all(&target);
            let _ = std::fs::remove_dir_all(&aside);
            let _ = std::fs::remove_file(&scratch.path);
            let _ = std::fs::remove_dir(&scratch.path);
        }
    }

    #[cfg(unix)]
    #[test]
    fn dropping_a_session_never_removes_a_tree_that_replaced_its_scratch() {
        for symlink in [false, true] {
            let target = foreign_dir("drop-target");
            let source = std::env::temp_dir().join(format!(
                "pokeemerald-scenario-replaced-{}-{symlink}.pack",
                std::process::id()
            ));
            std::fs::write(&source, b"not a real pack").unwrap();
            let base = private_base("drop");
            let session = Session::new_in(&PackSelection::Prepared(source.clone()), &base).unwrap();
            std::fs::remove_file(&source).unwrap();
            let scratch_path = session.scratch.path.clone();
            let aside = swap_in(&session.scratch, |path| {
                if symlink {
                    std::os::unix::fs::symlink(&target, path).unwrap();
                } else {
                    std::fs::create_dir(path).unwrap();
                    std::fs::write(path.join("keep.txt"), b"keep me").unwrap();
                }
            });
            drop(session);
            let survived = if symlink {
                target.join("keep.txt").exists()
            } else {
                scratch_path.join("keep.txt").exists()
            };
            // The aside tree is the real scratch: it keeps the pack.
            let kept_pack = aside.join(PACK_FILE_NAME).exists();
            let _ = std::fs::remove_dir_all(&target);
            let _ = std::fs::remove_dir_all(&aside);
            let _ = std::fs::remove_file(&scratch_path);
            let _ = std::fs::remove_dir_all(&scratch_path);
            assert!(survived, "cleanup removed content it did not create");
            assert!(kept_pack, "cleanup resolved the pathname, not the hold");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_session_whose_scratch_was_replaced_refuses_to_start() {
        let source = std::env::temp_dir().join(format!(
            "pokeemerald-scenario-nostart-{}.pack",
            std::process::id()
        ));
        std::fs::write(&source, b"not a real pack").unwrap();
        let base = private_base("nostart");
        let session = Session::new_in(&PackSelection::Prepared(source.clone()), &base).unwrap();
        std::fs::remove_file(&source).unwrap();
        let aside = swap_in(&session.scratch, |path| std::fs::create_dir(path).unwrap());
        let Err(error) = session.start() else {
            panic!("a replaced scratch directory must not start");
        };
        assert!(matches!(error, ScenarioError::Start(_)), "got {error:?}");
        let path = session.scratch.path.clone();
        drop(session);
        let _ = std::fs::remove_dir_all(&aside);
        let _ = std::fs::remove_dir_all(&path);
    }

    #[cfg(windows)]
    #[test]
    fn a_held_scratch_directory_cannot_be_renamed_away() {
        let scratch = Scratch::create_in(&std::env::temp_dir()).unwrap();
        let aside = scratch.path.with_extension("aside");
        assert!(
            std::fs::rename(&scratch.path, &aside).is_err(),
            "the hold must deny renaming the directory"
        );
        assert!(scratch.require_path(&scratch.path).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_swapped_in_before_the_claim_is_never_adopted_or_emptied() {
        let path =
            std::env::temp_dir().join(format!("pokeemerald-scenario-swap-{}", std::process::id()));
        let aside = path.with_extension("aside");
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::remove_dir_all(&aside);
        create_private_dir(&path).unwrap();
        std::fs::rename(&path, &aside).unwrap();
        let foreign = foreign_dir("swap");
        std::fs::rename(&foreign, &path).unwrap();
        let claimed = Scratch::claim(path.clone());
        let adopted = claimed.is_ok();
        drop(claimed);
        let survived = path.join("keep.txt").exists();
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::remove_dir_all(&aside);
        assert!(
            !adopted,
            "claim adopted a directory this session did not create"
        );
        assert!(
            survived,
            "dropping the adopted scratch deleted foreign content"
        );
    }

    #[test]
    fn a_session_that_cannot_load_its_pack_reports_start_failure_not_success() {
        let source = std::env::temp_dir().join(format!(
            "pokeemerald-scenario-garbage-{}.pack",
            std::process::id()
        ));
        std::fs::write(&source, b"not a real pack").unwrap();
        let session = Session::new(&PackSelection::Prepared(source.clone())).unwrap();
        std::fs::remove_file(&source).unwrap();
        let Err(error) = session.start() else {
            panic!("a garbage pack must not boot");
        };
        assert!(matches!(error, ScenarioError::Start(_)), "got {error:?}");
        assert!(
            !session.save.exists(),
            "a failed boot must not leave a save behind"
        );
    }
}

#[cfg(test)]
mod restart_tests {
    use pokeemerald_rs::main_menu::MainMenuItem;
    use pokeemerald_rs::{AppButtons, AppState};

    use super::*;

    const SAVE_FLOW_FRAME_BUDGET: usize = 4_000;

    fn press(app: &mut App, buttons: AppButtons) {
        app.set_headless_buttons(buttons).unwrap();
        assert!(app.step().unwrap());
        app.set_headless_buttons(AppButtons::NONE).unwrap();
        assert!(app.step().unwrap());
    }

    #[test]
    #[ignore = "needs a local pack produced by `cargo xtask extract`"]
    fn real_pack_restart_continues_the_game_the_first_boot_saved() {
        let _pack = crate::extract::REAL_PACK_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let session = Session::new(&PackSelection::checkout()).expect("owned session");

        let mut first = session.start().expect("first boot");
        // Drive the production new-game path to the first overworld frame.
        for frame in super::super::boot_to_first_fight::frames() {
            first.set_headless_buttons(frame.buttons).unwrap();
            assert!(first.step().unwrap());
            if first.state() == AppState::Overworld {
                break;
            }
        }
        assert_eq!(first.state(), AppState::Overworld);
        for _ in 0..60 {
            first.set_headless_buttons(AppButtons::NONE).unwrap();
            assert!(first.step().unwrap());
        }
        // START -> SAVE (BAG, player, SAVE) -> YES, then confirm messages.
        press(&mut first, AppButtons::START);
        press(&mut first, AppButtons::DOWN);
        press(&mut first, AppButtons::DOWN);
        let mut frames = 0;
        while !session.save.exists() {
            assert!(frames < SAVE_FLOW_FRAME_BUDGET, "save never written");
            press(&mut first, AppButtons::A);
            frames += 2;
        }
        drop(first);

        let mut restarted = session.start().expect("restart onto the same save");
        assert_eq!(restarted.state(), AppState::Title);
        restarted.set_headless_buttons(AppButtons::START).unwrap();
        assert!(restarted.step().unwrap());
        for _ in 0..=super::super::FADE_WAIT_FRAMES {
            restarted.set_headless_buttons(AppButtons::NONE).unwrap();
            assert!(restarted.step().unwrap());
        }
        assert_eq!(
            restarted.state(),
            AppState::MainMenu(MainMenuItem::Continue),
            "the restarted session must offer CONTINUE from the first session's save"
        );
    }
}
