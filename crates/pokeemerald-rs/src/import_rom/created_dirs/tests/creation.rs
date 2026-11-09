//! Creating the levels of a destination: which levels a run claims as its own, and how
//! it pins them under descriptor pressure and `..` components.

use super::*;

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

// Same platform boundary as the tests above; this pins
// `Dest::open_pinned` reaching a destination its pin cannot read.
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_vendor = "apple"
))]
#[test]
fn a_destination_acquired_from_a_pin_on_a_writable_but_not_readable_directory_takes_a_file() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = TempDir::new("pinned-unreadable-destination");
    let drop_box = dir.join("drop-box");
    fs::create_dir(&drop_box).expect("the destination is created");

    fs::set_permissions(&drop_box, fs::Permissions::from_mode(0o300))
        .expect("the destination closes");
    // A privileged user ignores directory permissions; see the tests above.
    let unreadable = fs::read_dir(&drop_box).is_err();
    let acquired = super::dest::open_traversal_directory(&drop_box)
        .and_then(|pin| super::dest::Dest::open_pinned(&pin));
    let created = acquired
        .as_ref()
        .map(|dest| dest.create_new(std::ffi::OsStr::new("probe")));
    let syncable = acquired.as_ref().is_ok_and(|dest| dest.handle().is_some());
    fs::set_permissions(&drop_box, fs::Permissions::from_mode(0o755))
        .expect("the destination reopens");

    if unreadable {
        created
            .expect("pinning and acquiring need only write and search")
            .expect("a writable, searchable destination takes a new name");
        assert!(drop_box.join("probe").is_file());
        assert!(!syncable, "an unreadable directory has no handle to sync");
    } else {
        eprintln!(
            "a_destination_acquired_from_a_pin_on_a_writable_but_not_readable_directory_takes_a_file: \
             skipped -- running privileged, 0o300 did not close the destination off"
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
fn a_created_level_keeps_owner_access_and_every_other_bit_as_the_umask_left_it() {
    use std::os::unix::fs::PermissionsExt as _;

    // The hook runs on each level right after `mkdirat` and before the
    // reopen, so it stands in for the umask having taken those bits.
    // Each level must be usable by the time the next is made inside it. Each
    // pair is the mode the umask left and the mode the level must end with.
    let mut left_by_umask: Vec<(u32, u32)> = vec![
        (0o500, 0o700),
        (0o555, 0o755),
        (0o750, 0o750),
        (0o600, 0o700),
    ];
    // Without owner read the read-mode pin of other Unix systems is refused.
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_vendor = "apple"
    ))]
    left_by_umask.extend([(0o333, 0o733), (0o300, 0o700)]);
    // Without owner search either, only an `O_PATH` pin still pins.
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
    left_by_umask.push((0o000, 0o700));
    let dir = TempDir::new("owner-access");
    let levels: Vec<PathBuf> = (0..left_by_umask.len())
        .scan(dir.path.clone(), |parent, index| {
            *parent = parent.join(format!("l{index}"));
            Some(parent.clone())
        })
        .collect();
    let mut next = 0;
    let mut hook = || -> Option<std::io::Error> {
        fs::set_permissions(
            &levels[next],
            fs::Permissions::from_mode(left_by_umask[next].0),
        )
        .unwrap();
        next += 1;
        None
    };

    let created = super::create_directories_with_hooks(
        levels.last().unwrap(),
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut hook,
        &mut || None,
    )
    .expect("every level is created, whatever the mode left");

    assert_eq!(created.len(), left_by_umask.len());
    assert!(
        created.iter().all(|level| level.own.is_some()),
        "every level is pinned"
    );
    let modes: Vec<u32> = levels.iter().map(|level| mode_of(level)).collect();
    let expected: Vec<u32> = left_by_umask.iter().map(|(_, end)| *end).collect();
    assert_eq!(modes, expected);
}

/// Whether mode bits do not bind this process: a `0o000` directory can
/// still be listed. Asked of the outcome rather than of the uid, so it needs
/// no libc.
#[cfg(unix)]
fn running_privileged(dir: &TempDir) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    let probe = dir.join("privilege-probe");
    fs::create_dir(&probe).unwrap();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o000)).unwrap();
    let privileged = fs::read_dir(&probe).is_ok();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir(&probe).unwrap();
    privileged
}

#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
#[test]
fn a_level_without_owner_search_but_with_read_is_restored_without_procfs() {
    // `0600` is what umask `0177` leaves. With the `/proc/self/fd` route
    // gone the held descriptor cannot restore it, so the real-descriptor
    // route must; a privileged runner passes either way, an unprivileged
    // one fails at the next level without it.
    let _no_procfs = super::dest::procfs_unavailable::make_unavailable();
    let dir = TempDir::new("owner-access-no-procfs");
    let levels: Vec<PathBuf> = ["a", "b", "c"]
        .iter()
        .scan(dir.path.clone(), |parent, name| {
            *parent = parent.join(name);
            Some(parent.clone())
        })
        .collect();
    let mut next = 0;
    let mut hook = || -> Option<std::io::Error> {
        use std::os::unix::fs::PermissionsExt as _;

        fs::set_permissions(&levels[next], fs::Permissions::from_mode(0o600)).unwrap();
        next += 1;
        None
    };

    super::create_directories_with_hooks(
        levels.last().unwrap(),
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut hook,
        &mut || None,
    )
    .expect("every level is created without procfs");

    for level in &levels {
        assert_eq!(mode_of(level), 0o700, "{level:?}");
    }
}

#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
#[test]
fn a_recovery_open_out_of_descriptors_sheds_a_pin_and_retries() {
    // The restore of a level without owner search opens it for reading; with
    // the table full that open answers `EMFILE`, which is the cue to shed a
    // pin and try again, not an umask failure.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::creation::a_recovery_open_out_of_descriptors_sheds_a_pin_and_retries",
        );
        return;
    }
    let _no_procfs = super::dest::procfs_unavailable::make_unavailable();
    let dir = TempDir::new("recovery-open-emfile");
    if running_privileged(&dir) {
        eprintln!("RECOVERY-EMFILE: privileged run, not exercised");
        return;
    }
    let levels: Vec<PathBuf> = (0..12)
        .scan(dir.path.clone(), |parent, index| {
            *parent = parent.join(format!("l{index}"));
            Some(parent.clone())
        })
        .collect();
    let mut next = 0;
    let mut hook = || -> Option<std::io::Error> {
        use std::os::unix::fs::PermissionsExt as _;

        fs::set_permissions(&levels[next], fs::Permissions::from_mode(0o600)).unwrap();
        next += 1;
        None
    };
    let fillers = fill_descriptor_table_leaving(4);
    let outcome = super::create_directories_with_hooks(
        levels.last().unwrap(),
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut hook,
        &mut || None,
    );
    drop(fillers);
    for level in &levels {
        unlock(level);
    }
    let created = outcome
        .map_err(|(created, source)| format!("failed after {} levels: {source}", created.len()))
        .expect("descriptor pressure on the recovery open is answered by shedding a pin");
    assert_eq!(created.len(), levels.len());
}

#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
#[test]
fn a_level_without_owner_search_or_read_fails_closed_without_procfs() {
    use std::os::unix::fs::PermissionsExt as _;

    // `0000` is what umask `0777` leaves: with procfs gone nothing bound to
    // the inode reaches it. It is refused, not `chmod`ed by name.
    let _no_procfs = super::dest::procfs_unavailable::make_unavailable();
    let dir = TempDir::new("owner-access-fails-closed");
    let privileged = running_privileged(&dir);
    let level = dir.join("closed");
    let mut hook = || -> Option<std::io::Error> {
        fs::set_permissions(&level, fs::Permissions::from_mode(0o000)).unwrap();
        None
    };

    let outcome = super::create_directories_with_hooks(
        &level,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut hook,
        &mut || None,
    );

    if privileged {
        // A privileged runner's `chmod` through the held descriptor is not
        // refused, so there is nothing to fail closed on.
        outcome.expect("a privileged runner restores the level");
        assert_eq!(mode_of(&level), 0o700);
        eprintln!(
            "a_level_without_owner_search_or_read_fails_closed_without_procfs: \
             privileged run, the refusal was not exercised"
        );
    } else {
        let (created, source) = outcome.expect_err("an unrestorable level fails the create");
        assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(source.to_string().contains("umask"), "{source}");
        assert_eq!(created_paths(&created), std::slice::from_ref(&level));
        assert!(
            created[0].own.is_some(),
            "the level stays recorded and pinned"
        );
        assert_eq!(mode_of(&level), 0o000, "the level is left as it was made");
        fs::set_permissions(&level, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn a_pin_the_mode_refused_is_retried_once_the_level_is_restored_through_a_descriptor() {
    use std::os::unix::fs::PermissionsExt as _;

    // Apple's `O_EXEC` answers `EACCES` for a level without owner search,
    // such as the `0600` umask `0177` leaves. The hook closes the level and
    // fails the first reopen the same way; the retry must find it restored.
    let dir = TempDir::new("pin-refused");
    let level = dir.join("closed");
    let mut calls = 0;
    let mut hook = || -> Option<std::io::Error> {
        calls += 1;
        (calls == 1).then(|| {
            fs::set_permissions(&level, fs::Permissions::from_mode(0o600)).unwrap();
            std::io::Error::from(rustix::io::Errno::ACCESS)
        })
    };

    let created = super::create_directories_with_hooks(
        &level,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut hook,
        &mut || None,
    )
    .expect("a refused pin is retried after the restore");

    assert_eq!(calls, 1, "the retry is not a second reopen attempt");
    assert!(created[0].own.is_some(), "the level ends up pinned");
    assert_eq!(mode_of(&level), 0o700);
}

#[cfg(unix)]
#[test]
fn a_pin_refused_for_a_level_without_owner_read_fails_closed() {
    use std::os::unix::fs::PermissionsExt as _;

    // `0000` / `0333` / `0300`: no inode-bound route, so the level is left
    // as it was made and recorded without a pin.
    let dir = TempDir::new("pin-refused-closed");
    let privileged = running_privileged(&dir);
    let level = dir.join("closed");
    let mut calls = 0;
    let mut hook = || -> Option<std::io::Error> {
        calls += 1;
        (calls == 1).then(|| {
            fs::set_permissions(&level, fs::Permissions::from_mode(0o000)).unwrap();
            std::io::Error::from(rustix::io::Errno::ACCESS)
        })
    };

    let outcome = super::create_directories_with_hooks(
        &level,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut hook,
        &mut || None,
    );

    if privileged {
        outcome.expect("a privileged runner can open the level");
        assert_eq!(mode_of(&level), 0o700);
        eprintln!(
            "a_pin_refused_for_a_level_without_owner_read_fails_closed: \
             privileged run, the refusal was not exercised"
        );
    } else {
        let (created, source) = outcome.expect_err("an unrestorable level fails the create");
        assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(source.to_string().contains("umask"), "{source}");
        assert_eq!(created_paths(&created), std::slice::from_ref(&level));
        assert!(
            created[0].own.is_none(),
            "the level is recorded without a pin"
        );
        assert_eq!(mode_of(&level), 0o000, "the level is left as it was made");
        fs::set_permissions(&level, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

/// A directory `level` in a fresh parent, with its parent descriptor and the
/// identity `statat` gave it.
#[cfg(unix)]
fn closed_level(
    tag: &str,
    mode: u32,
) -> (
    TempDir,
    std::os::fd::OwnedFd,
    std::ffi::OsString,
    rustix::fs::Stat,
) {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = TempDir::new(tag);
    let name = std::ffi::OsString::from("level");
    fs::create_dir(dir.join("level")).unwrap();
    fs::set_permissions(dir.join("level"), fs::Permissions::from_mode(mode)).unwrap();
    let parent = super::dest::open_traversal_directory(&dir.path).unwrap();
    let identity =
        rustix::fs::statat(&parent, &name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW).unwrap();
    (dir, parent, name, identity)
}

#[cfg(unix)]
fn mode_of(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;

    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// Opens `path` back up so the fixture can be cleaned up by a runner the mode
/// bits bind.
#[cfg(unix)]
fn unlock(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;

    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

#[cfg(unix)]
fn refuse_readable_open(
    _parent: &std::os::fd::OwnedFd,
    _name: &std::ffi::OsStr,
) -> std::io::Result<std::os::fd::OwnedFd> {
    Err(std::io::Error::from(rustix::io::Errno::ACCESS))
}

#[cfg(unix)]
#[test]
fn a_level_no_readable_open_reaches_is_refused_and_left_as_it_was() {
    // Mode `0000` is what umask `0777` leaves: nothing can open it, and no
    // `chmod` by name is tried.
    let (dir, parent, name, identity) = closed_level("fails-closed", 0o000);
    let outcome = owner_access::restore_by_name_with(
        &parent,
        &name,
        &identity,
        None,
        &mut refuse_readable_open,
    );
    assert!(matches!(outcome, Err(owner_access::Refused::Failed)));
    assert_eq!(mode_of(&dir.join("level")), 0o000);
    unlock(&dir.join("level"));
}

#[cfg(unix)]
#[test]
fn a_level_with_owner_read_is_restored_through_a_real_descriptor() {
    let (dir, parent, name, identity) = closed_level("by-fd", 0o640);
    owner_access::restore_by_name(&parent, &name, &identity, None).expect("restored");
    assert_eq!(mode_of(&dir.join("level")), 0o740);
}

#[cfg(unix)]
#[test]
fn a_replacement_present_before_the_restore_is_refused_and_left_alone() {
    use std::os::unix::fs::PermissionsExt as _;

    for (tag, real) in [("replaced-refused", false), ("replaced-real", true)] {
        let (dir, parent, name, identity) = closed_level(tag, 0o000);
        let level = dir.join("level");
        fs::rename(&level, dir.join("original")).unwrap();
        fs::create_dir(&level).unwrap();
        fs::set_permissions(&level, fs::Permissions::from_mode(0o555)).unwrap();

        let outcome = owner_access::restore_by_name_with(
            &parent,
            &name,
            &identity,
            None,
            &mut |parent, name| {
                if real {
                    super::dest::open_readable_created_at(parent, name)
                } else {
                    refuse_readable_open(parent, name)
                }
            },
        );

        assert!(outcome.is_err(), "{tag}: {outcome:?}");
        if real {
            assert!(
                matches!(outcome, Err(owner_access::Refused::Replaced(_))),
                "{tag}: the identity check names the swap"
            );
        }
        assert_eq!(
            mode_of(&level),
            0o555,
            "{tag}: the replacement is untouched"
        );
        unlock(&dir.join("original"));
        unlock(&level);
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_swapped_in_for_a_closed_level_leaves_its_referent_alone() {
    use std::os::unix::fs::PermissionsExt as _;

    // The regression for a `chmod` by name: a symlink to an owner-controlled
    // `0444` file lands in the level's name -- before the restore, and
    // while it runs, after the readable open was refused. Nothing may touch
    // the referent.
    for swap_during in [false, true] {
        let (dir, parent, name, identity) = closed_level("replaced-symlink", 0o000);
        let referent = dir.join("referent");
        fs::write(&referent, b"owner file").unwrap();
        fs::set_permissions(&referent, fs::Permissions::from_mode(0o444)).unwrap();
        let level = dir.join("level");
        let swap = || {
            fs::remove_dir(&level).unwrap();
            std::os::unix::fs::symlink(&referent, &level).unwrap();
        };
        if !swap_during {
            swap();
        }

        let outcome = owner_access::restore_by_name_with(
            &parent,
            &name,
            &identity,
            None,
            &mut |parent, name| {
                if swap_during {
                    swap();
                }
                super::dest::open_readable_created_at(parent, name)
            },
        );

        assert!(outcome.is_err(), "swap_during={swap_during}: {outcome:?}");
        assert_eq!(
            mode_of(&referent),
            0o444,
            "swap_during={swap_during}: the symlink's referent is untouched"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_level_that_cannot_be_examined_is_not_restored() {
    let (dir, parent, name, identity) = closed_level("vanished", 0o000);
    fs::remove_dir(dir.join("level")).unwrap();
    let outcome = owner_access::restore_by_name_with(
        &parent,
        &name,
        &identity,
        None,
        &mut super::dest::open_readable_created_at,
    );
    assert!(outcome.is_err());
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
            "import_rom::created_dirs::tests::creation::a_valid_deep_destination_creates_or_fully_rolls_back",
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
