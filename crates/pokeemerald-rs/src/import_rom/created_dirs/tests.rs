use std::fs;
use std::path::PathBuf;

use super::super::tests::TempDir;
#[cfg(unix)]
use super::super::tests::{
    fill_descriptor_table_leaving, in_descriptor_pressure_child, run_under_descriptor_pressure,
};
use super::{create_directories, directories_to_create, CreatedDirectory};

/// The paths [`create_directories`] recorded, in order -- the one field
/// both platform arms of [`CreatedDirectory`] carry, and what these tests
/// care about: which levels it claims, not how it pins them.
fn created_paths(created: &[CreatedDirectory]) -> Vec<PathBuf> {
    created.iter().map(|dir| dir.path.clone()).collect()
}

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

#[test]
fn every_level_the_import_must_create_is_listed_outermost_first() {
    // Each level is a name in the one before it, and it is the *parent*
    // that has to be synced to make that name durable -- so the list has to
    // name every level, in the order they are created.
    let dir = TempDir::new("levels");
    let one = dir.join("one");
    let two = one.join("two");
    let three = two.join("three");

    assert_eq!(
        directories_to_create(&three),
        [one.clone(), two.clone(), three.clone()]
    );
    // A destination that is already there is created by nobody, which is
    // also how a failed import knows not to remove it.
    assert!(directories_to_create(&dir.path).is_empty());

    fs::create_dir_all(&one).expect("the first level is created");
    assert_eq!(directories_to_create(&three), [two.clone(), three.clone()]);
    fs::create_dir_all(&three).expect("the rest are created");
    assert!(directories_to_create(&three).is_empty());
}

#[test]
fn only_the_levels_the_run_created_come_back_as_its_own() {
    // Ownership is the create's answer, not an earlier look's: a level that
    // is already there when the create reaches it belongs to whoever made
    // it, and a failed import must leave it standing even while it is
    // empty. Another importer racing this one hits exactly that path.
    let dir = TempDir::new("created-levels");
    let one = dir.join("one");
    let two = one.join("two");
    fs::create_dir(&one).expect("the outer level is created");

    let created = create_directories(&two).expect("the missing level is created");
    assert_eq!(created_paths(&created), std::slice::from_ref(&two));
    assert!(two.is_dir());

    let again = create_directories(&two).expect("an existing destination is not a failure");
    assert!(again.is_empty(), "a run that created nothing owns nothing");
}

// The traversal opens need only write and search on Linux, Android,
// FreeBSD, and macOS 13 or later; every other Unix falls back to a
// read-requiring open.
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_vendor = "apple"
))]
#[test]
fn a_level_is_created_under_a_parent_that_is_writable_but_not_readable() {
    use std::os::unix::fs::PermissionsExt as _;

    // Creating a name inside a directory needs write and search on it,
    // never read: a `0o300` parent is a destination the path-based
    // `mkdir` this replaced always accepted, and the levels below it are
    // this run's own to make and to read.
    let dir = TempDir::new("unreadable-parent");
    let parent = dir.join("drop-box");
    fs::create_dir(&parent).expect("the parent is created");
    let level = parent.join("pokeemerald-rs");

    fs::set_permissions(&parent, fs::Permissions::from_mode(0o300)).expect("the parent closes");
    // A privileged user ignores directory permissions, so the close-off
    // does not block them and there is nothing to assert. Asked of the
    // outcome rather than of the uid, so it needs no libc.
    let unreadable = fs::read_dir(&parent).is_err();
    let created = create_directories(&level);
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).expect("the parent reopens");

    if unreadable {
        let created = created
            .map_err(|(_, source)| source)
            .expect("a writable, searchable parent takes a new level");
        assert_eq!(created_paths(&created), std::slice::from_ref(&level));
        assert!(level.is_dir());
    } else {
        // A privileged user is not closed off by `0o300`, so the assertion
        // cannot run; say so rather than pass vacuously.
        eprintln!(
            "a_level_is_created_under_a_parent_that_is_writable_but_not_readable: \
             skipped -- running privileged, 0o300 did not close the parent off"
        );
    }
}

// Same platform boundary as the test above; this pins
// `open_traversal_directory` alone.
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_vendor = "apple"
))]
#[test]
fn a_traversal_handle_pins_a_parent_that_is_writable_but_not_readable() {
    use std::os::unix::fs::PermissionsExt as _;

    // `create_directories` pins its one path-resolved component -- the
    // first ancestor of the destination that already exists -- before any
    // `mkdirat`, so that open must ask no more of it than the path-based
    // `mkdir` it replaced: write and search, never read.
    let dir = TempDir::new("traversal-unreadable-parent");
    let parent = dir.join("drop-box");
    fs::create_dir(&parent).expect("the parent is created");

    fs::set_permissions(&parent, fs::Permissions::from_mode(0o300)).expect("the parent closes");
    // A privileged user ignores directory permissions, so the close-off
    // does not block them and there is nothing to assert.
    let unreadable = fs::read_dir(&parent).is_err();
    let pinned = super::dest::open_traversal_directory(&parent);
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).expect("the parent reopens");

    if unreadable {
        pinned.expect("pinning for traversal needs only write and search");
    } else {
        // Same privileged-sandbox caveat as
        // `a_level_is_created_under_a_parent_that_is_writable_but_not_readable`
        // above -- said explicitly rather than passed with nothing checked.
        eprintln!(
            "a_traversal_handle_pins_a_parent_that_is_writable_but_not_readable: \
             skipped -- running privileged, 0o300 did not close the parent off"
        );
    }
}

#[test]
fn a_creation_that_fails_part_way_hands_back_the_levels_it_made() {
    // The overlong component trips the create after `new/` is on disk, and
    // the prefix has to come back with the error for the caller to undo.
    let dir = TempDir::new("partial-create-levels");
    let outer = dir.join("new");
    let overlong = "x".repeat(300);

    let (created, _source) = create_directories(&outer.join(overlong))
        .expect_err("the overlong component cannot be created");

    assert_eq!(created_paths(&created), std::slice::from_ref(&outer));
    assert!(outer.is_dir());
}

#[cfg(unix)]
#[test]
fn a_level_removed_and_remade_under_its_inode_number_is_left_standing() {
    // Another account replaces the empty level at the same name before
    // cleanup. The held descriptor keeps the original inode allocated; the
    // recorded identity is overwritten here to stand in for a filesystem
    // that reused the inode number.
    let dir = TempDir::new("undo-inode-reuse");
    let level = dir.join("new");

    let mut created = create_directories(&level).expect("the missing level is created");
    assert_eq!(created_paths(&created), std::slice::from_ref(&level));

    fs::remove_dir(&level).expect("the created level is removed");
    fs::create_dir(&level).expect("somebody else's level takes the name");
    created[0].identity = rustix::fs::stat(&level).expect("the replacement's identity is readable");

    super::undo_created_directories(&created);

    assert!(
        level.is_dir(),
        "the cleanup took back a replacement that reused the created level's inode number"
    );
}

#[cfg(unix)]
#[test]
fn a_level_whose_reopen_failed_is_left_standing_by_the_cleanup() {
    // A level whose reopen failed has no pin, so cleanup cannot tell it
    // from a replacement and leaves it alone; driven through the real
    // reopen-failure branch.
    let dir = TempDir::new("undo-unpinned");
    let level = dir.join("new");

    let (created, _source) = super::create_directories_with_hooks(
        &level,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut || Some(std::io::Error::other("reopen forced to fail for the test")),
        &mut || None,
    )
    .expect_err("a forced reopen failure fails the create");
    assert_eq!(created_paths(&created), std::slice::from_ref(&level));
    assert!(
        created[0].own.is_none(),
        "a forced reopen failure must record no pin"
    );

    super::undo_created_directories(&created);

    assert!(
        level.is_dir(),
        "an unpinned level must be left standing, not removed by identity alone"
    );
}

#[cfg(unix)]
#[test]
fn a_level_swapped_between_stat_and_reopen_is_not_descended_into() {
    // The swap lands through the `before_reopen` hook, which runs at
    // exactly the point a real race would -- after the `statat` and before
    // the reopen -- so this needs no second thread.
    let dir = TempDir::new("swap-before-reopen");
    let first = dir.join("a");
    let target = first.join("b");
    let moved = dir.join("a-original");
    let mut swapped = false;
    let mut hook = || -> Option<std::io::Error> {
        if !swapped {
            swapped = true;
            fs::rename(&first, &moved).unwrap();
            fs::create_dir(&first).unwrap();
        }
        None
    };

    let _ = super::create_directories_with_hooks(
        &target,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut hook,
        &mut || None,
    );

    assert!(swapped, "the hook ran");
    assert!(
        !target.exists(),
        "a level was created inside the replacement directory swapped in after the stat"
    );
}

#[cfg(unix)]
#[test]
fn a_deep_valid_destination_is_created_without_one_descriptor_per_level() {
    let dir = TempDir::new("deep-create");
    let mut level = dir.join("d");
    for _ in 0..100 {
        level = level.join("d");
    }
    let created = create_directories(&level)
        .map_err(|(created, source)| (created.len(), source))
        .expect("a valid deep destination is created");
    assert_eq!(created.len(), 101);
    assert!(level.is_dir());
}

#[cfg(unix)]
#[test]
fn a_destination_past_the_pin_budget_rolls_back_only_the_pinned_suffix() {
    // A small injected budget keeps the cutoff independent of the runner's
    // `RLIMIT_NOFILE` and of how many descriptors it already holds; a few
    // levels beyond it exercise both sides.
    let dir = TempDir::new("pin-budget");
    let pin_limit = 4;
    let total = pin_limit + 5;
    let pin_from = total - pin_limit;
    let mut level = dir.join("d");
    for _ in 1..total {
        level = level.join("d");
    }

    let created = super::create_directories_with_hooks(
        &level,
        pin_limit,
        &mut || {},
        &mut || None,
        &mut || None,
    )
    .expect("a valid destination past the pin budget is still created");
    assert_eq!(created.len(), total);
    assert!(level.is_dir());
    for (index, entry) in created.iter().enumerate() {
        assert_eq!(
            entry.own.is_some(),
            index >= pin_from,
            "level {index} pin state (pin_from = {pin_from})"
        );
    }

    super::undo_created_directories(&created);

    for (index, entry) in created.iter().enumerate() {
        if index >= pin_from {
            assert!(
                !entry.path.is_dir(),
                "level {index} was pinned and verifiable, so rollback should remove it"
            );
        } else {
            assert!(
                entry.path.is_dir(),
                "level {index} was never pinned, so rollback must leave it standing"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn a_destination_with_dotdot_levels_pins_every_directory_it_actually_creates() {
    // Five levels descended and five `..` climbed back out, then a final
    // component: 11 lexical levels but 6 real directories, the injected
    // budget.
    let dir = TempDir::new("dotdot-budget");
    let pin_limit = 6;
    let mut level = dir.join("d");
    for _ in 1..5 {
        level = level.join("d");
    }
    for _ in 0..5 {
        level = level.join("..");
    }
    level = level.join("final");

    let created = super::create_directories_with_hooks(
        &level,
        pin_limit,
        &mut || {},
        &mut || None,
        &mut || None,
    )
    .expect("a destination that fits the budget is still created through its dotdot levels");

    assert_eq!(created.len(), pin_limit);
    for (index, entry) in created.iter().enumerate() {
        assert!(entry.own.is_some(), "level {index} fits the pin budget");
        assert!(entry.parent.is_some(), "level {index} fits the pin budget");
    }
    assert!(level.is_dir());
}

#[cfg(unix)]
#[test]
fn a_valid_deep_destination_creates_or_fully_rolls_back() {
    // Run under a tight `RLIMIT_NOFILE` with four descriptors free, where
    // creation has to shed pins to finish: a valid destination must never
    // come out half made -- full creation, or nothing left behind.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::a_valid_deep_destination_creates_or_fully_rolls_back",
        );
        return;
    }
    let dir = TempDir::new("valid-deep-fd");
    let total = 37;
    let mut level = dir.join("d");
    for _ in 1..total {
        level = level.join("d");
    }
    let _fillers = fill_descriptor_table_leaving(4);
    match create_directories(&level) {
        Ok(created) => assert_eq!(created.len(), total),
        Err((created, source)) => {
            let recorded = created.len();
            super::undo_created_directories(&created);
            drop(created);
            assert!(
                !dir.join("d").exists(),
                "create failed after {recorded} levels ({source}); rollback left {} behind",
                dir.join("d").display()
            );
            panic!("valid deep destination failed to create: {source}");
        }
    }
}

#[cfg(unix)]
#[test]
fn a_reopen_out_of_descriptors_sheds_every_earlier_level_in_order_under_repeated_pressure() {
    // The last level's reopen answers `EMFILE` three times, one per level
    // that must be fully released before the retry succeeds.
    let dir = TempDir::new("shed-in-order");
    let mut level = dir.join("d");
    for _ in 1..6 {
        level = level.join("d");
    }
    let mut calls = 0u32;
    let mut hook = move || -> Option<std::io::Error> {
        calls += 1;
        (6..=8)
            .contains(&calls)
            .then(|| std::io::Error::from(rustix::io::Errno::MFILE))
    };

    let created = super::create_directories_with_hooks(
        &level,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut hook,
        &mut || None,
    )
    .expect("shedding three pinned levels frees enough room for the retry to succeed");

    assert_eq!(created.len(), 6);
    for (index, entry) in created.iter().enumerate() {
        if index < 3 {
            assert!(entry.own.is_none(), "level {index} was fully shed");
            assert!(entry.parent.is_none(), "level {index} was fully shed");
        } else {
            assert!(entry.own.is_some(), "level {index} was never shed");
            assert!(entry.parent.is_some(), "level {index} was never shed");
        }
    }
    assert!(level.is_dir());
}

#[cfg(unix)]
#[test]
fn a_dotdot_open_out_of_descriptors_sheds_a_pinned_level_and_retries() {
    // The `..` traversal open, unlike the created-level reopen, used to
    // surface `EMFILE` straight to the caller with no chance to shed a
    // pinned level first. `before_open` forces it to fail once, exactly
    // the way `before_reopen` forces the created-level reopen to.
    let dir = TempDir::new("dotdot-open-shed");
    let dest = dir.join("a").join("b").join("..").join("c");

    let mut failed_once = false;
    let mut before_open = move || -> Option<std::io::Error> {
        if failed_once {
            None
        } else {
            failed_once = true;
            Some(std::io::Error::from(rustix::io::Errno::MFILE))
        }
    };

    let created = super::create_directories_with_hooks(
        &dest,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut || None,
        &mut before_open,
    )
    .expect("shedding a pinned level frees enough room for the dotdot open to retry");

    assert_eq!(created.len(), 3, "levels a, b, and c");
    assert!(
        created[0].own.is_none(),
        "the outermost level was shed to free room for the dotdot open"
    );
    assert!(dest.is_dir());
}

#[cfg(unix)]
#[test]
fn create_directories_with_hooks_never_pins_more_than_its_injected_limit() {
    let dir = TempDir::new("small-pin-limit");
    let limit = 3;
    let total = limit + 5;
    let mut level = dir.join("d");
    for _ in 1..total {
        level = level.join("d");
    }

    let created =
        super::create_directories_with_hooks(&level, limit, &mut || {}, &mut || None, &mut || None)
            .expect("a valid destination is created regardless of how small the pin limit is");

    assert_eq!(created.len(), total);
    let pinned = created.iter().filter(|entry| entry.own.is_some()).count();
    assert!(
        pinned <= limit,
        "pinned {pinned} records against an injected limit of {limit}"
    );
    assert!(level.is_dir());
}

#[cfg(unix)]
#[test]
fn pin_budget_for_stays_within_reserved_headroom_of_the_soft_limit() {
    // Pure formula, no real `RLIMIT_NOFILE` touched: `min(MAX_PINNED_LEVELS,
    // soft - RESERVED_DESCRIPTORS)`, floored at 0, and `None` (no limit)
    // reads as no ceiling at all.
    assert_eq!(
        super::pin_budget_for(Some(36)),
        36 - super::RESERVED_DESCRIPTORS
    );
    assert_eq!(
        super::pin_budget_for(Some(1_000_000)),
        super::MAX_PINNED_LEVELS
    );
    assert_eq!(super::pin_budget_for(Some(1)), 0);
    assert_eq!(super::pin_budget_for(None), super::MAX_PINNED_LEVELS);
}

#[cfg(unix)]
#[test]
fn a_dotdot_level_descends_from_the_pinned_parent_not_the_path() {
    // A component ahead of the `..` is swapped for a symlink into another
    // tree through the `before_dotdot` seam, once the levels ahead are made
    // and pinned; the level after the `..` must still land under the
    // pinned parent.
    let dir = TempDir::new("dotdot-swap");
    let dest = dir.join("l0").join("l1").join("..").join("downstream");

    // The attacker's tree mirrors the level below the swapped component,
    // so a reopened path resolves through it instead of failing.
    let mirror = dir.join("attacker");
    fs::create_dir_all(mirror.join("l1")).expect("the attacker's mirror is built");

    let swapped = dir.join("l0");
    let moved = dir.join("carried-off");
    let swapped_at_dotdot = std::rc::Rc::new(std::cell::Cell::new(false));
    let mut before_dotdot = {
        let (swapped, moved, mirror) = (swapped.clone(), moved.clone(), mirror.clone());
        let flag = std::rc::Rc::clone(&swapped_at_dotdot);
        move || {
            fs::rename(&swapped, &moved).expect("the pinned level is carried off");
            std::os::unix::fs::symlink(&mirror, &swapped).expect("a symlink takes its name");
            flag.set(true);
        }
    };

    let made_them = super::create_directories_with_hooks(
        &dest,
        super::MAX_PINNED_LEVELS,
        &mut before_dotdot,
        &mut || None,
        &mut || None,
    )
    .is_ok();

    assert!(
        swapped_at_dotdot.get(),
        "the descent never reached the `..` level the swap is hooked to"
    );
    assert!(
        !mirror.join("downstream").exists(),
        "a level after `..` was created inside the attacker's tree (run succeeded: {made_them})"
    );
    assert!(
        moved.join("downstream").is_dir(),
        "the level after `..` belongs under the pinned level's own parent, wherever it was moved"
    );
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
        super::super::dest::reopen_for_sync(parent)
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
            "import_rom::created_dirs::tests::the_held_final_directory_does_not_cost_a_new_levels_parent_sync",
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
        let real = super::super::dest::reopen_for_sync(parent)?;
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
            "import_rom::created_dirs::tests::a_destination_acquired_before_the_sync_survives_the_sync_shedding_its_level",
        );
        return;
    }
    let dir = TempDir::new("acquire-then-sync");
    let target = dir.join("new");
    let fillers = fill_descriptor_table_leaving(3);
    let creation = super::create_directories_pinned(&target).expect("one level is created");
    let mut created = creation.created;
    let dest = super::super::acquire_dest(&mut created, &target, creation.final_directory.as_ref())
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
            "import_rom::created_dirs::tests::acquiring_the_destination_under_three_free_descriptors_keeps_the_parent_sync",
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
    let dest = super::super::acquire_dest(&mut created, &target, creation.final_directory.as_ref())
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
    let dest = super::super::acquire_dest(&mut created, &target, creation.final_directory.as_ref())
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
    let dest = super::super::acquire_dest(&mut created, &target, creation.final_directory.as_ref())
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
            "import_rom::created_dirs::tests::a_full_table_after_acquisition_still_syncs_every_shed_parent",
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
    let dest = super::super::acquire_dest(&mut created, &target, creation.final_directory.as_ref())
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
            super::super::dest::Dest::open_pinned(&pin)
        },
        &mut |parent| {
            synced.push(record(parent));
            super::super::dest::reopen_for_sync(parent)
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
        super::super::dest::reopen_for_sync(parent)
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
            let reopened = super::super::dest::reopen_for_sync(parent)?;
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
            let reopened = super::super::dest::reopen_for_sync(parent)?;
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

#[cfg(unix)]
fn entry_names(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(unix)]
#[test]
fn undo_preserves_a_replacement_swapped_after_validation() {
    // The hook runs after the name validated and before the level is taken
    // back -- the window a name-based `unlinkat` would delete through. The
    // replacement is empty: a sentinel inside it would make a plain removal
    // refuse and hide the bug.
    let dir = TempDir::new("undo-after-validation");
    let level = dir.join("new");
    let moved = dir.join("original");
    let created = super::create_directories(&level).unwrap();
    let mut ran = 0;
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        ran += 1;
        fs::rename(&level, &moved).unwrap();
        fs::create_dir(&level).unwrap();
    });

    assert_eq!(ran, 1, "the hook ran once");
    assert!(level.is_dir(), "the replacement was deleted");
    assert!(moved.is_dir(), "the original is left standing");
    assert_eq!(
        entry_names(&dir.path),
        ["new", "original"],
        "no quarantine is left behind"
    );
}

#[cfg(unix)]
#[test]
fn undo_puts_back_a_level_that_gained_contents_after_validation() {
    let dir = TempDir::new("undo-gained-contents");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        fs::write(level.join("keep"), b"x").unwrap();
    });

    assert!(level.join("keep").is_file());
    assert_eq!(entry_names(&dir.path), ["new"]);
}

#[cfg(unix)]
#[test]
fn undo_does_not_strand_contents_when_the_name_is_retaken_before_restoration() {
    // `new` gains `keep`, so removal from the quarantine refuses and the
    // level must be put back; a racer re-creates `new` once the name frees,
    // so the put-back hits EEXIST. `keep` must not end up only under
    // `.undo-*/level` with nothing reported.
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    for attempt in 0..500 {
        let dir = TempDir::new(&format!("undo-retaken-{attempt}"));
        let level = dir.join("new");
        let created = super::create_directories(&level).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let mut racer = None;
        super::take_back::undo_created_directories_with_hooks(&created, &mut || {
            fs::write(level.join("keep"), b"x").unwrap();
            let (level, stop) = (level.clone(), Arc::clone(&stop));
            racer = Some(std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if fs::create_dir(&level).is_ok() {
                        return;
                    }
                }
            }));
            std::thread::yield_now();
        });
        stop.store(true, Ordering::Relaxed);
        racer.unwrap().join().unwrap();
        let stranded: Vec<String> = entry_names(&dir.path)
            .into_iter()
            .filter(|name| name.starts_with(".undo-"))
            .filter(|name| dir.path.join(name).join("level").join("keep").is_file())
            .collect();
        assert!(
            stranded.is_empty(),
            "attempt {attempt}: keep stranded in {stranded:?}, not at {}: {:?}",
            level.display(),
            entry_names(&dir.path)
        );
        let restored = entry_names(&dir.path).into_iter().any(|name| {
            (name == "new" || name.starts_with("new.restored-"))
                && dir.path.join(&name).join("keep").is_file()
        });
        assert!(
            restored,
            "attempt {attempt}: keep is neither at {} nor beside it: {:?}",
            level.display(),
            entry_names(&dir.path)
        );
    }
}

#[cfg(unix)]
#[test]
fn rollback_refuses_a_quarantine_name_already_taken() {
    // The quarantine name is injected so a directory (or symlink) is already
    // there when rollback tries to claim it. `mkdirat` must refuse it: the
    // staged entry is never filled, emptied, or removed, and the level stays.
    for staged_kind in ["empty-0700-dir", "dir-with-data", "symlink"] {
        let dir = TempDir::new(&format!("quarantine-taken-{staged_kind}"));
        let level = dir.join("new");
        let staged = dir.join(".undo-staged");
        let created = super::create_directories(&level).unwrap();
        match staged_kind {
            "empty-0700-dir" => fs::create_dir(&staged).unwrap(),
            "dir-with-data" => {
                fs::create_dir(&staged).unwrap();
                fs::write(staged.join("mine"), b"x").unwrap();
            }
            _ => std::os::unix::fs::symlink(&level, &staged).unwrap(),
        }
        let parent = created[0].parent.as_deref().unwrap();
        assert!(
            !super::take_back::take_back_level_via(parent, &created[0], ".undo-staged", &mut || {}),
            "{staged_kind}: rollback must not proceed"
        );
        assert!(level.is_dir(), "{staged_kind}: the level is left standing");
        assert!(
            fs::symlink_metadata(&staged).is_ok(),
            "{staged_kind}: the staged entry is left in place"
        );
        if staged_kind == "dir-with-data" {
            assert!(staged.join("mine").is_file(), "its contents are untouched");
        }
        assert_eq!(entry_names(&dir.path), [".undo-staged", "new"]);
    }
}

#[cfg(unix)]
#[test]
fn rollback_refuses_a_quarantine_swapped_between_mkdir_and_open() {
    use std::os::unix::fs::PermissionsExt;
    // Between rollback's `mkdirat` and `openat` the name is swapped for a
    // directory that is not a fresh private quarantine. It must be neither
    // filled nor removed, and the level must stay.
    for kind in ["non-empty-0700", "empty-0755"] {
        let dir = TempDir::new(&format!("quarantine-swap-{kind}"));
        let level = dir.join("new");
        let (name, away) = (dir.join(".undo-swap"), dir.join("away"));
        let created = super::create_directories(&level).unwrap();
        let parent = created[0].parent.as_deref().unwrap();
        let mut swapped = 0;
        let proceeded =
            super::take_back::take_back_level_via(parent, &created[0], ".undo-swap", &mut || {
                swapped += 1;
                fs::rename(&name, &away).unwrap();
                fs::create_dir(&name).unwrap();
                if kind == "non-empty-0700" {
                    fs::write(name.join("mine"), b"x").unwrap();
                } else {
                    fs::set_permissions(&name, fs::Permissions::from_mode(0o755)).unwrap();
                }
            });
        assert_eq!(swapped, 1, "{kind}: the hook ran once");
        assert!(!proceeded, "{kind}: rollback must not proceed");
        assert!(level.is_dir(), "{kind}: the level is left standing");
        assert!(name.is_dir(), "{kind}: the replacement was removed");
        assert_eq!(
            fs::read_dir(&name).unwrap().count(),
            usize::from(kind == "non-empty-0700"),
            "{kind}: the replacement was filled or emptied"
        );
        assert!(
            away.is_dir(),
            "{kind}: the original quarantine is untouched"
        );
    }
}

#[cfg(unix)]
#[test]
fn quarantine_names_are_random_and_hidden() {
    let a = super::take_back::unguessable_quarantine_name().unwrap();
    let b = super::take_back::unguessable_quarantine_name().unwrap();
    assert_ne!(a, b);
    assert!(a.starts_with(".undo-") && a.len() == ".undo-".len() + 16);
}

#[cfg(unix)]
#[test]
fn restore_capture_puts_contents_beside_a_retaken_name() {
    // Deterministic form of the retake race: the level sits in a quarantine
    // holding `keep`, and its name was re-created meanwhile. The capture must
    // come back beside it, not stay only under the hidden quarantine.
    let dir = TempDir::new("restore-retaken");
    let created = super::create_directories(&dir.join("new")).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let quarantine_path = dir.join(".undo-held");
    fs::create_dir(&quarantine_path).unwrap();
    let quarantine = rustix::fs::openat(
        parent,
        ".undo-held",
        rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .unwrap();
    fs::rename(
        dir.join("new"),
        quarantine_path.join(super::take_back::QUARANTINE_ENTRY),
    )
    .unwrap();
    fs::write(
        quarantine_path
            .join(super::take_back::QUARANTINE_ENTRY)
            .join("keep"),
        b"x",
    )
    .unwrap();
    fs::create_dir(dir.join("new")).unwrap();

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &super::take_back::located_beside(parent, &created[0], std::ffi::OsStr::new(".undo-held")),
        parent,
        &created[0],
    );

    assert_eq!(
        warning.as_deref(),
        Some(
            format!(
                "warning: {} was recreated during cleanup; its contents are in {}",
                dir.join("new").display(),
                dir.join("new.restored-0").display()
            )
            .as_str()
        )
    );
    assert!(dir.join("new.restored-0").join("keep").is_file());
    assert!(!quarantine_path
        .join(super::take_back::QUARANTINE_ENTRY)
        .exists());
    assert_eq!(fs::read_dir(dir.join("new")).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn rollback_takes_back_a_level_when_the_quarantine_open_takes_the_last_descriptor() {
    // The quarantine's `openat` takes the table's last free slot. Checking
    // that quarantine must not cost a second descriptor, or rollback strands
    // both the level and an empty `.undo-*` beside it.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::rollback_takes_back_a_level_when_the_quarantine_open_takes_the_last_descriptor",
        );
        return;
    }
    let dir = TempDir::new("quarantine-last-descriptor");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let mut fillers = None;
    let proceeded =
        super::take_back::take_back_level_via(parent, &created[0], ".undo-last", &mut || {
            fillers = Some(fill_descriptor_table_leaving(1));
        });
    drop(fillers);
    assert!(proceeded, "the level is taken back");
    assert!(!level.exists(), "the level is removed");
    assert!(entry_names(&dir.path).is_empty(), "no quarantine is left");
}

#[cfg(unix)]
#[test]
fn a_capture_no_name_takes_back_is_reported_at_its_exact_quarantine_path() {
    // The level's name and all sixteen `.restored-<n>` names are taken, so
    // the capture stays in the quarantine; the warning must name that exact
    // directory, not a `.undo-*` pattern among other quarantines.
    let dir = TempDir::new("restore-all-taken");
    let created = super::create_directories(&dir.join("new")).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let quarantine_path = dir.join(".undo-held");
    fs::create_dir(&quarantine_path).unwrap();
    let quarantine = rustix::fs::openat(
        parent,
        ".undo-held",
        rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .unwrap();
    let capture = quarantine_path.join(super::take_back::QUARANTINE_ENTRY);
    fs::rename(dir.join("new"), &capture).unwrap();
    fs::write(capture.join("keep"), b"x").unwrap();
    fs::create_dir(dir.join("new")).unwrap();
    for n in 0..16 {
        fs::create_dir(dir.join(&format!("new.restored-{n}"))).unwrap();
    }
    fs::create_dir(dir.join(".undo-other")).unwrap();

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &super::take_back::located_beside(parent, &created[0], std::ffi::OsStr::new(".undo-held")),
        parent,
        &created[0],
    );

    assert!(capture.join("keep").is_file(), "the capture stays put");
    assert_eq!(
        warning.as_deref(),
        Some(
            format!(
                "warning: could not put {} back; its contents remain in {}",
                dir.join("new").display(),
                capture.display()
            )
            .as_str()
        )
    );
}

#[cfg(unix)]
#[test]
fn a_swapped_quarantine_is_not_removed_when_its_open_fails() {
    // Between `mkdirat` and `openat` the quarantine name is swapped for
    // someone else's empty directory, and the descriptor table is full, so
    // the open fails. Nothing then proves what the name denotes: it must not
    // be unlinked by name.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::a_swapped_quarantine_is_not_removed_when_its_open_fails",
        );
        return;
    }
    let dir = TempDir::new("swap-open-fails");
    let level = dir.join("new");
    let (name, away) = (dir.join(".undo-v"), dir.join("away"));
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let mut fillers = None;
    let proceeded =
        super::take_back::take_back_level_via(parent, &created[0], ".undo-v", &mut || {
            fs::rename(&name, &away).unwrap();
            fs::create_dir(&name).unwrap(); // someone else's empty directory
            fillers = Some(fill_descriptor_table_leaving(0));
        });
    drop(fillers);
    assert!(!proceeded);
    assert!(level.is_dir());
    assert!(
        name.is_dir(),
        "the replacement directory was unlinked by its mutable name"
    );
}

#[cfg(unix)]
const UMASK_CHILD: &str = "POKEEMERALD_IMPORT_ROLLBACK_UMASK_CHILD";

#[cfg(unix)]
#[test]
fn rollback_takes_back_a_level_under_a_umask_that_masks_owner_bits() {
    // Under umask 0177 the quarantine's `mkdirat(0700)` yields 0600: still
    // private, but without the search bit the capture's rename needs. The
    // umask is process-wide, so the body runs alone in a child.
    const TEST: &str = "import_rom::created_dirs::tests::rollback_takes_back_a_level_under_a_umask_that_masks_owner_bits";
    if std::env::var_os(UMASK_CHILD).is_none() {
        let exe = std::env::current_exe().unwrap();
        let output = std::process::Command::new(exe)
            .args([TEST, "--exact", "--nocapture"])
            .env(UMASK_CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("1 passed"),
            "child status {:?}\nstdout:\n{stdout}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let dir = TempDir::new("umask-0177");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    let previous = rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o177));
    super::undo_created_directories(&created);
    rustix::process::umask(previous);
    assert!(!level.exists(), "the level is taken back");
    assert!(entry_names(&dir.path).is_empty(), "no quarantine is left");
}

/// Stages a capture in `<parent>/.undo-held/level` holding `keep`, with the
/// level's name and all sixteen `.restored-<n>` names taken, so
/// `restore_capture` must leave it there and report it.
#[cfg(unix)]
fn stage_unrestorable_capture(
    parent_path: &std::path::Path,
    parent: &std::os::fd::OwnedFd,
) -> std::os::fd::OwnedFd {
    let quarantine_path = parent_path.join(".undo-held");
    fs::create_dir(&quarantine_path).unwrap();
    let quarantine = rustix::fs::openat(
        parent,
        ".undo-held",
        rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .unwrap();
    let capture = quarantine_path.join(super::take_back::QUARANTINE_ENTRY);
    fs::create_dir(&capture).unwrap();
    fs::write(capture.join("keep"), b"x").unwrap();
    for n in 0..16 {
        fs::create_dir(parent_path.join(format!("new.restored-{n}"))).unwrap();
    }
    quarantine
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_stranded_capture_is_reported_under_its_parents_current_name() {
    // The level's parent `a` is renamed to `b` after creation; rollback acts
    // through the held parent, so the warning must name `b`, not `a`.
    let dir = TempDir::new("restore-renamed-parent");
    let created = super::create_directories(&dir.join("a").join("new")).unwrap();
    let level = created.last().unwrap();
    let parent = level.parent.as_deref().unwrap();
    fs::rename(dir.join("a"), dir.join("b")).unwrap();
    let moved = fs::canonicalize(dir.join("b")).unwrap();
    let quarantine = stage_unrestorable_capture(&moved, parent);

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &super::take_back::located_beside(parent, level, std::ffi::OsStr::new(".undo-held")),
        parent,
        level,
    );

    assert_eq!(
        warning.as_deref(),
        Some(
            format!(
                "warning: could not put {} back; its contents remain in {}",
                moved.join("new").display(),
                moved
                    .join(".undo-held")
                    .join(super::take_back::QUARANTINE_ENTRY)
                    .display()
            )
            .as_str()
        )
    );
}

#[cfg(unix)]
#[test]
fn a_cleanup_warning_keeps_a_control_character_in_a_path_on_one_line() {
    let dir = TempDir::new("restore-newline");
    let parent_path = dir.join("line\nbreak");
    fs::create_dir(&parent_path).unwrap();
    let created = super::create_directories(&parent_path.join("new")).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let quarantine = stage_unrestorable_capture(&parent_path, parent);

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &super::take_back::located_beside(parent, &created[0], std::ffi::OsStr::new(".undo-held")),
        parent,
        &created[0],
    )
    .expect("the capture is reported");

    assert!(!warning.contains('\n'), "{warning:?}");
    assert!(warning.contains("line\\nbreak"), "{warning:?}");
}

#[cfg(unix)]
const ROLLBACK_STDERR_CHILD: &str = "POKEEMERALD_IMPORT_ROLLBACK_STDERR_CHILD";

/// Whether this process is the child [`rerun_alone_for_stderr`] spawned.
#[cfg(unix)]
fn in_rollback_stderr_child() -> bool {
    std::env::var_os(ROLLBACK_STDERR_CHILD).is_some()
}

/// Reruns the test at `test_path` alone in a child, fails unless it passes
/// there, and returns the child's stderr, where rollback warns.
#[cfg(unix)]
fn rerun_alone_for_stderr(test_path: &str) -> String {
    let exe = std::env::current_exe().unwrap();
    let output = std::process::Command::new(exe)
        .args([test_path, "--exact", "--nocapture"])
        .env(ROLLBACK_STDERR_CHILD, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stdout.contains("1 passed"),
        "child failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    stderr
}

#[cfg(unix)]
#[test]
fn a_quarantine_that_cannot_be_made_reports_the_level_left_standing() {
    // A file already sits at the quarantine name, so `mkdirat` fails with
    // EEXIST; the warning names the level rollback leaves and the cause.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::a_quarantine_that_cannot_be_made_reports_the_level_left_standing",
        );
        assert!(
            stderr.contains("/new is left in place: could not create cleanup directory ")
                && stderr.contains("/.undo-f: "),
            "the retained level is not reported; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("quarantine-mkdir-fails");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    fs::write(dir.join(".undo-f"), b"theirs").unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    assert!(!super::take_back::take_back_level_via(
        parent,
        &created[0],
        ".undo-f",
        &mut || {}
    ));
    assert!(level.is_dir(), "the level is left standing");
    assert_eq!(fs::read(dir.join(".undo-f")).unwrap(), b"theirs");
}

/// The path a `<lead><path><tail>` line in `stderr` names.
#[cfg(unix)]
fn warned_path<'a>(stderr: &'a str, lead: &str, tail: &str) -> &'a str {
    stderr
        .lines()
        .find_map(|line| {
            let rest = line.strip_prefix(lead)?;
            rest.find(tail).map(|end| &rest[..end])
        })
        .unwrap_or_else(|| panic!("no {lead:?}...{tail:?} line; stderr:\n{stderr}"))
}

/// Asserts `warned` is the path the child printed on its `marker` line,
/// as recorded or made canonical: Linux reports it through `/proc`, which
/// resolves symlinks, elsewhere it is the recorded path, which may run
/// through one (macOS's `/var`).
#[cfg(unix)]
fn assert_names_marked_path(stderr: &str, warned: &str, marker: &str) {
    let line = stderr
        .lines()
        .find_map(|line| line.strip_prefix(marker))
        .unwrap_or_else(|| panic!("no {marker:?} line; stderr:\n{stderr}"));
    let (recorded, canonical) = line.split_once(" | ").expect("recorded | canonical");
    assert!(
        warned == recorded || warned == canonical,
        "warned {warned:?}, expected {recorded:?} or {canonical:?}"
    );
}

/// Prints `path` after `marker` as recorded and canonical, for
/// [`assert_names_marked_path`] in the parent.
#[cfg(unix)]
fn mark_path(marker: &str, path: &std::path::Path) {
    eprintln!(
        "{marker}{} | {}",
        path.display(),
        fs::canonicalize(path).unwrap().display()
    );
}

#[cfg(unix)]
#[test]
fn a_taken_back_level_removes_its_quarantine_without_a_warning() {
    // The happy path leaves no trace: the level and its quarantine are both
    // gone, and nothing is printed.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::a_taken_back_level_removes_its_quarantine_without_a_warning",
        );
        assert!(!stderr.contains("warning:"), "stderr:\n{stderr}");
        return;
    }
    let dir = TempDir::new("quarantine-removed");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    assert!(super::take_back::take_back_level_via(
        parent,
        &created[0],
        ".undo-q",
        &mut || {}
    ));
    assert!(entry_names(&dir.path).is_empty(), "no trace is left");
}

#[cfg(unix)]
#[test]
fn a_quarantine_whose_removal_fails_is_reported_at_its_path() {
    // Something lands in the quarantine before its removal, so
    // `unlinkat(REMOVEDIR)` refuses it; it stays, reported at its path.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::a_quarantine_whose_removal_fails_is_reported_at_its_path",
        );
        let warned = warned_path(
            &stderr,
            "warning: could not remove cleanup directory ",
            ": ",
        );
        assert_names_marked_path(&stderr, warned, "quarantine at: ");
        assert!(
            stderr.contains("; it is left in place"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("quarantine-unlink-fails");
    let level = dir.join("new");
    let name = dir.join(".undo-r");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let proceeded = super::take_back::take_back_level_with_hooks(
        parent,
        &created[0],
        ".undo-r",
        &mut || {},
        &mut || fs::write(name.join("held"), b"x").unwrap(),
    );
    assert!(proceeded, "the level itself was taken back");
    assert!(
        name.join("held").is_file(),
        "the quarantine is left, with what it holds"
    );
    mark_path("quarantine at: ", &name);
}

#[cfg(unix)]
#[test]
fn a_directory_swapped_in_at_the_quarantine_name_is_left_standing_and_reported() {
    // Before the quarantine's re-check, it is moved away and a directory of
    // someone else's, holding data, takes its name. The name no longer
    // resolves to the held quarantine, so neither is removed, and the
    // warning names the quarantine's path.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::a_directory_swapped_in_at_the_quarantine_name_is_left_standing_and_reported",
        );
        let warned = warned_path(
            &stderr,
            "warning: cleanup directory ",
            " no longer resolves to the one rollback made; it is left in place",
        );
        assert_names_marked_path(&stderr, warned, "name at: ");
        return;
    }
    let dir = TempDir::new("swap-at-quarantine-name");
    let level = dir.join("new");
    let (name, away) = (dir.join(".undo-r"), dir.join("away"));
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let mut swapped = 0;
    let proceeded = super::take_back::take_back_level_with_hooks(
        parent,
        &created[0],
        ".undo-r",
        &mut || {},
        &mut || {
            swapped += 1;
            fs::rename(&name, &away).unwrap();
            fs::create_dir(&name).unwrap();
            fs::write(name.join("theirs"), b"x").unwrap();
        },
    );
    assert_eq!(swapped, 1, "the hook ran once");
    assert!(proceeded, "the level itself was taken back");
    assert!(!level.exists(), "the level is removed");
    assert!(
        name.join("theirs").is_file(),
        "the replacement or its data was removed"
    );
    assert!(
        entry_names(&away).is_empty(),
        "the moved quarantine stays, empty"
    );
    mark_path("name at: ", &name);
}

#[cfg(unix)]
#[test]
fn a_deep_rollback_takes_back_every_level_and_its_quarantine() {
    // Three levels are taken back through one quarantine made beside the
    // outermost, which is then removed: no trace is left.
    let dir = TempDir::new("deep-one-quarantine");
    let created = super::create_directories(&dir.join("a").join("b").join("c")).unwrap();
    assert_eq!(created.len(), 3);
    super::undo_created_directories(&created);
    assert!(
        entry_names(&dir.path).is_empty(),
        "{:?}",
        entry_names(&dir.path)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_level_on_another_filesystem_is_taken_back_through_a_quarantine_beside_it() {
    // `new/../m/more`: `new` is made in the base directory, then `m`, a
    // symlink to a directory on /dev/shm, is descended through and `more`
    // is made there. A rename of `more` into the quarantine beside `new`
    // fails with EXDEV; `more` must be taken back through a quarantine on
    // its own filesystem, and `new` still through the shared one.
    use std::os::unix::fs::MetadataExt as _;
    let shm = std::path::Path::new("/dev/shm");
    let dir = TempDir::new("cross-device");
    let Ok(shm_meta) = fs::metadata(shm) else {
        panic!("this test needs /dev/shm");
    };
    assert_ne!(
        shm_meta.dev(),
        fs::metadata(&dir.path).unwrap().dev(),
        "this test needs /dev/shm on another filesystem than the temp dir"
    );
    let other = shm.join(format!(
        "pokeemerald-rs-cross-device-{}",
        std::process::id()
    ));
    fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(&other, dir.join("m")).unwrap();
    let target = dir.join("new").join("..").join("m").join("more");
    let created = super::create_directories(&target).unwrap();
    assert!(other.join("more").is_dir(), "more was made on /dev/shm");

    super::undo_created_directories(&created);

    let on_shm = entry_names(&other);
    let in_base = entry_names(&dir.path);
    fs::remove_dir_all(&other).unwrap();
    assert!(on_shm.is_empty(), "left on /dev/shm: {on_shm:?}");
    assert_eq!(in_base, ["m"], "left in the base: {in_base:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn a_level_on_another_filesystem_is_taken_back_when_one_descriptor_is_free() {
    use std::os::unix::fs::MetadataExt as _;
    // As above, but rollback starts with one free descriptor slot. The
    // shared quarantine beside `new` takes it, so the cross-device fallback
    // for `more` must free that quarantine first, or it cannot read
    // `/dev/urandom` and strands `more` and `new` both.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::a_level_on_another_filesystem_is_taken_back_when_one_descriptor_is_free",
        );
        return;
    }
    let shm = std::path::Path::new("/dev/shm");
    let dir = TempDir::new("cross-device-pressure");
    let Ok(shm_meta) = fs::metadata(shm) else {
        panic!("this test needs /dev/shm");
    };
    assert_ne!(
        shm_meta.dev(),
        fs::metadata(&dir.path).unwrap().dev(),
        "this test needs /dev/shm on another filesystem than the temp dir"
    );
    let other = shm.join(format!(
        "pokeemerald-rs-cross-device-pressure-{}",
        std::process::id()
    ));
    fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(&other, dir.join("m")).unwrap();
    let target = dir.join("new").join("..").join("m").join("more");
    let created = super::create_directories(&target).unwrap();
    assert!(other.join("more").is_dir(), "more was made on /dev/shm");

    let mut fillers = None;
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        if fillers.is_none() {
            fillers = Some(fill_descriptor_table_leaving(1));
        }
    });
    drop(fillers);

    let on_shm = entry_names(&other);
    let in_base = entry_names(&dir.path);
    fs::remove_dir_all(&other).unwrap();
    assert!(on_shm.is_empty(), "left on /dev/shm: {on_shm:?}");
    assert_eq!(in_base, ["m"], "left in the base: {in_base:?}");
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_stranded_capture_is_reported_in_its_quarantines_current_place() {
    // The quarantine was made in `a`, so the location recorded then is
    // `a/.undo-held`; `a` is renamed to `b` before the capture is given up
    // on, and the warning must name where it is now, under `b`.
    let dir = TempDir::new("restore-renamed-quarantine-home");
    let created = super::create_directories(&dir.join("a").join("new")).unwrap();
    let level = created.last().unwrap();
    let parent = level.parent.as_deref().unwrap();
    let recorded = dir.join("a").join(".undo-held");
    fs::rename(dir.join("a"), dir.join("b")).unwrap();
    let moved = fs::canonicalize(dir.join("b")).unwrap();
    let quarantine = stage_unrestorable_capture(&moved, parent);

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &recorded,
        parent,
        level,
    );

    assert_eq!(
        warning.as_deref(),
        Some(
            format!(
                "warning: could not put {} back; its contents remain in {}",
                moved.join("new").display(),
                moved
                    .join(".undo-held")
                    .join(super::take_back::QUARANTINE_ENTRY)
                    .display()
            )
            .as_str()
        )
    );
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_quarantine_whose_removal_fails_is_reported_under_its_homes_current_name() {
    // The quarantine's home `a` is renamed to `b` after the quarantine is
    // made, then something lands in it so its removal fails; the warning
    // must name `b/.undo-r`, where it is, not `a/.undo-r`.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::a_quarantine_whose_removal_fails_is_reported_under_its_homes_current_name",
        );
        let warned = warned_path(
            &stderr,
            "warning: could not remove cleanup directory ",
            ": ",
        );
        assert_names_marked_path(&stderr, warned, "quarantine at: ");
        return;
    }
    let dir = TempDir::new("quarantine-home-renamed");
    let created = super::create_directories(&dir.join("a").join("new")).unwrap();
    let level = created.last().unwrap();
    let parent = level.parent.as_deref().unwrap();
    let moved = dir.join("b");
    let proceeded = super::take_back::take_back_level_with_hooks(
        parent,
        level,
        ".undo-r",
        &mut || fs::rename(dir.join("a"), &moved).unwrap(),
        &mut || fs::write(moved.join(".undo-r").join("held"), b"x").unwrap(),
    );
    assert!(proceeded, "the level itself was taken back");
    assert!(moved.join(".undo-r").join("held").is_file());
    mark_path("quarantine at: ", &moved.join(".undo-r"));
}
