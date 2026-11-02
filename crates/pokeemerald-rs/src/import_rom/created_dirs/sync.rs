//! Making the created levels durable: syncing each level's entry in its
//! parent, including parents whose pins were shed under descriptor
//! pressure, and opening through the pins while shedding them.

use std::io;
#[cfg(not(unix))]
use std::path::Path;

use super::{dest, CreatedDirectory};
#[cfg(unix)]
use super::{is_out_of_descriptors_io, same_directory_identity, shed_outermost_pinned_level};

/// Syncs the parents that acquiring the destination shed before the sync
/// could reach them, by walking up from the acquired `dest` handle; with the
/// table full it releases the levels' own pins for the slot, which `dest`
/// makes redundant.
#[cfg(unix)]
pub(in crate::import_rom) fn sync_shed_parents(
    created: &mut [CreatedDirectory],
    dest: &dest::Dest,
) {
    sync_shed_parents_with(created, dest, &mut |fd| rustix::fs::fsync(fd).is_ok());
}

/// [`sync_shed_parents`] with the `fsync` injected, so a test can count it
/// or fail it; a failed one leaves its entries unsynced.
///
/// The climb goes up the destination's own ancestry and syncs each
/// directory a shed level's entry is in, matched by the identity captured
/// at creation: neither a level another creator made in between nor a
/// level off this ancestry (a `b` in `a/b/../c`) is assumed to sit one
/// `..` above the next record. Under descriptor pressure it syncs the
/// directory it stands in for the pinned levels whose entries are there,
/// which frees their parent pins to release alongside the own pins.
#[cfg(unix)]
pub(super) fn sync_shed_parents_with(
    created: &mut [CreatedDirectory],
    dest: &dest::Dest,
    fsync: &mut dyn FnMut(&std::os::fd::OwnedFd) -> bool,
) {
    let mut synced: Vec<bool> = created.iter().map(|level| level.entry_synced).collect();
    if !shed_parent_pending(created, &synced) {
        return;
    }
    let Ok(mut here) = dest.identity() else {
        return;
    };
    let mut above: Option<std::os::fd::OwnedFd> = None;
    loop {
        sync_entries_in(
            created,
            &mut synced,
            &here,
            true,
            above.as_ref().unwrap_or_else(|| dest.handle()),
            fsync,
        );
        if !shed_parent_pending(created, &synced) {
            return;
        }
        let parent = loop {
            let opened = match &above {
                None => dest.open_parent_for_sync(),
                Some(fd) => dest::open_parent_for_sync(fd),
            };
            match opened {
                Ok(fd) => break fd,
                Err(source) if is_out_of_descriptors_io(&source) => {
                    sync_entries_in(
                        created,
                        &mut synced,
                        &here,
                        false,
                        above.as_ref().unwrap_or_else(|| dest.handle()),
                        fsync,
                    );
                    if !release_a_descriptor(created, &synced) {
                        return;
                    }
                }
                Err(_) => return,
            }
        };
        let Ok(identity) = rustix::fs::fstat(&parent) else {
            return;
        };
        // `..` of the root is the root: what is left is off this ancestry.
        if same_directory_identity(&identity, &here) {
            return;
        }
        here = identity;
        above = Some(parent);
    }
}

/// Whether a shed level's parent entry still waits on [`sync_shed_parents`].
#[cfg(unix)]
fn shed_parent_pending(created: &[CreatedDirectory], synced: &[bool]) -> bool {
    created
        .iter()
        .zip(synced)
        .any(|(level, &synced)| level.parent.is_none() && !synced)
}

/// [`mark_entries_in`], then `fsync`s `directory` when any were marked; a
/// failed `fsync` takes the marks back, so the entries stay unsynced.
#[cfg(unix)]
fn sync_entries_in(
    created: &[CreatedDirectory],
    synced: &mut [bool],
    here: &rustix::fs::Stat,
    shed_only: bool,
    directory: &std::os::fd::OwnedFd,
    fsync: &mut dyn FnMut(&std::os::fd::OwnedFd) -> bool,
) {
    let before = synced.to_vec();
    if mark_entries_in(created, synced, here, shed_only) && !fsync(directory) {
        synced.copy_from_slice(&before);
    }
}

/// Marks the unsynced levels whose entries are in `here` as synced and
/// answers whether `here` needs its `fsync` for them; with `shed_only`,
/// only when one of them was shed, leaving pinned levels to the later pass.
#[cfg(unix)]
fn mark_entries_in(
    created: &[CreatedDirectory],
    synced: &mut [bool],
    here: &rustix::fs::Stat,
    shed_only: bool,
) -> bool {
    let held = |(level, synced): &(&CreatedDirectory, &mut bool)| {
        !**synced && same_directory_identity(&level.parent_identity, here)
    };
    let mut entries = created.iter().zip(synced.iter_mut()).filter(held);
    if shed_only && !entries.any(|(level, _)| level.parent.is_none()) {
        return false;
    }
    let mut any = false;
    for (_, synced) in created.iter().zip(synced.iter_mut()).filter(held) {
        *synced = true;
        any = true;
    }
    any
}

/// Closes one descriptor the levels hold, innermost first: a synced level's
/// parent pin, else a level's own pin no other level shares. Answers
/// whether one closed.
#[cfg(unix)]
fn release_a_descriptor(created: &mut [CreatedDirectory], synced: &[bool]) -> bool {
    for (level, _) in created
        .iter_mut()
        .zip(synced)
        .rev()
        .filter(|(_, &synced)| synced)
    {
        if level
            .parent
            .take()
            .and_then(std::rc::Rc::into_inner)
            .is_some()
        {
            return true;
        }
    }
    release_unshared_own(created)
}

/// Syncs the parent of each created level, outermost first, so a crash
/// leaves a prefix of the chain. A reopen refused for want of a descriptor
/// sheds an already-synced level's pin, else the innermost level's own,
/// costing rollback but never a sync. Best effort: durability never fails
/// a finished import.
#[cfg(unix)]
pub(in crate::import_rom) fn sync_created_directories(created: &mut [CreatedDirectory]) {
    sync_created_directories_with(created, &mut dest::reopen_for_sync);
}

/// [`sync_created_directories`] with its reopen injected, so a test can
/// refuse one for want of a descriptor.
#[cfg(unix)]
pub(super) fn sync_created_directories_with(
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
                    // An entry an earlier sync made durable stays durable;
                    // a failed redundant sync must not take that back.
                    created[index].entry_synced |= rustix::fs::fsync(&real).is_ok();
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
pub(in crate::import_rom) fn open_shedding_pins<T>(
    created: &mut [CreatedDirectory],
    open: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    open_shedding_pins_with(created, open, &mut dest::reopen_for_sync)
}

/// [`open_shedding_pins`] with the parent reopen injected, so a test can
/// see which parents are synced before their pins go.
#[cfg(unix)]
pub(super) fn open_shedding_pins_with<T>(
    created: &mut [CreatedDirectory],
    mut open: impl FnMut() -> io::Result<T>,
    reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> io::Result<T> {
    let mut pin_from = 0;
    loop {
        match open() {
            Err(source)
                if is_out_of_descriptors_io(&source)
                    && sync_and_shed_outermost_pinned_level(created, &mut pin_from, reopen) => {}
            result => return result,
        }
    }
}

/// [`shed_outermost_pinned_level`], syncing that level's entry first while
/// its parent pin still reaches it: once shed, the parent may be off the
/// destination's ancestry (behind a symlink or a `..`), where
/// [`sync_shed_parents`] cannot climb to it. The pin is `O_PATH`-like, so
/// the sync reopens it; with the table full, an own pin no other level
/// shares is released for that slot. Shedding stays outermost first, so a
/// crash still leaves a synced prefix; a sync that cannot be made leaves
/// the level to [`sync_shed_parents`] as before.
#[cfg(unix)]
pub(super) fn sync_and_shed_outermost_pinned_level(
    created: &mut [CreatedDirectory],
    pin_from: &mut usize,
    reopen: &mut dyn FnMut(&std::os::fd::OwnedFd) -> io::Result<std::os::fd::OwnedFd>,
) -> bool {
    let outermost = created
        .iter()
        .enumerate()
        .skip(*pin_from)
        .find(|(_, level)| level.own.is_some() || level.parent.is_some())
        .map(|(index, _)| index);
    if let Some(index) = outermost {
        if let Some(parent) = created[index]
            .parent
            .clone()
            .filter(|_| !created[index].entry_synced)
        {
            loop {
                match reopen(&parent) {
                    Ok(real) => {
                        created[index].entry_synced = rustix::fs::fsync(&real).is_ok();
                        break;
                    }
                    Err(source)
                        if is_out_of_descriptors_io(&source) && release_unshared_own(created) => {}
                    Err(_) => break,
                }
            }
        }
    }
    shed_outermost_pinned_level(created, pin_from)
}

/// Closes the innermost own pin no other level shares; answers whether one
/// closed.
#[cfg(unix)]
fn release_unshared_own(created: &mut [CreatedDirectory]) -> bool {
    for level in created.iter_mut().rev() {
        if level
            .own
            .as_ref()
            .is_some_and(|own| std::rc::Rc::strong_count(own) == 1)
        {
            level.own = None;
            return true;
        }
    }
    false
}

/// [`open_shedding_pins`]'s off-Unix arm: nothing is pinned, so `open` runs
/// once.
#[cfg(not(unix))]
pub(in crate::import_rom) fn open_shedding_pins<T>(
    _created: &mut [CreatedDirectory],
    mut open: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    open()
}

/// [`sync_created_directories`]'s off-Unix arm, through
/// [`dest::sync_directory`] since no descriptor is pinned to sync directly.
#[cfg(not(unix))]
pub(in crate::import_rom) fn sync_created_directories(created: &mut [CreatedDirectory]) {
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
