//! Removing a private directory whose claim did not finish, without ever
//! unlinking a name that was only checked a moment ago, and saying where one
//! that could not be removed is left.

use std::ffi::OsStr;
use std::io;

use super::super::same_directory_identity;
use super::{no_replace_unsupported, rename_no_replace};

/// What [`discard_private`] left behind.
pub(in super::super) enum Discard {
    /// The directory this run made is gone.
    Removed,
    /// This run's directory could not be removed and stands at `at`, a name
    /// in the parent: `error` says why. It was empty when made and may not be
    /// now, so it is not to be called empty or safe to delete.
    Kept {
        at: std::ffi::OsString,
        error: io::Error,
    },
    /// What stands at `at` was not found to be the directory this run made
    /// (someone replaced it), so it was not touched beyond being put back.
    Foreign { at: std::ffi::OsString },
    /// Nothing stood at `at` when cleanup reached it: someone moved this
    /// run's directory away, so nothing was removed and where it is now is
    /// unknown.
    Gone { at: std::ffi::OsString },
}

/// Removes the directory this run made as `private` in `parent`, which
/// `held` is open on, without ever unlinking a name that was only checked a
/// moment ago: the entry is first renamed (no-replace) to a fresh
/// unguessable name, and only what is found there to be `held` is removed.
/// Anything else was put at `private` by someone else after it was vouched
/// for; it is renamed back if `private` is still free, else left where it
/// is. `aside` is that fresh name, chosen by the caller so no descriptor is
/// needed here. A directory that is not removed is reported with the name it
/// actually stands at (`private` or `aside`, whichever the last rename left
/// it under) and, where this run's own directory could not be removed, the
/// error: unlinking it can fail because an entry appeared in it after it was
/// moved aside. `before_move` is the test seam that runs just before the
/// entry is moved aside.
pub(in super::super) fn discard_private(
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    private: &OsStr,
    aside: &OsStr,
    held: &std::os::fd::OwnedFd,
    before_move: &mut dyn FnMut(),
) -> Discard {
    discard_private_with(parent, private, aside, held, before_move, &mut || {})
}

/// [`discard_private`] with `before_unlink`, the test seam that runs once the
/// entry moved aside was found to be `held` and just before it is unlinked:
/// the gap a check of `private` followed by an unlink of `private` would
/// leave open.
pub(in super::super) fn discard_private_with(
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    private: &OsStr,
    aside: &OsStr,
    held: &std::os::fd::OwnedFd,
    before_move: &mut dyn FnMut(),
    before_unlink: &mut dyn FnMut(),
) -> Discard {
    before_move();
    match rename_no_replace(parent, private, parent, aside) {
        Ok(()) => {}
        Err(errno) if no_replace_unsupported(errno) => {
            // No rename that refuses to replace here, so the name is checked
            // and then unlinked, which a same-user writer can come between;
            // all it can cost is an empty directory of its own.
            let found =
                rustix::fs::statat(&**parent, private, rustix::fs::AtFlags::SYMLINK_NOFOLLOW);
            if matches!(found, Err(rustix::io::Errno::NOENT)) {
                return Discard::Gone {
                    at: private.to_owned(),
                };
            }
            let unchanged = found
                .ok()
                .zip(rustix::fs::fstat(held).ok())
                .is_some_and(|(found, ours)| same_directory_identity(&found, &ours));
            if !unchanged {
                return Discard::Foreign {
                    at: private.to_owned(),
                };
            }
            return match rustix::fs::unlinkat(&**parent, private, rustix::fs::AtFlags::REMOVEDIR) {
                Ok(()) => Discard::Removed,
                Err(errno) => Discard::Kept {
                    at: private.to_owned(),
                    error: errno.into(),
                },
            };
        }
        Err(rustix::io::Errno::NOENT) => {
            return Discard::Gone {
                at: private.to_owned(),
            }
        }
        // Nothing was moved: it still stands at `private`.
        Err(errno) => {
            return Discard::Kept {
                at: private.to_owned(),
                error: errno.into(),
            }
        }
    }
    let ours = match (
        rustix::fs::fstat(held),
        rustix::fs::statat(&**parent, aside, rustix::fs::AtFlags::SYMLINK_NOFOLLOW),
    ) {
        (Ok(ours), Ok(found)) => same_directory_identity(&found, &ours),
        _ => false,
    };
    if ours {
        before_unlink();
        match rustix::fs::unlinkat(&**parent, aside, rustix::fs::AtFlags::REMOVEDIR) {
            Ok(()) => Discard::Removed,
            // It stands at `aside`, not at `private`.
            Err(errno) => Discard::Kept {
                at: aside.to_owned(),
                error: errno.into(),
            },
        }
    } else {
        let at = match rename_no_replace(parent, aside, parent, private) {
            Ok(()) => private,
            Err(_) => aside,
        };
        Discard::Foreign { at: at.to_owned() }
    }
}

/// Says on stderr where a private directory that [`discard_private`] did not
/// remove now stands, as [`left_standing_warning`] words it.
pub(super) fn report_left_standing(outcome: &Discard, name: &OsStr) {
    if let Some(warning) = left_standing_warning(outcome, name) {
        eprintln!("{warning}");
    }
}

/// The warning for a private directory that [`discard_private`] did not
/// remove: the pathname the user can act on, relative to the level being
/// made (`name`). `None` once it is gone.
pub(in super::super) fn left_standing_warning(outcome: &Discard, name: &OsStr) -> Option<String> {
    let name = rom_import::OneLinePath(std::path::Path::new(name));
    match outcome {
        Discard::Removed => None,
        Discard::Kept { at, error } => Some(format!(
            "warning: could not remove the directory {} made while creating {name}: \
             {error}; it may no longer be empty, so check it before deleting",
            rom_import::OneLinePath(std::path::Path::new(at))
        )),
        Discard::Foreign { at } => Some(format!(
            "warning: {} was made while creating {name} but was replaced and was left \
             untouched; it is not this import's to delete",
            rom_import::OneLinePath(std::path::Path::new(at))
        )),
        Discard::Gone { at } => Some(format!(
            "warning: the directory {} made while creating {name} was moved away \
             before it could be removed, and nothing was removed",
            rom_import::OneLinePath(std::path::Path::new(at))
        )),
    }
}
