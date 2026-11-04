//! Claiming a level right after `mkdirat` made it, where a writer of the
//! parent may take its name.

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
fn a_level_of_another_user_is_refused_and_leaves_nothing_behind() {
    // Swapping in a directory of another user needs root, so the walk is
    // told to expect a different user instead: what is refused is a
    // directory it cannot vouch is its user's.
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
    assert_eq!(entry_names(&dir.path), Vec::<String>::new());
}

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
