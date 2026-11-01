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
//! This module walks the levels; the steps live in its children:
//!
//! - [`quarantine`] makes, vouches for and removes the quarantine.
//! - [`restore`] captures a level into it, re-checks the capture, and unlinks
//!   or puts it back.
//! - [`report`] says on stderr where a level or quarantine was left.
//!
//! # Accepted residuals
//!
//! The approved `rustix` fs scope has no call that unlinks a directory by
//! descriptor. The quarantine is a directory this rollback just made, owned
//! by the current user with no group or other bits, under a random 64-bit
//! name from `/dev/urandom`, so only a writer running as that user can act
//! inside it, and such a writer can already delete or rewrite everything
//! the import owns. The residuals of each step are documented with it, in
//! [`restore`] and [`quarantine`].
//!
//! Rollback is also not crash-safe: a process killed after a level was
//! renamed into its quarantine and before it was removed or put back leaves
//! it at `.undo-<hex>/level` with no warning, and no later run looks for
//! it. That level holds data only if another writer filled it, or swapped
//! in its own directory, during the import.

use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rom_import::OneLinePath;
use rustix::fs::AtFlags;

use super::{same_directory_identity, CreatedDirectory};

#[cfg(test)]
mod hooks;
mod quarantine;
mod report;
mod restore;

#[cfg(test)]
pub(super) use hooks::{
    take_back_level_capture_check_failing, take_back_level_held_stat_failing,
    take_back_level_looking_up, take_back_level_revalidation_failing, take_back_level_via,
    take_back_level_vouched_by, take_back_level_with_hooks, warn_if_moved_away_held_stat_failing,
};
use quarantine::{claim_quarantine, open_quarantine, remove_quarantine};
#[cfg(test)]
pub(super) use quarantine::{unguessable_quarantine_name, QUARANTINE_ENTRY};
#[cfg(test)]
pub(super) use report::located_beside;
use report::{warn_if_moved_away, warn_left_standing};
#[cfg(test)]
pub(super) use restore::restore_capture;
use restore::{is_this_level, take_back_into};

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
        // A held original whose `fstat` fails is the lookup's error, not a
        // verdict on whose directory sits at the name.
        let looked_up = rustix::fs::statat(parent, &dir.name, AtFlags::SYMLINK_NOFOLLOW)
            .and_then(|stat| Ok((stat, is_this_level(dir, &stat)?)));
        match looked_up {
            Ok((_, true)) => {
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
            Ok((stat, false)) => {
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
    let Some(name) = quarantine::unguessable_quarantine_name() else {
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
