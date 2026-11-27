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
/// `mkdirat` cannot have made a symlink or a file, so a name that no longer
/// holds a directory, a symlink included, is [`ClaimError::Failed`] rather
/// than followed. The entry and the mode are still this run's `mkdirat`'s, so
/// two things a claimed level gets from the walk are done here: `parent` is
/// synced, since no later pass syncs an unrecorded level's entry, and the
/// owner's access the umask took is restored, since the next level is made in
/// it.
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
    match open_unclaimed_level(created, pin_from, parent, name, owner, sync_reopen) {
        Ok(level) => ClaimError::Unowned(level),
        Err(source) => ClaimError::Failed(source),
    }
}

/// Opens the directory `name` in `parent`, just made there and not claimed,
/// without following a symlink, and gives its owner the access the umask
/// took, by the routes a claimed level's restore takes: through the
/// descriptor, or, where the umask took the bit that descriptor needs or the
/// held route is refused, through [`restore_through_parent`]. What stands at
/// `name` may be a same-user replacement, so only a directory `owner` owns is
/// changed, and it gains only its own owner's bits; one of another account
/// is walked through as it is.
fn open_unclaimed_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    owner: u32,
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> io::Result<std::os::fd::OwnedFd> {
    let found = rustix::fs::statat(&**parent, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)?;
    if rustix::fs::FileType::from_raw_mode(found.st_mode) != rustix::fs::FileType::Directory {
        return Err(io::Error::other(
            "directory level made at its final name is no longer a directory",
        ));
    }
    let ours = made_by(&found, owner);
    let mut restored = false;
    let level = loop {
        match dest::open_created_directory_at(parent, name) {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && sync_and_shed_outermost_pinned_level(created, pin_from, sync_reopen) => {}
            // The umask took the bit the open needs (owner search, on Apple).
            Err(source) if ours && !restored && is_access_denied(&source) => {
                restore_through_parent(created, pin_from, parent, name, &found, None, sync_reopen)?;
                restored = true;
            }
            result => break result?,
        }
    };
    if !rustix::fs::fstat(&level).is_ok_and(|held| same_directory_identity(&held, &found)) {
        return Err(io::Error::other(
            "directory level made at its final name was replaced before it could be opened",
        ));
    }
    if ours && !restored {
        match dest::restore_directory_owner_access(&level) {
            Err(source) if is_access_denied(&source) => restore_through_parent(
                created,
                pin_from,
                parent,
                name,
                &found,
                Some(&level),
                sync_reopen,
            )?,
            result => result?,
        }
    }
    Ok(level)
}
