//! The Unix take-back protocol: how [`super::undo_created_directories`]
//! removes a created level without trusting its mutable name.
//!
//! A level whose name still resolves to the identity captured at creation
//! is renamed (no-replace) into a quarantine: one private directory per
//! rollback, made beside the outermost level rollback may take back, so it
//! is never inside a level it empties. There the level is re-checked by
//! identity and unlinked through the held quarantine, or put back (no-
//! replace) under its name, or beside it as `<name>.restored-<n>`, if it is
//! not ours or holds something. Rollback removes nothing by a name in a
//! directory other accounts can write: the quarantine itself is left
//! standing, empty, with a warning naming it as safe to delete. Every step
//! that leaves a level standing says so on stderr.
//!
//! # Accepted residuals
//!
//! The approved `rustix` fs scope has no call that unlinks a directory by
//! descriptor. The quarantine is a directory this rollback just made, owned
//! by the current user with no group or other bits, under a random 64-bit
//! name from `/dev/urandom`, so only a writer running as that user can act
//! inside it, and such a writer can already delete or rewrite everything
//! the import owns.
//!
//! - The captured level is unlinked by its name inside the quarantine,
//!   after its identity check. That same writer could swap it in between;
//!   `unlinkat(.., REMOVEDIR)` removes only an empty directory, so the
//!   worst outcome is the loss of one empty directory.
//! - The freshness check proves the shape of what `openat` opened, not who
//!   made it: the same writer could substitute an empty private directory
//!   at the random name between `mkdirat` and `openat`. A level held there
//!   is still checked by identity before it is removed or put back.
//!
//! The other name-based steps are the no-replace renames, which fail rather
//! than replace anything, so they cannot destroy data. A quarantine that
//! cannot be opened after `mkdirat` is left standing, since nothing proves
//! the name still denotes it.
//!
//! The cost is a stale empty `.undo-<hex>` beside the outermost level after
//! each rollback that took a level back; the recovery scan #2472 tracks can
//! reap them. On a filesystem out of space, quota or inodes the
//! quarantine's `mkdirat` fails and the created levels are left standing.
//!
//! Rollback is also not crash-safe: a process killed after a level was
//! renamed into its quarantine and before it was removed or put back leaves
//! it at `.undo-<hex>/level` with no warning, and no later run looks for
//! it. That level holds data only if another writer filled it, or swapped
//! in its own directory, during the import.

use std::ffi::OsStr;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};

use rom_import::OneLinePath;
use rustix::fs::{AtFlags, Dir, Mode, OFlags};

use super::{same_directory_identity, CreatedDirectory};

/// [`super::undo_created_directories`] with `before_take` run once per
/// level, after the name has been validated and before the level is taken
/// back, where a test swaps the name.
pub(super) fn undo_created_directories_with_hooks(
    created: &[CreatedDirectory],
    before_take: &mut dyn FnMut(),
) {
    let mut quarantine = None;
    for dir in created.iter().rev() {
        let Some(parent) = dir.parent.as_deref() else {
            continue;
        };
        match rustix::fs::statat(parent, &dir.name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) if is_this_level(dir, &stat) => {
                before_take();
                if quarantine.is_none() {
                    quarantine = claim_quarantine(created, dir);
                }
                let Some(held) = quarantine.as_ref() else {
                    break;
                };
                if !take_back_into(held, parent, dir) {
                    break;
                }
            }
            // Gone already -- nothing to remove here, and the level above is
            // unaffected by that.
            Err(err) if err == rustix::io::Errno::NOENT => {}
            // Some other entry sits at the name now, or it could not be
            // examined at all: not this run's directory either way, so it
            // is left alone, and the levels above are left too.
            _ => break,
        }
    }
    if let Some(held) = quarantine {
        leave_quarantine(held, &mut || {});
    }
}

/// Whether `stat` is the directory `dir` created and still holds open.
fn is_this_level(dir: &CreatedDirectory, stat: &rustix::fs::Stat) -> bool {
    same_directory_identity(stat, &dir.identity)
        && dir.own.as_ref().is_some_and(|own| {
            rustix::fs::fstat(&**own).is_ok_and(|held| same_directory_identity(stat, &held))
        })
}

/// The entry name a captured level sits under inside its quarantine.
pub(super) const QUARANTINE_ENTRY: &str = "level";

/// `renameat` that refuses to replace an existing destination where the
/// platform can say so (Linux, Android, Apple); elsewhere it refuses with
/// `ENOSYS` rather than emulate that with a racy check.
fn rename_without_replacing(
    from_dir: impl AsFd,
    from: impl rustix::path::Arg + Copy,
    to_dir: impl AsFd,
    to: impl rustix::path::Arg + Copy,
) -> rustix::io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        rustix::fs::renameat_with(
            from_dir,
            from,
            to_dir,
            to,
            rustix::fs::RenameFlags::NOREPLACE,
        )
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
    {
        // No atomic no-replace rename here, and a check-then-rename could
        // overwrite a file a racer creates in the gap: fail closed, so the
        // level is left standing.
        let _ = (from_dir, from, to_dir, to);
        Err(rustix::io::Errno::NOSYS)
    }
}

/// A fresh quarantine basename no other account can predict.
pub(super) fn unguessable_quarantine_name() -> Option<String> {
    use std::io::Read as _;
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut urandom| urandom.read_exact(&mut bytes))
        .ok()?;
    Some(format!(".undo-{:016x}", u64::from_ne_bytes(bytes)))
}

/// Whether `quarantine` is an empty directory owned by this user with no
/// group or other permission bits, as a quarantine this rollback just made
/// with `0700` is under any umask; one that qualifies is then set to
/// exactly `0700` through its own descriptor, since a umask such as `0177`
/// masks the owner's search bit the capture's rename needs. It is read
/// through its own descriptor, never a second one opened for the listing,
/// so a quarantine that took the last free descriptor can still be vouched
/// for; an entry that cannot be read counts as not empty.
fn is_fresh_quarantine(quarantine: &mut rustix::fs::Dir) -> bool {
    use rustix::fs::{FileType, Mode};
    let Ok(stat) = quarantine.stat() else {
        return false;
    };
    let empty = quarantine
        .all(|entry| entry.is_ok_and(|entry| matches!(entry.file_name().to_bytes(), b"." | b"..")));
    FileType::from_raw_mode(stat.st_mode) == FileType::Directory
        && stat.st_uid == rustix::process::geteuid().as_raw()
        && (Mode::from_raw_mode(stat.st_mode) & Mode::RWXG.union(Mode::RWXO)).is_empty()
        && empty
        && quarantine
            .fd()
            .is_ok_and(|fd| rustix::fs::fchmod(fd, Mode::RWXU).is_ok())
}

/// Where `entry` beside `dir` is, for a warning. On Linux and Android it is
/// read from the held `parent` through `/proc/self/fd`, since a pinned
/// ancestor may have been renamed after creation and rollback acts through
/// `parent`, not the path; elsewhere, or without `/proc`, it is the
/// recorded path's sibling, which a rename since creation would make stale.
pub(super) fn located_beside(parent: impl AsFd, dir: &CreatedDirectory, entry: &OsStr) -> PathBuf {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::fd::AsRawFd as _;
        let link = format!("/proc/self/fd/{}", parent.as_fd().as_raw_fd());
        if let Ok(held) = std::fs::read_link(link) {
            if held.is_absolute() {
                return held.join(entry);
            }
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let _ = parent;
    dir.path.with_file_name(entry)
}

/// A quarantine this rollback made and holds open, in `home` under `name`.
pub(super) struct Quarantine<'a> {
    /// The quarantine itself; listed only through this descriptor.
    dir: Dir,
    /// The directory the quarantine was made in.
    home: BorrowedFd<'a>,
    /// Its random basename in `home`.
    name: String,
    /// Where it is, for warnings.
    path: PathBuf,
}

impl Quarantine<'_> {
    /// The held descriptor, for calls that do not depend on its position.
    fn fd(&self) -> Option<BorrowedFd<'_>> {
        self.dir.fd().ok()
    }
}

/// Where `dir` itself is, for a warning that it was left standing.
fn level_location(dir: &CreatedDirectory) -> PathBuf {
    match dir.parent.as_deref() {
        Some(parent) => located_beside(parent, dir, &dir.name),
        None => dir.path.clone(),
    }
}

/// Reports that `dir` was left standing, and why.
fn warn_left_standing(dir: &CreatedDirectory, cause: std::fmt::Arguments<'_>) {
    eprintln!(
        "warning: {} is left in place: {cause}",
        OneLinePath(&level_location(dir))
    );
}

/// The quarantine for one rollback, made under a fresh random name beside
/// the outermost pinned level, so no level it empties contains it; `None`,
/// with `dir` reported as left standing, if none can be made.
fn claim_quarantine<'a>(
    created: &'a [CreatedDirectory],
    dir: &CreatedDirectory,
) -> Option<Quarantine<'a>> {
    let outermost = created.iter().find(|level| level.parent.is_some())?;
    let home = outermost.parent.as_deref()?;
    let Some(name) = unguessable_quarantine_name() else {
        warn_left_standing(dir, format_args!("no entropy for a private cleanup name"));
        return None;
    };
    open_quarantine(home.as_fd(), outermost, &name, dir, &mut || {})
}

/// Makes and opens the quarantine `name` in `home` (beside `home_level`),
/// with `after_mkdir` run between the two, where a test swaps the name, and
/// checks it is fresh. `None`, with `dir` reported as left standing, if it
/// cannot be made, held or vouched for; nothing at the name is removed.
fn open_quarantine<'a>(
    home: BorrowedFd<'a>,
    home_level: &CreatedDirectory,
    name: &str,
    dir: &CreatedDirectory,
    after_mkdir: &mut dyn FnMut(),
) -> Option<Quarantine<'a>> {
    let path = located_beside(home, home_level, OsStr::new(name));
    if let Err(err) = rustix::fs::mkdirat(home, name, Mode::RWXU) {
        warn_left_standing(
            dir,
            format_args!(
                "could not create cleanup directory {}: {}",
                OneLinePath(&path),
                std::io::Error::from(err)
            ),
        );
        return None;
    }
    after_mkdir();
    // `Dir::new` lists through this same descriptor, so checking the
    // quarantine costs no second one.
    let opened = rustix::fs::openat(
        home,
        name,
        OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .and_then(Dir::new);
    let Ok(mut held) = opened else {
        // Without a descriptor nothing proves the name still denotes the
        // directory just made, so it is not unlinked by name.
        warn_left_standing(
            dir,
            format_args!(
                "could not open cleanup directory {}, which is left in place too",
                OneLinePath(&path)
            ),
        );
        return None;
    };
    // The name was swapped for someone else's directory before the open:
    // it is not ours to fill or remove.
    if !is_fresh_quarantine(&mut held) {
        warn_left_standing(
            dir,
            format_args!(
                "{} is not the private cleanup directory rollback made, so it is left untouched",
                OneLinePath(&path)
            ),
        );
        return None;
    }
    Some(Quarantine {
        dir: held,
        home,
        name: name.to_owned(),
        path,
    })
}

/// Takes `dir` back from `parent` through `quarantine`: renames it in
/// (no-replace), re-checks its identity there, and unlinks it through the
/// held quarantine, or puts it back if it is not ours or holds something.
/// Returns whether the walk may go on to the level above.
fn take_back_into(quarantine: &Quarantine<'_>, parent: &OwnedFd, dir: &CreatedDirectory) -> bool {
    let Some(held) = quarantine.fd() else {
        warn_left_standing(dir, format_args!("its cleanup directory could not be used"));
        return false;
    };
    if rename_without_replacing(parent, &dir.name, held, QUARANTINE_ENTRY).is_err() {
        // Already gone is already taken care of.
        let gone = matches!(
            rustix::fs::statat(parent, &dir.name, AtFlags::SYMLINK_NOFOLLOW),
            Err(err) if err == rustix::io::Errno::NOENT
        );
        if !gone {
            warn_left_standing(
                dir,
                format_args!("it could not be moved into its cleanup directory"),
            );
        }
        return gone;
    }
    let captured = rustix::fs::statat(held, QUARANTINE_ENTRY, AtFlags::SYMLINK_NOFOLLOW);
    let ours = captured.is_ok_and(|stat| is_this_level(dir, &stat));
    if ours && rustix::fs::unlinkat(held, QUARANTINE_ENTRY, AtFlags::REMOVEDIR).is_ok() {
        return true;
    }
    // A replacement that was captured, or this level holding something: put
    // it back under its name, or beside it if the name was retaken
    // meanwhile, so it never survives only under the hidden quarantine.
    if let Some(warning) = restore_capture(held, &quarantine.path, parent, dir) {
        eprintln!("{warning}");
    }
    false
}

/// Moves the capture in `quarantine` (at `quarantine_path`) back under
/// `dir`'s name, or under the first free `<name>.restored-<n>` beside it.
/// Returns the warning to print when the capture did not come back under
/// its own name, naming exactly where it is: the `.restored-<n>` path, or
/// the quarantine path if none took it.
pub(super) fn restore_capture(
    quarantine: BorrowedFd<'_>,
    quarantine_path: &Path,
    parent: &OwnedFd,
    dir: &CreatedDirectory,
) -> Option<String> {
    let beside = |entry: &OsStr| located_beside(parent, dir, entry);
    if rename_without_replacing(quarantine, QUARANTINE_ENTRY, parent, &dir.name).is_ok() {
        return None;
    }
    for n in 0..16 {
        let mut aside = dir.name.clone();
        aside.push(format!(".restored-{n}"));
        if rename_without_replacing(quarantine, QUARANTINE_ENTRY, parent, &aside).is_ok() {
            return Some(format!(
                "warning: {} was recreated during cleanup; its contents are in {}",
                OneLinePath(&beside(&dir.name)),
                OneLinePath(&beside(&aside))
            ));
        }
    }
    Some(format!(
        "warning: could not put {} back; its contents remain in {}",
        OneLinePath(&beside(&dir.name)),
        OneLinePath(&quarantine_path.join(QUARANTINE_ENTRY))
    ))
}

/// Leaves `quarantine` standing, never unlinked by its name, and reports
/// it once; `before_report` runs first, where a test swaps the name.
fn leave_quarantine(quarantine: Quarantine<'_>, before_report: &mut dyn FnMut()) {
    before_report();
    let Quarantine {
        mut dir,
        home,
        name,
        path,
    } = quarantine;
    let held = dir.stat();
    let by_name = rustix::fs::statat(home, name.as_str(), AtFlags::SYMLINK_NOFOLLOW);
    let still_named = matches!(
        (held, by_name),
        (Ok(held), Ok(by_name)) if same_directory_identity(&held, &by_name)
    );
    dir.rewind();
    let empty = dir
        .all(|entry| entry.is_ok_and(|entry| matches!(entry.file_name().to_bytes(), b"." | b"..")));
    let state = match (still_named, empty) {
        (true, true) => "it is empty and safe to delete",
        (true, false) => "it still holds what rollback could not put back",
        (false, _) => {
            "it was moved or replaced during cleanup, and whatever is there now was not touched"
        }
    };
    eprintln!(
        "warning: cleanup directory {} is left in place; {state}",
        OneLinePath(&path)
    );
}

#[cfg(test)]
/// [`undo_created_directories_with_hooks`]'s take-back of one level, with
/// the quarantine made beside it under `quarantine_name`, so a test can
/// stage something at the name before rollback tries to claim it, and
/// `after_mkdir` run between making the quarantine and opening it, where a
/// test swaps the name. Returns whether the level was taken back.
pub(super) fn take_back_level_via(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    quarantine_name: &str,
    after_mkdir: &mut dyn FnMut(),
) -> bool {
    take_back_level_with_hooks(parent, dir, quarantine_name, after_mkdir, &mut || {})
}

#[cfg(test)]
/// [`take_back_level_via`] with `before_report` run just before the
/// quarantine is reported and left standing, where a test swaps its name.
pub(super) fn take_back_level_with_hooks(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    quarantine_name: &str,
    after_mkdir: &mut dyn FnMut(),
    before_report: &mut dyn FnMut(),
) -> bool {
    let Some(quarantine) = open_quarantine(parent.as_fd(), dir, quarantine_name, dir, after_mkdir)
    else {
        return false;
    };
    let taken = take_back_into(&quarantine, parent, dir);
    leave_quarantine(quarantine, before_report);
    taken
}
