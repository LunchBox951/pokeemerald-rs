//! The Unix take-back protocol: how [`super::undo_created_directories`]
//! removes a created level without trusting its mutable name.
//!
//! A level whose name still resolves to the identity captured at creation
//! and that holds no entries is renamed (no-replace) into a quarantine, unless
//! it lacks the owner's write bit, which moving a directory to another parent
//! needs: such a level is unlinked by name in its pinned parent after the same
//! identity check (see [`restore`]), as is one whose owner cannot list it nor
//! be granted the bits to (see [`occupied`]). The quarantine is one private directory per
//! rollback, made beside the outermost level rollback may take back, so it
//! is never inside a level it empties. A level on another filesystem than
//! that quarantine (`EXDEV`, as behind a `..` into a mount) gets a
//! quarantine of its own beside it, removed before the walk goes on; the
//! shared one is removed and its descriptor released first, and a level
//! further out claims a fresh one. In a
//! quarantine the level is re-checked by identity and unlinked through the
//! held quarantine, or put back (no-replace) under its name, or beside it
//! as `<name>.restored-<n>`, if it is not ours or holds something. A name
//! too long for that suffix is cut short first, so the recovery name fits
//! the 255-byte component limit. Every step that leaves a level standing
//! says so on stderr.
//!
//! This module walks the levels; the steps live in its children:
//!
//! - [`quarantine`] makes, vouches for and removes the quarantine.
//! - [`occupied`] lists a held level, so one that holds entries is not moved.
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
//! Rollback is also not crash-safe. A level that holds entries when
//! [`occupied`] lists it is left where it is, never moved; but a process
//! killed after a level was renamed into its quarantine and before it was
//! removed or put back leaves it at `.undo-<hex>/level` with no warning, and
//! no later run looks for it. That capture holds data only if another writer
//! acted from the listing until that kill: filling the level before the
//! rename, filling it after the rename through a descriptor or working
//! directory it already held inside it, or swapping its own directory in
//! under the name, which the rename then captures in the level's place.

use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rom_import::OneLinePath;
use rustix::fs::AtFlags;

use super::{same_directory_identity, CreatedDirectory};

#[cfg(test)]
mod hooks;
mod occupied;
mod quarantine;
mod report;
mod restore;

#[cfg(test)]
pub(super) use hooks::{
    take_back_level_capture_check_failing, take_back_level_held_stat_failing,
    take_back_level_looking_up, take_back_level_revalidation_failing, take_back_level_via,
    take_back_level_vouched_by, take_back_level_with_hooks, warn_if_moved_away_held_stat_failing,
};
#[cfg(all(test, target_os = "linux"))]
pub(super) use occupied::SetHeldMode;
use occupied::{list_level, ModeLeft};
use quarantine::{claim_quarantine, open_quarantine, remove_quarantine};
#[cfg(test)]
pub(super) use quarantine::{unguessable_quarantine_name, QUARANTINE_ENTRY};
use report::{level_location, standing, warn_if_moved_away, warn_left_standing, Standing};
#[cfg(test)]
pub(super) use report::{located_beside, verified_location};
#[cfg(test)]
pub(super) use restore::restore_capture;
use restore::{
    holds_this_level, is_this_level, take_back_into, take_back_unwritable_in_place, unlink_in_place,
};

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
    undo_created_directories_setting_modes(
        created,
        before_take,
        in_quarantine,
        &super::super::dest::set_held_directory_mode,
    );
}

/// [`super::undo_created_directories`] with each held level's mode set
/// through `set_mode`, so a test can make putting a mode back fail.
#[cfg(all(test, target_os = "linux"))]
pub(super) fn undo_created_directories_setting_modes_with(
    created: &[CreatedDirectory],
    set_mode: &SetHeldMode<'_>,
) {
    undo_created_directories_setting_modes(created, &mut || {}, &mut |_, _| {}, set_mode);
}

/// [`undo_created_directories_with_capture_hooks`] with each held level's
/// mode set through `set_mode`.
fn undo_created_directories_setting_modes(
    created: &[CreatedDirectory],
    before_take: &mut dyn FnMut(),
    in_quarantine: &mut dyn FnMut(CaptureStage, BorrowedFd<'_>),
    set_mode: &occupied::SetHeldMode<'_>,
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
                // A level without owner write cannot be renamed into a
                // quarantine, so it is not given one: it is unlinked in its
                // parent, and needs no quarantine to be made first.
                match take_back_unwritable_in_place(parent, dir) {
                    Some(Outcome::Stopped) => break,
                    Some(_) => continue,
                    None => {}
                }
                // A level that holds entries is not moved at all: once in a
                // quarantine, a crash would leave them hidden there. The
                // in-place unlink above moves nothing and removes only an
                // empty level, so it needs no listing first.
                // A mode the listing could not put back is reported once
                // the level's take-back is done, if it still stands.
                let listing = list_level(dir, set_mode);
                let went_on =
                    match take_back_unless_listed_empty(parent, dir, listing.holds_entries) {
                        Some(outcome) => outcome != Outcome::Stopped,
                        None => take_back_through_quarantine(
                            &mut quarantine,
                            created,
                            parent,
                            dir,
                            in_quarantine,
                        ),
                    };
                warn_mode_left(parent, dir, listing.mode_left);
                if !went_on {
                    break;
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

/// Takes the empty, listed `dir` back through the shared `quarantine`,
/// claiming it first if there is none yet, or through one of its own beside
/// it when it is on another filesystem. Whether the walk goes on.
fn take_back_through_quarantine<'a>(
    quarantine: &mut Option<quarantine::Quarantine<'a>>,
    created: &'a [CreatedDirectory],
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    in_quarantine: &mut dyn FnMut(CaptureStage, BorrowedFd<'_>),
) -> bool {
    if quarantine.is_none() {
        *quarantine = claim_quarantine(created, dir);
    }
    let Some(held) = quarantine.as_ref() else {
        return false;
    };
    match take_back_into(held, parent, dir, in_quarantine) {
        Outcome::Taken => true,
        Outcome::Stopped => false,
        Outcome::CrossDevice => {
            // The shared quarantine's descriptor is released first, so the
            // fallback can open what it needs even when that descriptor took
            // the last free slot; a level further out claims a fresh one.
            if let Some(shared) = quarantine.take() {
                remove_quarantine(&shared, &mut || {});
            }
            take_back_beside(parent, dir)
        }
    }
}

/// Reports that `dir`'s mode, changed so its owner could list it, could not
/// be put back, if the level still stands; one that was removed needs no
/// report. It is named where it stands now when that is known, else under
/// its name while that name still holds it.
fn warn_mode_left(parent: &OwnedFd, dir: &CreatedDirectory, mode_left: Option<ModeLeft>) {
    let Some(ModeLeft { found, error }) = mode_left else {
        return;
    };
    let at = match standing(dir) {
        Standing::Gone => return,
        Standing::Linked(Some(path)) => path,
        _ => match holds_this_level(parent.as_fd(), &dir.name, dir) {
            Ok(false) => return,
            _ => level_location(dir),
        },
    };
    eprintln!(
        "warning: {} keeps the owner read and search bits rollback granted to list it; its mode could not be put back to 0o{:03o}: {error}",
        OneLinePath(&at),
        found.bits()
    );
}

/// `None` when `dir` lists empty, so it goes through the quarantine.
/// Otherwise it is not moved: one that holds entries, or whose listing
/// fails, is reported and left where it stands, except that one its owner is
/// refused listing (`EACCES`, with the read and search bits not grantable
/// through the held descriptor) is unlinked in place, which moves nothing
/// and removes only an empty level. That keeps the take-back of an empty
/// level whose owner-access restore failed, as before the listing.
fn take_back_unless_listed_empty(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    holds_entries: rustix::io::Result<bool>,
) -> Option<Outcome> {
    match holds_entries {
        Ok(false) => None,
        Ok(true) => {
            leave_unmoved(
                parent,
                dir,
                "it holds entries",
                "it holds entries, so rollback did not move it into a cleanup directory",
            );
            Some(Outcome::Stopped)
        }
        Err(err) if err == rustix::io::Errno::ACCESS => Some(unlink_in_place(parent, dir)),
        Err(err) => {
            let unlisted = format!("it could not be listed: {}", std::io::Error::from(err));
            leave_unmoved(parent, dir, &unlisted, &unlisted);
            Some(Outcome::Stopped)
        }
    }
}

/// Reports that `dir` is not moved because of `state` (it holds entries, or
/// could not be listed). The check read the held original, so if another
/// writer moved it since the name was validated, it is reported where it
/// stands now, not under its old name, and a name left vacant is said to be
/// vacant rather than taken. `in_place` is the reason given when the
/// original is still at its name.
fn leave_unmoved(parent: &OwnedFd, dir: &CreatedDirectory, state: &str, in_place: &str) {
    let looked_up = rustix::fs::statat(parent, &dir.name, AtFlags::SYMLINK_NOFOLLOW)
        .and_then(|stat| is_this_level(dir, &stat));
    match looked_up {
        Ok(true) => warn_left_standing(
            dir,
            format_args!("{in_place}; the levels above it are left too"),
        ),
        Err(err) if err == rustix::io::Errno::NOENT => {
            eprintln!(
                "warning: {} is no longer under its name, and the directory rollback made for it was not taken back ({state}); the levels above it are left too",
                OneLinePath(&level_location(dir))
            );
            warn_if_moved_away(dir);
        }
        Ok(false) => {
            warn_left_standing(
                dir,
                format_args!(
                    "{state} and another entry has taken its name; the levels above it are left too"
                ),
            );
            warn_if_moved_away(dir);
        }
        Err(err) => warn_left_standing(
            dir,
            format_args!(
                "{state} and could not be examined under its name: {}; the levels above it are left too",
                std::io::Error::from(err)
            ),
        ),
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
