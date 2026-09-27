//! [`stage`] fills a `create_new`-held file and [`StagedFile::publish`]
//! re-verifies that handle's identity before the promoting rename, so a
//! symlink planted at the name is refused rather than followed or published.
//! That check is not fused to the rename: a replacement landing in the gap
//! is still promoted. Once staging has created a pointer file, a failure
//! retains it and reports its last known path instead of removing it: a held
//! file's identity check cannot make a later, separate pathname deletion
//! conditional on that identity, so no failure path names and deletes this
//! file a second time.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Opens `path` for writing and fails if anything already holds that name,
/// refusing an existing file, directory, or symlink instead of following or
/// truncating it.
///
/// On Windows the returned handle shares nothing, so until it is dropped no
/// other opener -- in this process or any other -- can open, delete, or
/// rename that entry.
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
pub(super) fn stage(path: &Path, bytes: &[u8]) -> std::io::Result<StagedFile> {
    let file = create_new_exclusive(path)?;
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
}

impl StagedFile {
    /// Whether the staging path still names this staged file, rather than a
    /// symlink, directory, or other entry that took its name.
    ///
    /// A reading that failed is neither answer, and surfaces rather than
    /// passing for "replaced".
    fn still_ours(&self) -> std::io::Result<bool> {
        let found = std::fs::symlink_metadata(&self.path)?;
        Ok(found.file_type().is_file() && is_the_held_file(&self.hold, &found)?)
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

    /// Verifies ownership, then publishes this staging path to `dest` by
    /// rename; refuses to publish a replaced or unverifiable staging path.
    pub(super) fn publish(mut self, dest: &Path) -> std::io::Result<()> {
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
        std::fs::rename(&self.path, dest).map_err(|error| self.report_retained(&error))
    }
}
