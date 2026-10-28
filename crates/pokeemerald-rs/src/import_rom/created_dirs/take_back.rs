//! The Unix take-back protocol: how [`super::undo_created_directories`]
//! removes a created level without trusting its mutable name.
//!
//! A level whose name still resolves to the identity captured at creation
//! is renamed (no-replace) into a quarantine: one private directory per
//! rollback, made beside the outermost level rollback may take back, so it
//! is never inside a level it empties. A level on another filesystem than
//! that quarantine (`EXDEV`, as behind a `..` into a mount) gets a
//! quarantine of its own beside it, removed before the walk goes on; the
//! shared one is removed and its descriptor released first, and a level
//! further out claims a fresh one. In a
//! quarantine the level is re-checked by identity and unlinked through the
//! held quarantine, or put back (no-replace) under its name, or beside it
//! as `<name>.restored-<n>`, if it is not ours or holds something. Every
//! step that leaves a level standing says so on stderr.
//!
//! # Accepted residuals
//!
//! The approved `rustix` fs scope has no call that unlinks a directory by
//! descriptor. The quarantine is a directory this rollback just made, owned
//! by the current user with no group or other bits, under a random 64-bit
//! name from `/dev/urandom`, so only a writer running as that user can act
//! inside it, and such a writer can already delete or rewrite everything
//! the import owns.
//!
//! - The captured level is unlinked by its name inside the quarantine,
//!   after its identity check. That same writer could swap it in between;
//!   `unlinkat(.., REMOVEDIR)` removes only an empty directory, so the
//!   worst outcome is the loss of one empty directory. A level that writer
//!   moved out of the quarantine before that unlink is reported where it
//!   went on Linux and Android, where the held original's link count says
//!   whether it was moved or removed; elsewhere (Apple), a directory
//!   removed while held open still reads a nonzero `st_nlink`, so the two
//!   cannot be told apart without a trace after every rollback, and that
//!   moved level goes unreported. Where a level is reported missing from
//!   its name on such a platform, the warning says it may have been moved
//!   rather than claiming it was.
//! - The freshness check proves the shape of what `openat` opened, not who
//!   made it: the same writer could substitute an empty private directory
//!   at the random name between `mkdirat` and `openat`. A level held there
//!   is still checked by identity before it is removed or put back.
//!
//! One step rests on a name in a directory other accounts can write: the
//! empty quarantine's own `unlinkat(.., REMOVEDIR)`. Its name is re-checked
//! against the held quarantine by the `statat` just before it, and a
//! mismatch or a failed unlink leaves it standing with a warning naming its
//! path. A writer who moves the quarantine away and puts an empty directory
//! at its random name in the one-syscall window between the two loses that
//! empty directory; nothing else can be lost, since `REMOVEDIR` cannot
//! remove a directory holding anything. Leaving every quarantine standing
//! instead was rejected: it would leave a hidden directory and a warning
//! after every failed import, which is meant to leave no trace.
//!
//! The other name-based steps are the no-replace renames, which fail rather
//! than replace anything, so they cannot destroy data. A quarantine that
//! cannot be opened after `mkdirat` is left standing, since nothing proves
//! the name still denotes it. On a filesystem out of space, quota or
//! inodes the quarantine's `mkdirat` fails and the created levels are left
//! standing.
//!
//! Rollback is also not crash-safe: a process killed after a level was
//! renamed into its quarantine and before it was removed or put back leaves
//! it at `.undo-<hex>/level` with no warning, and no later run looks for
//! it. That level holds data only if another writer filled it, or swapped
//! in its own directory, during the import.

use std::ffi::OsStr;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};

use rom_import::OneLinePath;
use rustix::fs::{AtFlags, Dir, Mode, OFlags};

use super::{same_directory_identity, CreatedDirectory};

/// [`super::undo_created_directories`] with `before_take` run once per
/// level, after the name has been validated and before the level is taken
/// back, where a test swaps the name.
pub(super) fn undo_created_directories_with_hooks(
    created: &[CreatedDirectory],
    before_take: &mut dyn FnMut(),
) {
    undo_created_directories_with_capture_hooks(created, before_take, &mut |_, _| {});
}

/// Where in [`take_back_into`] its `in_quarantine` hook runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CaptureStage {
    /// The level was just renamed into the quarantine; its identity there
    /// is checked next.
    BeforeCheck,
    /// The capture was found to be this level; unlinking it is next.
    Verified,
    /// The capture was not removed; putting it back is next.
    BeforeRestore,
}

/// [`undo_created_directories_with_hooks`] with `in_quarantine` run, with
/// the held quarantine, at each [`CaptureStage`] of a take-back, where a
/// test acts on the capture.
pub(super) fn undo_created_directories_with_capture_hooks(
    created: &[CreatedDirectory],
    before_take: &mut dyn FnMut(),
    in_quarantine: &mut dyn FnMut(CaptureStage, BorrowedFd<'_>),
) {
    let mut quarantine = None;
    for dir in created.iter().rev() {
        let Some(parent) = dir.parent.as_deref() else {
            // Shedding releases pins outermost first, so every level from
            // here out is unpinned too: none can be vouched for, and the
            // walk stops at the first one still standing, saying so. One
            // missing from its recorded path may have been removed, or moved
            // with an ancestor renamed; nothing held tells which, so that is
            // said too.
            match std::fs::symlink_metadata(&dir.path) {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!(
                        "warning: {} is no longer at its recorded path, and rollback did not hold it open; if it was moved rather than removed, the directory this import made is left elsewhere",
                        OneLinePath(&dir.path)
                    );
                    continue;
                }
                _ => {
                    warn_left_standing(
                        dir,
                        format_args!(
                            "it was not held open, so rollback could not confirm it is the directory this import made; the levels above it are left too"
                        ),
                    );
                    break;
                }
            }
        };
        match rustix::fs::statat(parent, &dir.name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) if is_this_level(dir, &stat) => {
                before_take();
                if quarantine.is_none() {
                    quarantine = claim_quarantine(created, dir);
                }
                let Some(held) = quarantine.as_ref() else {
                    break;
                };
                match take_back_into(held, parent, dir, in_quarantine) {
                    Outcome::Taken => {}
                    Outcome::Stopped => break,
                    Outcome::CrossDevice => {
                        // The shared quarantine's descriptor is released
                        // first, so the fallback can open what it needs even
                        // when that descriptor took the last free slot; a
                        // level further out claims a fresh one.
                        if let Some(shared) = quarantine.take() {
                            remove_quarantine(&shared, &mut || {});
                        }
                        if !take_back_beside(parent, dir) {
                            break;
                        }
                    }
                }
            }
            // Gone already -- nothing to remove here, and the level above is
            // unaffected by that; but if it was moved rather than removed,
            // it still stands elsewhere, and that is reported.
            Err(err) if err == rustix::io::Errno::NOENT => warn_if_moved_away(dir),
            // Some other entry sits at the name now, or the level is no
            // longer held open to vouch for it: not provably this run's
            // directory, so it is left alone, and the levels above are left
            // too.
            Ok(stat) => {
                if same_directory_identity(&stat, &dir.identity) {
                    warn_left_standing(
                        dir,
                        format_args!(
                            "rollback no longer holds it open to confirm it is the directory this import made; the levels above it are left too"
                        ),
                    );
                } else {
                    warn_left_standing(
                        dir,
                        format_args!(
                            "another entry has taken its name; the levels above it are left too"
                        ),
                    );
                    warn_if_moved_away(dir);
                }
                break;
            }
            // It could not be examined at all: left alone, as are the
            // levels above.
            Err(err) => {
                warn_left_standing(
                    dir,
                    format_args!(
                        "it could not be examined: {}; the levels above it are left too",
                        std::io::Error::from(err)
                    ),
                );
                break;
            }
        }
    }
    if let Some(held) = quarantine {
        remove_quarantine(&held, &mut || {});
    }
}

/// What [`take_back_into`] did with one level.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// Removed, or already gone: the walk goes on.
    Taken,
    /// Left standing, and reported: the walk stops.
    Stopped,
    /// Not moved, because the quarantine is on another filesystem: nothing
    /// reported yet.
    CrossDevice,
}

/// Takes `dir` back through a quarantine of its own made beside it, on its
/// own filesystem, then removes that quarantine; answers whether the walk
/// may go on.
fn take_back_beside(parent: &OwnedFd, dir: &CreatedDirectory) -> bool {
    let Some(name) = unguessable_quarantine_name() else {
        warn_left_standing(dir, format_args!("no entropy for a private cleanup name"));
        return false;
    };
    let Some(local) = open_quarantine(parent.as_fd(), dir, &name, dir, &mut || {}) else {
        return false;
    };
    let outcome = take_back_into(&local, parent, dir, &mut |_, _| {});
    remove_quarantine(&local, &mut || {});
    match outcome {
        Outcome::Taken => true,
        Outcome::Stopped => false,
        Outcome::CrossDevice => {
            warn_left_standing(
                dir,
                format_args!("it could not be moved into a cleanup directory beside it"),
            );
            false
        }
    }
}

/// Whether `stat` is the directory `dir` created and still holds open.
fn is_this_level(dir: &CreatedDirectory, stat: &rustix::fs::Stat) -> bool {
    same_directory_identity(stat, &dir.identity)
        && dir.own.as_ref().is_some_and(|own| {
            rustix::fs::fstat(&**own).is_ok_and(|held| same_directory_identity(stat, &held))
        })
}

/// The entry name a captured level sits under inside its quarantine.
pub(super) const QUARANTINE_ENTRY: &str = "level";

/// `renameat` that refuses to replace an existing destination where the
/// platform can say so (Linux, Android, Apple); elsewhere it refuses with
/// `ENOSYS` rather than emulate that with a racy check.
fn rename_without_replacing(
    from_dir: impl AsFd,
    from: impl rustix::path::Arg + Copy,
    to_dir: impl AsFd,
    to: impl rustix::path::Arg + Copy,
) -> rustix::io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        rustix::fs::renameat_with(
            from_dir,
            from,
            to_dir,
            to,
            rustix::fs::RenameFlags::NOREPLACE,
        )
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
    {
        // No atomic no-replace rename here, and a check-then-rename could
        // overwrite a file a racer creates in the gap: fail closed, so the
        // level is left standing.
        let _ = (from_dir, from, to_dir, to);
        Err(rustix::io::Errno::NOSYS)
    }
}

/// A fresh quarantine basename no other account can predict.
pub(super) fn unguessable_quarantine_name() -> Option<String> {
    use std::io::Read as _;
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut urandom| urandom.read_exact(&mut bytes))
        .ok()?;
    Some(format!(".undo-{:016x}", u64::from_ne_bytes(bytes)))
}

/// Whether `quarantine` is an empty directory owned by this user with no
/// group or other permission bits, as a quarantine this rollback just made
/// with `0700` is under any umask; one that qualifies is then set to
/// exactly `0700` through its own descriptor, since a umask such as `0177`
/// masks the owner's search bit the capture's rename needs. It is read
/// through its own descriptor, never a second one opened for the listing,
/// so a quarantine that took the last free descriptor can still be vouched
/// for. A stat, listing or `fchmod` that fails is the error, not a verdict:
/// it says nothing about whose directory this is.
fn is_fresh_quarantine(quarantine: &mut Dir) -> rustix::io::Result<bool> {
    use rustix::fs::{FileType, Mode};
    let stat = quarantine.stat()?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::Directory
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || !(Mode::from_raw_mode(stat.st_mode) & Mode::RWXG.union(Mode::RWXO)).is_empty()
    {
        return Ok(false);
    }
    let mut listed = Ok(true);
    quarantine.all(|entry| match entry {
        Ok(entry) if matches!(entry.file_name().to_bytes(), b"." | b"..") => true,
        Ok(_) => {
            listed = Ok(false);
            false
        }
        Err(err) => {
            listed = Err(err);
            false
        }
    });
    if !listed? {
        return Ok(false);
    }
    rustix::fs::fchmod(quarantine.fd()?, Mode::RWXU)?;
    Ok(true)
}

/// Where `entry` beside `dir` is, for a warning. On Linux and Android it is
/// read from the held `parent` through `/proc/self/fd`, since a pinned
/// ancestor may have been renamed after creation and rollback acts through
/// `parent`, not the path; elsewhere, or without `/proc`, it is the
/// recorded path's sibling, which a rename since creation would make stale.
pub(super) fn located_beside(parent: impl AsFd, dir: &CreatedDirectory, entry: &OsStr) -> PathBuf {
    located_in(parent, entry).unwrap_or_else(|| dir.path.with_file_name(entry))
}

/// Where `entry` in the held directory `home` is now, read through
/// `/proc/self/fd` on Linux and Android; `None` elsewhere or without
/// `/proc`.
fn located_in(home: impl AsFd, entry: &OsStr) -> Option<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::fd::AsRawFd as _;
        let link = format!("/proc/self/fd/{}", home.as_fd().as_raw_fd());
        if let Ok(held) = std::fs::read_link(link) {
            if held.is_absolute() {
                return Some(held.join(entry));
            }
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let _ = (home, entry);
    None
}

/// Where the held directory `held` is now, read through `/proc/self/fd`
/// on Linux and Android; `None` elsewhere or without `/proc`.
fn held_location(held: impl AsFd) -> Option<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::fd::AsRawFd as _;
        let link = format!("/proc/self/fd/{}", held.as_fd().as_raw_fd());
        if let Ok(now) = std::fs::read_link(link) {
            if now.is_absolute() {
                return Some(now);
            }
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let _ = held;
    None
}

/// Where the quarantine is now, for a warning: its own held descriptor
/// first, since the quarantine itself may have been renamed; then `name`
/// in the held `home`, as [`located_beside`] reads a level's parent, since
/// `home` may have been renamed; then `recorded`, its location when it was
/// made, elsewhere or without `/proc`.
fn quarantine_location(
    quarantine: Option<BorrowedFd<'_>>,
    home: BorrowedFd<'_>,
    name: &str,
    recorded: &Path,
) -> PathBuf {
    quarantine
        .and_then(held_location)
        .or_else(|| located_in(home, OsStr::new(name)))
        .unwrap_or_else(|| recorded.to_owned())
}

/// A quarantine this rollback made and holds open, in `home` under `name`.
pub(super) struct Quarantine<'a> {
    /// The quarantine itself; listed only through this descriptor.
    dir: Dir,
    /// The directory the quarantine was made in.
    home: BorrowedFd<'a>,
    /// Its random basename in `home`.
    name: String,
    /// Where it was when it was made: the fallback for [`Self::location`].
    path: PathBuf,
}

impl Quarantine<'_> {
    /// The held descriptor, for calls that do not depend on its position.
    fn fd(&self) -> Option<BorrowedFd<'_>> {
        self.dir.fd().ok()
    }

    /// Where its name in `home` is now, for a warning about that name.
    fn name_location(&self) -> PathBuf {
        quarantine_location(None, self.home, &self.name, &self.path)
    }
}

/// Where `dir` itself is, for a warning that it was left standing.
fn level_location(dir: &CreatedDirectory) -> PathBuf {
    match dir.parent.as_deref() {
        Some(parent) => located_beside(parent, dir, &dir.name),
        None => dir.path.clone(),
    }
}

/// Reports that `dir` was left standing, and why.
fn warn_left_standing(dir: &CreatedDirectory, cause: std::fmt::Arguments<'_>) {
    eprintln!(
        "warning: {} is left in place: {cause}",
        OneLinePath(&level_location(dir))
    );
}

/// The quarantine for one rollback, made under a fresh random name beside
/// the outermost pinned level, so no level it empties contains it; `None`,
/// with `dir` reported as left standing, if none can be made.
fn claim_quarantine<'a>(
    created: &'a [CreatedDirectory],
    dir: &CreatedDirectory,
) -> Option<Quarantine<'a>> {
    let outermost = created.iter().find(|level| level.parent.is_some())?;
    let home = outermost.parent.as_deref()?;
    let Some(name) = unguessable_quarantine_name() else {
        warn_left_standing(dir, format_args!("no entropy for a private cleanup name"));
        return None;
    };
    open_quarantine(home.as_fd(), outermost, &name, dir, &mut || {})
}

/// Makes and opens the quarantine `name` in `home` (beside `home_level`),
/// with `after_mkdir` run between the two, where a test swaps the name, and
/// checks it is fresh. `None`, with `dir` reported as left standing, if it
/// cannot be made, held or vouched for; nothing at the name is removed.
fn open_quarantine<'a>(
    home: BorrowedFd<'a>,
    home_level: &CreatedDirectory,
    name: &str,
    dir: &CreatedDirectory,
    after_mkdir: &mut dyn FnMut(),
) -> Option<Quarantine<'a>> {
    open_quarantine_vouched_by(
        home,
        home_level,
        name,
        dir,
        after_mkdir,
        is_fresh_quarantine,
    )
}

/// [`open_quarantine`] with `vouch` in place of [`is_fresh_quarantine`], so
/// a test can make the freshness check fail with an error.
fn open_quarantine_vouched_by<'a>(
    home: BorrowedFd<'a>,
    home_level: &CreatedDirectory,
    name: &str,
    dir: &CreatedDirectory,
    after_mkdir: &mut dyn FnMut(),
    vouch: fn(&mut Dir) -> rustix::io::Result<bool>,
) -> Option<Quarantine<'a>> {
    let path = located_beside(home, home_level, OsStr::new(name));
    if let Err(err) = rustix::fs::mkdirat(home, name, Mode::RWXU) {
        warn_left_standing(
            dir,
            format_args!(
                "could not create cleanup directory {}: {}",
                OneLinePath(&path),
                std::io::Error::from(err)
            ),
        );
        return None;
    }
    after_mkdir();
    // `Dir::new` lists through this same descriptor, so checking the
    // quarantine costs no second one.
    let opened = rustix::fs::openat(
        home,
        name,
        OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .and_then(Dir::new);
    let Ok(mut held) = opened else {
        // Without a descriptor nothing proves the name still denotes the
        // directory just made, so it is not unlinked by name.
        warn_left_standing(
            dir,
            format_args!(
                "could not open cleanup directory {}, which is left in place too",
                OneLinePath(&path)
            ),
        );
        return None;
    };
    match vouch(&mut held) {
        Ok(true) => {}
        // The name was swapped for someone else's directory before the
        // open: it is not ours to fill or remove.
        Ok(false) => {
            warn_left_standing(
                dir,
                format_args!(
                    "{} is not the private cleanup directory rollback made, so it is left untouched",
                    OneLinePath(&path)
                ),
            );
            return None;
        }
        // The check itself failed: the directory may well be the one just
        // made, but nothing vouches for it, so it is left, with the error.
        Err(err) => {
            warn_left_standing(
                dir,
                format_args!(
                    "cleanup directory {} could not be checked: {}; it is left in place too",
                    OneLinePath(&path),
                    std::io::Error::from(err)
                ),
            );
            return None;
        }
    }
    Some(Quarantine {
        dir: held,
        home,
        name: name.to_owned(),
        path,
    })
}

/// Takes `dir` back from `parent` through `quarantine`: renames it in
/// (no-replace), re-checks its identity there, and unlinks it through the
/// held quarantine, or puts it back if it is not ours or holds something.
/// `in_quarantine` runs at each [`CaptureStage`], where a test acts on the
/// capture.
fn take_back_into(
    quarantine: &Quarantine<'_>,
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    in_quarantine: &mut dyn FnMut(CaptureStage, BorrowedFd<'_>),
) -> Outcome {
    take_back_into_revalidating(quarantine, parent, dir, in_quarantine, &holds_this_level)
}

/// What re-checks a put-back capture: [`holds_this_level`], or a test's
/// stand-in that fails the examination.
type Revalidate<'r> =
    dyn Fn(BorrowedFd<'_>, &OsStr, &CreatedDirectory) -> rustix::io::Result<bool> + 'r;

/// [`take_back_into`] with what was put back re-checked by `revalidate`
/// instead of [`holds_this_level`], so a test can make that examination
/// fail.
fn take_back_into_revalidating(
    quarantine: &Quarantine<'_>,
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    in_quarantine: &mut dyn FnMut(CaptureStage, BorrowedFd<'_>),
    revalidate: &Revalidate<'_>,
) -> Outcome {
    let Some(held) = quarantine.fd() else {
        warn_left_standing(dir, format_args!("its cleanup directory could not be used"));
        return Outcome::Stopped;
    };
    if let Err(err) = rename_without_replacing(parent, &dir.name, held, QUARANTINE_ENTRY) {
        if err == rustix::io::Errno::XDEV {
            return Outcome::CrossDevice;
        }
        // Already gone is already taken care of.
        let gone = matches!(
            rustix::fs::statat(parent, &dir.name, AtFlags::SYMLINK_NOFOLLOW),
            Err(err) if err == rustix::io::Errno::NOENT
        );
        if gone {
            warn_if_moved_away(dir);
            return Outcome::Taken;
        }
        warn_left_standing(
            dir,
            format_args!(
                "it could not be moved into its cleanup directory: {}",
                std::io::Error::from(err)
            ),
        );
        return Outcome::Stopped;
    }
    in_quarantine(CaptureStage::BeforeCheck, held);
    let captured = rustix::fs::statat(held, QUARANTINE_ENTRY, AtFlags::SYMLINK_NOFOLLOW);
    let ours = captured.as_ref().is_ok_and(|stat| is_this_level(dir, stat));
    let not_removed = if ours {
        in_quarantine(CaptureStage::Verified, held);
        match rustix::fs::unlinkat(held, QUARANTINE_ENTRY, AtFlags::REMOVEDIR) {
            // The unlink acts on a name: had another writer moved this level
            // out and put an empty directory in its place, the replacement
            // went and the level stands wherever it was moved, so it is
            // reported there. Only where a removed directory's link count
            // reads 0 can that be told apart from the level's own removal;
            // elsewhere this is an accepted residual (module doc).
            Ok(()) => {
                if matches!(standing(dir), Standing::Linked(_) | Standing::Unexamined(_)) {
                    warn_if_moved_away(dir);
                }
                return Outcome::Taken;
            }
            Err(err) => Some(err),
        }
    } else {
        None
    };
    in_quarantine(CaptureStage::BeforeRestore, held);
    // A replacement that was captured, or this level holding something: put
    // it back under its name, or beside it if the name was retaken
    // meanwhile, so it never survives only under the hidden quarantine.
    let put = put_back(
        held,
        quarantine.home,
        &quarantine.name,
        &quarantine.path,
        parent,
        dir,
    );
    // What was put back is this level only if it still is once there: a
    // writer could have moved it out of the quarantine and put another
    // directory in its place before the put-back, which then moved the
    // replacement and left this level wherever that writer put it. A
    // re-check that fails says neither: it is reported with its error, not
    // as another writer's directory.
    let original_put_back = match where_put_back(&put, dir, parent, held) {
        Some((home, name)) if ours => revalidate(home, name, dir),
        _ => Ok(false),
    };
    let mismatched = matches!(original_put_back, Ok(false));
    match &put {
        // This level, not removed, was moved out of the quarantine by
        // another writer before it could be put back: it stands wherever
        // that writer put it, so it is reported there, through the held
        // original, not at the quarantine it left. It no longer sits in its
        // parent, so, as with a level gone before its rename, the walk goes
        // on.
        PutBack::Gone(_) if ours => {
            warn_if_moved_away(dir);
            return Outcome::Taken;
        }
        // What came back, or stayed in the quarantine, is no longer this
        // level: the warnings written for it would place its contents
        // where they are not. This level is reported below, where it is
        // now; a capture stranded in the quarantine keeps it from being
        // removed, which its removal reports.
        PutBack::Aside { name, .. } if ours && mismatched => eprintln!(
            "warning: {} was put back during cleanup in place of {}, but it is not the directory rollback made",
            OneLinePath(&located_beside(parent, dir, name)),
            OneLinePath(&level_location(dir))
        ),
        PutBack::Stranded(_) if ours && mismatched => {}
        PutBack::Gone(warning) | PutBack::Aside { warning, .. } | PutBack::Stranded(warning) => {
            eprintln!("{warning}");
        }
        PutBack::InPlace => {
            if let (Some(err), Ok(true)) = (not_removed, original_put_back) {
                warn_left_standing(
                    dir,
                    format_args!("it could not be removed: {}", std::io::Error::from(err)),
                );
            }
        }
    }
    // The warning written for the capture stands; if what is there now
    // could not be re-checked, that is said too, with the error.
    if let Err(err) = original_put_back {
        warn_put_back_unexamined(quarantine, held, parent, dir, &put, not_removed, err);
    }
    // Something else took the name after validation: the replacement, put
    // back above, stops the walk, which leaves the levels above standing.
    // The original is reported where it went only if it still has a link,
    // since it may have been removed rather than moved. So is this level
    // when what was put back turned out to be another writer's directory.
    // A capture that could not be examined stops the walk too, and says so.
    match captured {
        Ok(_) if mismatched => {
            warn_left_standing(
                dir,
                format_args!("another entry has taken its name; the levels above it are left too"),
            );
            warn_if_moved_away(dir);
        }
        Ok(_) => {}
        // Whatever was captured could not be examined, so it was put back
        // unverified (or reported where it went), and the walk stops here.
        Err(err) => warn_left_standing(
            dir,
            format_args!(
                "it could not be examined in its cleanup directory: {}; the levels above it are left too",
                std::io::Error::from(err)
            ),
        ),
    }
    Outcome::Stopped
}

/// The held directory and name a capture of `dir` that `put` back from
/// `quarantine` now sits under, beside `parent` or still in the
/// quarantine; `None` if it left the quarantine.
fn where_put_back<'p>(
    put: &'p PutBack,
    dir: &'p CreatedDirectory,
    parent: &'p OwnedFd,
    quarantine: BorrowedFd<'p>,
) -> Option<(BorrowedFd<'p>, &'p OsStr)> {
    match put {
        PutBack::InPlace => Some((parent.as_fd(), &dir.name)),
        PutBack::Aside { name, .. } => Some((parent.as_fd(), name)),
        PutBack::Stranded(_) => Some((quarantine, QUARANTINE_ENTRY.as_ref())),
        PutBack::Gone(_) => None,
    }
}

/// Reports that what the cleanup put back for `dir` (as `put`, from the
/// held quarantine `held`) could not be re-checked to be the directory
/// rollback made, with the error `err`, so the walk stops there; a level
/// put back under its own name is reported with `not_removed`, why it was
/// not removed, as well.
fn warn_put_back_unexamined(
    quarantine: &Quarantine<'_>,
    held: BorrowedFd<'_>,
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    put: &PutBack,
    not_removed: Option<rustix::io::Errno>,
    err: rustix::io::Errno,
) {
    let location = match put {
        PutBack::InPlace => {
            let removal = not_removed.map_or_else(String::new, |removal| {
                format!(
                    "it could not be removed: {}; ",
                    std::io::Error::from(removal)
                )
            });
            warn_left_standing(
                dir,
                format_args!(
                    "{removal}it was put back under its name, where it could not be examined: {}; the levels above it are left too",
                    std::io::Error::from(err)
                ),
            );
            return;
        }
        PutBack::Aside { name, .. } => located_beside(parent, dir, name),
        PutBack::Stranded(_) => quarantine_location(
            Some(held),
            quarantine.home,
            &quarantine.name,
            &quarantine.path,
        )
        .join(QUARANTINE_ENTRY),
        PutBack::Gone(_) => return,
    };
    eprintln!(
        "warning: {} was put back during cleanup but could not be examined there: {}; the levels above it are left too",
        OneLinePath(&location),
        std::io::Error::from(err)
    );
}

/// Reports that the directory rollback made for `dir`, no longer under its
/// name, was moved away rather than removed, naming where the held
/// original is now where `/proc` can say. Silent when it has no link left
/// (removed outright) or is no longer held open, so nothing can be said;
/// where its link count cannot tell moved from removed ([`Standing::Unknown`]),
/// the warning says it may have been moved.
fn warn_if_moved_away(dir: &CreatedDirectory) {
    report_moved_away(dir, standing(dir));
}

/// [`warn_if_moved_away`] with `standing` as what the held original's
/// examination said, so a test can make it fail.
fn report_moved_away(dir: &CreatedDirectory, standing: Standing) {
    let level = level_location(dir);
    match standing {
        Standing::Gone => {}
        Standing::Linked(Some(now)) => eprintln!(
            "warning: {} is the directory rollback made for {}, moved away before rollback could take it back; it is left in place",
            OneLinePath(&now),
            OneLinePath(&level)
        ),
        Standing::Linked(None) => eprintln!(
            "warning: {} was moved away before rollback could take it back; the directory rollback made is left elsewhere",
            OneLinePath(&level)
        ),
        Standing::Unknown => eprintln!(
            "warning: {} is no longer under its name; if it was moved rather than removed, the directory rollback made is left elsewhere",
            OneLinePath(&level)
        ),
        Standing::Unexamined(err) => eprintln!(
            "warning: {} is no longer under its name, and the directory rollback made for it could not be examined: {}; if it was moved rather than removed, it is left elsewhere",
            OneLinePath(&level),
            std::io::Error::from(err)
        ),
    }
}

/// What the held original's link count says about it.
enum Standing {
    /// Removed outright (a link count of 0), or no longer held, so nothing
    /// can be said about it.
    Gone,
    /// Still linked somewhere; where, if `/proc` can say.
    Linked(Option<PathBuf>),
    /// Its link count does not tell: on Apple a directory removed while
    /// held open still reads a nonzero `st_nlink` (the
    /// `a_level_removed_and_replaced_after_validation_is_not_reported_as_moved`
    /// failure on macOS CI), so outside Linux and Android only a count of 0
    /// is trusted.
    Unknown,
    /// Its `fstat` failed: it may still be linked anywhere, so the error is
    /// reported rather than read as its removal.
    Unexamined(rustix::io::Errno),
}

/// [`Standing`] of the directory `dir` holds open.
fn standing(dir: &CreatedDirectory) -> Standing {
    let Some(own) = dir.own.as_deref() else {
        return Standing::Gone;
    };
    held_standing(own.as_fd())
}

/// [`Standing`] of the held directory `held`.
fn held_standing(held: BorrowedFd<'_>) -> Standing {
    held_standing_as(rustix::fs::fstat(held), held)
}

/// [`held_standing`] with `examined` as what `held`'s own `fstat` gave.
/// Only a link count of 0 proves removal; a failed examination is
/// [`Standing::Unexamined`].
fn held_standing_as(
    examined: rustix::io::Result<rustix::fs::Stat>,
    held: BorrowedFd<'_>,
) -> Standing {
    match examined {
        Ok(stat) if stat.st_nlink != 0 => {}
        Ok(_) => return Standing::Gone,
        Err(err) => return Standing::Unexamined(err),
    }
    if cfg!(any(target_os = "linux", target_os = "android")) {
        Standing::Linked(held_location(held))
    } else {
        Standing::Unknown
    }
}

/// Whether `name` in the held `home` is the directory `dir` created and
/// still holds open; a vacant name is not. A lookup or held-descriptor
/// `fstat` that fails otherwise is the error, not a verdict.
fn holds_this_level(
    home: BorrowedFd<'_>,
    name: &OsStr,
    dir: &CreatedDirectory,
) -> rustix::io::Result<bool> {
    let stat = match rustix::fs::statat(home, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(err) if err == rustix::io::Errno::NOENT => return Ok(false),
        Err(err) => return Err(err),
    };
    if !same_directory_identity(&stat, &dir.identity) {
        return Ok(false);
    }
    let Some(own) = dir.own.as_deref() else {
        return Ok(false);
    };
    Ok(same_directory_identity(&stat, &rustix::fs::fstat(own)?))
}

/// Moves the capture in `quarantine` (named `quarantine_name` in the held
/// `quarantine_home`, and at `recorded` when it was made) back under
/// `dir`'s name, or under the first free `<name>.restored-<n>` beside it.
/// Returns the warning to print when the capture did not come back under
/// its own name, naming exactly where it is: the `.restored-<n>` path, or
/// the quarantine's current path if none took it.
#[cfg(test)]
pub(super) fn restore_capture(
    quarantine: BorrowedFd<'_>,
    quarantine_home: BorrowedFd<'_>,
    quarantine_name: &str,
    recorded: &Path,
    parent: &OwnedFd,
    dir: &CreatedDirectory,
) -> Option<String> {
    match put_back(
        quarantine,
        quarantine_home,
        quarantine_name,
        recorded,
        parent,
        dir,
    ) {
        PutBack::InPlace => None,
        PutBack::Aside { warning, .. } | PutBack::Stranded(warning) | PutBack::Gone(warning) => {
            Some(warning)
        }
    }
}

/// Where [`put_back`] left a capture.
enum PutBack {
    /// Back under its own name.
    InPlace,
    /// Under the `.restored-<n>` name `name`: the warning names it.
    Aside {
        /// The warning naming where it is.
        warning: String,
        /// The `.restored-<n>` name it is under.
        name: std::ffi::OsString,
    },
    /// Still in the quarantine: the warning names exactly where.
    Stranded(String),
    /// No longer in the quarantine, so another writer moved or removed it
    /// before it could be put back: the warning says so.
    Gone(String),
}

/// [`restore_capture`], telling a capture that vanished from the
/// quarantine apart from one stranded there.
fn put_back(
    quarantine: BorrowedFd<'_>,
    quarantine_home: BorrowedFd<'_>,
    quarantine_name: &str,
    recorded: &Path,
    parent: &OwnedFd,
    dir: &CreatedDirectory,
) -> PutBack {
    let beside = |entry: &OsStr| located_beside(parent, dir, entry);
    if rename_without_replacing(quarantine, QUARANTINE_ENTRY, parent, &dir.name).is_ok() {
        return PutBack::InPlace;
    }
    for n in 0..16 {
        let mut aside = dir.name.clone();
        aside.push(format!(".restored-{n}"));
        if rename_without_replacing(quarantine, QUARANTINE_ENTRY, parent, &aside).is_ok() {
            return PutBack::Aside {
                warning: format!(
                    "warning: {} was recreated during cleanup; its contents are in {}",
                    OneLinePath(&beside(&dir.name)),
                    OneLinePath(&beside(&aside))
                ),
                name: aside,
            };
        }
    }
    let location =
        quarantine_location(Some(quarantine), quarantine_home, quarantine_name, recorded);
    if matches!(
        rustix::fs::statat(quarantine, QUARANTINE_ENTRY, AtFlags::SYMLINK_NOFOLLOW),
        Err(err) if err == rustix::io::Errno::NOENT
    ) {
        return PutBack::Gone(format!(
            "warning: could not put {} back: it left cleanup directory {}, moved or removed by another writer",
            OneLinePath(&beside(&dir.name)),
            OneLinePath(&location)
        ));
    }
    PutBack::Stranded(format!(
        "warning: could not put {} back; its contents remain in {}",
        OneLinePath(&beside(&dir.name)),
        OneLinePath(&location.join(QUARANTINE_ENTRY))
    ))
}

/// Removes the empty `quarantine` by its name, only while that name still
/// resolves to the held quarantine, checked by the `statat` just before the
/// unlink; a mismatch, a name or held quarantine that could not be
/// examined, or a failed unlink leaves it standing. A failed unlink or examination is reported at
/// its name's path, with the error, since that is what the call acts on; a
/// mismatch through [`warn_quarantine_not_at_name`]. `before_removal` runs
/// before that check, where a test swaps the name.
fn remove_quarantine(quarantine: &Quarantine<'_>, before_removal: &mut dyn FnMut()) {
    remove_quarantine_held_as(quarantine, quarantine.dir.stat(), before_removal);
}

/// [`remove_quarantine`] with `held` as what the held quarantine's own
/// `fstat` gave, so a test can make that examination fail.
fn remove_quarantine_held_as(
    quarantine: &Quarantine<'_>,
    held: rustix::io::Result<rustix::fs::Stat>,
    before_removal: &mut dyn FnMut(),
) {
    before_removal();
    let by_name = rustix::fs::statat(
        quarantine.home,
        quarantine.name.as_str(),
        AtFlags::SYMLINK_NOFOLLOW,
    );
    // A lookup that failed other than for a vacant name says nothing about
    // where the quarantine is: it is left, and the error is the cause.
    if let Some(&err) = by_name
        .as_ref()
        .err()
        .filter(|&&err| err != rustix::io::Errno::NOENT)
    {
        eprintln!(
            "warning: cleanup directory {} could not be examined: {}; it is left in place",
            OneLinePath(&quarantine.name_location()),
            std::io::Error::from(err)
        );
        return;
    }
    // The name resolves to something, but the held quarantine could not be
    // examined to compare with it: that says nothing about where the
    // quarantine is either, so it is left with that error as the cause.
    if let (Err(err), Ok(_)) = (&held, &by_name) {
        eprintln!(
            "warning: cleanup directory {} could not be examined: {}; it is left in place",
            OneLinePath(&quarantine.name_location()),
            std::io::Error::from(*err)
        );
        return;
    }
    let resolves = match (&held, &by_name) {
        (Ok(held), Ok(by_name)) => same_directory_identity(held, by_name),
        _ => false,
    };
    if !resolves {
        let standing = match quarantine.fd() {
            Some(fd) => held_standing_as(held, fd),
            None => Standing::Unknown,
        };
        warn_quarantine_not_at_name(quarantine, standing, &by_name);
        return;
    }
    if let Err(err) = rustix::fs::unlinkat(
        quarantine.home,
        quarantine.name.as_str(),
        AtFlags::REMOVEDIR,
    ) {
        eprintln!(
            "warning: could not remove cleanup directory {}: {}; it is left in place",
            OneLinePath(&quarantine.name_location()),
            std::io::Error::from(err)
        );
    }
}

/// Reports a quarantine whose name no longer resolves to it, so it was not
/// removed: whatever now holds the name is described as such, and the held
/// quarantine, the directory rollback actually leaves, is named where it is
/// now where `/proc` can say, as `standing` (the held quarantine's
/// examination) tells. Its name is all that is known when (outside Linux
/// and Android) its link count does not tell whether it was moved or
/// removed; when the held quarantine could not be examined, the error is
/// reported too.
fn warn_quarantine_not_at_name(
    quarantine: &Quarantine<'_>,
    standing: Standing,
    by_name: &rustix::io::Result<rustix::fs::Stat>,
) {
    let name = quarantine.name_location();
    let vacant = matches!(by_name, Err(err) if *err == rustix::io::Errno::NOENT);
    if !vacant || matches!(standing, Standing::Unknown | Standing::Unexamined(_)) {
        eprintln!(
            "warning: cleanup directory {} no longer resolves to the one rollback made; it is left in place",
            OneLinePath(&name)
        );
    }
    match standing {
        Standing::Linked(Some(now)) => eprintln!(
            "warning: {} is the cleanup directory rollback made, moved away from {}; it is left in place",
            OneLinePath(&now),
            OneLinePath(&name)
        ),
        Standing::Linked(None) => eprintln!(
            "warning: cleanup directory {} was moved away before rollback could remove it; it is left elsewhere",
            OneLinePath(&name)
        ),
        Standing::Unexamined(err) => eprintln!(
            "warning: the cleanup directory rollback made for {} could not be examined: {}; if it was moved rather than removed, it is left elsewhere",
            OneLinePath(&name),
            std::io::Error::from(err)
        ),
        Standing::Gone | Standing::Unknown => {}
    }
}

#[cfg(test)]
/// [`undo_created_directories_with_hooks`]'s take-back of one level, with
/// the quarantine made beside it under `quarantine_name`, so a test can
/// stage something at the name before rollback tries to claim it, and
/// `after_mkdir` run between making the quarantine and opening it, where a
/// test swaps the name. Returns whether the level was taken back.
pub(super) fn take_back_level_via(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    quarantine_name: &str,
    after_mkdir: &mut dyn FnMut(),
) -> bool {
    take_back_level_with_hooks(parent, dir, quarantine_name, after_mkdir, &mut || {})
}

#[cfg(test)]
/// [`take_back_level_via`] with the new quarantine vouched for by `vouch`
/// instead of [`is_fresh_quarantine`], so a test can make that check fail
/// with an error.
pub(super) fn take_back_level_vouched_by(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    quarantine_name: &str,
    vouch: fn(&mut Dir) -> rustix::io::Result<bool>,
) -> bool {
    let Some(quarantine) =
        open_quarantine_vouched_by(parent.as_fd(), dir, quarantine_name, dir, &mut || {}, vouch)
    else {
        return false;
    };
    let taken = take_back_into(&quarantine, parent, dir, &mut |_, _| {}) == Outcome::Taken;
    remove_quarantine(&quarantine, &mut || {});
    taken
}

#[cfg(test)]
/// [`take_back_level_via`] with the quarantine's name re-checked for its
/// removal as `looked_up` instead, so a test can make that lookup fail
/// other than with `ENOENT` (an over-long name gives `ENAMETOOLONG`).
pub(super) fn take_back_level_looking_up(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    quarantine_name: &str,
    looked_up: &str,
) -> bool {
    let Some(mut quarantine) =
        open_quarantine(parent.as_fd(), dir, quarantine_name, dir, &mut || {})
    else {
        return false;
    };
    let taken = take_back_into(&quarantine, parent, dir, &mut |_, _| {}) == Outcome::Taken;
    looked_up.clone_into(&mut quarantine.name);
    remove_quarantine(&quarantine, &mut || {});
    taken
}

#[cfg(test)]
/// [`take_back_level_via`] with the held quarantine's examination for its
/// removal failing with `err`, and `before_removal` run just before its
/// name is re-checked, where a test moves it.
pub(super) fn take_back_level_held_stat_failing(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    quarantine_name: &str,
    err: rustix::io::Errno,
    before_removal: &mut dyn FnMut(),
) -> bool {
    let Some(quarantine) = open_quarantine(parent.as_fd(), dir, quarantine_name, dir, &mut || {})
    else {
        return false;
    };
    let taken = take_back_into(&quarantine, parent, dir, &mut |_, _| {}) == Outcome::Taken;
    remove_quarantine_held_as(&quarantine, Err(err), before_removal);
    taken
}

#[cfg(test)]
/// [`take_back_level_via`] with the re-check of whatever was put back
/// failing with `err`, and `in_quarantine` run at each [`CaptureStage`].
pub(super) fn take_back_level_revalidation_failing(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    quarantine_name: &str,
    err: rustix::io::Errno,
    in_quarantine: &mut dyn FnMut(CaptureStage, BorrowedFd<'_>),
) -> bool {
    let Some(quarantine) = open_quarantine(parent.as_fd(), dir, quarantine_name, dir, &mut || {})
    else {
        return false;
    };
    let taken =
        take_back_into_revalidating(&quarantine, parent, dir, in_quarantine, &|_, _, _| Err(err))
            == Outcome::Taken;
    remove_quarantine(&quarantine, &mut || {});
    taken
}

#[cfg(test)]
/// Reports `dir` as no longer under its name with its held original's
/// examination failing with `err`, as [`warn_if_moved_away`] would.
pub(super) fn warn_if_moved_away_held_stat_failing(dir: &CreatedDirectory, err: rustix::io::Errno) {
    let own = dir.own.as_deref().expect("the level is held open");
    report_moved_away(dir, held_standing_as(Err(err), own.as_fd()));
}

#[cfg(test)]
/// [`take_back_level_via`] with `before_removal` run just before the
/// quarantine's name is re-checked for its removal, where a test swaps it.
pub(super) fn take_back_level_with_hooks(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    quarantine_name: &str,
    after_mkdir: &mut dyn FnMut(),
    before_removal: &mut dyn FnMut(),
) -> bool {
    let Some(quarantine) = open_quarantine(parent.as_fd(), dir, quarantine_name, dir, after_mkdir)
    else {
        return false;
    };
    let taken = take_back_into(&quarantine, parent, dir, &mut |_, _| {}) == Outcome::Taken;
    remove_quarantine(&quarantine, before_removal);
    taken
}
