//! Making a level and claiming the directory `mkdirat` made.
//!
//! `mkdirat` hands back no descriptor, so a level made at its final name can
//! be exchanged by another writer of the parent for a directory it prepared
//! before anything looks at the name. The identity read afterwards then
//! belongs to the replacement, and every later check against that identity
//! agrees. Where the platform can rename without replacing (Linux, Android,
//! Apple), a level is made instead under a private name no other account
//! can predict, opened by that name so its identity is read from a
//! descriptor, and only then renamed (no-replace) into place; a name someone
//! else took meanwhile is theirs, and the private directory is moved aside
//! and removed only if it is still the one checked, never unlinked by a name
//! that could have been swapped since. A
//! level whose name already resolves to a directory is never claimed at all:
//! the walk goes through it, as it needs no write access to the parent. The
//! reopen that follows compares the name against that identity while the
//! claim still holds the directory open, so its inode cannot be freed and
//! reused by a replacement, and a swap after the claim is refused too.
//! Elsewhere a level is made at its final name, where a same-user
//! replacement in the gap cannot be told apart, so it is not claimed: it is
//! walked through like a level that already existed, never recorded, and
//! never removed on rollback. So it is where the kernel or filesystem
//! answers the no-replace rename with `EINVAL`, `ENOTSUP` or `ENOSYS` (an NFS
//! mount, say): the private directory is removed after checking its
//! identity, and the level is made at its final name and left unclaimed, so
//! a failed import there leaves the levels it made standing.
//!
//! # Accepted residuals
//!
//! A writer of the parent running as this user who lists it fast enough to
//! find the private name before it is opened can still exchange it. Such a
//! writer can already delete or rewrite everything the import owns (see
//! `take_back`), and a level is refused unless it is a directory of this
//! user's. A swap after the reopen is `take_back`'s to withstand.
//!
//! The private name defends against an account that cannot predict it, not
//! against a parent that lets other accounts rename its entries. Where the
//! parent is writable by others and not sticky (no `S_ISVTX`), any account
//! that can write it may rename an entry it does not own, and one that lists
//! the parent finds the private name before it is opened. It can move the
//! private directory away and put at that name a directory this user owns
//! that it can move there: the owner check accepts it, the claim renames it
//! into place, and rollback treats it as the level this run made. This is not
//! defended against. A sticky parent (`/tmp`, say) confines renames to an
//! entry's owner; a parent writable by others without the sticky bit is not
//! covered, and nothing here checks the parent's mode, so a level there is
//! claimed as in any other parent. Whether to refuse such a parent is
//! undecided. A new level is not required to be empty: a second import creating beneath it
//! may legitimately have put an entry there already.

use std::ffi::OsStr;
use std::io;

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
use super::take_back;
use super::{
    dest, is_access_denied, is_out_of_descriptors, is_out_of_descriptors_io, owner_access,
    same_directory_identity, sync_and_shed_outermost_pinned_level, CreatedDirectory,
};

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
mod discard;
mod final_name;

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
pub(super) use discard::discard_private;
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
use discard::report_left_standing;
#[cfg(all(
    test,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
pub(super) use discard::{discard_private_with, left_standing_warning, Discard};

#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
pub(super) use final_name::claim_level;

/// `mkdirat`s `name` in `parent`, shedding a pin and retrying while
/// descriptors run out.
#[cfg(unix)]
pub(super) fn mkdir_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> rustix::io::Result<()> {
    loop {
        match rustix::fs::mkdirat(
            &**parent,
            name,
            rustix::fs::Mode::RWXU | rustix::fs::Mode::RWXG | rustix::fs::Mode::RWXO,
        ) {
            Err(err)
                if is_out_of_descriptors(err)
                    && sync_and_shed_outermost_pinned_level(created, pin_from, sync_reopen) => {}
            result => return result,
        }
    }
}

/// Why [`claim_level`] did not make a level of its own.
#[cfg(unix)]
pub(super) enum ClaimError {
    /// `name` was already taken, by a directory, a symlink or a file: not
    /// this run's to record, and the caller decides whether it can be walked
    /// through.
    Taken(io::Error),
    /// A directory was made at `name` itself, where nothing can tie what
    /// stands there now to this run's `mkdirat`: a same-user writer of the
    /// parent may have put its own there since. It is walked through as an
    /// existing level, never recorded, so rollback leaves it standing. The
    /// descriptor is the directory found at `name`, opened without following
    /// a symlink, which the walk descends through; a name that no longer
    /// holds a directory is [`Self::Failed`]. Its parent has been synced and
    /// its owner's access restored, as for a claimed level.
    Unowned(std::os::fd::OwnedFd),
    /// Anything else, including everything that went wrong once a directory
    /// was made: the caller must not take the name for an existing level.
    Failed(io::Error),
}

#[cfg(unix)]
impl ClaimError {
    /// `errno` as [`Self::Taken`] if it says the name exists, else
    /// [`Self::Failed`].
    fn from_errno(errno: rustix::io::Errno) -> Self {
        if errno == rustix::io::Errno::EXIST {
            Self::Taken(errno.into())
        } else {
            Self::Failed(errno.into())
        }
    }
}

/// A level [`claim_level`] made: the identity of the directory, and a
/// descriptor on it, which the caller keeps until the reopen has compared the
/// name against `identity`. While it is open the directory's inode cannot be
/// freed, so a directory put at the name after this one was removed cannot
/// take the same device and inode and pass that comparison.
#[cfg(unix)]
pub(super) struct Claimed {
    /// The device, inode, type and owner of the directory this run made.
    pub(super) identity: rustix::fs::Stat,
    /// The directory itself, held open.
    pub(super) held: std::os::fd::OwnedFd,
}

/// Makes the level `name` in `parent` and answers with the identity of the
/// directory this call made, owned by `owner`, and a descriptor holding it.
/// `after_mkdir` runs once the directory is made and before anything looks at
/// it, where a test replaces it.
///
/// Where the platform can rename without replacing, the directory is made
/// under a private name no other account can predict, opened by that name,
/// and stat'ed through the descriptor; only then is it renamed (no-replace)
/// to `name`. A writer of `parent` cannot exchange it for its own before
/// the identity is taken, since it cannot know the name, and a swap of the
/// private name after that leaves `name` holding a directory other than the
/// identity returned, which the reopen refuses. A `name` that is already
/// taken is [`ClaimError::Taken`], as `mkdirat` would have said, and the
/// private directory is removed by [`discard_private`], which only removes
/// what it finds to be the one that was verified; one that fails
/// verification is left untouched, since it may be someone else's. Elsewhere
/// there is no such rename, so the directory is made at `name` and is
/// [`ClaimError::Unowned`], as nothing can show that what stands there is it.
#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
pub(super) fn claim_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    owner: u32,
    after_mkdir: &mut dyn FnMut(),
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> Result<Claimed, ClaimError> {
    let private = fresh_private_name(created, pin_from, sync_reopen)?;
    // Chosen now, while a descriptor can still be shed for it: cleanup runs
    // with the verified directory held open and must not fail for lack of one.
    let aside = fresh_private_name(created, pin_from, sync_reopen)?;
    mkdir_level(created, pin_from, parent, &private, sync_reopen)
        .map_err(|errno| ClaimError::Failed(errno.into()))?;
    after_mkdir();
    let (identity, held) = identity_of(created, pin_from, parent, &private, owner, sync_reopen)
        .map_err(|source| {
            // It cannot be removed: nothing shows the name still holds the
            // directory that was made. Say where it is instead.
            ClaimError::Failed(io::Error::new(
                source.kind(),
                format!(
                    "{source}; the directory {} made for the level could not be \
                     checked and was left in place",
                    std::path::Path::new(&private).display()
                ),
            ))
        })?;
    match rename_no_replace(parent, &private, parent, name) {
        Ok(()) => Ok(Claimed { identity, held }),
        Err(errno) => {
            report_left_standing(
                &discard_private(parent, &private, &aside, &held, &mut || {}),
                name,
            );
            if no_replace_unsupported(errno) {
                // This filesystem cannot refuse to replace (an NFS mount, say):
                // make the level at its final name, as where no such rename
                // exists, rather than refuse to import at all, but do not
                // claim it. The private directory is of no further use, and
                // its descriptor would cost the parent sync there a slot.
                drop(held);
                return Err(final_name::claim_at_final_name(
                    created,
                    pin_from,
                    parent,
                    name,
                    owner,
                    after_mkdir,
                    sync_reopen,
                ));
            }
            Err(ClaimError::from_errno(errno))
        }
    }
}

/// [`take_back::rename_without_replacing`] for the moves this module makes,
/// where a test can make the filesystem answer as one with no such rename
/// (an NFS mount) without any process-wide state.
#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
fn rename_no_replace(
    from_dir: &std::os::fd::OwnedFd,
    from: &OsStr,
    to_dir: &std::os::fd::OwnedFd,
    to: &OsStr,
) -> rustix::io::Result<()> {
    #[cfg(test)]
    if let Some(errno) = tests_seam::REFUSED.with(std::cell::Cell::get) {
        tests_seam::REFUSALS.with(|count| count.set(count.get() + 1));
        return Err(errno);
    }
    take_back::rename_without_replacing(from_dir, from, to_dir, to)
}

/// The test seam behind [`rename_no_replace`]: the errno every no-replace
/// rename made on this thread answers with.
#[cfg(all(
    test,
    unix,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
pub(super) mod tests_seam {
    thread_local! {
        pub(super) static REFUSED: std::cell::Cell<Option<rustix::io::Errno>> =
            const { std::cell::Cell::new(None) };
        pub(super) static REFUSALS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    /// Makes no-replace renames on this thread fail with `errno` until the
    /// guard drops.
    pub(in super::super) struct Refusing;

    pub(in super::super) fn refuse_noreplace(errno: rustix::io::Errno) -> Refusing {
        REFUSED.with(|cell| cell.set(Some(errno)));
        REFUSALS.with(|count| count.set(0));
        Refusing
    }

    /// How many renames the seam has failed since it was installed.
    pub(in super::super) fn refusals() -> usize {
        REFUSALS.with(std::cell::Cell::get)
    }

    impl Drop for Refusing {
        fn drop(&mut self) {
            REFUSED.with(|cell| cell.set(None));
        }
    }
}

/// Whether `errno` says the filesystem or kernel has no no-replace rename,
/// as opposed to this rename having failed.
#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
fn no_replace_unsupported(errno: rustix::io::Errno) -> bool {
    matches!(
        errno,
        rustix::io::Errno::INVAL | rustix::io::Errno::NOTSUP | rustix::io::Errno::NOSYS
    )
}

/// A fresh private name in a directory others can write, shedding a pin
/// and retrying while descriptors run out.
#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
fn fresh_private_name(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> Result<std::ffi::OsString, ClaimError> {
    let hex = loop {
        match take_back::unguessable_hex() {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && sync_and_shed_outermost_pinned_level(created, pin_from, sync_reopen) => {}
            result => break result.map_err(ClaimError::Failed)?,
        }
    };
    Ok(std::ffi::OsString::from(format!(".import-{hex}")))
}

/// The identity of the directory `name` in `parent`, read through a
/// descriptor opened by that name, which is handed back so the entry can be
/// recognised again by [`discard_private`].
#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
fn identity_of(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    owner: u32,
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> io::Result<(rustix::fs::Stat, std::os::fd::OwnedFd)> {
    let held = loop {
        let attempt = match dest::open_created_directory_at(parent, name) {
            // The umask took the bit the open needs (owner search, on Apple):
            // the owner's access is restored through the parent, as for a
            // claimed level's reopen, and the open tried once more.
            Err(source) if is_access_denied(&source) => {
                restore_private_by_name(created, pin_from, parent, name, owner, sync_reopen)
                    .and_then(|()| dest::open_created_directory_at(parent, name))
            }
            other => other,
        };
        match attempt {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && sync_and_shed_outermost_pinned_level(created, pin_from, sync_reopen) => {}
            result => break result?,
        }
    };
    let identity = vouch_owned_by(rustix::fs::fstat(&held)?, owner)?;
    Ok((identity, held))
}

/// [`restore_through_parent`] for the private directory `name` in `parent`
/// whose open the umask refused, when it is a directory `owner` owns, found
/// without following a symlink. Its identity is then read again through the
/// descriptor [`identity_of`] opens.
#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
fn restore_private_by_name(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    owner: u32,
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> io::Result<()> {
    let found = vouch_owned_by(
        rustix::fs::statat(&**parent, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)?,
        owner,
    )?;
    restore_through_parent(created, pin_from, parent, name, &found, None, sync_reopen)
}

/// [`owner_access::restore_by_name`] on the level `name` in `parent`, which
/// must still be `identity`, and `pin` when one is held on it: the owner's
/// access restored through a real descriptor opened from the parent. A full
/// descriptor table sheds a pin and tries again; a level the route cannot
/// reach is an [`owner_access::unrestorable`] error.
#[cfg(unix)]
fn restore_through_parent(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::os::fd::OwnedFd,
    name: &OsStr,
    identity: &rustix::fs::Stat,
    pin: Option<&std::os::fd::OwnedFd>,
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> io::Result<()> {
    loop {
        match owner_access::restore_by_name(parent, name, identity, pin) {
            Ok(()) => return Ok(()),
            Err(owner_access::Refused::OutOfDescriptors(source)) => {
                if !sync_and_shed_outermost_pinned_level(created, pin_from, sync_reopen) {
                    return Err(source);
                }
            }
            Err(owner_access::Refused::Replaced(replaced)) => return Err(replaced),
            Err(owner_access::Refused::Failed) => return Err(owner_access::unrestorable()),
        }
    }
}

/// `identity` if it is a directory of `owner`'s, else the refusal.
#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
fn vouch_owned_by(identity: rustix::fs::Stat, owner: u32) -> io::Result<rustix::fs::Stat> {
    if made_by(&identity, owner) {
        Ok(identity)
    } else {
        Err(io::Error::other(
            "directory level is not a directory owned by this user",
        ))
    }
}

/// What reopening a level [`create_directories_with_hooks`] just made turned
/// up, checked against its `statat` identity; see the module docs for why.
#[cfg(unix)]
pub(super) enum ReopenedLevel {
    /// Matches `identity`; safe to descend into.
    Pinned(std::rc::Rc<std::os::fd::OwnedFd>),
    /// The reopen itself failed; recorded with no pin.
    Unpinned(io::Error),
    /// The reopen's own identity did not match, or could not be read.
    /// Either way, not provably the level `mkdirat` made -- left unrecorded.
    Unverified(io::Error),
}

/// Reopens the level just claimed at `name` in `parent` and checks it
/// against the claim's identity, shedding a pin and retrying while
/// descriptors run out; `before_reopen` is its test seam. The claim's own
/// descriptor stays open throughout, so the level's inode cannot be freed and
/// taken by a directory put at `name` since. The level is still reopened by
/// name, so the descriptor descended through is opened after `before_reopen`;
/// only when no descriptor is left to open and no pin to shed does the
/// claim's descriptor stand in, and then only while `name` still resolves to
/// it.
#[cfg(unix)]
pub(super) fn reopen_created_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    claim: Claimed,
    before_reopen: &mut dyn FnMut() -> Option<io::Error>,
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> ReopenedLevel {
    let Claimed { identity, held } = claim;
    let mut held = Some(held);
    let identity = &identity;
    let descend = loop {
        let attempt = match before_reopen() {
            Some(err) => Err(err),
            None => dest::open_created_directory_at(parent, name),
        };
        // The umask may have taken the bit the pin needs (owner search on
        // Apple, owner read on other Unix systems). The mode is restored
        // through a real descriptor from the parent, identity-checked, and
        // the pin tried once more; a mode with no owner read is refused.
        let attempt = match attempt {
            Err(source) if is_access_denied(&source) => {
                match owner_access::restore_by_name(parent, name, identity, None) {
                    Ok(()) => dest::open_created_directory_at(parent, name),
                    Err(owner_access::Refused::Replaced(replaced)) => {
                        return ReopenedLevel::Unverified(replaced);
                    }
                    Err(owner_access::Refused::OutOfDescriptors(source)) => Err(source),
                    Err(owner_access::Refused::Failed) => Err(owner_access::unrestorable()),
                }
            }
            other => other,
        };
        match attempt {
            Err(source) if is_out_of_descriptors_io(&source) => {
                if sync_and_shed_outermost_pinned_level(created, pin_from, sync_reopen) {
                    continue;
                }
                break match held.take() {
                    Some(level) if still_named(parent, name, identity) => Ok(level),
                    _ => Err(source),
                };
            }
            result => break result,
        }
    }
    .map(std::rc::Rc::new);
    let descend = match descend {
        Ok(fd) => fd,
        Err(source) => return ReopenedLevel::Unpinned(source),
    };
    match rustix::fs::fstat(&*descend) {
        Ok(reopened) if same_directory_identity(&reopened, identity) => {
            ReopenedLevel::Pinned(descend)
        }
        Ok(_) => ReopenedLevel::Unverified(io::Error::other(
            "directory level was replaced before it could be reopened",
        )),
        Err(source) => ReopenedLevel::Unverified(source.into()),
    }
}

/// Whether `name` in `parent` is the directory `identity` names now.
#[cfg(unix)]
fn still_named(parent: &std::os::fd::OwnedFd, name: &OsStr, identity: &rustix::fs::Stat) -> bool {
    rustix::fs::statat(parent, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
        .is_ok_and(|found| same_directory_identity(&found, identity))
}

/// Whether `stat` is a directory owned by `owner`: the one thing about a
/// level just made that a replacement from another user cannot match.
pub(super) fn made_by(stat: &rustix::fs::Stat, owner: u32) -> bool {
    rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::Directory
        && stat.st_uid == owner
}
