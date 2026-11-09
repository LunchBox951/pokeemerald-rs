//! Restoring the owner's access to a level the umask closed to its owner,
//! when the held descriptor alone cannot do it: the by-name route through a
//! real read descriptor, `fstat`-checked against the identity `mkdirat` gave
//! the level, and `fchmod`ed there so a later swap of the name cannot
//! redirect it. Nothing is ever `chmod`ed by name; a level this route cannot
//! reach fails the create with [`unrestorable`].
//!
//! Which umasks and targets each route covers, and which are refused, is
//! the contract in the [`created_dirs`](super) module docs.

use std::ffi::OsStr;
use std::io;
use std::os::fd::OwnedFd;

use super::{dest, is_out_of_descriptors_io, same_directory_identity};

/// Why [`restore_by_name`] did not restore a level.
#[derive(Debug)]
pub(super) enum Refused {
    /// The name no longer provably names the level `mkdirat` made, or it
    /// could not be examined; the caller must not descend into it.
    Replaced(io::Error),
    /// The readable open found the descriptor table full (`EMFILE` or
    /// `ENFILE`); the caller sheds a pin and tries again.
    OutOfDescriptors(io::Error),
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
    let real = match open_readable(parent, name) {
        Ok(real) => real,
        Err(source) if is_out_of_descriptors_io(&source) => {
            return Err(Refused::OutOfDescriptors(source));
        }
        Err(_) => return Err(Refused::Failed),
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
