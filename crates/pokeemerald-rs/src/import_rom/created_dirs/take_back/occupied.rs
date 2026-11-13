//! Whether a level rollback holds open has anything in it, read through that
//! held descriptor, so a level that gained entries is left where it is
//! instead of being moved into a quarantine.
//!
//! A level without the owner's read or search bit (a umask that took it,
//! with the restore of owner access after creation failing) cannot be listed
//! as it stands, yet it may keep the write bit a rename into the quarantine
//! needs, and an empty one must still be taken back. The missing bits are
//! granted through the held descriptor for the listing alone, as creation
//! restores owner access through it, and the level's mode is put back as it
//! was found afterwards. Where they cannot be granted, the listing answers
//! `EACCES` and the caller unlinks the level in place, which moves nothing
//! and removes only an empty level.
//!
//! # Accepted residuals
//!
//! - A writer that fills the level after this listing still gets it moved:
//!   before the rename, or after it through a descriptor or working
//!   directory already inside the level. A crash before the capture is
//!   removed or put back leaves it hidden at `.undo-<hex>/level`; the
//!   capture is re-checked and put back as usual when the process lives.
//! - A listing that fails other than with `EACCES`, for want of a free
//!   descriptor or otherwise, is no verdict that the level is empty: it is
//!   reported and leaves the level standing, unmoved.
//! - A level whose bits were granted and whose original mode then cannot be
//!   put back keeps the owner's read and search bits. Nothing in it is lost
//!   or moved; its owner can only list it, as creation meant it to.

use std::os::fd::{AsFd, OwnedFd};

use rustix::fs::{Dir, Mode, OFlags};

use super::super::super::dest::set_held_directory_mode;
use super::super::CreatedDirectory;

/// Whether `dir`'s held original lists an entry other than `.` and `..`.
/// `Ok(false)` for a level that is not held open; a failure to list is the
/// error, not a verdict that the level is empty. A level its owner cannot
/// list is listed with the owner's read and search bits granted for the
/// listing alone; `EACCES` means they were already there or could not be
/// granted.
pub(super) fn level_holds_entries(dir: &CreatedDirectory) -> rustix::io::Result<bool> {
    let Some(own) = dir.own.as_deref() else {
        return Ok(false);
    };
    match list_held(own) {
        Err(err) if err == rustix::io::Errno::ACCESS => list_with_owner_read(own, err),
        listed => listed,
    }
}

/// Lists `own` with the owner's read and search bits granted through it,
/// then puts its mode back. `denied` is the listing's own error, returned
/// when the level already had both bits or they could not be granted.
fn list_with_owner_read(own: &OwnedFd, denied: rustix::io::Errno) -> rustix::io::Result<bool> {
    let found = Mode::from_raw_mode(rustix::fs::fstat(own)?.st_mode) & Mode::all();
    let listable = Mode::RUSR | Mode::XUSR;
    if found.contains(listable) || set_held_directory_mode(own, found | listable).is_err() {
        return Err(denied);
    }
    let listed = list_held(own);
    // A failure here is an accepted residual (module doc): the level only
    // keeps the bits its owner was meant to have.
    let _ = set_held_directory_mode(own, found);
    listed
}

/// Whether `own` lists an entry other than `.` and `..`.
fn list_held(own: &OwnedFd) -> rustix::io::Result<bool> {
    // `own` is opened for lookups only, so the listing reopens it for reading
    // through itself: it is the level, not whatever sits at its name.
    let opened = rustix::fs::openat(
        own.as_fd(),
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .and_then(Dir::new);
    for entry in opened? {
        if !matches!(entry?.file_name().to_bytes(), b"." | b"..") {
            return Ok(true);
        }
    }
    Ok(false)
}
