//! Making a level and reopening it, vouching that it is ours.
//!
//! `mkdirat` hands back no descriptor, so between it and the first look at
//! the name another writer of the parent can exchange the new directory for
//! one it prepared. The identity read afterwards then belongs to the
//! replacement, and every later check against that identity agrees. What
//! those checks cannot show is whose directory it is, so a level is
//! recorded and pinned only when it is also a directory owned by the
//! current user ([`made_by`]); a replacement owned by anyone else is refused
//! and left unrecorded.
//!
//! # Accepted residual
//!
//! A replacement prepared by a writer running as this same user passes that
//! check, and no stat or open through the name can tell it from the level
//! `mkdirat` made. Such a writer can already delete or rewrite everything
//! the import owns (see `take_back`), and rollback removes only a level that
//! is still empty, so the cost is an empty directory of that writer's taken
//! back. A new level is not required to be empty: a second import creating
//! beneath it may legitimately have put an entry there already.

use std::ffi::OsStr;
use std::io;

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
