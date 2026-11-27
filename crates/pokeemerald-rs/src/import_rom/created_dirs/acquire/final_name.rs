//! Making a level at its final name, where no no-replace rename can claim it
//! (Unix targets without one, and filesystems that refuse it): the level is
//! never claimed, but its entry and its mode are still this run's to finish.

use std::ffi::OsStr;
use std::io;

use super::super::{
    dest, is_access_denied, is_out_of_descriptors_io, same_directory_identity,
    sync_and_shed_outermost_pinned_level, sync_new_entry, CreatedDirectory,
};
#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
use super::Claimed;
use super::{made_by, mkdir_level, restore_through_parent, ClaimError};

/// [`claim_level`](super::claim_level) where there is no no-replace rename:
/// the directory is made at `name` itself and never claimed.
#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
pub(in super::super) fn claim_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    owner: u32,
    after_mkdir: &mut dyn FnMut(),
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> Result<Claimed, ClaimError> {
    Err(claim_at_final_name(
        created,
        pin_from,
        parent,
        name,
        owner,
        after_mkdir,
        sync_reopen,
    ))
}

/// Makes the level `name` in `parent` at that name, and answers why it is
/// not claimed. `mkdirat` returns no descriptor, so a same-user writer of
/// `parent` can move the new directory away and put its own at `name` before
/// anything looks there, and no check made afterwards can tell the two
/// apart. The level is therefore [`ClaimError::Unowned`]: walked through like
/// one that already existed, never recorded, never removed on rollback.
///
/// The entry and the mode are still this run's `mkdirat`'s, so two things a
/// claimed level gets from the walk are done here: `parent` is synced, since
/// no later pass syncs an unrecorded level's entry, and the owner's access
/// the umask took is restored, since the next level is made in it. A failed
/// restore is [`ClaimError::Failed`].
pub(super) fn claim_at_final_name(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    owner: u32,
    after_mkdir: &mut dyn FnMut(),
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> ClaimError {
    if let Err(errno) = mkdir_level(created, pin_from, parent, name, sync_reopen) {
        return ClaimError::from_errno(errno);
    }
    after_mkdir();
    // Best effort, like a claimed level's sync.
    sync_new_entry(created, pin_from, parent, sync_reopen);
    if let Err(source) =
        restore_unclaimed_owner_access(created, pin_from, parent, name, owner, sync_reopen)
    {
        return ClaimError::Failed(source);
    }
    ClaimError::Unowned(io::Error::other(
        "directory level made at its final name is no longer a directory",
    ))
}

/// Gives the owner of the directory `name` in `parent`, just made there and
/// not claimed, the access the umask took, by the routes a claimed level's
/// restore takes: through a descriptor held on it, or, where the umask took
/// the bit that descriptor needs or the held route is refused, through
/// [`restore_through_parent`]. What stands at `name` may be a same-user
/// replacement, so only a directory `owner` owns, found without following a
/// symlink, is changed, and it gains only its own owner's bits; anything
/// else is left to the walk, which refuses what is not a directory.
fn restore_unclaimed_owner_access(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    owner: u32,
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> io::Result<()> {
    let Ok(found) = rustix::fs::statat(&**parent, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
    else {
        return Ok(());
    };
    if !made_by(&found, owner)
        || rustix::fs::Mode::from_raw_mode(found.st_mode).contains(rustix::fs::Mode::RWXU)
    {
        return Ok(());
    }
    let pin = loop {
        match dest::open_created_directory_at(parent, name) {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && sync_and_shed_outermost_pinned_level(created, pin_from, sync_reopen) => {}
            result => break result.ok(),
        }
    }
    .filter(|pin| rustix::fs::fstat(pin).is_ok_and(|held| same_directory_identity(&held, &found)));
    if let Some(pin) = &pin {
        match dest::restore_directory_owner_access(pin) {
            Err(source) if is_access_denied(&source) => {}
            result => return result,
        }
    }
    restore_through_parent(
        created,
        pin_from,
        parent,
        name,
        &found,
        pin.as_ref(),
        sync_reopen,
    )
}
