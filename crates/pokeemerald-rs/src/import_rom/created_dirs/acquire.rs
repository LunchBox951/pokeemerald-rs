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
//! reopen that follows compares the name against that identity, so a swap
//! after the claim is refused too. Elsewhere a level is made at its final
//! name and a same-user replacement in the gap cannot be told apart.
//!
//! # Accepted residuals
//!
//! A writer of the parent running as this user who lists it fast enough to
//! find the private name before it is opened can still exchange it. Such a
//! writer can already delete or rewrite everything the import owns (see
//! `take_back`), and a level is refused unless it is a directory of this
//! user's. A swap after the reopen is `take_back`'s to withstand. A new
//! level is not required to be empty: a second import creating beneath it
//! may legitimately have put an entry there already.

use std::ffi::OsStr;
use std::io;

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
use super::take_back;
use super::{
    dest, is_out_of_descriptors, is_out_of_descriptors_io, same_directory_identity,
    sync_and_shed_outermost_pinned_level, CreatedDirectory,
};

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

/// Makes the level `name` in `parent` and answers with the identity of the
/// directory this call made, owned by `owner`. `after_mkdir` runs once the
/// directory is made and before anything looks at it, where a test replaces
/// it.
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
/// there is no such rename, so the directory is made at `name` and stat'ed there, which a same-user writer can race.
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
) -> Result<rustix::fs::Stat, ClaimError> {
    let private = fresh_private_name(created, pin_from, sync_reopen)?;
    // Chosen now, while a descriptor can still be shed for it: cleanup runs
    // with the verified directory held open and must not fail for lack of one.
    let aside = fresh_private_name(created, pin_from, sync_reopen)?;
    mkdir_level(created, pin_from, parent, &private, sync_reopen)
        .map_err(|errno| ClaimError::Failed(errno.into()))?;
    after_mkdir();
    let (identity, held) = identity_of(created, pin_from, parent, &private, owner, sync_reopen)
        .map_err(ClaimError::Failed)?;
    match take_back::rename_without_replacing(&**parent, &private, &**parent, name) {
        Ok(()) => Ok(identity),
        Err(errno) => {
            if !discard_private(parent, &private, &aside, &held, &mut || {}) {
                eprintln!(
                    "warning: could not remove the empty directory {} made while \
                     creating {}; it is safe to delete",
                    std::path::Path::new(&private).display(),
                    std::path::Path::new(name).display()
                );
            }
            Err(ClaimError::from_errno(errno))
        }
    }
}

/// Removes the empty directory this run made as `private` in `parent`,
/// which `held` is open on, without ever unlinking a name that was only
/// checked a moment ago: the entry is first renamed (no-replace) to a fresh
/// unguessable name, and only what is found there to be `held` is removed.
/// Anything else was put at `private` by someone else after it was vouched
/// for; it is renamed back if `private` is still free, else left where it
/// is. `aside` is that fresh name, chosen by the caller so no descriptor is
/// needed here. Answers whether the directory is gone; a directory left
/// standing is empty and harmless, and the caller says so. `before_move` is
/// the test seam that runs just before the entry is moved aside.
#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
pub(super) fn discard_private(
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    private: &OsStr,
    aside: &OsStr,
    held: &std::os::fd::OwnedFd,
    before_move: &mut dyn FnMut(),
) -> bool {
    before_move();
    if take_back::rename_without_replacing(&**parent, private, &**parent, aside).is_err() {
        return false;
    }
    let ours = match (
        rustix::fs::fstat(held),
        rustix::fs::statat(&**parent, aside, rustix::fs::AtFlags::SYMLINK_NOFOLLOW),
    ) {
        (Ok(ours), Ok(found)) => same_directory_identity(&found, &ours),
        _ => false,
    };
    if ours {
        rustix::fs::unlinkat(&**parent, aside, rustix::fs::AtFlags::REMOVEDIR).is_ok()
    } else {
        let _ = take_back::rename_without_replacing(&**parent, aside, &**parent, private);
        false
    }
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

/// [`claim_level`] where there is no no-replace rename: the directory is made
/// at `name` itself.
#[cfg(all(
    unix,
    not(any(target_os = "linux", target_os = "android", target_vendor = "apple"))
))]
pub(super) fn claim_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    owner: u32,
    after_mkdir: &mut dyn FnMut(),
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> Result<rustix::fs::Stat, ClaimError> {
    mkdir_level(created, pin_from, parent, name, sync_reopen).map_err(ClaimError::from_errno)?;
    after_mkdir();
    let found = loop {
        match rustix::fs::statat(&**parent, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW) {
            Err(err)
                if is_out_of_descriptors(err)
                    && sync_and_shed_outermost_pinned_level(created, pin_from, sync_reopen) => {}
            result => break result.map_err(|errno| ClaimError::Failed(errno.into()))?,
        }
    };
    vouch_owned_by(found, owner).map_err(ClaimError::Failed)
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
        match dest::open_created_directory_at(parent, name) {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && sync_and_shed_outermost_pinned_level(created, pin_from, sync_reopen) => {}
            result => break result?,
        }
    };
    let identity = vouch_owned_by(rustix::fs::fstat(&held)?, owner)?;
    Ok((identity, held))
}

/// `identity` if it is a directory of `owner`'s, else the refusal.
#[cfg(unix)]
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

/// Reopens the level just created at `name` in `parent` and checks it
/// against `identity`, shedding a pin and retrying while descriptors run
/// out; `before_reopen` is its test seam.
#[cfg(unix)]
pub(super) fn reopen_created_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    identity: &rustix::fs::Stat,
    before_reopen: &mut dyn FnMut() -> Option<io::Error>,
    sync_reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> ReopenedLevel {
    let descend = loop {
        let attempt = match before_reopen() {
            Some(err) => Err(err),
            None => dest::open_created_directory_at(parent, name).map(std::rc::Rc::new),
        };
        match attempt {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && sync_and_shed_outermost_pinned_level(created, pin_from, sync_reopen) => {}
            result => break result,
        }
    };
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

/// Whether `stat` is a directory owned by `owner`: the one thing about a
/// level just made that a replacement from another user cannot match.
pub(super) fn made_by(stat: &rustix::fs::Stat, owner: u32) -> bool {
    rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::Directory
        && stat.st_uid == owner
}
