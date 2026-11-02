//! Taking one level back through its quarantine: renaming it in without
//! replacing anything, re-checking its identity there, and unlinking it
//! through the held quarantine, or putting it back under its name, or beside
//! it as `<name>.restored-<n>`.
//!
//! # Accepted residuals
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
//!
//! The name-based steps here are the no-replace renames, which fail rather
//! than replace anything, so they cannot destroy data.

use std::ffi::OsStr;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::Path;

use rom_import::OneLinePath;
use rustix::fs::AtFlags;

use super::super::{same_directory_identity, CreatedDirectory};
use super::quarantine::{Quarantine, QUARANTINE_ENTRY};
use super::report::{
    level_location, located_beside, quarantine_location, standing, warn_if_moved_away,
    warn_left_standing, warn_put_back_unexamined, Standing,
};
use super::{CaptureStage, Outcome};

/// Whether `stat` is the directory `dir` created and still holds open. A
/// held original whose `fstat` fails is the error, not a verdict.
pub(super) fn is_this_level(
    dir: &CreatedDirectory,
    stat: &rustix::fs::Stat,
) -> rustix::io::Result<bool> {
    is_this_level_held_as(dir, stat, &|own| rustix::fs::fstat(own))
}

/// How the held original is examined: `fstat`, or a test's stand-in that
/// fails.
type HeldStat<'h> = dyn Fn(BorrowedFd<'_>) -> rustix::io::Result<rustix::fs::Stat> + 'h;

/// [`is_this_level`] with the held original examined by `held_stat`.
fn is_this_level_held_as(
    dir: &CreatedDirectory,
    stat: &rustix::fs::Stat,
    held_stat: &HeldStat<'_>,
) -> rustix::io::Result<bool> {
    if !same_directory_identity(stat, &dir.identity) {
        return Ok(false);
    }
    let Some(own) = dir.own.as_deref() else {
        return Ok(false);
    };
    Ok(same_directory_identity(stat, &held_stat(own.as_fd())?))
}

/// Whether `name` in the held `home` is the directory `dir` created and
/// still holds open; a vacant name is not. A lookup or held-descriptor
/// `fstat` that fails otherwise is the error, not a verdict.
pub(super) fn holds_this_level(
    home: BorrowedFd<'_>,
    name: &OsStr,
    dir: &CreatedDirectory,
) -> rustix::io::Result<bool> {
    let stat = match rustix::fs::statat(home, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(err) if err == rustix::io::Errno::NOENT => return Ok(false),
        Err(err) => return Err(err),
    };
    is_this_level(dir, &stat)
}

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

/// Takes `dir` back from `parent` through `quarantine`: renames it in
/// (no-replace), re-checks its identity there, and unlinks it through the
/// held quarantine, or puts it back if it is not ours or holds something.
/// `in_quarantine` runs at each [`CaptureStage`], where a test acts on the
/// capture.
pub(super) fn take_back_into(
    quarantine: &Quarantine<'_>,
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    in_quarantine: &mut dyn FnMut(CaptureStage, BorrowedFd<'_>),
) -> Outcome {
    take_back_into_revalidating(
        quarantine,
        parent,
        dir,
        in_quarantine,
        &|own| rustix::fs::fstat(own),
        &holds_this_level,
    )
}

/// What re-checks a put-back capture: [`holds_this_level`], or a test's
/// stand-in that fails the examination.
type Revalidate<'r> =
    dyn Fn(BorrowedFd<'_>, &OsStr, &CreatedDirectory) -> rustix::io::Result<bool> + 'r;

/// [`take_back_into`] with the capture's held original examined by
/// `held_stat` instead of `fstat`, and what was put back re-checked by
/// `revalidate` instead of [`holds_this_level`], so a test can make either
/// examination fail.
pub(super) fn take_back_into_revalidating(
    quarantine: &Quarantine<'_>,
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    in_quarantine: &mut dyn FnMut(CaptureStage, BorrowedFd<'_>),
    held_stat: &HeldStat<'_>,
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
    // The capture's lookup or its held original's `fstat` failing is the
    // examination's error, reported below, not a verdict that it is not ours.
    let captured = check_capture(held, dir, held_stat);
    let ours = matches!(captured, Ok(true));
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

/// Whether the capture in `quarantine` is `dir`, with its held original
/// examined by `held_stat`; a failed lookup or examination is the error.
fn check_capture(
    quarantine: BorrowedFd<'_>,
    dir: &CreatedDirectory,
    held_stat: &HeldStat<'_>,
) -> rustix::io::Result<bool> {
    rustix::fs::statat(quarantine, QUARANTINE_ENTRY, AtFlags::SYMLINK_NOFOLLOW)
        .and_then(|stat| is_this_level_held_as(dir, &stat, held_stat))
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

/// Moves the capture in `quarantine` (named `quarantine_name` in the held
/// `quarantine_home`, and at `recorded` when it was made) back under
/// `dir`'s name, or under the first free `<name>.restored-<n>` beside it.
/// Returns the warning to print when the capture did not come back under
/// its own name, naming exactly where it is: the `.restored-<n>` path, or
/// the quarantine's current path if none took it.
#[cfg(test)]
pub(in super::super) fn restore_capture(
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
pub(super) enum PutBack {
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
