//! Restoring the owner's access to a level the umask closed to its owner,
//! when the held descriptor alone cannot do it.
//!
//! The held-descriptor setter in `dest` covers a level that is pinned and
//! whose owner search bit survives (and, with procfs, one whose bit does
//! not). Two cases are left: a pin the umask made impossible to take --
//! Apple's `O_EXEC` open needs owner search, the read-mode open of other
//! Unix systems needs owner read -- and a Linux or Android level without
//! owner search on a system without procfs.
//!
//! Both are answered through the level's held parent, and only through an
//! operation bound to the inode: the level is opened for reading by its
//! basename (refusing a final symlink), the open is `fstat`-checked against
//! the identity `mkdirat` gave the level, and `fchmod` acts on that
//! descriptor, which a later swap of the name cannot redirect. That needs the
//! owner's read bit. A mode without it (`0000` after umask `0777`; `0333` and
//! `0300` after `0444` and `0477` on Unix systems that pin with a read-mode
//! open) has no inode-bound route on these targets -- rustix 1.1 has no
//! `fchmodat2(AT_EMPTY_PATH)`, and `fchmod` refuses an `O_PATH` descriptor --
//! and a `chmod` by name can land on a directory or symlink swapped in after
//! the check, which no later check can undo. So such a level is not touched
//! and the create fails with [`unrestorable`].

use std::ffi::OsStr;
use std::io;
use std::os::fd::OwnedFd;

use super::{dest, same_directory_identity};

/// Why [`restore_by_name`] did not restore a level.
#[derive(Debug)]
pub(super) enum Refused {
    /// The name no longer provably names the level `mkdirat` made, or it
    /// could not be examined; the caller must not descend into it.
    Replaced(io::Error),
    /// No inode-bound route reaches the level, or it did not take.
    Failed,
}

/// The error a level the umask left unusable ends the create with.
pub(super) fn unrestorable() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "the umask took the owner's access to this directory level, and it cannot be restored \
         on this platform without the owner's read permission (or /proc on Linux and Android)",
    )
}

fn replaced() -> Refused {
    Refused::Replaced(io::Error::other(
        "directory level was replaced before its owner access could be restored",
    ))
}

/// Gives the level `name` in `parent`, which `mkdirat` made with `identity`,
/// the owner's read, write and search, keeping every other mode bit.
///
/// `pin` is the level's own descriptor when one is held.
pub(super) fn restore_by_name(
    parent: &OwnedFd,
    name: &OsStr,
    identity: &rustix::fs::Stat,
    pin: Option<&OwnedFd>,
) -> Result<(), Refused> {
    restore_by_name_with(
        parent,
        name,
        identity,
        pin,
        &mut dest::open_readable_created_at,
    )
}

/// [`restore_by_name`] with `open_readable` as its test seam, standing in
/// for the readable open so a runner the mode bits do not bind can drive the
/// refusal.
pub(super) fn restore_by_name_with(
    parent: &OwnedFd,
    name: &OsStr,
    identity: &rustix::fs::Stat,
    pin: Option<&OwnedFd>,
    open_readable: &mut dyn FnMut(&OwnedFd, &OsStr) -> io::Result<OwnedFd>,
) -> Result<(), Refused> {
    let Ok(real) = open_readable(parent, name) else {
        return Err(Refused::Failed);
    };
    let opened = rustix::fs::fstat(&real).map_err(|source| Refused::Replaced(source.into()))?;
    if !same_directory_identity(&opened, identity) {
        return Err(replaced());
    }
    if let Some(pin) = pin {
        let held = rustix::fs::fstat(pin).map_err(|source| Refused::Replaced(source.into()))?;
        if !same_directory_identity(&held, identity) {
            return Err(replaced());
        }
    }
    dest::restore_real_directory_owner_access(&real).map_err(|_| Refused::Failed)
}
