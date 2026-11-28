//! Owner access on a created level: the bits a umask took are restored
//! before the next level is made, through a descriptor tied to the level,
//! and a level no such route reaches fails closed and is left as it was.

use super::creation::created_paths;
use super::*;

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
            "import_rom::created_dirs::tests::owner_access_restore::a_recovery_open_out_of_descriptors_sheds_a_pin_and_retries",
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

fn mode_of(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;

    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// Opens `path` back up so the fixture can be cleaned up by a runner the mode
/// bits bind.
fn unlock(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;

    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

fn refuse_readable_open(
    _parent: &std::os::fd::OwnedFd,
    _name: &std::ffi::OsStr,
) -> std::io::Result<std::os::fd::OwnedFd> {
    Err(std::io::Error::from(rustix::io::Errno::ACCESS))
}

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

#[test]
fn a_level_with_owner_read_is_restored_through_a_real_descriptor() {
    let (dir, parent, name, identity) = closed_level("by-fd", 0o640);
    owner_access::restore_by_name(&parent, &name, &identity, None).expect("restored");
    assert_eq!(mode_of(&dir.join("level")), 0o740);
}

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
