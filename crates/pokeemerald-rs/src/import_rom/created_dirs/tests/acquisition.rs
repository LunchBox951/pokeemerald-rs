//! Vouching for a level right after `mkdirat` made it, where the name may
//! already hold a replacement.

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

#[test]
fn a_level_of_another_user_is_neither_recorded_nor_descended_into() {
    // Swapping in a directory of another user needs root, so this makes the
    // level `mkdirat` just made stand for one by expecting a different
    // user: what is refused is a level the walk cannot vouch is its user's.
    let dir = TempDir::new("level-of-another-user");
    let first = dir.join("a");
    let target = first.join("b");
    let owner = rustix::process::geteuid().as_raw();
    let mut after_mkdir = 0;

    let result = create_as(&target, owner.wrapping_add(1), &mut || after_mkdir += 1);

    assert_eq!(after_mkdir, 1, "the walk stopped at the first level");
    let Err((created, _)) = result else {
        panic!("a level of another user was accepted as the level `mkdirat` made");
    };
    assert!(created.is_empty(), "the level was recorded");
    assert!(!target.exists(), "a level was created inside it");
    super::undo_created_directories(&created);
    assert!(first.is_dir(), "rollback removed a level it never recorded");
}

#[test]
fn a_replacement_of_another_user_swapped_in_after_mkdir_is_neither_recorded_nor_descended_into() {
    // The seam sits after `mkdirat` and before the `statat` that used to take
    // the replacement's identity, after which every check against it agreed.
    // Only root can hand a replacement to another user, so this one runs
    // there; `a_level_of_another_user_is_neither_recorded_nor_descended_into`
    // covers the same refusal for everyone else.
    if rustix::process::geteuid().as_raw() != 0 {
        eprintln!("skipped: chowning the replacement to another user needs root");
        return;
    }
    let dir = TempDir::new("swap-after-mkdir");
    let first = dir.join("a");
    let target = first.join("b");
    let moved = dir.join("a-original");

    let result = create_as(&target, 0, &mut || {
        fs::rename(&first, &moved).unwrap();
        fs::create_dir(&first).unwrap();
        rustix::fs::chown(&first, Some(rustix::fs::Uid::from_raw(65534)), None).unwrap();
    });

    let Err((created, _)) = result else {
        panic!("a replacement was accepted as the level `mkdirat` made");
    };
    assert!(created.is_empty(), "the replacement was recorded");
    assert!(
        !target.exists(),
        "a level was created inside the replacement"
    );
    super::undo_created_directories(&created);
    assert!(first.is_dir(), "rollback removed the replacement");
    assert!(moved.is_dir(), "the level `mkdirat` made is left standing");
}

#[test]
fn a_level_is_looked_at_only_after_its_mkdir_has_succeeded() {
    let dir = TempDir::new("after-mkdir-seam");
    let target = dir.join("a").join("b");
    let owner = rustix::process::geteuid().as_raw();
    let mut seen = Vec::new();

    let creation = create_as(&target, owner, &mut || {
        seen.push((dir.join("a").is_dir(), target.is_dir()));
    })
    .expect("both levels are created");

    assert_eq!(creation.created.len(), 2);
    assert_eq!(
        seen,
        [(true, false), (true, true)],
        "the seam runs once per level, with its directory already made"
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
