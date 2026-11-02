//! Whether a level rollback holds open has anything in it, read through that
//! held descriptor, so a level that gained entries is left where it is
//! instead of being moved into a quarantine.
//!
//! # Accepted residuals
//!
//! - A writer that fills the level after this listing and before the rename
//!   still gets it moved, and a crash in that window leaves it hidden at
//!   `.undo-<hex>/level`; the capture is then re-checked and put back as
//!   usual when the process lives.
//! - A listing that cannot run for want of a free descriptor is no verdict
//!   either way, and does not stop the rollback: the level is taken back as
//!   before this check, whose protocol keeps whatever it holds. Any other
//!   failure to list is reported and leaves the level standing.

use std::os::fd::AsFd;

use rustix::fs::{Dir, Mode, OFlags};
use rustix::io::Errno;

use super::super::CreatedDirectory;

/// Whether `dir`'s held original lists an entry other than `.` and `..`.
/// `Ok(false)` for a level that is not held open or that cannot be listed
/// for want of a free descriptor; any other failure to list is the error,
/// not a verdict that the level is empty.
pub(super) fn level_holds_entries(dir: &CreatedDirectory) -> rustix::io::Result<bool> {
    let Some(own) = dir.own.as_deref() else {
        return Ok(false);
    };
    // `own` is opened for lookups only, so the listing reopens it for reading
    // through itself: it is the level, not whatever sits at its name.
    let opened = rustix::fs::openat(
        own.as_fd(),
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .and_then(Dir::new);
    let mut listing = match opened {
        Ok(listing) => listing,
        Err(Errno::MFILE | Errno::NFILE) => return Ok(false),
        Err(err) => return Err(err),
    };
    for entry in &mut listing {
        if !matches!(entry?.file_name().to_bytes(), b"." | b"..") {
            return Ok(true);
        }
    }
    Ok(false)
}
