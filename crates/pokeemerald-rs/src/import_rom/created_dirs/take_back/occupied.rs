//! Whether a level rollback holds open has anything in it, read through that
//! held descriptor, so a level that gained entries is left where it is
//! instead of being moved into a quarantine.
//!
//! # Accepted residuals
//!
//! - A writer that fills the level after this listing still gets it moved:
//!   before the rename, or after it through a descriptor or working
//!   directory already inside the level. A crash before the capture is
//!   removed or put back leaves it hidden at `.undo-<hex>/level`; the
//!   capture is re-checked and put back as usual when the process lives.
//! - A listing that fails, for want of a free descriptor or otherwise, is no
//!   verdict that the level is empty: it is reported and leaves the level
//!   standing, unmoved.

use std::os::fd::AsFd;

use rustix::fs::{Dir, Mode, OFlags};

use super::super::CreatedDirectory;

/// Whether `dir`'s held original lists an entry other than `.` and `..`.
/// `Ok(false)` for a level that is not held open; a failure to list is the
/// error, not a verdict that the level is empty.
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
    for entry in opened? {
        if !matches!(entry?.file_name().to_bytes(), b"." | b"..") {
            return Ok(true);
        }
    }
    Ok(false)
}
