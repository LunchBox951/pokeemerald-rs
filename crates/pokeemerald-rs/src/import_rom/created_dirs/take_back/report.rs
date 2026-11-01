//! What rollback says on stderr: where a level or quarantine it left
//! standing is now, read through the held descriptors and `/proc/self/fd`
//! where that can say, and why it was left.

use std::ffi::OsStr;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};

use rom_import::OneLinePath;

use super::super::CreatedDirectory;
use super::quarantine::{Quarantine, QUARANTINE_ENTRY};
use super::restore::PutBack;

/// Where `entry` beside `dir` is, for a warning. On Linux and Android it is
/// read from the held `parent` through `/proc/self/fd`, since a pinned
/// ancestor may have been renamed after creation and rollback acts through
/// `parent`, not the path; elsewhere, or without `/proc`, it is the
/// recorded path's sibling, which a rename since creation would make stale.
pub(in super::super) fn located_beside(
    parent: impl AsFd,
    dir: &CreatedDirectory,
    entry: &OsStr,
) -> PathBuf {
    located_in(parent, entry).unwrap_or_else(|| dir.path.with_file_name(entry))
}

/// Where `entry` in the held directory `home` is now, read through
/// `/proc/self/fd` on Linux and Android; `None` elsewhere or without
/// `/proc`.
fn located_in(home: impl AsFd, entry: &OsStr) -> Option<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::fd::AsRawFd as _;
        let link = format!("/proc/self/fd/{}", home.as_fd().as_raw_fd());
        if let Ok(held) = std::fs::read_link(link) {
            if held.is_absolute() {
                return Some(held.join(entry));
            }
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let _ = (home, entry);
    None
}

/// Where the held directory `held` is now, read through `/proc/self/fd`
/// on Linux and Android; `None` elsewhere or without `/proc`.
fn held_location(held: impl AsFd) -> Option<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::fd::AsRawFd as _;
        let link = format!("/proc/self/fd/{}", held.as_fd().as_raw_fd());
        if let Ok(now) = std::fs::read_link(link) {
            if now.is_absolute() {
                return Some(now);
            }
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let _ = held;
    None
}

/// Where the quarantine is now, for a warning: its own held descriptor
/// first, since the quarantine itself may have been renamed; then `name`
/// in the held `home`, as [`located_beside`] reads a level's parent, since
/// `home` may have been renamed; then `recorded`, its location when it was
/// made, elsewhere or without `/proc`.
pub(super) fn quarantine_location(
    quarantine: Option<BorrowedFd<'_>>,
    home: BorrowedFd<'_>,
    name: &str,
    recorded: &Path,
) -> PathBuf {
    quarantine
        .and_then(held_location)
        .or_else(|| located_in(home, OsStr::new(name)))
        .unwrap_or_else(|| recorded.to_owned())
}

/// Where `dir` itself is, for a warning that it was left standing.
pub(super) fn level_location(dir: &CreatedDirectory) -> PathBuf {
    match dir.parent.as_deref() {
        Some(parent) => located_beside(parent, dir, &dir.name),
        None => dir.path.clone(),
    }
}

/// Reports that `dir` was left standing, and why.
pub(super) fn warn_left_standing(dir: &CreatedDirectory, cause: std::fmt::Arguments<'_>) {
    eprintln!(
        "warning: {} is left in place: {cause}",
        OneLinePath(&level_location(dir))
    );
}

/// Reports that what the cleanup put back for `dir` (as `put`, from the
/// held quarantine `held`) could not be re-checked to be the directory
/// rollback made, with the error `err`, so the walk stops there; a level
/// put back under its own name is reported with `not_removed`, why it was
/// not removed, as well.
pub(super) fn warn_put_back_unexamined(
    quarantine: &Quarantine<'_>,
    held: BorrowedFd<'_>,
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    put: &PutBack,
    not_removed: Option<rustix::io::Errno>,
    err: rustix::io::Errno,
) {
    let location = match put {
        PutBack::InPlace => {
            let removal = not_removed.map_or_else(String::new, |removal| {
                format!(
                    "it could not be removed: {}; ",
                    std::io::Error::from(removal)
                )
            });
            warn_left_standing(
                dir,
                format_args!(
                    "{removal}it was put back under its name, where it could not be examined: {}; the levels above it are left too",
                    std::io::Error::from(err)
                ),
            );
            return;
        }
        PutBack::Aside { name, .. } => located_beside(parent, dir, name),
        PutBack::Stranded(_) => quarantine_location(
            Some(held),
            quarantine.home,
            &quarantine.name,
            &quarantine.path,
        )
        .join(QUARANTINE_ENTRY),
        PutBack::Gone(_) => return,
    };
    eprintln!(
        "warning: {} was put back during cleanup but could not be examined there: {}; the levels above it are left too",
        OneLinePath(&location),
        std::io::Error::from(err)
    );
}

/// Reports that the directory rollback made for `dir`, no longer under its
/// name, was moved away rather than removed, naming where the held
/// original is now where `/proc` can say. Silent when it has no link left
/// (removed outright) or is no longer held open, so nothing can be said;
/// where its link count cannot tell moved from removed ([`Standing::Unknown`]),
/// the warning says it may have been moved.
pub(super) fn warn_if_moved_away(dir: &CreatedDirectory) {
    report_moved_away(dir, standing(dir));
}

/// [`warn_if_moved_away`] with `standing` as what the held original's
/// examination said, so a test can make it fail.
pub(super) fn report_moved_away(dir: &CreatedDirectory, standing: Standing) {
    let level = level_location(dir);
    match standing {
        Standing::Gone => {}
        Standing::Linked(Some(now)) => eprintln!(
            "warning: {} is the directory rollback made for {}, moved away before rollback could take it back; it is left in place",
            OneLinePath(&now),
            OneLinePath(&level)
        ),
        Standing::Linked(None) => eprintln!(
            "warning: {} was moved away before rollback could take it back; the directory rollback made is left elsewhere",
            OneLinePath(&level)
        ),
        Standing::Unknown => eprintln!(
            "warning: {} is no longer under its name; if it was moved rather than removed, the directory rollback made is left elsewhere",
            OneLinePath(&level)
        ),
        Standing::Unexamined(err) => eprintln!(
            "warning: {} is no longer under its name, and the directory rollback made for it could not be examined: {}; if it was moved rather than removed, it is left elsewhere",
            OneLinePath(&level),
            std::io::Error::from(err)
        ),
    }
}

/// What the held original's link count says about it.
pub(super) enum Standing {
    /// Removed outright (a link count of 0), or no longer held, so nothing
    /// can be said about it.
    Gone,
    /// Still linked somewhere; where, if `/proc` can say.
    Linked(Option<PathBuf>),
    /// Its link count does not tell: on Apple a directory removed while
    /// held open still reads a nonzero `st_nlink` (the
    /// `a_level_removed_and_replaced_after_validation_is_not_reported_as_moved`
    /// failure on macOS CI), so outside Linux and Android only a count of 0
    /// is trusted.
    Unknown,
    /// Its `fstat` failed: it may still be linked anywhere, so the error is
    /// reported rather than read as its removal.
    Unexamined(rustix::io::Errno),
}

/// [`Standing`] of the directory `dir` holds open.
pub(super) fn standing(dir: &CreatedDirectory) -> Standing {
    let Some(own) = dir.own.as_deref() else {
        return Standing::Gone;
    };
    held_standing(own.as_fd())
}

/// [`Standing`] of the held directory `held`.
fn held_standing(held: BorrowedFd<'_>) -> Standing {
    held_standing_as(rustix::fs::fstat(held), held)
}

/// [`held_standing`] with `examined` as what `held`'s own `fstat` gave.
/// Only a link count of 0 proves removal; a failed examination is
/// [`Standing::Unexamined`].
pub(super) fn held_standing_as(
    examined: rustix::io::Result<rustix::fs::Stat>,
    held: BorrowedFd<'_>,
) -> Standing {
    match examined {
        Ok(stat) if stat.st_nlink != 0 => {}
        Ok(_) => return Standing::Gone,
        Err(err) => return Standing::Unexamined(err),
    }
    if cfg!(any(target_os = "linux", target_os = "android")) {
        Standing::Linked(held_location(held))
    } else {
        Standing::Unknown
    }
}

/// Reports a quarantine whose name no longer resolves to it, so it was not
/// removed: whatever now holds the name is described as such, and the held
/// quarantine, the directory rollback actually leaves, is named where it is
/// now where `/proc` can say, as `standing` (the held quarantine's
/// examination) tells. Its name is all that is known when (outside Linux
/// and Android) its link count does not tell whether it was moved or
/// removed; when the held quarantine could not be examined, the error is
/// reported too.
pub(super) fn warn_quarantine_not_at_name(
    quarantine: &Quarantine<'_>,
    standing: Standing,
    by_name: &rustix::io::Result<rustix::fs::Stat>,
) {
    let name = quarantine.name_location();
    let vacant = matches!(by_name, Err(err) if *err == rustix::io::Errno::NOENT);
    if !vacant || matches!(standing, Standing::Unknown | Standing::Unexamined(_)) {
        eprintln!(
            "warning: cleanup directory {} no longer resolves to the one rollback made; it is left in place",
            OneLinePath(&name)
        );
    }
    match standing {
        Standing::Linked(Some(now)) => eprintln!(
            "warning: {} is the cleanup directory rollback made, moved away from {}; it is left in place",
            OneLinePath(&now),
            OneLinePath(&name)
        ),
        Standing::Linked(None) => eprintln!(
            "warning: cleanup directory {} was moved away before rollback could remove it; it is left elsewhere",
            OneLinePath(&name)
        ),
        Standing::Unexamined(err) => eprintln!(
            "warning: the cleanup directory rollback made for {} could not be examined: {}; if it was moved rather than removed, it is left elsewhere",
            OneLinePath(&name),
            std::io::Error::from(err)
        ),
        Standing::Gone | Standing::Unknown => {}
    }
}
