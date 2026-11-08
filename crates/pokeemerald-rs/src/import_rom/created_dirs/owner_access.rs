//! Restoring the owner's access to a level the umask closed to its owner,
//! when the held descriptor alone cannot do it.
//!
//! The held-descriptor setter in `dest` covers a level that is pinned and
//! whose owner search bit survives (and, with procfs, one whose bit does
//! not). Two cases are left: a pin the umask made impossible to take --
//! Apple's `O_EXEC` open needs owner search, the read-mode open of other
//! Unix systems needs owner read -- and a Linux or Android level without
//! owner search on a system without procfs. Both are answered here through
//! the level's held parent and its basename, which are always reachable
//! because the parent's own access was restored a level earlier.
//!
//! A basename is not an identity: it can be swapped for another directory
//! or a symlink after `mkdirat`. So nothing here chmods by name unless the
//! name has just been shown, by a no-follow `statat`, to still be the
//! directory `mkdirat` made, and it is shown again right after. A real
//! readable descriptor is preferred where the mode left one: it is checked
//! by `fstat`, and `fchmod` on it cannot be redirected. The residual of the
//! named route is a swap that lands between the check and the `chmod`, which
//! gives the replacement the owner's bits (and, where `chmodat` cannot refuse
//! a symlink, its referent) before the second look rejects it; the second
//! look keeps the run from ever descending into it.

use std::ffi::OsStr;
use std::io;
use std::os::fd::OwnedFd;

use rustix::fs::{AtFlags, FileType, Mode, Stat};

use super::{dest, same_directory_identity};

/// Why [`restore_by_name`] did not restore a level.
#[derive(Debug)]
pub(super) enum Refused {
    /// The name no longer provably names the level `mkdirat` made, or could
    /// not be examined; the caller must not descend into it.
    Replaced(io::Error),
    /// The level is still itself but the restore did not take.
    Failed,
}

fn replaced() -> Refused {
    Refused::Replaced(io::Error::other(
        "directory level was replaced before its owner access could be restored",
    ))
}

/// Looks `name` up in `parent` without following a symlink and requires it
/// to be the directory `identity` -- and `pin`, when one is held. Any lookup
/// that fails is a refusal: what cannot be examined is not vouched for.
fn named_level(
    parent: &OwnedFd,
    name: &OsStr,
    identity: &Stat,
    pin: Option<&OwnedFd>,
) -> Result<Stat, Refused> {
    let found = rustix::fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|source| Refused::Replaced(source.into()))?;
    if FileType::from_raw_mode(found.st_mode) != FileType::Directory
        || !same_directory_identity(&found, identity)
    {
        return Err(replaced());
    }
    if let Some(pin) = pin {
        let held = rustix::fs::fstat(pin).map_err(|source| Refused::Replaced(source.into()))?;
        if !same_directory_identity(&held, identity) {
            return Err(replaced());
        }
    }
    Ok(found)
}

/// Gives the level `name` in `parent`, which `mkdirat` made with `identity`,
/// the owner's read, write and search, keeping every other mode bit.
///
/// `pin` is the level's own descriptor when one is held.
pub(super) fn restore_by_name(
    parent: &OwnedFd,
    name: &OsStr,
    identity: &Stat,
    pin: Option<&OwnedFd>,
) -> Result<(), Refused> {
    restore_by_name_with(
        parent,
        name,
        identity,
        pin,
        &mut dest::open_readable_created_at,
        &mut || {},
    )
}

/// [`restore_by_name`] with its two test seams: `open_readable` stands in
/// for the readable open (so the named route can be driven by a runner the
/// mode bits do not bind), and `between` runs after the first identity check
/// and before the named `chmod` -- the point a real swap would land.
pub(super) fn restore_by_name_with(
    parent: &OwnedFd,
    name: &OsStr,
    identity: &Stat,
    pin: Option<&OwnedFd>,
    open_readable: &mut dyn FnMut(&OwnedFd, &OsStr) -> io::Result<OwnedFd>,
    between: &mut dyn FnMut(),
) -> Result<(), Refused> {
    // A mode that still leaves owner read, as umask 0177 does, lets the
    // level be opened for real; once `fstat` says it is the right directory,
    // `fchmod` on that descriptor cannot be redirected by a later swap.
    if let Ok(real) = open_readable(parent, name) {
        let opened = rustix::fs::fstat(&real).map_err(|source| Refused::Replaced(source.into()))?;
        if !same_directory_identity(&opened, identity) {
            return Err(replaced());
        }
        if let Some(pin) = pin {
            named_level(parent, name, identity, Some(pin))?;
        }
        return dest::restore_real_directory_owner_access(&real).map_err(|_| Refused::Failed);
    }

    let before = named_level(parent, name, identity, pin)?;
    let mode = Mode::from_raw_mode(before.st_mode) | Mode::RWXU;
    between();
    let chmod = dest::chmod_named(parent, name, mode);
    // The second look comes first: a swap is reported as one even when the
    // `chmod` also failed on it.
    named_level(parent, name, identity, pin)?;
    chmod.map_err(|_| Refused::Failed)
}
