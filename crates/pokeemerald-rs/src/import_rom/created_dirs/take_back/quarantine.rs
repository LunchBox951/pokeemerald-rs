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
//! nothing proves the name still denotes it, except when the open is refused
//! for permission (a umask that strips the owner's read bit): the name is
//! then removed only if it still reads as an empty-looking directory this
//! user owns with no group or other bits, by `unlinkat(.., REMOVEDIR)`,
//! which cannot remove one holding anything. Nothing held proves the name
//! is the directory just made, so a writer running as this user who swaps
//! in an empty directory of its own with no group or other bits, before
//! the check or in the one-syscall window before the unlink, loses that
//! empty directory; such a writer can already delete or rewrite everything
//! the import owns.
//!
//! On a filesystem out of space, quota or inodes the quarantine's `mkdirat`
//! fails; that, and the permission refusal above, make no quarantine
//! available, and the walk removes each level in place instead (see
//! `remove_in_place` in the parent module).

use std::ffi::OsStr;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
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

/// The calls that make and enter a quarantine, which a test replaces to fail
/// one of them or to fail the freshness check with an error.
#[derive(Clone, Copy)]
pub(in super::super) struct Steps {
    /// Vouches for the entered quarantine: [`is_fresh_quarantine`].
    pub(in super::super) vouch: fn(&mut Dir) -> rustix::io::Result<bool>,
    /// Makes the quarantine `name` in `home`.
    pub(in super::super) mkdir: fn(BorrowedFd<'_>, &str) -> rustix::io::Result<()>,
    /// Opens the quarantine `name` in `home` for listing.
    pub(in super::super) open: fn(BorrowedFd<'_>, &str) -> rustix::io::Result<OwnedFd>,
}

impl Steps {
    /// The real calls.
    pub(in super::super) const REAL: Steps = Steps {
        vouch: is_fresh_quarantine,
        mkdir: |home, name| rustix::fs::mkdirat(home, name, Mode::RWXU),
        open: |home, name| {
            rustix::fs::openat(
                home,
                name,
                OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
        },
    };
}

/// What asking for a quarantine came to.
pub(super) enum Claim<'a> {
    /// Made, held and vouched for.
    Held(Quarantine<'a>),
    /// None can be had here, for want of space, quota or inodes, or because
    /// the umask left it unenterable (and it was removed again); nothing
    /// is reported yet, and the cause is the string.
    Unavailable(
        #[cfg_attr(
            not(test),
            expect(
                dead_code,
                reason = "only the tests' left-standing report reads the cause"
            )
        )]
        String,
    ),
    /// None was had, for a reason already reported: nothing at the name was
    /// removed.
    Refused,
}

/// The quarantine for one rollback, made under a fresh random name beside
/// the outermost pinned level, so no level it empties contains it. `dir` is
/// reported as left standing if that is [`Claim::Refused`].
pub(super) fn claim_quarantine<'a>(
    created: &'a [CreatedDirectory],
    dir: &CreatedDirectory,
    steps: Steps,
) -> Claim<'a> {
    let Some(outermost) = created.iter().find(|level| level.parent.is_some()) else {
        return Claim::Refused;
    };
    let Some(home) = outermost.parent.as_deref() else {
        return Claim::Refused;
    };
    let Some(name) = unguessable_quarantine_name() else {
        warn_left_standing(dir, format_args!("no entropy for a private cleanup name"));
        return Claim::Refused;
    };
    open_quarantine_with(home.as_fd(), outermost, &name, dir, &mut || {}, steps)
}

#[cfg(test)]
/// [`open_quarantine_with`] for a caller with no use for telling
/// [`Claim::Unavailable`] from [`Claim::Refused`]: it is reported as `dir`
/// left standing.
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

#[cfg(test)]
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
    let steps = Steps {
        vouch,
        ..Steps::REAL
    };
    match open_quarantine_with(home, home_level, name, dir, after_mkdir, steps) {
        Claim::Held(held) => Some(held),
        Claim::Unavailable(cause) => {
            warn_left_standing(dir, format_args!("{cause}"));
            None
        }
        Claim::Refused => None,
    }
}

/// Whether `err` says the filesystem has nothing left to make an entry with.
fn is_out_of_space(err: rustix::io::Errno) -> bool {
    err == rustix::io::Errno::NOSPC || err == rustix::io::Errno::DQUOT
}

/// Makes and opens the quarantine `name` in `home` (beside `home_level`)
/// through `steps`, with `after_mkdir` run between the two, where a test
/// swaps the name, and checks it is fresh. Nothing at the name is removed
/// unless it is the quarantine just made and unenterable.
pub(super) fn open_quarantine_with<'a>(
    home: BorrowedFd<'a>,
    home_level: &CreatedDirectory,
    name: &str,
    dir: &CreatedDirectory,
    after_mkdir: &mut dyn FnMut(),
    steps: Steps,
) -> Claim<'a> {
    let path = located_beside(home, home_level, OsStr::new(name));
    if let Err(err) = (steps.mkdir)(home, name) {
        let cause = format!(
            "could not create cleanup directory {}: {}",
            OneLinePath(&path),
            std::io::Error::from(err)
        );
        if is_out_of_space(err) {
            return Claim::Unavailable(cause);
        }
        warn_left_standing(dir, format_args!("{cause}"));
        return Claim::Refused;
    }
    after_mkdir();
    // `Dir::new` lists through this same descriptor, so checking the
    // quarantine costs no second one.
    let opened = (steps.open)(home, name).and_then(Dir::new);
    let mut held = match opened {
        Ok(held) => held,
        // A umask that strips the owner's read bit leaves the quarantine
        // unenterable by this open, before anything can fix its mode.
        Err(err) if err == rustix::io::Errno::ACCESS => {
            remove_unentered_quarantine(home, name, &path);
            return Claim::Unavailable(format!(
                "could not open cleanup directory {}: {}",
                OneLinePath(&path),
                std::io::Error::from(err)
            ));
        }
        Err(_) => {
            // Without a descriptor nothing proves the name still denotes
            // the directory just made, so it is not unlinked by name.
            warn_left_standing(
                dir,
                format_args!(
                    "could not open cleanup directory {}, which is left in place too",
                    OneLinePath(&path)
                ),
            );
            return Claim::Refused;
        }
    };
    match (steps.vouch)(&mut held) {
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
            return Claim::Refused;
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
            return Claim::Refused;
        }
    }
    Claim::Held(Quarantine {
        dir: held,
        home,
        name: name.to_owned(),
        path,
    })
}

/// Removes the quarantine `name` in `home` that could not be entered, only
/// while it still reads as a directory this user owns with no group or other
/// bits, which is what `mkdirat(0700)` made under any umask. `REMOVEDIR`
/// removes it only if empty; a mismatch or a failed unlink leaves it standing
/// with a warning naming its path.
fn remove_unentered_quarantine(home: BorrowedFd<'_>, name: &str, path: &std::path::Path) {
    use rustix::fs::FileType;
    let ours = rustix::fs::statat(home, name, AtFlags::SYMLINK_NOFOLLOW).map(|stat| {
        FileType::from_raw_mode(stat.st_mode) == FileType::Directory
            && stat.st_uid == rustix::process::geteuid().as_raw()
            && (Mode::from_raw_mode(stat.st_mode) & Mode::RWXG.union(Mode::RWXO)).is_empty()
    });
    let removed = match ours {
        Ok(true) => rustix::fs::unlinkat(home, name, AtFlags::REMOVEDIR),
        Ok(false) => {
            eprintln!(
                "warning: {} is not the private cleanup directory rollback made, so it is left untouched",
                OneLinePath(path)
            );
            return;
        }
        Err(err) => Err(err),
    };
    if let Err(err) = removed {
        eprintln!(
            "warning: could not remove cleanup directory {}: {}; it is left in place",
            OneLinePath(path),
            std::io::Error::from(err)
        );
    }
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
