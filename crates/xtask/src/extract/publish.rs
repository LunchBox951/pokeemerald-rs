//! Atomic publication of the extracted pack: stage under an exclusive name, then rename.
//!
//! On Unix the staging directory is pinned open once: the staging file is
//! created against that handle, its identity is re-read through the handle
//! immediately before the promoting `renameat`, and a staging entry that is no
//! longer the file held open is refused with the destination untouched. A
//! POSIX rename still resolves its source by name, so the instant between that
//! re-read and the rename is not closed; the post-rename identity check
//! reports a swap that lands in it.

use std::path::{Path, PathBuf};

#[cfg(windows)]
use win_fs::WindowsFileIdentity;

use super::ExtractError;

/// Publishes `bytes` at `output_path` by staging under an exclusively
/// created, unpredictable sibling name and renaming that sibling over the
/// destination, so a symlink planted at a guessable staging name is refused
/// rather than followed or promoted. Mirrors
/// [`engine::save::file::staging`](../../../engine/src/save/file/staging.rs)'s
/// fix for the identical risk in save files.
pub(super) fn write_pack_atomically(output_path: &Path, bytes: &[u8]) -> Result<(), ExtractError> {
    write_pack_atomically_with_names(
        output_path,
        bytes,
        crate::staging_names::candidates(output_path),
    )
}

/// As [`write_pack_atomically`], staging at the first of `candidates` that
/// [`create_new_exclusive`] finds free, so a test can hand over one
/// deterministic name instead of the production walk.
fn write_pack_atomically_with_names(
    output_path: &Path,
    bytes: &[u8],
    candidates: impl IntoIterator<Item = PathBuf>,
) -> Result<(), ExtractError> {
    let write_failed = |error: std::io::Error| {
        ExtractError::WriteFailed(output_path.to_path_buf(), error.to_string())
    };

    let staged = match stage_at_first_free_name(candidates, bytes) {
        Ok(staged) => staged,
        Err(error) => return Err(write_failed(error)),
    };
    staged.publish(output_path).map_err(write_failed)
}

/// Opens `path` for writing and fails if anything already holds that name,
/// refusing an existing file, directory, or symlink instead of following or
/// truncating it. (Unix creates against a held directory instead, see
/// [`create_new_exclusive_in`].)
///
/// On Windows the returned handle shares nothing, so until it is dropped no
/// other opener -- in this process or any other -- can open, delete, or
/// rename that entry.
#[cfg(not(unix))]
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

/// The directory part of `path`, and its final component. A bare file name
/// lives in `.`.
#[cfg(unix)]
fn split_parent(path: &Path) -> std::io::Result<(&Path, &std::ffi::OsStr)> {
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} does not name a file", path.display()),
        )
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok((parent, name))
}

/// `O_SEARCH` on Apple targets, which have no `O_PATH`: search access to a
/// directory without the read access `O_RDONLY` demands. Apple spells it
/// `O_EXEC | O_DIRECTORY` (`libc`'s `O_EXEC`, `0x4000_0000`); `rustix` 1.1 has
/// no flag for it, so the value travels as externally defined `OFlags` bits.
#[cfg(target_vendor = "apple")]
const APPLE_SEARCH: rustix::fs::OFlags = rustix::fs::OFlags::from_bits_retain(0x4000_0000);

/// Pins the directory at `path` open, following a symlink in its own name
/// as the caller's output directory may legitimately be one.
///
/// The handle asks for search access only: the path-based flow this replaced
/// needed no more than write and search on the output directory, so a
/// directory that cannot be listed (mode `0300`) must still publish.
/// `O_PATH` on Linux and `O_SEARCH` on Apple give that; elsewhere `O_RDONLY`
/// is the closest the platform offers.
#[cfg(unix)]
fn open_directory(path: &Path) -> std::io::Result<std::fs::File> {
    let open = |access: rustix::fs::OFlags| {
        rustix::fs::open(
            path,
            access | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
    };
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let fd = open(rustix::fs::OFlags::PATH);
    #[cfg(target_vendor = "apple")]
    let fd = match open(rustix::fs::OFlags::RDONLY) {
        Err(rustix::io::Errno::ACCESS) => open(APPLE_SEARCH),
        fd => fd,
    };
    #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
    let fd = open(rustix::fs::OFlags::RDONLY);
    Ok(std::fs::File::from(fd?))
}

/// As [`create_new_exclusive`], naming `name` relative to the held `dir`, so
/// the entry is created in the very directory later operations are bound to.
#[cfg(unix)]
fn create_new_exclusive_in(
    dir: &std::fs::File,
    name: &std::ffi::OsStr,
) -> std::io::Result<std::fs::File> {
    let fd = rustix::fs::openat(
        dir,
        name,
        rustix::fs::OFlags::WRONLY
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::RUSR
            | rustix::fs::Mode::WUSR
            | rustix::fs::Mode::RGRP
            | rustix::fs::Mode::WGRP
            | rustix::fs::Mode::ROTH
            | rustix::fs::Mode::WOTH,
    )?;
    Ok(std::fs::File::from(fd))
}

/// Stages `bytes` at the first of `candidates` that `create_new_exclusive`
/// finds free, reporting the last collision once they have all turned out to
/// be taken.
fn stage_at_first_free_name(
    candidates: impl IntoIterator<Item = PathBuf>,
    bytes: &[u8],
) -> std::io::Result<StagedPack> {
    let mut last_collision = None;
    for path in candidates {
        match fill_new_file(&path, bytes) {
            Ok(staged) => return Ok(staged),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                last_collision = Some(err);
            }
            Err(err) => return Err(err),
        }
    }
    Err(last_collision.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "exhausted staging attempts",
        )
    }))
}

/// Writes and syncs `bytes` into a `path` `create_new_exclusive` has just
/// claimed. A failure past that open leaves the entry where it is and says so:
/// the name may no longer be ours to unlink, and unlinking by pathname cannot
/// tell.
fn fill_new_file(path: &Path, bytes: &[u8]) -> std::io::Result<StagedPack> {
    use std::io::Write as _;

    #[cfg(unix)]
    let (dir, name, file) = {
        let (parent, name) = split_parent(path)?;
        let dir = open_directory(parent)?;
        let file = create_new_exclusive_in(&dir, name)?;
        (dir, name.to_os_string(), file)
    };
    #[cfg(not(unix))]
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
    let hold = match WindowsHold::new(file, path) {
        Ok(hold) => hold,
        Err(source) => return Err(retained_error(path, &source)),
    };
    let staged = StagedPack {
        path: path.to_path_buf(),
        hold,
        #[cfg(unix)]
        dir,
        #[cfg(unix)]
        name,
    };
    match result {
        Ok(()) => Ok(staged),
        Err(source) => Err(staged.report_retained(&source)),
    }
}

/// The handle that wrote the staged pack, kept open until the pack is
/// promoted or abandoned: while it lives the file cannot be freed, so nothing
/// that takes its name can inherit its identity and pass for it.
#[cfg(not(windows))]
type Hold = std::fs::File;

/// The handle that wrote the staged pack, kept open until the pack is
/// promoted or abandoned. It shares nothing ([`create_new_exclusive`]), so
/// while it lives the entry cannot be opened, deleted, or renamed at all --
/// by this process either, which is why it is an `Option`:
/// [`StagedPack::release_hold`] empties it when the name has to be given up.
#[cfg(windows)]
type Hold = WindowsHold;

/// The Windows hold: the exclusive handle while it lives, and a witness
/// handle that asks for no access, so it survives the hold's release and the
/// promoting rename without blocking either, and still refers to the staged
/// file wherever its name has gone.
///
/// The file is recognised by the witness's identity read at the moment of
/// comparison, never by a snapshot taken before the rename: on FAT, where
/// `FileIdInfo` is refused and the fallback index is the directory entry's
/// location, a rename that changes the long-name slot count moves the file to
/// a new entry and so changes its identity.
#[cfg(windows)]
struct WindowsHold {
    file: Option<std::fs::File>,
    witness: std::fs::File,
}

#[cfg(windows)]
impl WindowsHold {
    /// Opens the witness at `path` while `file`, the exclusive handle that
    /// just created it, still shares nothing, so the entry cannot have been
    /// replaced; the identities are compared anyway.
    fn new(file: std::fs::File, path: &Path) -> std::io::Result<Self> {
        let witness = open_without_access(path)?;
        if WindowsFileIdentity::of(&witness)? != WindowsFileIdentity::of(&file)? {
            return Err(std::io::Error::other(format!(
                "the witness opened at {} is not the staged file",
                path.display()
            )));
        }
        Ok(Self {
            file: Some(file),
            witness,
        })
    }
}

/// Opens `path` asking for no access and sharing everything, without
/// following a final symlink: Windows' sharing check does not apply to such
/// an open, so it coexists with the exclusive hold and never blocks a rename
/// or delete of the entry.
#[cfg(windows)]
fn open_without_access(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    std::fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_ALL)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

/// Whether `found` and `held` are one object, judged by identities `read`
/// from both now. Reading the held side now rather than reusing an earlier
/// reading is what keeps a file system whose identities move on rename (FAT)
/// from reporting the very file it just renamed as a replacement.
#[cfg(any(windows, test))]
fn is_same_object_now<H, I: PartialEq>(
    found: &H,
    held: &H,
    read: impl Fn(&H) -> std::io::Result<I>,
) -> std::io::Result<bool> {
    Ok(read(found)? == read(held)?)
}

/// Ends `hold` where the platform needs it ended.
#[cfg(not(any(unix, windows)))]
fn release(_hold: &mut Hold) {}

/// Ends `hold` where the platform needs it ended: Windows refuses to rename
/// or delete an entry whose open handle shares nothing, and refuses it to the
/// holder too, so the hold cannot outlive the last operation that needs the
/// staging name.
#[cfg(windows)]
fn release(hold: &mut Hold) {
    drop(hold.file.take());
}

/// Whether `found` describes the very file `hold` holds open: same device
/// and inode.
#[cfg(unix)]
fn is_the_held_file(hold: &Hold, _path: &Path, found: &std::fs::Metadata) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt as _;

    let staged = hold.metadata()?;
    Ok((staged.dev(), staged.ino()) == (found.dev(), found.ino()))
}

/// Whether the entry at `path` is the very file `hold`'s witness refers to:
/// a fresh [`open_without_access`] of `path` has the same volume and file ID
/// as the witness has now. Works before and after the hold is released, and
/// after the rename.
#[cfg(windows)]
fn is_the_held_file(hold: &Hold, path: &Path, found: &std::fs::Metadata) -> std::io::Result<bool> {
    if !found.file_type().is_file() {
        return Ok(false);
    }
    let fresh = open_without_access(path)?;
    Ok(fresh.metadata()?.file_type().is_file()
        && is_same_object_now(&fresh, &hold.witness, WindowsFileIdentity::of)?)
}

/// Whether `found` describes the very file `hold` holds open. Where there is
/// no identity to read back the answer is that it does.
#[cfg(not(any(unix, windows)))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "one signature for every platform; only the unix and windows arms can fail to read an identity"
)]
fn is_the_held_file(
    _hold: &Hold,
    _path: &Path,
    _found: &std::fs::Metadata,
) -> std::io::Result<bool> {
    Ok(true)
}

/// `source`, noting that the staging entry at `path` was left where it is.
fn retained_error(path: &Path, source: &std::io::Error) -> std::io::Error {
    std::io::Error::new(
        source.kind(),
        format!(
            "{source}; the staging file {} was left in place",
            path.display()
        ),
    )
}

/// A staged pack and the hold that keeps the staging name its own.
struct StagedPack {
    path: PathBuf,
    hold: Hold,
    /// The staging directory, pinned open when the file was created: the
    /// entry is named and promoted relative to it, never by a pathname that
    /// could resolve elsewhere by then.
    #[cfg(unix)]
    dir: std::fs::File,
    /// The staging entry's name within [`Self::dir`].
    #[cfg(unix)]
    name: std::ffi::OsString,
}

impl StagedPack {
    /// Whether `path` names this staged file, rather than a symlink,
    /// directory, or other entry, judged against the retained handle's
    /// identity.
    fn matches(&self, path: &Path) -> std::io::Result<bool> {
        let found = std::fs::symlink_metadata(path)?;
        Ok(found.file_type().is_file() && is_the_held_file(&self.hold, path, &found)?)
    }

    /// Whether the entry named `name` in the held `dir` is a regular file with
    /// the identity of the file held open, read through the directory handle so
    /// the answer cannot come from a different directory.
    #[cfg(unix)]
    fn entry_is_the_held_file(
        &self,
        dir: &std::fs::File,
        name: &std::ffi::OsStr,
    ) -> std::io::Result<bool> {
        let held = rustix::fs::fstat(&self.hold)?;
        let found = rustix::fs::statat(dir, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)?;
        Ok(
            rustix::fs::FileType::from_raw_mode(found.st_mode) == rustix::fs::FileType::RegularFile
                && (found.st_dev, found.st_ino) == (held.st_dev, held.st_ino),
        )
    }

    /// Renames the staging entry over `dest` relative to the held directory
    /// handles. The caller has just confirmed the entry is the held file.
    #[cfg(unix)]
    fn rename_held(&self, dest: &Path) -> std::io::Result<()> {
        let (dest_parent, dest_name) = split_parent(dest)?;
        let (staging_parent, _) = split_parent(&self.path)?;
        let other_dir;
        let dest_dir = if dest_parent == staging_parent {
            &self.dir
        } else {
            other_dir = open_directory(dest_parent)?;
            &other_dir
        };
        rustix::fs::renameat(&self.dir, &self.name, dest_dir, dest_name)?;
        Ok(())
    }

    /// Confirms the staging entry is still the file held open, or returns the
    /// error that refuses the publication and says the entry was left in
    /// place. On Unix the identity is read through the held directory.
    fn confirm_ownership(&self) -> std::io::Result<()> {
        #[cfg(unix)]
        let confirmed = self.entry_is_the_held_file(&self.dir, &self.name);
        #[cfg(not(unix))]
        let confirmed = self.matches(&self.path);
        match confirmed {
            Ok(true) => Ok(()),
            Ok(false) => Err(self.report_retained(&std::io::Error::other(format!(
                "staging file {} was replaced before it could be published",
                self.path.display()
            )))),
            Err(error) => {
                let wrapped = std::io::Error::new(
                    error.kind(),
                    format!(
                        "ownership of the staging file {} could not be confirmed before publishing: {error}",
                        self.path.display()
                    ),
                );
                Err(self.report_retained(&wrapped))
            }
        }
    }

    /// Gives up the hold, so that the rename which promotes the staged pack
    /// can take its name.
    #[cfg(not(unix))]
    fn release_hold(&mut self) {
        release(&mut self.hold);
    }

    /// Verifies ownership, then promotes the staged pack to `dest` by rename.
    /// A replaced or unverifiable staging entry is refused and left in place;
    /// a destination that does not verify afterwards is reported, never
    /// unlinked.
    ///
    /// On Unix the entry's identity is read through the held staging
    /// directory immediately before a `renameat` relative to that directory,
    /// so a replacement that lands before the check is refused with `dest`
    /// untouched. The check and the rename are still two calls, so a swap in
    /// the instant between them is only detected afterwards, by the identity
    /// of `dest`. On Windows the exclusive hold must be released before the
    /// rename, so a swap in that window is detected afterwards by file
    /// identity, not prevented, and nothing is rolled back; the witness
    /// handle's identity is re-read for that check, so a file system whose
    /// identities move on rename still recognises the promoted file.
    fn publish(self, dest: &Path) -> std::io::Result<()> {
        self.publish_with(dest, || {}, || {})
    }

    /// [`Self::publish`], plus `before_rename` run immediately before the
    /// promoting rename and `on_rename_failure` run right after a failed one.
    /// Production passes no-ops; tests use them to land a replacement at the
    /// staging pathname inside those windows. On Unix `before_rename` runs
    /// ahead of the ownership check, which is what it exercises.
    fn publish_with(
        self,
        dest: &Path,
        before_rename: impl FnOnce(),
        on_rename_failure: impl FnOnce(),
    ) -> std::io::Result<()> {
        self.publish_with_window(dest, before_rename, || {}, on_rename_failure)
    }

    /// [`Self::publish_with`], plus `after_ownership_check` run between the
    /// passed ownership check and the rename: the instant a POSIX rename
    /// cannot close, which tests use to prove the post-rename identity check
    /// still reports a swap that lands in it.
    fn publish_with_window(
        #[cfg_attr(
            unix,
            expect(
                unused_mut,
                reason = "only the non-Unix flow releases the hold, which needs `&mut`"
            )
        )]
        mut self,
        dest: &Path,
        before_rename: impl FnOnce(),
        after_ownership_check: impl FnOnce(),
        on_rename_failure: impl FnOnce(),
    ) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            before_rename();
            self.confirm_ownership()?;
        }
        #[cfg(not(unix))]
        {
            self.confirm_ownership()?;
            self.release_hold();
            before_rename();
        }
        after_ownership_check();
        #[cfg(unix)]
        let renamed = self.rename_held(dest);
        #[cfg(not(unix))]
        let renamed = std::fs::rename(&self.path, dest);
        if let Err(error) = renamed {
            on_rename_failure();
            return Err(self.report_retained(&error));
        }
        #[cfg(any(unix, windows))]
        match self.matches(dest) {
            Ok(true) => {}
            Ok(false) => {
                return Err(Self::report_unverified(
                    dest,
                    &std::io::Error::other(format!(
                        "the promoted pack {} does not match the staged file",
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
                            "the promoted pack {} could not be confirmed after publishing: {error}",
                            dest.display()
                        ),
                    ),
                ));
            }
        }
        Ok(())
    }

    /// `source`, noting that the staging entry was left where it is: its
    /// pathname may name another file by now, so it is never unlinked.
    fn report_retained(&self, source: &std::io::Error) -> std::io::Error {
        retained_error(&self.path, source)
    }

    /// `source`, noting that the promoted entry at `dest` was left where it is
    /// because it could not be verified as the staged file.
    #[cfg(any(unix, windows))]
    fn report_unverified(dest: &Path, source: &std::io::Error) -> std::io::Error {
        std::io::Error::new(
            source.kind(),
            format!(
                "{source}; the unverified entry at {} was left in place",
                dest.display()
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{write_pack_atomically_with_names, ExtractError};
    use crate::staging_names::path_with_value as staging_path_with_value;

    /// A fixed staging candidate value the deterministic tests hand to
    /// [`write_pack_atomically_with_names`] in place of the production
    /// unpredictable walk.
    const TEST_STAGING_VALUE: u64 = 0x00AB_CDEF_0123;

    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-extract-atomic-write-test-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creating scratch dir");
        dir
    }

    #[test]
    fn a_failed_staging_write_leaves_the_existing_pack_untouched() {
        let dir = scratch_dir("write");
        let output_path = dir.join("pokeemerald.pack");
        let original = b"an existing, usable pack";
        std::fs::write(&output_path, original).unwrap();

        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);
        std::fs::create_dir(&staging_path).unwrap();

        let err = write_pack_atomically_with_names(
            &output_path,
            b"a truncated replacement",
            std::iter::once(staging_path.clone()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, ExtractError::WriteFailed(path, _) if path == &output_path),
            "{err:?}"
        );
        assert_eq!(
            std::fs::read(&output_path).unwrap(),
            original,
            "a failed staging write destroyed the pack that was already on disk"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_rename_retains_the_staging_file_and_leaves_the_destination_untouched() {
        let dir = scratch_dir("rename");
        let output_path = dir.join("destination");
        std::fs::create_dir(&output_path).unwrap();
        let marker_path = output_path.join("marker");
        std::fs::write(&marker_path, b"unchanged").unwrap();

        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);
        let err = write_pack_atomically_with_names(
            &output_path,
            b"replacement",
            std::iter::once(staging_path.clone()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, ExtractError::WriteFailed(path, _) if path == &output_path),
            "{err:?}"
        );
        assert_eq!(
            std::fs::read(&staging_path).unwrap(),
            b"replacement",
            "a failed rename must retain its staging file, not unlink by pathname"
        );
        assert!(
            err.to_string()
                .contains(&staging_path.display().to_string()),
            "the retained staging file must be named: {err}"
        );
        assert_eq!(std::fs::read(&marker_path).unwrap(), b"unchanged");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_staging_file_that_was_never_created_is_not_reported_as_abandoned() {
        let dir = scratch_dir("never-created");
        let output_path = dir.join("absent").join("pokeemerald.pack");
        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);

        let err = write_pack_atomically_with_names(
            &output_path,
            b"replacement",
            std::iter::once(staging_path.clone()),
        )
        .unwrap_err();

        assert!(
            !staging_path.exists(),
            "no staging file should exist after a failed create"
        );
        assert!(
            !err.to_string().contains("was left in place"),
            "nothing was staged, so no staging file was left in place: {err}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_colliding_first_candidate_is_left_untouched_in_favor_of_the_next_free_name() {
        let dir = scratch_dir("collision-retry");
        let output_path = dir.join("pokeemerald.pack");
        let occupied = staging_path_with_value(&output_path, TEST_STAGING_VALUE);
        let free = staging_path_with_value(&output_path, TEST_STAGING_VALUE + 1);
        std::fs::write(&occupied, b"someone else's staging file").unwrap();

        write_pack_atomically_with_names(
            &output_path,
            b"a freshly built pack",
            [occupied.clone(), free.clone()],
        )
        .unwrap();

        assert_eq!(
            std::fs::read(&occupied).unwrap(),
            b"someone else's staging file",
            "a name already taken must be left to its owner, not overwritten"
        );
        assert!(
            !free.exists(),
            "the free candidate is consumed and renamed onto the output, not left behind"
        );
        assert_eq!(
            std::fs::read(&output_path).unwrap(),
            b"a freshly built pack"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// Whether this process is root, which bypasses directory modes. Reads
    /// the owner of `/proc/self`, as `scenario::session`'s tests do, because
    /// no `geteuid` is among the approved `rustix` features.
    #[cfg(unix)]
    fn running_as_root() -> bool {
        use std::os::unix::fs::MetadataExt as _;
        std::fs::metadata("/proc/self").is_ok_and(|meta| meta.uid() == 0)
    }

    #[cfg(unix)]
    #[test]
    fn a_write_and_search_only_output_directory_still_publishes() {
        use std::os::unix::fs::PermissionsExt as _;
        if running_as_root() {
            eprintln!("skipped: root ignores directory modes, so 0300 stays readable");
            return;
        }
        let dir = scratch_dir("write-search-only");
        let output_path = dir.join("pokeemerald.pack");
        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o300)).unwrap();
        let result = write_pack_atomically_with_names(
            &output_path,
            b"a fresh pack",
            std::iter::once(staging_path),
        );
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let written = std::fs::read(&output_path);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(written.unwrap(), b"a fresh pack");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_planted_at_the_staging_name_is_never_followed_or_published() {
        let dir = scratch_dir("staging-symlink");
        let output_path = dir.join("pokeemerald.pack");
        let original = b"an existing, usable pack";
        std::fs::write(&output_path, original).unwrap();
        let bystander = dir.join("bystander");
        std::fs::write(&bystander, b"not a pack").unwrap();

        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);
        std::os::unix::fs::symlink(&bystander, &staging_path).unwrap();

        let err = write_pack_atomically_with_names(
            &output_path,
            b"a freshly built pack",
            std::iter::once(staging_path.clone()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, ExtractError::WriteFailed(path, _) if path == &output_path),
            "{err:?}"
        );

        assert_eq!(
            std::fs::read(&bystander).unwrap(),
            b"not a pack",
            "a symlink planted at the staging name redirected the pack bytes"
        );
        assert!(
            std::fs::symlink_metadata(&staging_path)
                .is_ok_and(|meta| meta.file_type().is_symlink()),
            "a refused planted symlink must be left alone, not consumed as staging"
        );
        assert_eq!(
            std::fs::read(&output_path).unwrap(),
            original,
            "a refused staging symlink must not touch the published pack"
        );
        assert!(
            !std::fs::symlink_metadata(&output_path)
                .is_ok_and(|meta| meta.file_type().is_symlink()),
            "the published pack path must not become a planted symlink"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    /// A concurrent writer stand-in whose `Drop` runs once the staging file
    /// exists and before the ownership check, the window an exclusive create
    /// cannot cover.
    #[cfg(unix)]
    struct SwapStagingOnceCreated {
        names: std::vec::IntoIter<std::path::PathBuf>,
        staging_path: std::path::PathBuf,
        replacement: Replacement,
        bystander: std::path::PathBuf,
    }

    #[cfg(unix)]
    #[derive(Clone, Copy)]
    enum Replacement {
        SymlinkToBystander,
        UnrelatedRegularFile,
    }

    #[cfg(unix)]
    impl Iterator for SwapStagingOnceCreated {
        type Item = std::path::PathBuf;

        fn next(&mut self) -> Option<Self::Item> {
            self.names.next()
        }
    }

    #[cfg(unix)]
    impl Drop for SwapStagingOnceCreated {
        fn drop(&mut self) {
            std::fs::remove_file(&self.staging_path)
                .expect("the publish must have staged a file at this name by now");
            match self.replacement {
                Replacement::SymlinkToBystander => {
                    std::os::unix::fs::symlink(&self.bystander, &self.staging_path).unwrap();
                }
                Replacement::UnrelatedRegularFile => {
                    std::fs::write(&self.staging_path, b"an intruder's file").unwrap();
                }
            }
        }
    }

    #[cfg(unix)]
    fn assert_post_creation_swap_is_refused(label: &str, replacement: Replacement) {
        let dir = scratch_dir(label);
        let output_path = dir.join("pokeemerald.pack");
        let original = b"an existing, usable pack";
        std::fs::write(&output_path, original).unwrap();
        let bystander = dir.join("bystander");
        std::fs::write(&bystander, b"not a pack").unwrap();
        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);

        let err = write_pack_atomically_with_names(
            &output_path,
            b"a freshly built pack",
            SwapStagingOnceCreated {
                names: vec![staging_path.clone()].into_iter(),
                staging_path: staging_path.clone(),
                replacement,
                bystander: bystander.clone(),
            },
        )
        .unwrap_err();

        assert!(
            matches!(&err, ExtractError::WriteFailed(path, _) if path == &output_path),
            "{err:?}"
        );
        assert!(
            err.to_string()
                .contains("was replaced before it could be published"),
            "the ownership check, not something else, must be what refused the swap: {err}"
        );
        assert_eq!(
            std::fs::read(&output_path).unwrap(),
            original,
            "an entry swapped in after staging was published over the pack"
        );
        assert!(
            !std::fs::symlink_metadata(&output_path)
                .is_ok_and(|meta| meta.file_type().is_symlink()),
            "the published pack path must not become a planted symlink"
        );
        assert_eq!(
            std::fs::read(&bystander).unwrap(),
            b"not a pack",
            "the swapped-in entry's target must be neither written nor unlinked"
        );
        let intruder = std::fs::symlink_metadata(&staging_path)
            .expect("an entry this publish does not own must be left where its owner put it");
        match replacement {
            Replacement::SymlinkToBystander => {
                assert!(
                    intruder.file_type().is_symlink(),
                    "the planted symlink must survive as a symlink, not be replaced or followed"
                );
                assert_eq!(
                    std::fs::read_link(&staging_path).unwrap(),
                    bystander,
                    "the surviving symlink must still point at the bystander it was planted with"
                );
            }
            Replacement::UnrelatedRegularFile => {
                assert!(
                    intruder.file_type().is_file(),
                    "the planted regular file must survive as a regular file, not be replaced"
                );
                assert_eq!(
                    std::fs::read(&staging_path).unwrap(),
                    b"an intruder's file",
                    "the surviving intruder file must keep the bytes it was planted with"
                );
            }
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_swapped_in_after_the_staging_file_is_created_is_refused_not_published() {
        assert_post_creation_swap_is_refused("post-stage-symlink", Replacement::SymlinkToBystander);
    }

    #[cfg(unix)]
    #[test]
    fn a_regular_file_swapped_in_after_the_staging_file_is_created_is_refused_not_published() {
        assert_post_creation_swap_is_refused(
            "post-stage-regular",
            Replacement::UnrelatedRegularFile,
        );
    }

    /// A FAT-like volume for [`super::is_same_object_now`]: a file's identity
    /// is the directory slot its entry occupies, as `FastFAT`'s fallback index
    /// is, and a rename that needs a different number of long-name slots
    /// (`pokeemerald.pack.tmp.<10 hex>` needs four, `pokeemerald.pack` three)
    /// moves the entry to a freshly allocated slot. The Windows CI volume is
    /// NTFS, whose file ID survives a rename, so this is where that case runs.
    struct FatLikeVolume {
        slot_of: std::cell::RefCell<Vec<u64>>,
        next_free_slot: std::cell::Cell<u64>,
    }

    /// An open handle on one of the volume's files.
    struct FatLikeHandle {
        file: usize,
    }

    impl FatLikeVolume {
        fn with_files(count: usize) -> Self {
            let slots = (0..count as u64).collect::<Vec<_>>();
            Self {
                next_free_slot: std::cell::Cell::new(slots.len() as u64),
                slot_of: std::cell::RefCell::new(slots),
            }
        }

        fn rename_to_a_new_slot(&self, file: usize) {
            self.slot_of.borrow_mut()[file] = self.next_free_slot.get();
            self.next_free_slot.set(self.next_free_slot.get() + 1);
        }

        fn identity(&self, handle: &FatLikeHandle) -> u64 {
            self.slot_of.borrow()[handle.file]
        }
    }

    #[test]
    fn a_rename_that_moves_the_fat_entry_still_matches_the_promoted_file() {
        let volume = FatLikeVolume::with_files(1);
        let witness = FatLikeHandle { file: 0 };
        let before_rename = volume.identity(&witness);

        volume.rename_to_a_new_slot(0);
        let fresh_open_of_destination = FatLikeHandle { file: 0 };

        assert_ne!(
            volume.identity(&fresh_open_of_destination),
            before_rename,
            "the pre-rename identity no longer names the promoted file"
        );
        assert!(
            super::is_same_object_now(&fresh_open_of_destination, &witness, |h| {
                Ok(volume.identity(h))
            })
            .unwrap(),
            "the promoted file must match the witness read after the rename"
        );
    }

    #[test]
    fn a_file_renamed_over_the_destination_in_place_of_ours_does_not_match() {
        let volume = FatLikeVolume::with_files(2);
        let witness = FatLikeHandle { file: 0 };

        volume.rename_to_a_new_slot(1);
        let fresh_open_of_destination = FatLikeHandle { file: 1 };

        assert!(
            !super::is_same_object_now(&fresh_open_of_destination, &witness, |h| {
                Ok(volume.identity(h))
            })
            .unwrap()
        );
    }

    #[test]
    fn an_identity_that_cannot_be_read_is_an_error_not_a_match() {
        let refused = super::is_same_object_now(&(), &(), |()| {
            Err::<u64, _>(std::io::Error::other("refused"))
        });
        assert!(refused.is_err());
    }

    /// Stages `bytes` at the deterministic name and publishes it with `plant`
    /// run between the caller's setup and the rename. `previous` is the pack
    /// already at the destination, if any.
    ///
    /// On Unix the replacement must be refused before the rename: the
    /// destination keeps `previous` (or stays absent) and the planted entry
    /// survives at the staging name. On Windows the exclusive hold has been
    /// released by then, so the swap is promoted and only reported afterwards.
    #[cfg(any(unix, windows))]
    fn assert_swap_before_rename_is_refused(
        label: &str,
        previous: Option<&[u8]>,
        plant: impl FnOnce(&std::path::Path, &std::path::Path),
    ) {
        let dir = scratch_dir(label);
        let output_path = dir.join("pokeemerald.pack");
        if let Some(previous) = previous {
            std::fs::write(&output_path, previous).unwrap();
        }
        let bystander = dir.join("bystander");
        std::fs::write(&bystander, b"not a pack").unwrap();
        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);
        let carried = dir.join("carried-away");

        let staged =
            super::stage_at_first_free_name(std::iter::once(staging_path.clone()), b"our pack")
                .unwrap();
        let err = staged
            .publish_with(
                &output_path,
                || {
                    std::fs::rename(&staging_path, &carried).unwrap();
                    plant(&staging_path, &bystander);
                },
                || {},
            )
            .unwrap_err();

        #[cfg(unix)]
        {
            assert!(
                err.to_string()
                    .contains("was replaced before it could be published"),
                "the identity read through the held directory must refuse the swap: {err}"
            );
            match previous {
                Some(previous) => assert_eq!(
                    std::fs::read(&output_path).unwrap(),
                    previous,
                    "a swapped-in entry replaced the pack that was already published"
                ),
                None => assert!(
                    std::fs::symlink_metadata(&output_path).is_err(),
                    "a swapped-in entry was promoted to a destination that had no pack"
                ),
            }
            assert!(
                std::fs::symlink_metadata(&staging_path).is_ok(),
                "the entry this publish does not own must be left where its owner put it"
            );
        }
        #[cfg(windows)]
        {
            assert!(
                err.to_string().contains("does not match the staged file"),
                "the post-rename identity check must refuse the swap: {err}"
            );
            assert!(
                err.to_string().contains(&output_path.display().to_string()),
                "the unverified destination must be named: {err}"
            );
            assert!(
                std::fs::symlink_metadata(&output_path).is_ok(),
                "an unverified destination must be left in place, not unlinked"
            );
        }
        assert_eq!(std::fs::read(&carried).unwrap(), b"our pack");
        assert_eq!(std::fs::read(&bystander).unwrap(), b"not a pack");

        let _ = std::fs::remove_dir_all(dir);
    }

    /// The instant between the ownership check and the rename cannot be
    /// closed on POSIX, so a swap landing in it is promoted; the post-rename
    /// identity check must still report it and leave the entry in place.
    #[cfg(unix)]
    #[test]
    fn a_swap_landing_between_the_check_and_the_rename_is_reported_afterwards() {
        let dir = scratch_dir("swap-between-check-and-rename");
        let output_path = dir.join("pokeemerald.pack");
        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);
        let carried = dir.join("carried-away");
        let staged =
            super::stage_at_first_free_name(std::iter::once(staging_path.clone()), b"our pack")
                .unwrap();
        let err = staged
            .publish_with_window(
                &output_path,
                || {},
                || {
                    std::fs::rename(&staging_path, &carried).unwrap();
                    std::fs::write(&staging_path, b"an intruder's file").unwrap();
                },
                || {},
            )
            .unwrap_err();
        let promoted = std::fs::read(&output_path).ok();
        let carried_bytes = std::fs::read(&carried).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            err.to_string().contains("does not match the staged file"),
            "the post-rename identity check must report the swap: {err}"
        );
        assert!(
            err.to_string().contains(&output_path.display().to_string()),
            "the unverified destination must be named: {err}"
        );
        assert_eq!(promoted.as_deref(), Some(&b"an intruder's file"[..]));
        assert_eq!(carried_bytes, b"our pack");
    }

    /// Publication is bound to the directory pinned at staging, not to the
    /// staging pathname: when that directory is moved away and another one
    /// takes its place, the pack lands beside the staged file and the
    /// replacement directory is never written into. The pathname of the
    /// destination no longer holds the pack, which the post-rename check
    /// reports.
    #[cfg(unix)]
    #[test]
    fn a_staging_directory_replaced_before_publishing_is_never_written_into() {
        let dir = scratch_dir("staging-dir-replaced");
        let out_dir = dir.join("out");
        std::fs::create_dir(&out_dir).unwrap();
        let moved_dir = dir.join("out-moved");
        let output_path = out_dir.join("pokeemerald.pack");
        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);
        let staged =
            super::stage_at_first_free_name(std::iter::once(staging_path.clone()), b"our pack")
                .unwrap();
        let err = staged
            .publish_with(
                &output_path,
                || {
                    std::fs::rename(&out_dir, &moved_dir).unwrap();
                    std::fs::create_dir(&out_dir).unwrap();
                    std::fs::write(&staging_path, b"an intruder's file").unwrap();
                },
                || {},
            )
            .unwrap_err();

        let promoted = std::fs::read(moved_dir.join("pokeemerald.pack")).ok();
        let replacement_entries = std::fs::read_dir(&out_dir).unwrap().count();
        let intruder = std::fs::read(&staging_path).ok();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            err.to_string()
                .contains("could not be confirmed after publishing"),
            "the destination pathname no longer holds the pack: {err}"
        );
        assert_eq!(
            promoted.as_deref(),
            Some(&b"our pack"[..]),
            "the pack must be promoted within the directory the staged file lives in"
        );
        assert_eq!(
            replacement_entries, 1,
            "only the planted entry may be in the replacement directory"
        );
        assert_eq!(intruder.as_deref(), Some(&b"an intruder's file"[..]));
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn a_regular_file_swapped_in_just_before_the_rename_is_refused() {
        assert_swap_before_rename_is_refused("swap-before-rename-file", None, |staging, _| {
            std::fs::write(staging, b"an intruder's file").unwrap();
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_regular_file_swapped_in_just_before_the_rename_leaves_the_previous_pack() {
        assert_swap_before_rename_is_refused(
            "swap-before-rename-file-previous",
            Some(b"previous pack"),
            |staging, _| std::fs::write(staging, b"an intruder's file").unwrap(),
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_swapped_in_just_before_the_rename_is_refused() {
        assert_swap_before_rename_is_refused(
            "swap-before-rename-link",
            None,
            |staging, bystander| std::os::unix::fs::symlink(bystander, staging).unwrap(),
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_swapped_in_just_before_the_rename_leaves_the_previous_pack() {
        assert_swap_before_rename_is_refused(
            "swap-before-rename-link-previous",
            Some(b"previous pack"),
            |staging, bystander| std::os::unix::fs::symlink(bystander, staging).unwrap(),
        );
    }

    /// Needs symlink privilege (Developer Mode or an elevated token); creation
    /// failing is a failure, never a skip.
    #[cfg(windows)]
    #[test]
    fn a_symlink_swapped_in_just_before_the_rename_is_refused() {
        assert_swap_before_rename_is_refused(
            "swap-before-rename-link",
            None,
            |staging, bystander| std::os::windows::fs::symlink_file(bystander, staging).unwrap(),
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_file_replacing_the_staged_one_after_the_hold_is_released_is_refused_before_the_rename() {
        let dir = scratch_dir("windows-replaced-after-release");
        let output_path = dir.join("pokeemerald.pack");
        std::fs::write(&output_path, b"existing destination").unwrap();
        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);
        let carried = dir.join("carried-away");

        let mut staged =
            super::stage_at_first_free_name(std::iter::once(staging_path.clone()), b"our pack")
                .unwrap();
        staged.release_hold();
        std::fs::rename(&staging_path, &carried).unwrap();
        std::fs::write(&staging_path, b"an intruder's file").unwrap();
        let err = staged.publish(&output_path).unwrap_err();

        assert!(
            err.to_string()
                .contains("was replaced before it could be published"),
            "the retained identity must refuse the replacement: {err}"
        );
        assert_eq!(
            std::fs::read(&output_path).unwrap(),
            b"existing destination"
        );
        assert_eq!(std::fs::read(&staging_path).unwrap(), b"an intruder's file");
        assert_eq!(std::fs::read(&carried).unwrap(), b"our pack");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_replacement_landing_after_a_failed_rename_is_never_unlinked() {
        let dir = scratch_dir("swap-after-failed-rename");
        let output_path = dir.join("destination");
        std::fs::create_dir(&output_path).unwrap();
        let staging_path = staging_path_with_value(&output_path, TEST_STAGING_VALUE);

        let staged =
            super::stage_at_first_free_name(std::iter::once(staging_path.clone()), b"our pack")
                .unwrap();
        let err = staged
            .publish_with(
                &output_path,
                || {},
                || {
                    std::fs::remove_file(&staging_path).unwrap();
                    std::fs::write(&staging_path, b"an intruder's file").unwrap();
                },
            )
            .unwrap_err();

        assert!(
            err.to_string()
                .contains(&staging_path.display().to_string()),
            "{err}"
        );
        assert_eq!(
            std::fs::read(&staging_path).unwrap(),
            b"an intruder's file",
            "failure cleanup removed a file it could not prove was its own"
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
