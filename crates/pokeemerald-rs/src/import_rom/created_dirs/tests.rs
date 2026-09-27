use std::fs;
use std::path::PathBuf;

use super::super::tests::TempDir;
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

// Linux, Android, FreeBSD, and macOS 13 or later are where `dest`'s own
// traversal opens (`open_traversal_directory`, `open_directory_at`,
// `open_created_directory_at`) actually ask a pinned parent for no more
// than write and search -- see their own docs in `dest.rs` for exactly
// which arm each of those four takes and why. Every other Unix this
// crate builds for, and an Apple kernel older than Ventura (#1312),
// still falls back to a real, read-requiring open, so this assertion is
// not a `cfg(unix)`-wide guarantee and must not claim to be one.
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
        // Running as a privileged user (this crate's own CI containers,
        // in particular) makes `0o300` above no closer-off at all, so the
        // assertion this test exists for never runs. Said out loud rather
        // than passed vacuously and silently, so a report reading test
        // output sees why zero assertions is the right count here, not a
        // regression that quietly stopped asserting.
        eprintln!(
            "a_level_is_created_under_a_parent_that_is_writable_but_not_readable: \
             skipped -- running privileged, 0o300 did not close the parent off"
        );
    }
}

// Same platform boundary as
// `a_level_is_created_under_a_parent_that_is_writable_but_not_readable`
// above, and for the same reason: this pins `open_traversal_directory`
// itself, not the whole `create_directories` path, but the guarantee it
// asserts is exactly as platform-scoped.
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
    // Another account removes the empty level this run made and makes its
    // own at the same name before the cleanup runs. ext4 and XFS commonly
    // hand the freed inode number straight to that replacement, so a
    // device and inode comparison alone would take it for this run's level
    // and `unlinkat` it. The record's held descriptor keeps the original
    // inode allocated, so the filesystem cannot reuse it; the recorded
    // identity is overwritten with the replacement's here to stand in for a
    // filesystem that did, on any filesystem this runs on.
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
    // A level recorded without its own descriptor (the reopen right after
    // `mkdirat` failed) has nothing pinning its inode, so the cleanup cannot
    // tell it from a same-number replacement and leaves it alone. Driven
    // through the injected `before_reopen` hook rather than clearing `own`
    // by hand afterwards, so this exercises the real reopen-failure branch
    // instead of a stand-in for its end state.
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
    // Pin budget is `MAX_PINNED_LEVELS`; a few levels beyond it exercises
    // both sides of the cutoff without building an unnecessarily deep tree.
    let dir = TempDir::new("pin-budget");
    let total = super::MAX_PINNED_LEVELS + 5;
    let pin_from = total - super::MAX_PINNED_LEVELS;
    let mut level = dir.join("d");
    for _ in 1..total {
        level = level.join("d");
    }

    let created = create_directories(&level)
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
    // `directories_to_create`'s lexical walk counts a `..` as a level of its
    // own -- it names no new component, so it never reaches `created` -- and
    // the pin budget must be measured against what actually lands there, not
    // against that longer lexical list. Twenty missing levels descended into
    // and twenty `..` climbed back out, then a final component: 41 lexical
    // levels but only 21 real directories, comfortably inside
    // `MAX_PINNED_LEVELS`.
    let dir = TempDir::new("dotdot-budget");
    let mut level = dir.join("d");
    for _ in 1..20 {
        level = level.join("d");
    }
    for _ in 0..20 {
        level = level.join("..");
    }
    level = level.join("final");

    let created = create_directories(&level)
        .expect("a destination that fits the budget is still created through its dotdot levels");

    assert_eq!(created.len(), 21);
    for (index, entry) in created.iter().enumerate() {
        assert!(entry.own.is_some(), "level {index} fits the pin budget");
        assert!(entry.parent.is_some(), "level {index} fits the pin budget");
    }
    assert!(level.is_dir());
}

#[cfg(unix)]
#[test]
fn a_valid_deep_destination_creates_or_fully_rolls_back() {
    // Meant to also be run under a tight `RLIMIT_NOFILE` (see the PR record
    // for `prlimit --nofile=32/36/64` results); at the ordinary limit this
    // always takes the `Ok` arm. Either way, a valid destination must never
    // come out half made: full creation, or nothing left behind.
    let dir = TempDir::new("valid-deep-fd");
    let total = 37;
    let mut level = dir.join("d");
    for _ in 1..total {
        level = level.join("d");
    }
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
    // Six levels; the last one's reopen answers `EMFILE` three times running
    // before finally deferring to the real reopen, driving the real
    // shedding-and-retry path (not a stand-in for its end state) through
    // three consecutive `shed_outermost_pinned_level` calls -- one per
    // level it must fully release before the retry can succeed.
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
    // A `..` level used to be reopened by its whole path, walking every
    // component before it again -- including one this run already created
    // and pinned. Another account could swap that component for a symlink
    // into a tree of its own after the pin, landing the reopen there; the
    // level *after* the `..` would then be created inside the attacker's
    // tree, through an `mkdirat` the pinned descent was supposed to keep
    // out of reach.
    //
    // The swap lands through `create_directories_with_hooks`'s injected
    // `before_dotdot` hook, once both levels ahead of the `..` are made and
    // pinned and before the `..` is resolved -- the one point the race
    // matters, every run, with no second thread to schedule.
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
