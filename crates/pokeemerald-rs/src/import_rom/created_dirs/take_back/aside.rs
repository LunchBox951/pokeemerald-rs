//! Taking one level back where no quarantine can be made: the filesystem is
//! out of space, quota or inodes, so the quarantine's `mkdirat` fails with
//! `ENOSPC` or `EDQUOT` (see [`super::quarantine`]).
//!
//! The level is renamed (no-replace) to a random `.undo-<hex>` name in its own
//! pinned parent, which makes no directory, so it needs no inode of its own.
//! Whatever the rename moved is then re-checked by identity against the held
//! original and unlinked with `unlinkat(.., REMOVEDIR)` under that name, or,
//! if it is not this level or holds something, put back (no-replace) under the
//! level's name or beside it as `<name>.restored-<n>`, as a captured level is
//! in [`super::restore`]. A directory this run did not create is therefore
//! never unlinked unless it is swapped in at the random name in the one call
//! between the check and the unlink.
//!
//! # Accepted residuals
//!
//! - The random name is not private: it sits in the level's own parent,
//!   which other accounts may write. A writer who finds it and, between the
//!   identity check and the unlink, moves the level away and puts an empty
//!   directory of its own there loses that empty directory;
//!   `unlinkat(.., REMOVEDIR)` removes only an empty one, so nothing else can
//!   be lost. This is the residual the quarantine's own removal and the
//!   in-place removal accept (see [`super::quarantine`] and
//!   [`super::restore`]), and a level so moved is reported where it went
//!   where the held original's link count says so.
//! - A rename needs directory space too, on some filesystems: if it fails,
//!   the level is left standing with a warning that names the cause.

use std::ffi::OsString;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rom_import::OneLinePath;
use rustix::fs::AtFlags;

use super::super::CreatedDirectory;
use super::quarantine::unguessable_quarantine_name;
use super::report::{
    level_location, located_beside, standing, warn_if_moved_away, warn_left_standing, Standing,
};
use super::restore::{holds_this_level, is_this_level, rename_without_replacing};
use super::{CaptureStage, Outcome};

/// Where [`put_back_aside`] left a level moved aside.
enum PutBack {
    /// Back under its own name.
    InPlace,
    /// Under the `.restored-<n>` name given.
    Aside(OsString),
    /// Still under its random name: nothing else would take it.
    Stranded,
    /// No longer under its random name: another writer moved or removed it.
    Gone,
}

/// Takes `dir` back from the held `parent` without a quarantine: moves it
/// to a random name there, re-checks it, and unlinks it, or puts it back.
/// `in_aside` runs at each [`CaptureStage`] with `parent`, where a test acts
/// on the capture.
pub(super) fn take_back_aside(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    in_aside: &mut dyn FnMut(CaptureStage, BorrowedFd<'_>),
) -> Outcome {
    let Some(random) = unguessable_quarantine_name() else {
        warn_left_standing(dir, format_args!("no entropy for a private cleanup name"));
        return Outcome::Stopped;
    };
    let aside = OsString::from(random);
    if let Some(outcome) = move_aside(parent, dir, &aside) {
        return outcome;
    }
    in_aside(CaptureStage::BeforeCheck, parent.as_fd());
    // The lookup or the held original's `fstat` failing is the examination's
    // error, reported below, not a verdict that it is not ours.
    let captured = rustix::fs::statat(parent, &aside, AtFlags::SYMLINK_NOFOLLOW)
        .and_then(|stat| is_this_level(dir, &stat));
    let ours = matches!(captured, Ok(true));
    let not_removed = if ours {
        in_aside(CaptureStage::Verified, parent.as_fd());
        match rustix::fs::unlinkat(parent, &aside, AtFlags::REMOVEDIR) {
            // The unlink acts on a name: had a writer moved this level out
            // and put an empty directory in its place, the replacement went
            // and the level stands wherever it was moved, so it is reported
            // there, where the link count can tell (module doc).
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
    in_aside(CaptureStage::BeforeRestore, parent.as_fd());
    let put = put_back_aside(parent, dir, &aside);
    // What was put back is this level only if it still is once there: a
    // writer could have swapped the random name before the put-back. A
    // re-check that fails says neither.
    let original_put_back = if ours {
        match &put {
            PutBack::InPlace => holds_this_level(parent.as_fd(), &dir.name, dir),
            PutBack::Aside(name) => holds_this_level(parent.as_fd(), name, dir),
            PutBack::Stranded => holds_this_level(parent.as_fd(), &aside, dir),
            PutBack::Gone => Ok(false),
        }
    } else {
        Ok(false)
    };
    let mismatched = matches!(original_put_back, Ok(false));
    // This level was moved away by another writer after the check: it
    // stands wherever it went, and no longer sits in its parent.
    if ours && matches!(put, PutBack::Gone) {
        warn_if_moved_away(dir);
        return Outcome::Taken;
    }
    report_put_back(parent, dir, &aside, &put, ours && mismatched);
    if let (PutBack::InPlace, Some(err), Ok(true)) = (&put, not_removed, original_put_back) {
        warn_left_standing(
            dir,
            format_args!("it could not be removed: {}", std::io::Error::from(err)),
        );
    }
    if let Err(err) = original_put_back {
        eprintln!(
            "warning: {} could not be re-checked after it was put back: {}",
            OneLinePath(&level_location(dir)),
            std::io::Error::from(err)
        );
    }
    match captured {
        Ok(_) if mismatched => {
            warn_left_standing(
                dir,
                format_args!("another entry has taken its name; the levels above it are left too"),
            );
            warn_if_moved_away(dir);
        }
        Ok(_) => {}
        Err(err) => warn_left_standing(
            dir,
            format_args!(
                "it could not be examined after being moved aside: {}; the levels above it are left too",
                std::io::Error::from(err)
            ),
        ),
    }
    Outcome::Stopped
}

/// Says where `put` left what was moved aside from `dir`'s name. `foreign`:
/// it was this level's capture, but what came back is another directory,
/// so the warnings written for the level would misplace its contents; this
/// level is reported by the caller, where it is now.
fn report_put_back(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    aside: &OsString,
    put: &PutBack,
    foreign: bool,
) {
    let beside = |entry: &std::ffi::OsStr| located_beside(parent, dir, entry);
    match put {
        PutBack::Gone => eprintln!(
            "warning: could not put {} back: it left {}, moved or removed by another writer",
            OneLinePath(&beside(&dir.name)),
            OneLinePath(&beside(aside))
        ),
        PutBack::Aside(name) if foreign => eprintln!(
            "warning: {} was put back during cleanup in place of {}, but it is not the directory rollback made",
            OneLinePath(&beside(name)),
            OneLinePath(&level_location(dir))
        ),
        PutBack::Aside(name) => eprintln!(
            "warning: {} was recreated during cleanup; its contents are in {}",
            OneLinePath(&beside(&dir.name)),
            OneLinePath(&beside(name))
        ),
        // What stayed under the random name is no longer this level, so the
        // warning for a stranded capture would misplace its contents: it is
        // named as another directory.
        PutBack::Stranded if foreign => eprintln!(
            "warning: a directory that is not the one rollback made could not be put back and remains at {}",
            OneLinePath(&beside(aside))
        ),
        PutBack::Stranded => eprintln!(
            "warning: could not put {} back; its contents remain in {}",
            OneLinePath(&beside(&dir.name)),
            OneLinePath(&beside(aside))
        ),
        PutBack::InPlace => {}
    }
}

/// Moves `dir` to `aside` in `parent`, no-replace; `None` if it moved, else
/// the outcome: already gone is taken, and a failed move leaves it standing.
fn move_aside(parent: &OwnedFd, dir: &CreatedDirectory, aside: &OsString) -> Option<Outcome> {
    let Err(err) = rename_without_replacing(parent, &dir.name, parent, aside) else {
        return None;
    };
    // Already gone is already taken care of.
    let gone = matches!(
        rustix::fs::statat(parent, &dir.name, AtFlags::SYMLINK_NOFOLLOW),
        Err(err) if err == rustix::io::Errno::NOENT
    );
    if gone {
        warn_if_moved_away(dir);
        return Some(Outcome::Taken);
    }
    warn_left_standing(
        dir,
        format_args!(
            "no cleanup directory could be made, and it could not be moved aside for removal: {}",
            std::io::Error::from(err)
        ),
    );
    Some(Outcome::Stopped)
}

/// Moves what sits at `aside` in `parent` back under `dir`'s name, or under
/// the first free `<name>.restored-<n>`; every move is no-replace.
fn put_back_aside(parent: &OwnedFd, dir: &CreatedDirectory, aside: &OsString) -> PutBack {
    if rename_without_replacing(parent, aside, parent, &dir.name).is_ok() {
        return PutBack::InPlace;
    }
    for n in 0..16 {
        let mut restored = dir.name.clone();
        restored.push(format!(".restored-{n}"));
        if rename_without_replacing(parent, aside, parent, &restored).is_ok() {
            return PutBack::Aside(restored);
        }
    }
    let vacant = matches!(
        rustix::fs::statat(parent, aside, AtFlags::SYMLINK_NOFOLLOW),
        Err(err) if err == rustix::io::Errno::NOENT
    );
    if vacant {
        PutBack::Gone
    } else {
        PutBack::Stranded
    }
}
