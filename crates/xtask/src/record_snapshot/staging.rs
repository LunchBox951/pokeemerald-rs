//! Staged through a held directory ([`stage_in`]), the file is looked up and
//! renamed relative to that directory, so publication cannot land elsewhere.
//!
//! [`stage_in`] fills a `create_new`-held file and [`StagedFile::publish`]
//! re-verifies that handle's identity before the promoting rename, so a
//! symlink planted at the name is refused rather than followed or published.
//! On Unix the same handle survives the rename, so publication also
//! re-verifies the promoted entry's identity before reporting success: a
//! replacement that wins the gap between the check and the rename is
//! promoted onto the pointer path but never accepted as the published
//! pointer. Once staging has created a pointer file, a failure retains it
//! (or, past a successful rename, retains the unverified promoted entry) and
//! reports its last known path instead of removing it: a held file's
//! identity check cannot make a later, separate pathname deletion
//! conditional on that identity, so no failure path names and deletes an
//! entry a second time.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Opens `path` for writing and fails if anything already holds that name,
/// refusing an existing file, directory, or symlink instead of following or
/// truncating it.
///
/// On Windows the returned handle shares nothing, so until it is dropped no
/// other opener -- in this process or any other -- can open, delete, or
/// rename that entry.
#[cfg(any(not(unix), test))]
fn create_new_exclusive(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;

        options.share_mode(0);
    }
    options.open(path)
}

/// Creates `path` exclusively and writes `bytes` into it, syncing before
/// returning the held staging file. Fails with `AlreadyExists` if `path`
/// already names anything, including a planted symlink; a failure past the
/// exclusive create retains the file and reports its path, so this call
/// never deletes an entry a different owner put there.
#[cfg(any(not(unix), test))]
pub(super) fn stage(path: &Path, bytes: &[u8]) -> std::io::Result<StagedFile> {
    stage_file(create_new_exclusive(path)?, path, bytes)
}

/// [`stage_in`] creates the file relative to the held directory `dir` under
/// `path`'s final component, so the entry can only appear in that directory.
/// The staged file then publishes relative to `dir` as well.
#[cfg(unix)]
pub(super) fn stage_in(
    dir: &std::fs::File,
    path: &Path,
    bytes: &[u8],
) -> std::io::Result<StagedFile> {
    stage_in_with(dir, path, bytes, std::fs::File::try_clone)
}

/// [`stage_in`] with the directory handle's duplication supplied, so tests can
/// fail it.
#[cfg(unix)]
pub(super) fn stage_in_with(
    dir: &std::fs::File,
    path: &Path,
    bytes: &[u8],
    clone_dir: impl FnOnce(&std::fs::File) -> std::io::Result<std::fs::File>,
) -> std::io::Result<StagedFile> {
    // Duplicated before the create, so a failure here leaves nothing behind.
    let dir = clone_dir(dir)?;
    let fd = rustix::fs::openat(
        &dir,
        entry_name(path)?,
        rustix::fs::OFlags::WRONLY
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::NOFOLLOW,
        // `File::create`'s own 0o666; the process umask narrows it.
        rustix::fs::Mode::RUSR
            | rustix::fs::Mode::WUSR
            | rustix::fs::Mode::RGRP
            | rustix::fs::Mode::WGRP
            | rustix::fs::Mode::ROTH
            | rustix::fs::Mode::WOTH,
    )?;
    let mut staged = stage_file(std::fs::File::from(fd), path, bytes);
    if let Ok(staged) = &mut staged {
        staged.dir = Some(dir);
    }
    staged
}

#[cfg(unix)]
fn entry_name(path: &Path) -> std::io::Result<&std::ffi::OsStr> {
    path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} names no directory entry", path.display()),
        )
    })
}

fn stage_file(file: std::fs::File, path: &Path, bytes: &[u8]) -> std::io::Result<StagedFile> {
    let result = (|| {
        let mut writer = std::io::BufWriter::new(&file);
        writer.write_all(bytes)?;
        writer.flush()?;
        file.sync_all()
    })();
    #[cfg(not(windows))]
    let hold = file;
    #[cfg(windows)]
    let hold = Some(file);
    let staged = StagedFile {
        path: path.to_path_buf(),
        hold,
        #[cfg(unix)]
        dir: None,
    };
    match result {
        Ok(()) => Ok(staged),
        Err(source) => Err(staged.report_retained(&source)),
    }
}

/// The handle that wrote the staged pointer, kept open until it is published
/// or abandoned: while it lives the file cannot be freed, so nothing that
/// takes its name can inherit its identity and pass for it. A rename is
/// indifferent to it, so nothing has to give it up early.
#[cfg(not(windows))]
type Hold = std::fs::File;

/// As [`Hold`] above, but `None` once [`StagedFile::release_hold`] gives it
/// up: Windows bars the promoting rename against a name any handle still
/// shares nothing over, so the handle has to be released first.
#[cfg(windows)]
type Hold = Option<std::fs::File>;

/// Ends `hold` where the platform needs it ended: nothing, since the hold
/// stays for the life of the [`StagedFile`] (see [`Hold`]).
#[cfg(not(windows))]
fn release(_hold: &mut Hold) {}

/// Ends `hold` where the platform needs it ended: drops the handle so the
/// staging name can be renamed again (see [`Hold`]).
#[cfg(windows)]
fn release(hold: &mut Hold) {
    drop(hold.take());
}

/// Whether `found` describes the very file `hold` holds open: same device
/// and inode.
#[cfg(unix)]
fn is_the_held_file(hold: &Hold, found: &std::fs::Metadata) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt as _;

    let staged = hold.metadata()?;
    Ok((staged.dev(), staged.ino()) == (found.dev(), found.ino()))
}

/// Whether `found` describes the very file `hold` holds open. Windows reads
/// back no identity, so the answer rests on what the hold forbids (see
/// [`create_new_exclusive`]): while it lives it forbids everything, so the
/// name cannot have come to mean another file. Once released, no answer is
/// available, and that is reported rather than guessed.
#[cfg(windows)]
fn is_the_held_file(hold: &Hold, _found: &std::fs::Metadata) -> std::io::Result<bool> {
    if hold.is_some() {
        Ok(true)
    } else {
        Err(std::io::Error::other(
            "the staging hold was released, so the file's identity can no longer be confirmed",
        ))
    }
}

/// Whether `found` describes the very file `hold` holds open. With neither
/// an inode nor a sharing hold to go on, a plain regular-file test is all
/// there is.
#[cfg(not(any(unix, windows)))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "one signature for every platform; only the unix and windows arms can fail to read an identity"
)]
fn is_the_held_file(_hold: &Hold, _found: &std::fs::Metadata) -> std::io::Result<bool> {
    Ok(true)
}

/// A staged pointer file and the hold that keeps its name its own.
pub(super) struct StagedFile {
    path: PathBuf,
    hold: Hold,
    /// The held directory this file was staged in, when it was staged through
    /// one: every later lookup and the promoting rename then name entries
    /// relative to it, never through a pathname.
    #[cfg(unix)]
    dir: Option<std::fs::File>,
}

impl StagedFile {
    /// Whether `path` names this staged file, rather than a symlink,
    /// directory, or other entry that took its name.
    ///
    /// A reading that failed is neither answer, and surfaces rather than
    /// passing for "replaced".
    fn matches(&self, path: &Path) -> std::io::Result<bool> {
        #[cfg(unix)]
        if let Some(dir) = &self.dir {
            let found = rustix::fs::statat(
                dir,
                entry_name(path)?,
                rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
            )?;
            let staged = rustix::fs::fstat(&self.hold)?;
            return Ok(rustix::fs::FileType::from_raw_mode(found.st_mode)
                == rustix::fs::FileType::RegularFile
                && (staged.st_dev, staged.st_ino) == (found.st_dev, found.st_ino));
        }
        let found = std::fs::symlink_metadata(path)?;
        Ok(found.file_type().is_file() && is_the_held_file(&self.hold, &found)?)
    }

    /// Whether the staging path still names this staged file.
    fn still_ours(&self) -> std::io::Result<bool> {
        self.matches(&self.path)
    }

    /// Gives up the hold, so that the rename which promotes the staged
    /// pointer can take its name.
    fn release_hold(&mut self) {
        release(&mut self.hold);
    }

    /// Folds a stage or publish failure into `source` and reports this
    /// staged file's last known path. No failure path unlinks it: a held
    /// file's identity check and a pathname deletion are separate lookups,
    /// so nothing can make one conditional on the other; the file is left
    /// for an operator to inspect and remove.
    fn report_retained(&self, source: &std::io::Error) -> std::io::Error {
        std::io::Error::new(
            source.kind(),
            format!(
                "{source}; the staging file was not removed; last known path: {}",
                self.path.display()
            ),
        )
    }

    /// Folds a post-rename identity failure into `source` and reports the
    /// promoted pointer's path. Nothing unlinks it: an unverifiable identity
    /// cannot make a pathname deletion of `dest` conditional on the very
    /// identity it failed to prove, so the promoted entry is left for an
    /// operator to inspect and remove.
    #[cfg(unix)]
    fn report_unverified(dest: &Path, source: &std::io::Error) -> std::io::Error {
        std::io::Error::new(
            source.kind(),
            format!(
                "{source}; the promoted pointer was not removed; last known path: {}",
                dest.display()
            ),
        )
    }

    /// Renames the staged file onto `dest`: relative to the held directory
    /// when there is one, so the rename cannot leave it.
    fn rename_to(&self, dest: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        if let Some(dir) = &self.dir {
            return rustix::fs::renameat(dir, entry_name(&self.path)?, dir, entry_name(dest)?)
                .map_err(Into::into);
        }
        std::fs::rename(&self.path, dest)
    }

    /// Verifies ownership, then publishes this staging path to `dest` by
    /// rename; refuses to publish a replaced or unverifiable staging path.
    pub(super) fn publish(self, dest: &Path) -> std::io::Result<()> {
        self.publish_with(dest, || {}, || {})
    }

    /// [`Self::publish`], plus `before_rename` run immediately before the
    /// promoting rename and `on_rename_failure` run right after a failed one
    /// and before that failure is retained and reported. Production always
    /// passes no-ops; tests use `before_rename` to land a replacement at the
    /// staging pathname ahead of the rename that promotes it, and
    /// `on_rename_failure` to land one at the staging pathname a failed
    /// rename leaves behind.
    pub(super) fn publish_with(
        mut self,
        dest: &Path,
        before_rename: impl FnOnce(),
        on_rename_failure: impl FnOnce(),
    ) -> std::io::Result<()> {
        match self.still_ours() {
            Ok(true) => {}
            Ok(false) => {
                return Err(self.report_retained(&std::io::Error::other(format!(
                    "staging file {} was replaced before it could be published",
                    self.path.display()
                ))));
            }
            Err(error) => {
                let wrapped = std::io::Error::new(
                    error.kind(),
                    format!(
                        "ownership of the staging file {} could not be confirmed before publishing: {error}",
                        self.path.display()
                    ),
                );
                return Err(self.report_retained(&wrapped));
            }
        }
        self.release_hold();
        before_rename();
        self.rename_to(dest).map_err(|error| {
            on_rename_failure();
            self.report_retained(&error)
        })?;
        #[cfg(unix)]
        match self.matches(dest) {
            Ok(true) => {}
            Ok(false) => {
                return Err(Self::report_unverified(
                    dest,
                    &std::io::Error::other(format!(
                        "the promoted pointer {} does not match the staged file",
                        dest.display()
                    )),
                ));
            }
            Err(error) => {
                return Err(Self::report_unverified(
                    dest,
                    &std::io::Error::new(
                        error.kind(),
                        format!(
                            "the promoted pointer {} could not be confirmed after publishing: {error}",
                            dest.display()
                        ),
                    ),
                ));
            }
        }
        Ok(())
    }
}
