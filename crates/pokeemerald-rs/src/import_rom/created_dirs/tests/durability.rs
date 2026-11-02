//! Making the created levels durable: syncing each level's entry in its parent,
//! including parents whose pins were shed under descriptor pressure.

use super::*;

/// [`super::create_directories_pinned`] with every proactive entry sync
/// refused, so the levels come back unsynced and the later shedding and
/// recovery syncs are what the test exercises.
#[cfg(unix)]
fn create_pinned_unsynced(
    target: &std::path::Path,
) -> Result<super::DirectoryCreation, (Vec<CreatedDirectory>, std::io::Error)> {
    super::create_directories_pinned_with_hooks(
        target,
        super::pin_budget(),
        &mut || {},
        &mut || None,
        &mut || None,
        &mut |_| Err(std::io::Error::other("sync reopen refused")),
    )
}

#[cfg(unix)]
#[test]
fn a_sync_out_of_descriptors_still_syncs_every_levels_parent() {
    // The outermost level's reopen is refused before anything is synced,
    // so shedding must not take the parent being retried.
    let dir = TempDir::new("sync-shed");
    let level = dir.join("a").join("b").join("c");
    let mut created = super::create_directories_with_hooks(
        &level,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut || None,
        &mut || None,
    )
    .expect("three levels are created");

    let mut refused_once = false;
    let mut synced = Vec::new();
    super::sync::sync_created_directories_with(&mut created, &mut |parent| {
        if !refused_once {
            refused_once = true;
            return Err(std::io::Error::from(rustix::io::Errno::MFILE));
        }
        let stat = rustix::fs::fstat(parent)?;
        synced.push((stat.st_dev, stat.st_ino));
        super::super::super::dest::reopen_for_sync(parent)
    });

    let expected: Vec<_> = [dir.path.clone(), dir.join("a"), dir.join("a").join("b")]
        .iter()
        .map(|path| {
            let stat = rustix::fs::stat(path).expect("the level exists");
            (stat.st_dev, stat.st_ino)
        })
        .collect();
    assert_eq!(
        synced, expected,
        "every created level's parent is synced, outermost first"
    );
    assert!(
        created[0].parent.is_some() && created[0].own.is_some(),
        "the level being synced keeps its pins"
    );
}

#[cfg(unix)]
#[test]
fn the_held_final_directory_does_not_cost_a_new_levels_parent_sync() {
    // With the table full after creation, the sync's only recovery is
    // shedding the innermost level's own pin, so nothing else may share it.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::durability::the_held_final_directory_does_not_cost_a_new_levels_parent_sync",
        );
        return;
    }
    let dir = TempDir::new("held-final-sync");
    let target = dir.join("new");
    // Existing parent + new level consume both free slots.
    let fillers = fill_descriptor_table_leaving(2);
    let creation = super::create_directories_pinned(&target).expect("one level is created");
    let mut created = creation.created;
    let mut synced = false;
    super::sync::sync_created_directories_with(&mut created, &mut |parent| {
        let real = super::super::super::dest::reopen_for_sync(parent)?;
        synced = true;
        Ok(real)
    });
    drop(creation.final_directory);
    drop(fillers);
    assert!(synced, "the new level's parent entry must be synced");
}

#[cfg(unix)]
#[test]
fn a_destination_acquired_before_the_sync_survives_the_sync_shedding_its_level() {
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::durability::a_destination_acquired_before_the_sync_survives_the_sync_shedding_its_level",
        );
        return;
    }
    let dir = TempDir::new("acquire-then-sync");
    let target = dir.join("new");
    let fillers = fill_descriptor_table_leaving(3);
    let creation = super::create_directories_pinned(&target).expect("one level is created");
    let mut created = creation.created;
    let dest =
        super::super::super::acquire_dest(&mut created, &target, creation.final_directory.as_ref())
            .expect("the destination opens through the pin");
    drop(creation.final_directory);
    super::sync_created_directories(&mut created);
    for level in &mut created {
        level.own = None;
    }
    drop(fillers);
    drop(
        dest.create_new(std::ffi::OsStr::new("probe"))
            .expect("created through the handle"),
    );
    assert!(
        target.join("probe").exists(),
        "the file landed in the created level"
    );
}

#[cfg(unix)]
#[test]
fn acquiring_the_destination_under_three_free_descriptors_keeps_the_parent_sync() {
    // Mirrors `import_to_with_hooks`: ROM open, one new level, acquire, then
    // the parent sync; the table holds no fourth slot for a reopen.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::durability::acquiring_the_destination_under_three_free_descriptors_keeps_the_parent_sync",
        );
        return;
    }
    let dir = TempDir::new("acquire-sync-three");
    let target = dir.join("new");
    let rom_path = dir.join("rom.gba");
    std::fs::write(&rom_path, b"rom").expect("rom written");
    let fillers = fill_descriptor_table_leaving(3);
    let rom = std::fs::File::open(&rom_path).expect("rom opens");
    let creation = create_pinned_unsynced(&target).expect("one level is created");
    let mut created = creation.created;
    let dest =
        super::super::super::acquire_dest(&mut created, &target, creation.final_directory.as_ref())
            .expect("the destination opens through the pin");
    drop(creation.final_directory);
    let mut synced = Vec::new();
    super::sync::sync_shed_parents_with(&mut created, &dest, &mut |parent| {
        let stat = rustix::fs::fstat(parent).expect("parent stats");
        synced.push((stat.st_dev, stat.st_ino));
        true
    });
    super::sync_created_directories(&mut created);
    drop(dest);
    drop(rom);
    drop(fillers);
    let parent = rustix::fs::stat(&dir.path).expect("parent stats");
    assert_eq!(
        synced,
        [(parent.st_dev, parent.st_ino)],
        "the new level's parent entry must be synced"
    );
}

#[cfg(unix)]
#[test]
fn a_shed_parent_past_a_raced_level_is_synced_not_its_child() {
    // `base/a/x/b`: this run makes `a` and `b`, another creator makes `x` in
    // between, and acquisition then sheds `a`'s pins. `a`'s entry lives in
    // `base`, so `base` is the directory to sync for it.
    let dir = TempDir::new("shed-parent-gap");
    let a = dir.join("a");
    let x = a.join("x");
    let target = x.join("b");
    let mut raced = false;
    let creation = super::create_directories_pinned_with_hooks(
        &target,
        super::pin_budget(),
        &mut || {},
        &mut || {
            if !raced {
                raced = true;
                std::fs::create_dir(&x).expect("the other creator makes x");
            }
            None
        },
        &mut || None,
        &mut |_| Err(std::io::Error::other("sync reopen refused")),
    )
    .expect("the destination is created");
    let mut created = creation.created;
    assert_eq!(created.len(), 2, "only a and b are this run's");
    let dest =
        super::super::super::acquire_dest(&mut created, &target, creation.final_directory.as_ref())
            .expect("the destination opens through the pin");
    drop(creation.final_directory);
    // What acquisition's `open_shedding_pins` does to the outermost level.
    created[0].parent = None;
    created[0].own = None;
    let mut synced = Vec::new();
    super::sync::sync_shed_parents_with(&mut created, &dest, &mut |parent| {
        let stat = rustix::fs::fstat(parent).expect("parent stats");
        synced.push((stat.st_dev, stat.st_ino));
        true
    });
    let base = rustix::fs::stat(&dir.path).expect("base stats");
    assert_eq!(
        synced,
        [(base.st_dev, base.st_ino)],
        "a's entry is in base, so base is what must be synced"
    );
}

#[cfg(unix)]
#[test]
fn a_shed_level_off_the_destinations_ancestry_does_not_stop_the_parent_sync() {
    // `base/a/b/../c`: the walk up from `c` never passes `b`, which must not
    // stop it from reaching shed `a`, whose entry lives in `base`.
    let dir = TempDir::new("shed-parent-sibling");
    let target = dir.join("a").join("b").join("..").join("c");
    let creation = create_pinned_unsynced(&target).expect("the destination is created");
    let mut created = creation.created;
    assert_eq!(created.len(), 3, "a, b and c are this run's");
    let dest =
        super::super::super::acquire_dest(&mut created, &target, creation.final_directory.as_ref())
            .expect("the destination opens through the pin");
    drop(creation.final_directory);
    // What acquisition's `open_shedding_pins` does to the outermost level.
    created[0].parent = None;
    created[0].own = None;
    let mut synced = Vec::new();
    super::sync::sync_shed_parents_with(&mut created, &dest, &mut |parent| {
        let stat = rustix::fs::fstat(parent).expect("parent stats");
        synced.push((stat.st_dev, stat.st_ino));
        true
    });
    let base = rustix::fs::stat(&dir.path).expect("base stats");
    assert_eq!(
        synced,
        [(base.st_dev, base.st_ino)],
        "a's entry is in base, so base is what must be synced"
    );
}

#[cfg(unix)]
#[test]
fn a_full_table_after_acquisition_still_syncs_every_shed_parent() {
    // Every level's own pin but the last is shared with the next level's
    // parent pin, so releasing own pins alone frees one descriptor at most.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::durability::a_full_table_after_acquisition_still_syncs_every_shed_parent",
        );
        return;
    }
    let dir = TempDir::new("shed-parent-full-table");
    let paths: Vec<_> = ["l0", "l1", "l2", "l3"]
        .iter()
        .scan(dir.path.clone(), |path, name| {
            *path = path.join(name);
            Some(path.clone())
        })
        .collect();
    let target = paths.last().expect("four levels").clone();
    let creation = create_pinned_unsynced(&target).expect("the destination is created");
    let mut created = creation.created;
    assert_eq!(created.len(), 4);
    let dest =
        super::super::super::acquire_dest(&mut created, &target, creation.final_directory.as_ref())
            .expect("the destination opens through the pin");
    drop(creation.final_directory);
    // Acquisition shed the two outermost levels, then the table filled.
    for level in &mut created[..2] {
        level.parent = None;
        level.own = None;
    }
    let fillers = fill_descriptor_table_leaving(0);
    let mut synced = Vec::new();
    super::sync::sync_shed_parents_with(&mut created, &dest, &mut |parent| {
        let stat = rustix::fs::fstat(parent).expect("parent stats");
        synced.push((stat.st_dev, stat.st_ino));
        true
    });
    drop(fillers);
    let expected: Vec<_> = [&paths[0], &dir.path]
        .iter()
        .map(|path| {
            let stat = rustix::fs::stat(*path).expect("the level exists");
            (stat.st_dev, stat.st_ino)
        })
        .collect();
    // Pinned levels' parents may be synced on the way, to free their pins.
    assert!(
        synced.ends_with(&expected),
        "both shed levels' parents are synced, innermost first: {synced:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_shed_parent_behind_a_symlinked_level_is_still_synced() {
    // `base/a/x/c`: this run makes `a`, another creator makes `x` a symlink
    // to `external` (outside `base`), and this run makes `c` in `external`.
    // Acquisition sheds `a`'s pins; `a`'s entry is in `base`, which is not
    // on the destination's `..` ancestry.
    let dir = TempDir::new("shed-parent-symlink");
    let base = dir.join("base");
    let external = dir.join("external");
    std::fs::create_dir(&base).expect("base made");
    std::fs::create_dir(&external).expect("external made");
    let a = base.join("a");
    let x = a.join("x");
    let target = x.join("c");
    let mut raced = false;
    let creation = super::create_directories_pinned_with_hooks(
        &target,
        super::pin_budget(),
        &mut || {},
        &mut || {
            if !raced {
                raced = true;
                std::os::unix::fs::symlink(&external, &x).expect("the other creator links x");
            }
            None
        },
        &mut || None,
        &mut |_| Err(std::io::Error::other("sync reopen refused")),
    )
    .expect("the destination is created");
    let mut created = creation.created;
    assert_eq!(created.len(), 2, "only a and c are this run's");
    let mut synced = Vec::new();
    let record = |parent: &std::os::fd::OwnedFd| {
        let stat = rustix::fs::fstat(parent).expect("parent stats");
        (stat.st_dev, stat.st_ino)
    };
    // Acquisition refused once for want of a descriptor: the real shedding
    // path sheds `a`, the outermost pinned level.
    let mut refused = false;
    let pin = creation
        .final_directory
        .clone()
        .or_else(|| created[1].own_pin())
        .expect("pinned");
    let dest = super::sync::open_shedding_pins_with(
        &mut created,
        || {
            if !refused {
                refused = true;
                return Err(std::io::Error::from(rustix::io::Errno::MFILE));
            }
            super::super::super::dest::Dest::open_pinned(&pin)
        },
        &mut |parent| {
            synced.push(record(parent));
            super::super::super::dest::reopen_for_sync(parent)
        },
    )
    .expect("the destination opens through the pin");
    drop(pin);
    drop(creation.final_directory);
    assert!(created[0].parent.is_none(), "a was shed");
    super::sync::sync_shed_parents_with(&mut created, &dest, &mut |parent| {
        synced.push(record(parent));
        true
    });
    super::sync::sync_created_directories_with(&mut created, &mut |parent| {
        synced.push(record(parent));
        super::super::super::dest::reopen_for_sync(parent)
    });
    let base_stat = rustix::fs::stat(&base).expect("base stats");
    assert!(
        synced.contains(&(base_stat.st_dev, base_stat.st_ino)),
        "a's entry is in base, so base must be synced: {synced:?}"
    );
}

#[cfg(unix)]
#[test]
fn creation_syncs_a_parent_before_shedding_its_pin_behind_a_symlinked_level() {
    // `base/a/b/x/c`: this run makes `a` and `b`, another creator makes `x`
    // a symlink to `external`, and opening `x` runs out of descriptors.
    // Shedding `a` drops the only pin to `base`, which the destination's
    // `..` ancestry cannot reach from `external`, so `a`'s entry must be
    // synced in `base` before the pin goes.
    let dir = TempDir::new("creation-shed-symlink");
    let base = dir.join("base");
    let external = dir.join("external");
    std::fs::create_dir(&base).expect("base made");
    std::fs::create_dir(&external).expect("external made");
    let b = base.join("a").join("b");
    let x = b.join("x");
    let target = x.join("c");
    let mut reopens = 0;
    let mut refused = false;
    let mut synced = Vec::new();
    let creation = super::create_directories_pinned_with_hooks(
        &target,
        super::pin_budget(),
        &mut || {},
        &mut || {
            reopens += 1;
            if reopens == 2 {
                std::os::unix::fs::symlink(&external, &x).expect("the other creator links x");
            }
            None
        },
        &mut || {
            (!std::mem::replace(&mut refused, true))
                .then(|| std::io::Error::from(rustix::io::Errno::MFILE))
        },
        &mut |parent| {
            let reopened = super::super::super::dest::reopen_for_sync(parent)?;
            let stat = rustix::fs::fstat(parent).expect("parent stats");
            synced.push((stat.st_dev, stat.st_ino));
            Ok(reopened)
        },
    )
    .expect("the destination is created");
    assert_eq!(creation.created.len(), 3, "a, b and c are this run's");
    assert!(creation.created[0].parent.is_none(), "a was shed");
    assert!(creation.created[0].entry_synced, "a's entry was synced");
    let base_stat = rustix::fs::stat(&base).expect("base stats");
    // Creation syncs each new entry's parent as it goes, so `a`'s entry was
    // already durable when the shed came and no second reopen was needed.
    assert_eq!(
        synced
            .iter()
            .filter(|&&entry| entry == (base_stat.st_dev, base_stat.st_ino))
            .count(),
        1,
        "base is synced once, before a's pin is shed: {synced:?}"
    );
    assert_eq!(
        synced.first(),
        Some(&(base_stat.st_dev, base_stat.st_ino)),
        "a's entry is the first one synced"
    );
}

#[cfg(unix)]
#[test]
fn a_shed_levels_entry_is_synced_when_no_slot_is_left_to_sync_it_at_shedding() {
    // `base/a/b/x/c` as above, but from the moment opening `x` runs out of
    // descriptors every sync reopen fails too, and every own pin is shared
    // so none can be released for a slot. `a`'s entry in `base`, which the
    // destination's `..` ancestry cannot reach from `external`, must
    // already have been synced while descriptors were to spare.
    let dir = TempDir::new("creation-shed-symlink-full");
    let base = dir.join("base");
    let external = dir.join("external");
    std::fs::create_dir(&base).expect("base made");
    std::fs::create_dir(&external).expect("external made");
    let x = base.join("a").join("b").join("x");
    let target = x.join("c");
    let reopens = std::cell::Cell::new(0);
    let pressure = std::cell::Cell::new(false);
    let mut synced = Vec::new();
    let creation = super::create_directories_pinned_with_hooks(
        &target,
        super::pin_budget(),
        &mut || {},
        &mut || {
            reopens.set(reopens.get() + 1);
            if reopens.get() == 2 {
                std::os::unix::fs::symlink(&external, &x).expect("the other creator links x");
            }
            None
        },
        &mut || (!pressure.replace(true)).then(|| std::io::Error::from(rustix::io::Errno::MFILE)),
        &mut |parent| {
            if pressure.get() {
                return Err(std::io::Error::from(rustix::io::Errno::MFILE));
            }
            let reopened = super::super::super::dest::reopen_for_sync(parent)?;
            let stat = rustix::fs::fstat(parent).expect("parent stats");
            synced.push((stat.st_dev, stat.st_ino));
            Ok(reopened)
        },
    )
    .expect("the import still succeeds under pressure");
    assert_eq!(creation.created.len(), 3, "a, b and c are this run's");
    assert!(creation.created[0].parent.is_none(), "a was shed");
    assert!(
        creation.created[0].entry_synced,
        "a's entry was synced before its parent was shed"
    );
    let base_stat = rustix::fs::stat(&base).expect("base stats");
    assert!(
        synced.contains(&(base_stat.st_dev, base_stat.st_ino)),
        "base was synced while descriptors were to spare: {synced:?}"
    );
}

#[cfg(unix)]
fn reopen_to_an_unsyncable_descriptor(
    _parent: &std::os::fd::OwnedFd,
) -> std::io::Result<std::os::fd::OwnedFd> {
    // `fsync` of a pipe fails with `EINVAL`.
    let (read, _write) = std::io::pipe()?;
    Ok(read.into())
}

#[cfg(unix)]
#[test]
fn a_failed_shed_time_sync_leaves_the_entry_unsynced() {
    let dir = TempDir::new("shed-sync-fails");
    let target = dir.join("new");
    let creation = create_pinned_unsynced(&target).expect("one level is created");
    let mut created = creation.created;
    let mut refused = false;
    let opened = super::sync::open_shedding_pins_with(
        &mut created,
        || {
            if std::mem::replace(&mut refused, true) {
                return Ok(());
            }
            Err(std::io::Error::from(rustix::io::Errno::MFILE))
        },
        &mut reopen_to_an_unsyncable_descriptor,
    );
    assert!(opened.is_ok(), "the import goes on after the failed sync");
    assert!(created[0].parent.is_none(), "the level was shed");
    assert!(!created[0].entry_synced, "a failed sync is not a sync");
}

#[cfg(unix)]
#[test]
fn a_failed_recovery_sync_leaves_the_entry_unsynced_and_the_final_pass_retries() {
    let dir = TempDir::new("final-sync-fails");
    let target = dir.join("new");
    let creation = create_pinned_unsynced(&target).expect("one level is created");
    let mut created = creation.created;
    super::sync::sync_created_directories_with(
        &mut created,
        &mut reopen_to_an_unsyncable_descriptor,
    );
    assert!(!created[0].entry_synced, "a failed sync is not a sync");
    super::sync_created_directories(&mut created);
    assert!(created[0].entry_synced, "the retry syncs the entry");
}

#[cfg(unix)]
#[test]
fn a_failed_final_sync_does_not_unsync_an_entry_synced_at_creation() {
    // `a/b`, both entries synced at creation. A redundant final-pass sync
    // that fails must not mark them unsynced: a later open out of
    // descriptors would then try to sync `a` again and release `b`'s own
    // pin for the slot, and a failed import could no longer take `b` back.
    let dir = TempDir::new("final-sync-fails-after-creation");
    let target = dir.join("a").join("b");
    let creation = super::create_directories_pinned(&target).expect("both levels are created");
    let mut created = creation.created;
    drop(creation.final_directory);
    assert!(
        created.iter().all(|level| level.entry_synced),
        "creation synced both entries"
    );
    super::sync::sync_created_directories_with(
        &mut created,
        &mut reopen_to_an_unsyncable_descriptor,
    );
    assert!(
        created.iter().all(|level| level.entry_synced),
        "a failed redundant sync keeps an earlier sync"
    );
    let mut pressure = true;
    let opened: std::io::Result<()> = super::sync::open_shedding_pins_with(
        &mut created,
        || {
            if std::mem::replace(&mut pressure, false) {
                return Err(std::io::Error::from(rustix::io::Errno::MFILE));
            }
            Err(std::io::Error::other("the temporary file is refused"))
        },
        &mut |_| Err(std::io::Error::from(rustix::io::Errno::MFILE)),
    );
    assert!(opened.is_err(), "the import fails");
    super::undo_created_directories(&created);
    assert!(!target.exists(), "b is taken back");
}
