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
    super::sync_created_directories_with(&mut created, &mut |parent| {
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
