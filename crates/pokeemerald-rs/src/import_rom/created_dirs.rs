//! The directory levels an import creates on the way to its destination,
//! from creation through durability or rollback.
//!
//! Only a level whose `mkdir` succeeded is recorded, and only a recorded
//! level is ever synced or taken back. On Unix a level is removed only
//! through its pinned parent, and only while the name there still resolves
//! to the identity captured at creation; any level the check cannot vouch
//! for, including one past [`pin_budget`] or whose pin was shed to free a
//! descriptor, is left standing. Off Unix each level is addressed by path.

#[cfg(unix)]
use std::ffi::OsStr;
#[cfg(not(unix))]
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::dest;

/// The levels `dir` is missing, outermost first; empty when `dir` is
/// already a directory. A component that exists but is not a directory is
/// listed too, so creation fails on it.
pub(super) fn directories_to_create(dir: &Path) -> Vec<PathBuf> {
    let mut missing = Vec::new();
    let mut current = Some(dir);
    while let Some(path) = current.filter(|path| !path.as_os_str().is_empty()) {
        if path.is_dir() {
            break;
        }
        missing.push(path.to_path_buf());
        current = path.parent();
    }
    missing.reverse();
    missing
}

/// A directory level [`create_directories`] made, and what
/// [`sync_created_directories`] and [`undo_created_directories`] use to find
/// it again.
#[derive(Debug)]
#[cfg(unix)]
pub(super) struct CreatedDirectory {
    /// Where this level was created; production never re-resolves it.
    #[allow(
        dead_code,
        reason = "read only by tests::created_paths, not by production Unix code"
    )]
    path: PathBuf,
    /// The directory this level was created in, shared with the level
    /// above's [`CreatedDirectory::own`]; `None` for an unpinned level.
    parent: Option<std::rc::Rc<std::os::fd::OwnedFd>>,
    /// This level's own basename inside `parent`.
    name: std::ffi::OsString,
    /// This level's device and inode, captured right after creation, in
    /// the platform-native [`rustix::fs::Stat`] since `st_dev` differs in
    /// width and signedness across Unixes.
    identity: rustix::fs::Stat,
    /// This level itself, held open so its inode cannot be reused before
    /// cleanup; `None` for an unpinned level, which cleanup leaves standing.
    own: Option<std::rc::Rc<std::os::fd::OwnedFd>>,
}

/// [`CreatedDirectory`]'s off-Unix shape: see its own docs for why.
#[derive(Debug)]
#[cfg(not(unix))]
pub(super) struct CreatedDirectory {
    /// Where this level was created.
    path: PathBuf,
}

/// Whether `a` and `b` name the same file: the same device and inode,
/// `create_directories_with_hooks`'s and [`undo_created_directories`]'s
/// shared test for "is this still the directory this run made".
#[cfg(unix)]
fn same_directory_identity(a: &rustix::fs::Stat, b: &rustix::fs::Stat) -> bool {
    a.st_dev == b.st_dev && a.st_ino == b.st_ino
}

/// The most levels ever pinned at once; [`pin_budget`] may be lower.
#[cfg(unix)]
const MAX_PINNED_LEVELS: usize = 32;

/// Descriptors the import needs outside the pinned chain: stdio, the ROM,
/// the traversal handle, the destination and temporary-pack opens, and
/// margin. A guess, not a measurement; [`open_shedding_pins`] covers the
/// shortfall.
#[cfg(unix)]
const RESERVED_DESCRIPTORS: usize = 10;

/// The most levels this run should try to keep pinned at once, given the
/// process's own soft `RLIMIT_NOFILE` right now: never more than
/// [`MAX_PINNED_LEVELS`], and never so many that [`RESERVED_DESCRIPTORS`]
/// worth of headroom would not survive them.
#[cfg(unix)]
fn pin_budget() -> usize {
    pin_budget_for(rustix::process::getrlimit(rustix::process::Resource::Nofile).current)
}

/// [`pin_budget`]'s formula over a soft limit, `None` for no limit.
#[cfg(unix)]
fn pin_budget_for(soft: Option<u64>) -> usize {
    match soft {
        Some(soft) => {
            let soft = usize::try_from(soft).unwrap_or(usize::MAX);
            MAX_PINNED_LEVELS.min(soft.saturating_sub(RESERVED_DESCRIPTORS))
        }
        None => MAX_PINNED_LEVELS,
    }
}

/// Whether `error` is the OS refusing one more descriptor (`EMFILE` or
/// `ENFILE`) -- the two [`shed_outermost_pinned_level`] exists to answer by
/// giving one back rather than failing the whole create.
#[cfg(unix)]
fn is_out_of_descriptors(error: rustix::io::Errno) -> bool {
    error == rustix::io::Errno::MFILE || error == rustix::io::Errno::NFILE
}

/// [`is_out_of_descriptors`] for an already-converted [`io::Error`], the
/// shape a reopen's own seam hands back.
#[cfg(unix)]
fn is_out_of_descriptors_io(error: &io::Error) -> bool {
    rustix::io::Errno::from_io_error(error).is_some_and(is_out_of_descriptors)
}

/// Releases the outermost level at or after the `pin_from` cursor that
/// still holds a pin, advancing the cursor past it; `false` when none is
/// left. Adjacent levels share a descriptor, so a release may take a
/// second call to close it.
#[cfg(unix)]
fn shed_outermost_pinned_level(created: &mut [CreatedDirectory], pin_from: &mut usize) -> bool {
    while let Some(level) = created.get_mut(*pin_from) {
        if level.own.is_none() && level.parent.is_none() {
            *pin_from += 1;
            continue;
        }
        level.own = None;
        level.parent = None;
        *pin_from += 1;
        return true;
    }
    false
}

/// Creates the levels `dir` is missing, outermost first, one level at a
/// time so that only a create this run won is recorded; a level that
/// already exists belongs to someone else. A failure hands back the levels
/// made before it. On Unix only the first existing ancestor is resolved by
/// path; every level below it is reached through its parent's descriptor
/// (`dest` owns the per-platform open flags).
#[cfg(unix)]
pub(super) fn create_directories(
    dir: &Path,
) -> Result<Vec<CreatedDirectory>, (Vec<CreatedDirectory>, io::Error)> {
    create_directories_with_hooks(dir, pin_budget(), &mut || {}, &mut || None, &mut || None)
}

/// What reopening a level [`create_directories_with_hooks`] just made turned
/// up, checked against its `statat` identity; see the module docs for why.
#[cfg(unix)]
enum ReopenedLevel {
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
fn reopen_created_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    identity: &rustix::fs::Stat,
    before_reopen: &mut dyn FnMut() -> Option<io::Error>,
) -> ReopenedLevel {
    let descend = loop {
        let attempt = match before_reopen() {
            Some(err) => Err(err),
            None => dest::open_created_directory_at(parent, name).map(std::rc::Rc::new),
        };
        match attempt {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && shed_outermost_pinned_level(created, pin_from) => {}
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

/// Records a level just `mkdirat`ed at `name` in `parent_fd`: captures its
/// identity, reopens it, and answers with the descriptor the next level
/// descends through.
#[cfg(unix)]
fn create_missing_level(
    created: &mut Vec<CreatedDirectory>,
    pin_from: &mut usize,
    pin_limit: usize,
    parent_fd: std::rc::Rc<std::os::fd::OwnedFd>,
    path: PathBuf,
    name: std::ffi::OsString,
    before_reopen: &mut dyn FnMut() -> Option<io::Error>,
) -> Result<std::rc::Rc<std::os::fd::OwnedFd>, io::Error> {
    // The identity is captured before the reopen so a failed reopen cannot
    // un-record a level that was made; anything but a directory at the name
    // is a swap and is refused.
    let found = loop {
        match rustix::fs::statat(&*parent_fd, &name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW) {
            Err(err)
                if is_out_of_descriptors(err) && shed_outermost_pinned_level(created, pin_from) => {
            }
            result => break result,
        }
    };
    let identity = match found {
        Ok(found)
            if rustix::fs::FileType::from_raw_mode(found.st_mode)
                == rustix::fs::FileType::Directory =>
        {
            found
        }
        Ok(_) => {
            return Err(io::Error::other(
                "directory level was replaced during creation",
            ));
        }
        Err(source) => return Err(source.into()),
    };
    match reopen_created_level(
        created,
        pin_from,
        &parent_fd,
        &name,
        &identity,
        before_reopen,
    ) {
        ReopenedLevel::Pinned(descend) => {
            created.push(CreatedDirectory {
                path,
                parent: Some(parent_fd),
                name,
                identity,
                own: Some(std::rc::Rc::clone(&descend)),
            });
            // This level just made the pinned window one longer than
            // `pin_limit`; shed the outermost to bring it back -- one call
            // always suffices, since nothing but this push could have grown
            // the window since the last time it was checked.
            if created.len() - *pin_from > pin_limit {
                shed_outermost_pinned_level(created, pin_from);
            }
            Ok(descend)
        }
        // A real reopen failure, not a budget choice: kept on the same
        // terms as before `MAX_PINNED_LEVELS` existed, so cleanup still
        // stops here rather than past it.
        ReopenedLevel::Unpinned(source) => {
            created.push(CreatedDirectory {
                path,
                parent: Some(parent_fd),
                name,
                identity,
                own: None,
            });
            Err(source)
        }
        ReopenedLevel::Unverified(source) => Err(source),
    }
}

/// Opens `name` inside `parent` for a `..` or an already-existing level,
/// shedding a pin and retrying while descriptors run out; `before_open` is
/// its test seam.
#[cfg(unix)]
fn open_existing_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    parent: &std::rc::Rc<std::os::fd::OwnedFd>,
    name: &OsStr,
    before_open: &mut dyn FnMut() -> Option<io::Error>,
) -> io::Result<std::os::fd::OwnedFd> {
    loop {
        let attempt = match before_open() {
            Some(err) => Err(err),
            None => dest::open_directory_at(parent, name),
        };
        match attempt {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && shed_outermost_pinned_level(created, pin_from) => {}
            result => return result,
        }
    }
}

/// [`create_directories`]'s body on Unix. `before_dotdot` runs before a
/// `..` level is resolved, `before_reopen` and `before_open` may replace
/// the reopen of a created level or the open of an existing one with a
/// failure, and `pin_budget` caps the pinned levels; production passes
/// no-ops and [`pin_budget`].
#[cfg(unix)]
fn create_directories_with_hooks(
    dir: &Path,
    pin_limit: usize,
    before_dotdot: &mut dyn FnMut(),
    before_reopen: &mut dyn FnMut() -> Option<io::Error>,
    before_open: &mut dyn FnMut() -> Option<io::Error>,
) -> Result<Vec<CreatedDirectory>, (Vec<CreatedDirectory>, io::Error)> {
    let levels = directories_to_create(dir);
    let Some(first) = levels.first() else {
        return Ok(Vec::new());
    };
    // A bare relative name has no parent, and `""` is not a directory any OS
    // accepts, so it is the current directory — the same rule
    // [`super::pack_directory`] resolves the destination with.
    let start = first
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut parent_fd = match dest::open_traversal_directory(start) {
        Ok(fd) => std::rc::Rc::new(fd),
        Err(source) => return Err((Vec::new(), source)),
    };

    // Pins are counted against `created`, not `levels`: a `..` creates
    // nothing.
    let mut pin_from = 0;
    let mut created = Vec::new();
    for level in levels {
        // A `..` level (`file_name` is `None`) resolves relative to the
        // descent, never by reopening the full path.
        let Some(name) = level.file_name().map(std::ffi::OsStr::to_os_string) else {
            before_dotdot();
            parent_fd = match open_existing_level(
                &mut created,
                &mut pin_from,
                &parent_fd,
                OsStr::new(".."),
                before_open,
            ) {
                Ok(fd) => std::rc::Rc::new(fd),
                Err(source) => return Err((created, source)),
            };
            continue;
        };
        let mkdir_result = loop {
            match rustix::fs::mkdirat(
                &*parent_fd,
                &name,
                rustix::fs::Mode::RWXU | rustix::fs::Mode::RWXG | rustix::fs::Mode::RWXO,
            ) {
                Err(err)
                    if is_out_of_descriptors(err)
                        && shed_outermost_pinned_level(&mut created, &mut pin_from) => {}
                result => break result,
            }
        };
        match mkdir_result {
            Ok(()) => {
                match create_missing_level(
                    &mut created,
                    &mut pin_from,
                    pin_limit,
                    parent_fd,
                    level,
                    name,
                    before_reopen,
                ) {
                    Ok(descend) => parent_fd = descend,
                    Err(source) => return Err((created, source)),
                }
            }
            // An existing directory is no failure and not this run's to
            // record; a final symlink to one counts, as for `Path::is_dir`.
            Err(mkdir_err) => {
                let already_a_directory =
                    rustix::fs::statat(&*parent_fd, &name, rustix::fs::AtFlags::empty()).is_ok_and(
                        |stat| {
                            rustix::fs::FileType::from_raw_mode(stat.st_mode)
                                == rustix::fs::FileType::Directory
                        },
                    );
                if already_a_directory {
                    match open_existing_level(
                        &mut created,
                        &mut pin_from,
                        &parent_fd,
                        &name,
                        before_open,
                    ) {
                        Ok(fd) => parent_fd = std::rc::Rc::new(fd),
                        // The open's own failure is the real diagnosis now;
                        // the `mkdir` collision only ever proved the name
                        // was taken, which it still is.
                        Err(source) => return Err((created, source)),
                    }
                } else {
                    return Err((created, mkdir_err.into()));
                }
            }
        }
    }
    Ok(created)
}

/// [`create_directories`]'s off-Unix arm: no descriptor to pin, so each
/// level is [`std::fs::create_dir`] addressed by path.
#[cfg(not(unix))]
pub(super) fn create_directories(
    dir: &Path,
) -> Result<Vec<CreatedDirectory>, (Vec<CreatedDirectory>, io::Error)> {
    let mut created = Vec::new();
    for level in directories_to_create(dir) {
        match fs::create_dir(&level) {
            Ok(()) => created.push(CreatedDirectory { path: level }),
            // A level that stands as a directory now is no failure, whoever
            // made it — `create_dir_all`'s own rule. It is simply not this
            // run's to record.
            Err(_) if level.is_dir() => {}
            Err(source) => return Err((created, source)),
        }
    }
    Ok(created)
}

/// Syncs the parent of each created level, outermost first, so a crash
/// leaves a prefix of the chain. A reopen refused for want of a descriptor
/// sheds an already-synced level's pin, else the innermost level's own,
/// costing rollback but never a sync. Best effort: durability never fails
/// a finished import.
#[cfg(unix)]
pub(super) fn sync_created_directories(created: &mut [CreatedDirectory]) {
    sync_created_directories_with(created, &mut dest::reopen_for_sync);
}

/// [`sync_created_directories`] with its reopen injected, so a test can
/// refuse one for want of a descriptor.
#[cfg(unix)]
fn sync_created_directories_with(
    created: &mut [CreatedDirectory],
    reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) {
    for index in 0..created.len() {
        let Some(parent) = created[index].parent.clone() else {
            continue;
        };
        let mut synced_from = 0;
        loop {
            match reopen(&parent) {
                Ok(real) => {
                    let _ = rustix::fs::fsync(&real);
                    break;
                }
                Err(source) if is_out_of_descriptors_io(&source) => {
                    let (synced, unsynced) = created.split_at_mut(index);
                    let released = shed_outermost_pinned_level(synced, &mut synced_from)
                        || unsynced
                            .last_mut()
                            .and_then(|level| level.own.take())
                            .is_some();
                    if !released {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }
}

/// Runs `open`, shedding the outermost pinned level and retrying while it
/// is refused for want of a descriptor; for the opens an import makes
/// after creation.
#[cfg(unix)]
pub(super) fn open_shedding_pins<T>(
    created: &mut [CreatedDirectory],
    mut open: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    let mut pin_from = 0;
    loop {
        match open() {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && shed_outermost_pinned_level(created, &mut pin_from) => {}
            result => return result,
        }
    }
}

/// [`open_shedding_pins`]'s off-Unix arm: nothing is pinned, so `open` runs
/// once.
#[cfg(not(unix))]
pub(super) fn open_shedding_pins<T>(
    _created: &mut [CreatedDirectory],
    mut open: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    open()
}

/// [`sync_created_directories`]'s off-Unix arm, through
/// [`dest::sync_directory`] since no descriptor is pinned to sync directly.
#[cfg(not(unix))]
pub(super) fn sync_created_directories(created: &mut [CreatedDirectory]) {
    for dir in created {
        // A bare relative name has no parent, and `""` is not a directory
        // any OS accepts, so it is the current directory — the same rule
        // [`super::pack_directory`] resolves the destination with.
        let parent = dir
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        dest::sync_directory(parent);
    }
}

/// Removes, innermost first, each created level that is still empty and
/// still resolves in its pinned parent to the identity captured at
/// creation, so a failed import leaves no chain of empty directories. The
/// first refusal ends the walk; an unpinned level is skipped.
#[cfg(unix)]
pub(super) fn undo_created_directories(created: &[CreatedDirectory]) {
    for dir in created.iter().rev() {
        let Some(parent) = dir.parent.as_deref() else {
            continue;
        };
        match rustix::fs::statat(parent, &dir.name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat)
                if same_directory_identity(&stat, &dir.identity)
                    && dir.own.as_ref().is_some_and(|own| {
                        rustix::fs::fstat(&**own)
                            .is_ok_and(|held| same_directory_identity(&stat, &held))
                    }) =>
            {
                match rustix::fs::unlinkat(parent, &dir.name, rustix::fs::AtFlags::REMOVEDIR) {
                    Ok(()) => {}
                    // Already gone is already taken care of.
                    Err(err) if err == rustix::io::Errno::NOENT => {}
                    // Refused for real (it holds something): the levels
                    // above hold it too and are not this run's to take back.
                    Err(_) => break,
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
}

/// [`undo_created_directories`]'s off-Unix arm: no identity to check, so
/// each recorded level is removed by path.
#[cfg(not(unix))]
pub(super) fn undo_created_directories(created: &[CreatedDirectory]) {
    for dir in created.iter().rev() {
        if fs::remove_dir(&dir.path).is_err() {
            // A missing or non-directory level was never made; a directory
            // that refuses removal holds something, and so do the levels
            // above it.
            match fs::symlink_metadata(&dir.path) {
                Ok(meta) if meta.is_dir() => break,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests;
