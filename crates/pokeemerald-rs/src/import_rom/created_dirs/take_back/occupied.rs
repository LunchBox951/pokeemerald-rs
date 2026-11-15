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
//!
//! A mode that cannot be put back after the listing is no accepted residual:
//! the listing hands it to the caller as [`ModeLeft`], which reports it for a
//! level still standing once its take-back is done (see
//! [`super::warn_mode_left`]). A level that was removed needs no report.

use std::os::fd::{AsFd, OwnedFd};

use rustix::fs::{Dir, Mode, OFlags};

use super::super::CreatedDirectory;

/// How a held level's mode is set: `dest::set_held_directory_mode`, or a
/// test's stand-in that fails.
pub(in super::super) type SetHeldMode<'s> = dyn Fn(&OwnedFd, Mode) -> std::io::Result<()> + 's;

/// A level whose owner was granted the bits to list it, and whose mode
/// could not then be put back as it was found.
#[derive(Debug)]
pub(super) struct ModeLeft {
    /// The mode the level was found at, and is not back at.
    pub(super) found: Mode,
    /// Why putting it back failed.
    pub(super) error: std::io::Error,
}

/// What listing a held level found.
pub(super) struct Listing {
    /// Whether it lists an entry other than `.` and `..`; a failure to list
    /// is the error, not a verdict that the level is empty.
    pub(super) holds_entries: rustix::io::Result<bool>,
    /// Set when the mode the listing changed could not be put back.
    pub(super) mode_left: Option<ModeLeft>,
}

/// Lists `dir`'s held original, with its mode set through `set_mode`.
/// A level that is not held open lists empty. A level its owner cannot list
/// is listed with the owner's read and search bits granted for the listing
/// alone; `EACCES` means they were already there or could not be granted.
pub(super) fn list_level(dir: &CreatedDirectory, set_mode: &SetHeldMode<'_>) -> Listing {
    let Some(own) = dir.own.as_deref() else {
        return Listing {
            holds_entries: Ok(false),
            mode_left: None,
        };
    };
    match list_held(own) {
        Err(err) if err == rustix::io::Errno::ACCESS => list_with_owner_read(own, err, set_mode),
        listed => Listing {
            holds_entries: listed,
            mode_left: None,
        },
    }
}

/// Lists `own` with the owner's read and search bits granted through it,
/// then puts its mode back, saying so if that fails. `denied` is the
/// listing's own error, returned when the level already had both bits or
/// they could not be granted.
fn list_with_owner_read(
    own: &OwnedFd,
    denied: rustix::io::Errno,
    set_mode: &SetHeldMode<'_>,
) -> Listing {
    let unchanged = |holds_entries| Listing {
        holds_entries,
        mode_left: None,
    };
    let found = match rustix::fs::fstat(own) {
        Ok(stat) => Mode::from_raw_mode(stat.st_mode) & Mode::all(),
        Err(err) => return unchanged(Err(err)),
    };
    let listable = Mode::RUSR | Mode::XUSR;
    if found.contains(listable) || set_mode(own, found | listable).is_err() {
        return unchanged(Err(denied));
    }
    Listing {
        holds_entries: list_held(own),
        mode_left: set_mode(own, found)
            .err()
            .map(|error| ModeLeft { found, error }),
    }
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
