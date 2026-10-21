//! The Unix take-back protocol: how [`super::undo_created_directories`]
//! removes a created level without trusting its mutable name.
//!
//! A level whose name still resolves to the identity captured at creation
//! is renamed (no-replace) into a quarantine: one private directory per
//! rollback, made beside the outermost level rollback may take back, so it
//! is never inside a level it empties. A level on another filesystem than
//! that quarantine (`EXDEV`, as behind a `..` into a mount) gets a
//! quarantine of its own beside it, removed before the walk goes on; the
//! shared one is removed and its descriptor released first, and a level
//! further out claims a fresh one. In a
//! quarantine the level is re-checked by identity and unlinked through the
//! held quarantine, or put back (no-replace) under its name, or beside it
//! as `<name>.restored-<n>`, if it is not ours or holds something. Every
//! step that leaves a level standing says so on stderr.
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
//! One step rests on a name in a directory other accounts can write: the
//! empty quarantine's own `unlinkat(.., REMOVEDIR)`. Its name is re-checked
//! against the held quarantine by the `statat` just before it, and a
//! mismatch or a failed unlink leaves it standing with a warning naming its
//! path. A writer who moves the quarantine away and puts an empty directory
//! at its random name in the one-syscall window between the two loses that
//! empty directory; nothing else can be lost, since `REMOVEDIR` cannot
//! remove a directory holding anything. Leaving every quarantine standing
//! instead was rejected: it would leave a hidden directory and a warning
//! after every failed import, which is meant to leave no trace.
//!
//! The other name-based steps are the no-replace renames, which fail rather
//! than replace anything, so they cannot destroy data. A quarantine that
//! cannot be opened after `mkdirat` is left standing, since nothing proves
//! the name still denotes it. On a filesystem out of space, quota or
//! inodes the quarantine's `mkdirat` fails and the created levels are left
//! standing.
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
            // Shedding releases pins outermost first, so every level from
            // here out is unpinned too: none can be vouched for, and the
            // walk stops at the first one still standing, saying so.
            match std::fs::symlink_metadata(&dir.path) {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                _ => {
                    warn_left_standing(
                        dir,
                        format_args!(
                            "it was not held open, so rollback could not confirm it is the directory this import made; the levels above it are left too"
                        ),
                    );
                    break;
                }
            }
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
                match take_back_into(held, parent, dir) {
                    Outcome::Taken => {}
                    Outcome::Stopped => break,
                    Outcome::CrossDevice => {
                        // The shared quarantine's descriptor is released
                        // first, so the fallback can open what it needs even
                        // when that descriptor took the last free slot; a
                        // level further out claims a fresh one.
                        if let Some(shared) = quarantine.take() {
                            remove_quarantine(&shared, &mut || {});
                        }
                        if !take_back_beside(parent, dir) {
                            break;
                        }
                    }
                }
            }
            // Gone already -- nothing to remove here, and the level above is
            // unaffected by that; but if it was moved rather than removed,
            // it still stands elsewhere, and that is reported.
            Err(err) if err == rustix::io::Errno::NOENT => warn_if_moved_away(dir),
            // Some other entry sits at the name now, or the level is no
            // longer held open to vouch for it: not provably this run's
            // directory, so it is left alone, and the levels above are left
            // too.
            Ok(stat) => {
                if same_directory_identity(&stat, &dir.identity) {
                    warn_left_standing(
                        dir,
                        format_args!(
                            "rollback no longer holds it open to confirm it is the directory this import made; the levels above it are left too"
                        ),
                    );
                } else {
                    warn_left_standing(
                        dir,
                        format_args!(
                            "another entry has taken its name; the levels above it are left too"
                        ),
                    );
                    warn_if_moved_away(dir);
                }
                break;
            }
            // It could not be examined at all: left alone, as are the
            // levels above.
            Err(err) => {
                warn_left_standing(
                    dir,
                    format_args!(
                        "it could not be examined: {}; the levels above it are left too",
                        std::io::Error::from(err)
                    ),
                );
                break;
            }
        }
    }
    if let Some(held) = quarantine {
        remove_quarantine(&held, &mut || {});
    }
}

/// What [`take_back_into`] did with one level.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// Removed, or already gone: the walk goes on.
    Taken,
    /// Left standing, and reported: the walk stops.
    Stopped,
    /// Not moved, because the quarantine is on another filesystem: nothing
    /// reported yet.
    CrossDevice,
}

/// Takes `dir` back through a quarantine of its own made beside it, on its
/// own filesystem, then removes that quarantine; answers whether the walk
/// may go on.
fn take_back_beside(parent: &OwnedFd, dir: &CreatedDirectory) -> bool {
    let Some(name) = unguessable_quarantine_name() else {
        warn_left_standing(dir, format_args!("no entropy for a private cleanup name"));
        return false;
    };
    let Some(local) = open_quarantine(parent.as_fd(), dir, &name, dir, &mut || {}) else {
        return false;
    };
    let outcome = take_back_into(&local, parent, dir);
    remove_quarantine(&local, &mut || {});
    match outcome {
        Outcome::Taken => true,
        Outcome::Stopped => false,
        Outcome::CrossDevice => {
            warn_left_standing(
                dir,
                format_args!("it could not be moved into a cleanup directory beside it"),
            );
            false
        }
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
fn quarantine_location(
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

/// A quarantine this rollback made and holds open, in `home` under `name`.
pub(super) struct Quarantine<'a> {
    /// The quarantine itself; listed only through this descriptor.
    dir: Dir,
    /// The directory the quarantine was made in.
    home: BorrowedFd<'a>,
    /// Its random basename in `home`.
    name: String,
    /// Where it was when it was made: the fallback for [`Self::location`].
    path: PathBuf,
}

impl Quarantine<'_> {
    /// The held descriptor, for calls that do not depend on its position.
    fn fd(&self) -> Option<BorrowedFd<'_>> {
        self.dir.fd().ok()
    }

    /// Where its name in `home` is now, for a warning about that name.
    fn name_location(&self) -> PathBuf {
        quarantine_location(None, self.home, &self.name, &self.path)
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
fn take_back_into(
    quarantine: &Quarantine<'_>,
    parent: &OwnedFd,
    dir: &CreatedDirectory,
) -> Outcome {
    let Some(held) = quarantine.fd() else {
        warn_left_standing(dir, format_args!("its cleanup directory could not be used"));
        return Outcome::Stopped;
    };
    if let Err(err) = rename_without_replacing(parent, &dir.name, held, QUARANTINE_ENTRY) {
        if err == rustix::io::Errno::XDEV {
            return Outcome::CrossDevice;
        }
        // Already gone is already taken care of.
        let gone = matches!(
            rustix::fs::statat(parent, &dir.name, AtFlags::SYMLINK_NOFOLLOW),
            Err(err) if err == rustix::io::Errno::NOENT
        );
        if gone {
            warn_if_moved_away(dir);
            return Outcome::Taken;
        }
        warn_left_standing(
            dir,
            format_args!(
                "it could not be moved into its cleanup directory: {}",
                std::io::Error::from(err)
            ),
        );
        return Outcome::Stopped;
    }
    let captured = rustix::fs::statat(held, QUARANTINE_ENTRY, AtFlags::SYMLINK_NOFOLLOW).ok();
    let ours = captured
        .as_ref()
        .is_some_and(|stat| is_this_level(dir, stat));
    let not_removed = if ours {
        match rustix::fs::unlinkat(held, QUARANTINE_ENTRY, AtFlags::REMOVEDIR) {
            Ok(()) => return Outcome::Taken,
            Err(err) => Some(err),
        }
    } else {
        None
    };
    // A replacement that was captured, or this level holding something: put
    // it back under its name, or beside it if the name was retaken
    // meanwhile, so it never survives only under the hidden quarantine.
    match restore_capture(
        held,
        quarantine.home,
        &quarantine.name,
        &quarantine.path,
        parent,
        dir,
    ) {
        Some(warning) => eprintln!("{warning}"),
        None => {
            if let Some(err) = not_removed {
                warn_left_standing(
                    dir,
                    format_args!("it could not be removed: {}", std::io::Error::from(err)),
                );
            }
        }
    }
    if captured.is_some() && !ours {
        warn_replaced(dir);
    }
    Outcome::Stopped
}

/// Reports that the directory rollback made at `dir` was moved away and
/// something else took its name before it could be taken back, naming
/// where the held original is now where `/proc` can say.
fn warn_replaced(dir: &CreatedDirectory) {
    let level = level_location(dir);
    match dir.own.as_deref().and_then(held_location) {
        Some(now) => eprintln!(
            "warning: {} is the directory rollback made for {}, moved away and replaced before rollback could take it back",
            OneLinePath(&now),
            OneLinePath(&level)
        ),
        None => eprintln!(
            "warning: {} was moved away and replaced before rollback could take it back; the directory rollback made is left elsewhere",
            OneLinePath(&level)
        ),
    }
}

/// Reports that the directory rollback made for `dir`, no longer under its
/// name, was moved away rather than removed, naming where the held
/// original is now where `/proc` can say. Silent when it has no link left
/// (removed outright) or is no longer held open, so nothing can be said.
fn warn_if_moved_away(dir: &CreatedDirectory) {
    let Some(own) = dir.own.as_deref() else {
        return;
    };
    match rustix::fs::fstat(own) {
        Ok(held) if held.st_nlink != 0 => {}
        _ => return,
    }
    let level = level_location(dir);
    match held_location(own) {
        Some(now) => eprintln!(
            "warning: {} is the directory rollback made for {}, moved away before rollback could take it back; it is left in place",
            OneLinePath(&now),
            OneLinePath(&level)
        ),
        None => eprintln!(
            "warning: {} was moved away before rollback could take it back; the directory rollback made is left elsewhere",
            OneLinePath(&level)
        ),
    }
}

/// Moves the capture in `quarantine` (named `quarantine_name` in the held
/// `quarantine_home`, and at `recorded` when it was made) back under
/// `dir`'s name, or under the first free `<name>.restored-<n>` beside it.
/// Returns the warning to print when the capture did not come back under
/// its own name, naming exactly where it is: the `.restored-<n>` path, or
/// the quarantine's current path if none took it.
pub(super) fn restore_capture(
    quarantine: BorrowedFd<'_>,
    quarantine_home: BorrowedFd<'_>,
    quarantine_name: &str,
    recorded: &Path,
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
        OneLinePath(
            &quarantine_location(Some(quarantine), quarantine_home, quarantine_name, recorded)
                .join(QUARANTINE_ENTRY)
        )
    ))
}

/// Removes the empty `quarantine` by its name, only while that name still
/// resolves to the held quarantine, checked by the `statat` just before the
/// unlink; a mismatch or a failed unlink leaves it standing, reported at
/// its name's path, which is what the check and the unlink act on. `before_removal` runs before that check, where a test swaps
/// the name.
fn remove_quarantine(quarantine: &Quarantine<'_>, before_removal: &mut dyn FnMut()) {
    let held = quarantine.dir.stat();
    before_removal();
    let by_name = rustix::fs::statat(
        quarantine.home,
        quarantine.name.as_str(),
        AtFlags::SYMLINK_NOFOLLOW,
    );
    if !matches!((held, by_name), (Ok(held), Ok(by_name)) if same_directory_identity(&held, &by_name))
    {
        eprintln!(
            "warning: cleanup directory {} no longer resolves to the one rollback made; it is left in place",
            OneLinePath(&quarantine.name_location())
        );
        return;
    }
    if let Err(err) = rustix::fs::unlinkat(
        quarantine.home,
        quarantine.name.as_str(),
        AtFlags::REMOVEDIR,
    ) {
        eprintln!(
            "warning: could not remove cleanup directory {}: {}; it is left in place",
            OneLinePath(&quarantine.name_location()),
            std::io::Error::from(err)
        );
    }
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
/// [`take_back_level_via`] with `before_removal` run just before the
/// quarantine's name is re-checked for its removal, where a test swaps it.
pub(super) fn take_back_level_with_hooks(
    parent: &OwnedFd,
    dir: &CreatedDirectory,
    quarantine_name: &str,
    after_mkdir: &mut dyn FnMut(),
    before_removal: &mut dyn FnMut(),
) -> bool {
    let Some(quarantine) = open_quarantine(parent.as_fd(), dir, quarantine_name, dir, after_mkdir)
    else {
        return false;
    };
    let taken = take_back_into(&quarantine, parent, dir) == Outcome::Taken;
    remove_quarantine(&quarantine, before_removal);
    taken
}
