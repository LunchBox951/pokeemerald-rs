//! Test seams over the take-back of one level, each with the quarantine or
//! the examination it acts through replaced.

use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rustix::fs::Dir;

use super::super::CreatedDirectory;
use super::quarantine::{
    open_quarantine, open_quarantine_vouched_by, remove_quarantine, remove_quarantine_held_as,
};
use super::report::{held_standing_as, report_moved_away};
use super::restore::{holds_this_level, take_back_into, take_back_into_revalidating};
use super::{CaptureStage, Outcome};

#[cfg(test)]
/// [`undo_created_directories_with_hooks`]'s take-back of one level, with
/// the quarantine made beside it under `quarantine_name`, so a test can
/// stage something at the name before rollback tries to claim it, and
/// `after_mkdir` run between making the quarantine and opening it, where a
/// test swaps the name. Returns whether the level was taken back.
pub(in super::super) fn take_back_level_via(
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
pub(in super::super) fn take_back_level_vouched_by(
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
pub(in super::super) fn take_back_level_looking_up(
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
pub(in super::super) fn take_back_level_held_stat_failing(
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
pub(in super::super) fn take_back_level_revalidation_failing(
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
    let taken = take_back_into_revalidating(
        &quarantine,
        parent,
        dir,
        in_quarantine,
        &|own| rustix::fs::fstat(own),
        &|_, _, _| Err(err),
    ) == Outcome::Taken;
    remove_quarantine(&quarantine, &mut || {});
    taken
}

#[cfg(test)]
/// [`take_back_level_via`] with the captured level's held original
/// examined as failing with `err` when the capture is checked, while every
/// later examination is real.
pub(in super::super) fn take_back_level_capture_check_failing(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    quarantine_name: &str,
    err: rustix::io::Errno,
) -> bool {
    let Some(quarantine) = open_quarantine(parent.as_fd(), dir, quarantine_name, dir, &mut || {})
    else {
        return false;
    };
    let taken = take_back_into_revalidating(
        &quarantine,
        parent,
        dir,
        &mut |_, _| {},
        &|_| Err(err),
        &holds_this_level,
    ) == Outcome::Taken;
    remove_quarantine(&quarantine, &mut || {});
    taken
}

#[cfg(test)]
/// Reports `dir` as no longer under its name with its held original's
/// examination failing with `err`, as [`warn_if_moved_away`] would.
pub(in super::super) fn warn_if_moved_away_held_stat_failing(
    dir: &CreatedDirectory,
    err: rustix::io::Errno,
) {
    let own = dir.own.as_deref().expect("the level is held open");
    report_moved_away(dir, held_standing_as(Err(err), own.as_fd()));
}

#[cfg(test)]
/// [`take_back_level_via`] with `before_removal` run just before the
/// quarantine's name is re-checked for its removal, where a test swaps it.
pub(in super::super) fn take_back_level_with_hooks(
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
