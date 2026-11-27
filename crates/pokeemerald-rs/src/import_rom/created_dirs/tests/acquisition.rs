//! Claiming a level right after `mkdirat` made it, where a writer of the
//! parent may take its name. Where the platform can rename without
//! replacing, a level is made under a private name first; elsewhere it is
//! made at its final name, and the tests say which contract they check.

use super::*;

/// Creates `target` as `owner`, running `after_mkdir` right after each
/// level's `mkdirat` succeeded, before anything looks at the name.
pub(super) fn create_as(
    target: &std::path::Path,
    owner: u32,
    after_mkdir: &mut dyn FnMut(),
) -> Result<super::DirectoryCreation, (Vec<CreatedDirectory>, std::io::Error)> {
    super::create_directories_pinned_after_mkdir(
        target,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut || None,
        &mut || None,
        &mut dest::reopen_for_sync,
        owner,
        after_mkdir,
    )
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
/// The names in `dir` the creation made privately.
pub(super) fn private_entries(dir: &std::path::Path) -> Vec<String> {
    if !dir.is_dir() {
        return Vec::new();
    }
    entry_names(dir)
        .into_iter()
        .filter(|name| name.starts_with(".import-"))
        .collect()
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_replacement_put_at_a_levels_name_after_mkdir_is_not_claimed() {
    // The hook runs right after `mkdirat` succeeded, before anything looks
    // at the new directory -- the window in which `statat` used to take a
    // replacement's identity, after which every check against it agreed. It
    // puts an empty directory of this same user at the level's name, moving
    // the one `mkdirat` made out of the way if that is already there.
    let dir = TempDir::new("swap-after-mkdir");
    let first = dir.join("a");
    let target = first.join("b");
    let moved = dir.join("a-original");
    let owner = rustix::process::geteuid().as_raw();
    let mut swapped = 0;

    let creation = create_as(&target, owner, &mut || {
        if swapped == 0 {
            swapped += 1;
            if first.exists() {
                fs::rename(&first, &moved).unwrap();
            }
            fs::create_dir(&first).unwrap();
        }
    })
    .expect("the destination is created inside the directory that was there");

    assert_eq!(swapped, 1, "the replacement was put in place once");
    assert_eq!(
        creation
            .created
            .iter()
            .map(|level| level.name.clone())
            .collect::<Vec<_>>(),
        ["b"],
        "only the level made inside the replacement is recorded, not the replacement"
    );
    super::undo_created_directories(&creation.created);
    assert!(first.is_dir(), "rollback removed the replacement");
    assert!(!target.exists(), "the level inside it is taken back");
    assert_eq!(private_entries(&dir.path), Vec::<String>::new());
    assert_eq!(private_entries(&first), Vec::<String>::new());
}

// A level made at its final name is never vouched for, only walked through:
// `final_name::a_level_of_another_user_made_at_its_final_name_is_walked_through`
// on Unix targets without a no-replace rename.
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_level_it_cannot_vouch_for_is_refused_and_left_untouched() {
    // Swapping in a directory of another user needs root, so the walk is
    // told to expect a different user instead: what is refused is a
    // directory it cannot vouch is its user's. Whatever failed once the
    // directory was made is the creation's failure: the name is not taken
    // for an existing level, nothing is recorded or descended into, and the
    // directory is not removed, since it was never shown to be ours.
    let dir = TempDir::new("level-of-another-user");
    let target = dir.join("a").join("b");
    let owner = rustix::process::geteuid().as_raw();
    let mut after_mkdir = 0;

    let result = create_as(&target, owner.wrapping_add(1), &mut || after_mkdir += 1);

    assert_eq!(after_mkdir, 1, "the walk stopped at the first level");
    let Err((created, _)) = result else {
        panic!("a level of another user was accepted as the level `mkdirat` made");
    };
    assert!(created.is_empty(), "the level was recorded");
    assert!(!target.exists(), "a level was created inside it");
    assert_eq!(
        entry_names(&dir.path).len(),
        1,
        "the unvouched directory was removed or another was made"
    );
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_private_directory_replaced_before_it_is_vouched_for_is_not_removed() {
    // The seam moves the directory `mkdirat` made away and puts another at
    // its private name. The walk cannot vouch for either, so cleanup must
    // not delete what it never verified.
    let dir = TempDir::new("private-replaced");
    let target = dir.join("a").join("b");
    let moved = dir.join("moved");
    let owner = rustix::process::geteuid().as_raw();

    let result = create_as(&target, owner.wrapping_add(1), &mut || {
        let private = dir.join(&private_entries(&dir.path)[0]);
        fs::rename(&private, &moved).unwrap();
        fs::create_dir(&private).unwrap();
    });

    assert!(result.is_err(), "an unvouched directory was accepted");
    assert!(moved.is_dir(), "the directory `mkdirat` made was removed");
    assert_eq!(
        private_entries(&dir.path).len(),
        1,
        "the replacement under the private name was removed"
    );
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_level_is_made_under_a_private_name_until_it_is_claimed() {
    let dir = TempDir::new("after-mkdir-seam");
    let first = dir.join("a");
    let target = first.join("b");
    let owner = rustix::process::geteuid().as_raw();
    let mut seen = Vec::new();

    let creation = create_as(&target, owner, &mut || {
        seen.push((
            private_entries(&dir.path).len() + private_entries(&first).len(),
            first.join("b").exists(),
            first.exists() && private_entries(&dir.path).is_empty(),
        ));
    })
    .expect("both levels are created");

    assert_eq!(creation.created.len(), 2);
    assert_eq!(
        seen,
        [(1, false, false), (1, false, true)],
        "each level exists only under its private name when the seam runs"
    );
    assert_eq!(private_entries(&dir.path), Vec::<String>::new());
    assert_eq!(private_entries(&first), Vec::<String>::new());
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn an_existing_level_under_a_read_only_parent_is_walked_through() {
    // Only the levels that were missing are recorded, each claimed under a
    // private name; `final_name` checks the same walk where none is claimed.
    walk_through_a_read_only_parent("existing-under-read-only", &["new", "c"]);
}

/// Walking through a level that is already there asks nothing of its
/// parent: `dev` opened it, and a claim that first made a private entry in
/// that parent failed with `EACCES` under a parent this user cannot write.
/// Only the new level below it needs a writable directory. The walk lists
/// `read-only/existing` as missing because `new/..` does not exist yet when
/// the list is made, so it reaches both by name. `recorded` is what the run
/// must record.
pub(super) fn walk_through_a_read_only_parent(label: &str, recorded: &[&str]) {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = TempDir::new(label);
    let parent = dir.join("read-only");
    let existing = parent.join("existing");
    let target = dir
        .join("new")
        .join("..")
        .join("read-only")
        .join("existing")
        .join("c");
    fs::create_dir_all(&existing).expect("the existing levels are created");

    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).expect("the parent closes");
    // A privileged user ignores directory permissions, so the close-off
    // does not block them and there is nothing to assert.
    let writable = fs::create_dir(parent.join("probe")).is_ok();
    let owner = rustix::process::geteuid().as_raw();
    let created = create_as(&target, owner, &mut || {});
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).expect("the parent reopens");

    if writable {
        eprintln!("{label}: skipped -- running privileged, 0o555 did not close the parent off");
        return;
    }
    let created = created
        .map_err(|(_, source)| source)
        .expect("an existing level is walked through whatever its parent allows");
    assert_eq!(
        created
            .created
            .iter()
            .map(|level| level.name.clone())
            .collect::<Vec<_>>(),
        recorded,
        "only the levels that were missing and claimed are recorded"
    );
    assert!(existing.join("c").is_dir());
}

/// A private directory in `dir` as `claim_level` leaves one after a lost
/// rename: its name, the parent it is in, and a descriptor open on it.
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
fn private_directory_in(
    dir: &std::path::Path,
) -> (
    std::rc::Rc<std::os::fd::OwnedFd>,
    std::ffi::OsString,
    std::os::fd::OwnedFd,
) {
    let parent = std::rc::Rc::new(dest::open_traversal_directory(dir).unwrap());
    let name = std::ffi::OsString::from(".import-0123456789abcdef");
    fs::create_dir(dir.join(&name)).unwrap();
    let held = dest::open_created_directory_at(&parent, &name).unwrap();
    (parent, name, held)
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_private_directory_that_is_still_the_verified_one_is_removed() {
    let dir = TempDir::new("discard-unchanged");
    let (parent, name, held) = private_directory_in(&dir.path);

    let aside = std::ffi::OsString::from(".import-fedcba9876543210");
    let outcome = super::acquire::discard_private(&parent, &name, &aside, &held, &mut || {});

    assert!(
        matches!(outcome, super::acquire::Discard::Removed),
        "the verified directory was not removed"
    );
    assert_eq!(entry_names(&dir.path), Vec::<String>::new());
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_replacement_put_at_the_private_name_after_it_was_verified_is_not_removed() {
    // Before cleanup looks at the private name, the verified directory is
    // moved away and another put at the name, which must come out of the
    // cleanup where it was. The gap between the check and the unlink is
    // `a_replacement_put_at_the_private_name_once_it_was_checked_is_not_removed`.
    let dir = TempDir::new("discard-replaced");
    let (parent, name, held) = private_directory_in(&dir.path);
    let moved = dir.join("moved");
    let replacement = dir.path.join(&name);
    let mut swapped = 0;

    let aside = std::ffi::OsString::from(".import-fedcba9876543210");
    let outcome = super::acquire::discard_private(&parent, &name, &aside, &held, &mut || {
        swapped += 1;
        fs::rename(&replacement, &moved).unwrap();
        fs::create_dir(&replacement).unwrap();
    });

    assert_eq!(swapped, 1, "the replacement was put in place once");
    assert!(moved.is_dir(), "the verified directory was removed");
    assert!(
        matches!(&outcome, super::acquire::Discard::Foreign { at } if *at == name),
        "the replacement stands at the private name and is reported there"
    );
    assert!(
        replacement.is_dir(),
        "the empty replacement was removed or left under another name"
    );
    assert_eq!(
        entry_names(&dir.path),
        [".import-0123456789abcdef", "moved"]
    );
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_replacement_put_at_the_private_name_once_it_was_checked_is_not_removed() {
    // The gap the cleanup closes: between finding the directory to be the
    // verified one and unlinking it, a writer of the parent moves it away
    // from the private name, if it is still there, and puts its own there.
    // Cleanup unlinks only the entry it checked, under the name nobody else
    // knows, so the replacement at the private name is left standing; a
    // check of the private name followed by an unlink of it would delete
    // the replacement.
    let dir = TempDir::new("discard-replaced-after-check");
    let (parent, name, held) = private_directory_in(&dir.path);
    let moved = dir.join("moved");
    let replacement = dir.path.join(&name);
    let mut swapped = 0;

    let aside = std::ffi::OsString::from(".import-fedcba9876543210");
    let outcome = super::acquire::discard_private_with(
        &parent,
        &name,
        &aside,
        &held,
        &mut || {},
        &mut || {
            swapped += 1;
            if replacement.exists() {
                fs::rename(&replacement, &moved).unwrap();
            }
            fs::create_dir(&replacement).unwrap();
        },
    );

    assert_eq!(swapped, 1, "the replacement was put in place once");
    assert!(
        replacement.is_dir(),
        "the replacement put at the private name after the check was removed"
    );
    assert!(
        matches!(outcome, super::acquire::Discard::Removed),
        "the verified directory was not removed"
    );
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_private_directory_that_became_nonempty_is_left_where_the_caller_reports_it() {
    // The directory is the verified one, moved aside, and then cannot be
    // unlinked because an entry appeared in it. It stands at the aside name,
    // not the private one, and that is the pathname and error reported.
    let dir = TempDir::new("discard-nonempty");
    let (parent, name, held) = private_directory_in(&dir.path);
    let aside = std::ffi::OsString::from(".import-fedcba9876543210");
    let entry = dir.path.join(&name).join("entry");

    let outcome = super::acquire::discard_private(&parent, &name, &aside, &held, &mut || {
        fs::write(&entry, b"x").unwrap();
    });

    match outcome {
        super::acquire::Discard::Kept { at, error } => {
            assert_eq!(at, aside, "reported where it was not left");
            assert!(
                dir.path.join(&at).join("entry").is_file(),
                "the directory is not at the pathname reported"
            );
            assert_eq!(
                error.raw_os_error(),
                Some(rustix::io::Errno::NOTEMPTY.raw_os_error())
            );
        }
        _ => panic!("a directory that could not be unlinked was not reported as kept"),
    }
    assert!(
        !dir.path.join(&name).exists(),
        "nothing is left at the private name"
    );
    assert_eq!(entry_names(&dir.path), [".import-fedcba9876543210"]);
}

#[test]
fn only_a_directory_of_the_given_user_is_made_by_that_user() {
    let dir = TempDir::new("made-by");
    let owner = rustix::process::geteuid().as_raw();
    let directory = rustix::fs::stat(&dir.path).unwrap();
    let file_path = dir.join("file");
    fs::write(&file_path, b"x").unwrap();
    let file = rustix::fs::stat(&file_path).unwrap();

    assert!(super::acquire::made_by(&directory, owner));
    assert!(
        !super::acquire::made_by(&directory, owner.wrapping_add(1)),
        "a directory of another user's was vouched for"
    );
    assert!(
        !super::acquire::made_by(&file, owner),
        "a file was vouched for as a level"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_private_directory_whose_open_fails_is_not_left_behind_silently() {
    // The seam runs right after `mkdirat` made the private directory and
    // fills the descriptor table, so the open in `identity_of` fails with
    // `EMFILE` with no pin to shed. The directory cannot be shown to be the
    // one that was made, so it is not removed; its location is named in the
    // failure instead.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::acquisition::a_private_directory_whose_open_fails_is_not_left_behind_silently",
        );
        return;
    }
    let dir = TempDir::new("private-open-fails");
    let target = dir.join("a");
    let owner = rustix::process::geteuid().as_raw();
    let mut fillers = None;

    let result = create_as(&target, owner, &mut || {
        fillers = Some(fill_descriptor_table_leaving(0));
    });
    drop(fillers);

    let Err((created, source)) = result else {
        panic!("creation succeeded although the open failed");
    };
    assert!(created.is_empty());
    let left = private_entries(&dir.path);
    let message = source.to_string();
    assert!(
        left.is_empty() || left.iter().all(|name| message.contains(name.as_str())),
        "private directory {left:?} left behind; failure was only: {message}"
    );
}

/// How many of this process's descriptors are open on the directory `path`
/// names now, read through `/proc/self/fd`.
#[cfg(target_os = "linux")]
fn descriptors_on(path: &std::path::Path) -> usize {
    use std::os::unix::fs::MetadataExt as _;
    let level = fs::symlink_metadata(path).unwrap();
    fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| fs::metadata(entry.ok()?.path()).ok())
        .filter(|found| found.dev() == level.dev() && found.ino() == level.ino())
        .count()
}

/// Creates the one missing level `dir/a` and, just before its reopen,
/// counts the descriptors held on it, then removes it and makes a fresh
/// directory at its name, as a same-user writer of `dir` could. Answers the
/// count and the creation's outcome.
#[cfg(target_os = "linux")]
fn replace_level_before_its_reopen(
    dir: &TempDir,
) -> (
    usize,
    Result<super::DirectoryCreation, (Vec<CreatedDirectory>, std::io::Error)>,
) {
    let level = dir.join("a");
    let mut held = None;
    let result = super::create_directories_pinned_after_mkdir(
        &level,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut || {
            if held.is_none() {
                held = Some(descriptors_on(&level));
                fs::remove_dir(&level).unwrap();
                fs::create_dir(&level).unwrap();
            }
            None
        },
        &mut || None,
        &mut dest::reopen_for_sync,
        rustix::process::geteuid().as_raw(),
        &mut || {},
    );
    (held.expect("the reopen was reached"), result)
}

#[cfg(target_os = "linux")]
#[test]
fn a_claimed_level_stays_open_until_its_reopen_is_checked() {
    // Between the claim and the reopen the level is synced into its parent,
    // and a same-user writer of the parent can remove it there. Were nothing
    // holding the level open, its inode could be freed and handed to a
    // directory made at the name next, which the reopen's device-and-inode
    // check would take for the level this run made. The claim's descriptor
    // must still be open when the reopen runs, so the replacement is refused.
    // A level made at its final name is never claimed, so has no reopen; see
    // `a_replacement_put_at_a_levels_name_after_the_fallback_mkdir_is_not_claimed`.
    let dir = TempDir::new("held-until-reopen");
    let (held, result) = replace_level_before_its_reopen(&dir);
    assert_eq!(held, 1, "the claimed level is held open until its reopen");
    let (created, _) = result.expect_err("the replacement is not taken for the level");
    assert!(created.is_empty(), "the replacement is not recorded");
    assert!(dir.join("a").is_dir(), "the replacement is left standing");
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_private_directory_warning_keeps_a_control_character_in_a_name_on_one_line() {
    // The level's name comes from the destination path, which may hold a
    // newline or a terminal escape; the warning must stay one line and show
    // it escaped, as the rollback warnings do.
    let name = std::ffi::OsStr::new("line\nbreak\u{1b}[2J");
    let outcomes = [
        super::acquire::Discard::Kept {
            at: ".import-0123456789abcdef".into(),
            error: std::io::Error::other("busy"),
        },
        super::acquire::Discard::Foreign {
            at: ".import-0123456789abcdef".into(),
        },
    ];
    for outcome in &outcomes {
        let warning = super::acquire::left_standing_warning(outcome, name).expect("a warning");
        assert!(!warning.contains('\n'), "{warning:?}");
        assert!(!warning.contains('\u{1b}'), "{warning:?}");
        assert!(warning.contains("line\\nbreak"), "{warning:?}");
    }
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_private_directory_moved_away_before_cleanup_is_not_reported_as_standing() {
    // A writer of the parent moves the private directory away before cleanup
    // reaches it. Nothing is at the private name any more, so the warning
    // must not send the user to check a directory there.
    let dir = TempDir::new("discard-moved-away");
    let (parent, name, held) = private_directory_in(&dir.path);
    let moved = dir.join("moved");
    let private = dir.path.join(&name);
    let aside = std::ffi::OsString::from(".import-fedcba9876543210");

    let outcome = super::acquire::discard_private(&parent, &name, &aside, &held, &mut || {
        fs::rename(&private, &moved).unwrap();
    });

    assert!(moved.is_dir(), "cleanup removed the directory moved away");
    let warning = super::acquire::left_standing_warning(&outcome, std::ffi::OsStr::new("a"))
        .expect("the user is told the directory was not removed");
    assert!(
        !warning.contains("check it before deleting") && warning.contains("moved away"),
        "{warning:?}"
    );
}
