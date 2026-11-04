//! Claiming a level right after `mkdirat` made it, where a writer of the
//! parent may take its name. Where the platform can rename without
//! replacing, a level is made under a private name first; elsewhere it is
//! made at its final name, and the tests say which contract they check.

use super::*;

/// Creates `target` as `owner`, running `after_mkdir` right after each
/// level's `mkdirat` succeeded, before anything looks at the name.
fn create_as(
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
fn private_entries(dir: &std::path::Path) -> Vec<String> {
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

#[test]
fn an_existing_level_under_a_read_only_parent_is_walked_through() {
    use std::os::unix::fs::PermissionsExt as _;

    // Walking through a level that is already there asks nothing of its
    // parent: `dev` opened it, and a claim that first made a private entry
    // in that parent failed with `EACCES` under a parent this user cannot
    // write. Only the new level below it needs a writable directory. The
    // walk lists `read-only/existing` as missing because `new/..` does not
    // exist yet when the list is made, so it reaches both by name.
    let dir = TempDir::new("existing-under-read-only");
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
        eprintln!(
            "an_existing_level_under_a_read_only_parent_is_walked_through: \
             skipped -- running privileged, 0o555 did not close the parent off"
        );
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
        ["new", "c"],
        "only the levels that were missing are recorded"
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
    let removed = super::acquire::discard_private(&parent, &name, &aside, &held, &mut || {});

    assert!(removed, "the verified directory was not removed");
    assert_eq!(entry_names(&dir.path), Vec::<String>::new());
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[test]
fn a_replacement_put_at_the_private_name_after_it_was_verified_is_not_removed() {
    // Removing the private directory used to be a check of its identity and
    // then an unlink of the same name, two steps a writer of the parent
    // could come between to have its own directory deleted. The seam is
    // that gap: the verified directory is moved away and another put at the
    // name, which must come out of the cleanup where it was.
    let dir = TempDir::new("discard-replaced");
    let (parent, name, held) = private_directory_in(&dir.path);
    let moved = dir.join("moved");
    let replacement = dir.path.join(&name);
    let mut swapped = 0;

    let aside = std::ffi::OsString::from(".import-fedcba9876543210");
    let removed = super::acquire::discard_private(&parent, &name, &aside, &held, &mut || {
        swapped += 1;
        fs::rename(&replacement, &moved).unwrap();
        fs::create_dir(&replacement).unwrap();
    });

    assert_eq!(swapped, 1, "the replacement was put in place once");
    assert!(moved.is_dir(), "the verified directory was removed");
    assert!(
        !removed,
        "the verified directory is gone, so nothing was removed"
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

#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
#[test]
fn a_level_is_made_at_its_final_name_where_no_rename_can_refuse_to_replace() {
    // The fallback contract: the directory is made at the name itself, so
    // the seam sees it there and nothing is made privately. What it does not
    // give is protection from a same-user writer replacing it in the gap.
    let dir = TempDir::new("final-name");
    let first = dir.join("a");
    let target = first.join("b");
    let owner = rustix::process::geteuid().as_raw();
    let mut seen = Vec::new();

    let creation = create_as(&target, owner, &mut || {
        seen.push(entry_names(&dir.path));
    })
    .expect("both levels are created");

    assert_eq!(creation.created.len(), 2);
    assert_eq!(seen[0], ["a"], "the level is already at its final name");
}
