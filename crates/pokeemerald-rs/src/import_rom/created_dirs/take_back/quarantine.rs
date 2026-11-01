//! The quarantine one rollback takes levels back through: made under an
//! unguessable name beside the outermost level it may take back, vouched for
//! as fresh, and removed once empty.
//!
//! # Accepted residuals
//!
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
//! A quarantine that cannot be opened after `mkdirat` is left standing, since
//! nothing proves the name still denotes it. On a filesystem out of space,
//! quota or inodes the quarantine's `mkdirat` fails and the created levels
//! are left standing.

use std::ffi::OsStr;
use std::os::fd::{AsFd, BorrowedFd};
use std::path::PathBuf;

use rom_import::OneLinePath;
use rustix::fs::{AtFlags, Dir, Mode, OFlags};

use super::super::{same_directory_identity, CreatedDirectory};
use super::report::{
    held_standing_as, located_beside, quarantine_location, warn_left_standing,
    warn_quarantine_not_at_name, Standing,
};

/// The entry name a captured level sits under inside its quarantine.
pub(in super::super) const QUARANTINE_ENTRY: &str = "level";

/// A fresh quarantine basename no other account can predict.
pub(in super::super) fn unguessable_quarantine_name() -> Option<String> {
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
/// for. A stat, listing or `fchmod` that fails is the error, not a verdict:
/// it says nothing about whose directory this is.
fn is_fresh_quarantine(quarantine: &mut Dir) -> rustix::io::Result<bool> {
    use rustix::fs::{FileType, Mode};
    let stat = quarantine.stat()?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::Directory
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || !(Mode::from_raw_mode(stat.st_mode) & Mode::RWXG.union(Mode::RWXO)).is_empty()
    {
        return Ok(false);
    }
    let mut listed = Ok(true);
    quarantine.all(|entry| match entry {
        Ok(entry) if matches!(entry.file_name().to_bytes(), b"." | b"..") => true,
        Ok(_) => {
            listed = Ok(false);
            false
        }
        Err(err) => {
            listed = Err(err);
            false
        }
    });
    if !listed? {
        return Ok(false);
    }
    rustix::fs::fchmod(quarantine.fd()?, Mode::RWXU)?;
    Ok(true)
}

/// A quarantine this rollback made and holds open, in `home` under `name`.
pub(super) struct Quarantine<'a> {
    /// The quarantine itself; listed only through this descriptor.
    dir: Dir,
    /// The directory the quarantine was made in.
    pub(super) home: BorrowedFd<'a>,
    /// Its random basename in `home`.
    pub(super) name: String,
    /// Where it was when it was made: the fallback for [`Self::location`].
    pub(super) path: PathBuf,
}

impl Quarantine<'_> {
    /// The held descriptor, for calls that do not depend on its position.
    pub(super) fn fd(&self) -> Option<BorrowedFd<'_>> {
        self.dir.fd().ok()
    }

    /// Where its name in `home` is now, for a warning about that name.
    pub(super) fn name_location(&self) -> PathBuf {
        quarantine_location(None, self.home, &self.name, &self.path)
    }
}

/// The quarantine for one rollback, made under a fresh random name beside
/// the outermost pinned level, so no level it empties contains it; `None`,
/// with `dir` reported as left standing, if none can be made.
pub(super) fn claim_quarantine<'a>(
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
pub(super) fn open_quarantine<'a>(
    home: BorrowedFd<'a>,
    home_level: &CreatedDirectory,
    name: &str,
    dir: &CreatedDirectory,
    after_mkdir: &mut dyn FnMut(),
) -> Option<Quarantine<'a>> {
    open_quarantine_vouched_by(
        home,
        home_level,
        name,
        dir,
        after_mkdir,
        is_fresh_quarantine,
    )
}

/// [`open_quarantine`] with `vouch` in place of [`is_fresh_quarantine`], so
/// a test can make the freshness check fail with an error.
pub(super) fn open_quarantine_vouched_by<'a>(
    home: BorrowedFd<'a>,
    home_level: &CreatedDirectory,
    name: &str,
    dir: &CreatedDirectory,
    after_mkdir: &mut dyn FnMut(),
    vouch: fn(&mut Dir) -> rustix::io::Result<bool>,
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
    match vouch(&mut held) {
        Ok(true) => {}
        // The name was swapped for someone else's directory before the
        // open: it is not ours to fill or remove.
        Ok(false) => {
            warn_left_standing(
                dir,
                format_args!(
                    "{} is not the private cleanup directory rollback made, so it is left untouched",
                    OneLinePath(&path)
                ),
            );
            return None;
        }
        // The check itself failed: the directory may well be the one just
        // made, but nothing vouches for it, so it is left, with the error.
        Err(err) => {
            warn_left_standing(
                dir,
                format_args!(
                    "cleanup directory {} could not be checked: {}; it is left in place too",
                    OneLinePath(&path),
                    std::io::Error::from(err)
                ),
            );
            return None;
        }
    }
    Some(Quarantine {
        dir: held,
        home,
        name: name.to_owned(),
        path,
    })
}

/// Removes the empty `quarantine` by its name, only while that name still
/// resolves to the held quarantine, checked by the `statat` just before the
/// unlink; a mismatch, a name or held quarantine that could not be
/// examined, or a failed unlink leaves it standing. A failed unlink or examination is reported at
/// its name's path, with the error, since that is what the call acts on; a
/// mismatch through [`warn_quarantine_not_at_name`]. `before_removal` runs
/// before that check, where a test swaps the name.
pub(super) fn remove_quarantine(quarantine: &Quarantine<'_>, before_removal: &mut dyn FnMut()) {
    remove_quarantine_held_as(quarantine, quarantine.dir.stat(), before_removal);
}

/// [`remove_quarantine`] with `held` as what the held quarantine's own
/// `fstat` gave, so a test can make that examination fail.
pub(super) fn remove_quarantine_held_as(
    quarantine: &Quarantine<'_>,
    held: rustix::io::Result<rustix::fs::Stat>,
    before_removal: &mut dyn FnMut(),
) {
    before_removal();
    let by_name = rustix::fs::statat(
        quarantine.home,
        quarantine.name.as_str(),
        AtFlags::SYMLINK_NOFOLLOW,
    );
    // A lookup that failed other than for a vacant name says nothing about
    // where the quarantine is: it is left, and the error is the cause.
    if let Some(&err) = by_name
        .as_ref()
        .err()
        .filter(|&&err| err != rustix::io::Errno::NOENT)
    {
        eprintln!(
            "warning: cleanup directory {} could not be examined: {}; it is left in place",
            OneLinePath(&quarantine.name_location()),
            std::io::Error::from(err)
        );
        return;
    }
    // The name resolves to something, but the held quarantine could not be
    // examined to compare with it: that says nothing about where the
    // quarantine is either, so it is left with that error as the cause.
    if let (Err(err), Ok(_)) = (&held, &by_name) {
        eprintln!(
            "warning: cleanup directory {} could not be examined: {}; it is left in place",
            OneLinePath(&quarantine.name_location()),
            std::io::Error::from(*err)
        );
        return;
    }
    let resolves = match (&held, &by_name) {
        (Ok(held), Ok(by_name)) => same_directory_identity(held, by_name),
        _ => false,
    };
    if !resolves {
        let standing = match quarantine.fd() {
            Some(fd) => held_standing_as(held, fd),
            None => Standing::Unknown,
        };
        warn_quarantine_not_at_name(quarantine, standing, &by_name);
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
