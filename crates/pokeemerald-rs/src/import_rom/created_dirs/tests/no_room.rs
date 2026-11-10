//! Rollback on a filesystem with no room for the quarantine's `mkdirat`:
//! each level is moved aside to a random name beside it instead, re-checked
//! there, and unlinked, or put back.

use std::os::fd::BorrowedFd;

use super::super::take_back::{undo_created_directories_making_with, CaptureStage};
use super::*;

/// A `mkdirat` that fails with `err`, as a full filesystem does.
#[cfg(unix)]
fn mkdir_failing(
    err: rustix::io::Errno,
) -> impl FnMut(BorrowedFd<'_>, &str) -> rustix::io::Result<()> {
    move |_, _| Err(err)
}

/// Rolls `created` back with every quarantine `mkdirat` failing with `err`,
/// running `in_aside` at each capture stage with the level's parent.
#[cfg(unix)]
fn undo_failing(
    created: &[CreatedDirectory],
    err: rustix::io::Errno,
    before_take: &mut dyn FnMut(),
    in_aside: &mut dyn FnMut(CaptureStage, BorrowedFd<'_>),
) {
    undo_created_directories_making_with(created, before_take, in_aside, &mut mkdir_failing(err));
}

/// The one `.undo-<hex>` name in `dir` right now.
#[cfg(unix)]
fn the_aside_name(dir: &std::path::Path) -> std::path::PathBuf {
    let mut found: Vec<_> = entry_names(dir)
        .into_iter()
        .filter(|name| name.starts_with(".undo-"))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "one random name beside the level: {found:?}"
    );
    dir.join(found.remove(0))
}

#[cfg(unix)]
#[test]
fn a_full_filesystem_still_takes_back_every_created_level() {
    // ENOSPC and EDQUOT on the quarantine's `mkdirat`: the three-level prefix
    // is taken back with nothing printed and nothing left, `.undo-*` included.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::no_room::a_full_filesystem_still_takes_back_every_created_level",
        );
        assert!(!stderr.contains("warning:"), "stderr:\n{stderr}");
        return;
    }
    for err in [rustix::io::Errno::NOSPC, rustix::io::Errno::DQUOT] {
        let dir = TempDir::new("no-room-all");
        let created = super::create_directories(&dir.join("a").join("b").join("c")).unwrap();
        assert_eq!(created.len(), 3);
        undo_failing(&created, err, &mut || {}, &mut |_, _| {});
        assert!(
            entry_names(&dir.path).is_empty(),
            "{err}: {:?}",
            entry_names(&dir.path)
        );
    }
}

#[cfg(unix)]
#[test]
fn a_full_filesystem_leaves_a_level_holding_something_and_the_levels_above() {
    // `b` holds a file: `c` is taken back, `b` is moved aside, found not
    // empty, and put back under its name, and `a` above it is left too.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::no_room::a_full_filesystem_leaves_a_level_holding_something_and_the_levels_above",
        );
        assert!(
            stderr.contains("/a/b is left in place: it could not be removed: "),
            "stderr:\n{stderr}"
        );
        assert!(!stderr.contains("/a is left in place"), "stderr:\n{stderr}");
        return;
    }
    let dir = TempDir::new("no-room-holding");
    let created = super::create_directories(&dir.join("a").join("b").join("c")).unwrap();
    fs::write(dir.join("a").join("b").join("data"), b"theirs").unwrap();
    undo_failing(
        &created,
        rustix::io::Errno::NOSPC,
        &mut || {},
        &mut |_, _| {},
    );
    let a = dir.join("a");
    assert_eq!(entry_names(&dir.path), ["a"]);
    assert_eq!(entry_names(&a), ["b"], "no random name is left beside b");
    assert_eq!(entry_names(&a.join("b")), ["data"]);
    assert_eq!(fs::read(a.join("b").join("data")).unwrap(), b"theirs");
}

#[cfg(unix)]
#[test]
fn a_full_filesystem_does_not_treat_another_mkdir_error_as_no_room() {
    // A read-only filesystem is not "no room": the level is left standing
    // with the old warning, and nothing is moved.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::no_room::a_full_filesystem_does_not_treat_another_mkdir_error_as_no_room",
        );
        assert!(
            stderr.contains("/new is left in place: could not create cleanup directory "),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("no-room-rofs");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    undo_failing(
        &created,
        rustix::io::Errno::ROFS,
        &mut || {},
        &mut |_, _| {},
    );
    assert!(level.is_dir(), "the level is left standing");
    assert_eq!(entry_names(&dir.path), ["new"]);
}

#[cfg(unix)]
#[test]
fn a_full_filesystem_leaves_a_directory_that_took_the_name_before_the_move() {
    // The level is moved away and somebody else's directory (holding a file)
    // takes its name after the walk's check and before the move aside: the
    // move carries that directory off, the identity check refuses it, and it
    // is put back under the name, contents intact. The level is reported
    // where it went.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::no_room::a_full_filesystem_leaves_a_directory_that_took_the_name_before_the_move",
        );
        assert!(
            stderr.contains("another entry has taken its name"),
            "stderr:\n{stderr}"
        );
        assert!(
            stderr.contains("moved away before rollback could take it back"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("no-room-swap-before");
    let (level, away) = (dir.join("new"), dir.join("away"));
    let created = super::create_directories(&level).unwrap();
    undo_failing(
        &created,
        rustix::io::Errno::NOSPC,
        &mut || {
            fs::rename(&level, &away).unwrap();
            fs::create_dir(&level).unwrap();
            fs::write(level.join("theirs"), b"x").unwrap();
        },
        &mut |_, _| {},
    );
    assert_eq!(fs::read(level.join("theirs")).unwrap(), b"x");
    assert!(away.is_dir(), "the level this run made is not chased down");
    assert_eq!(entry_names(&dir.path), ["away", "new"]);
}

#[cfg(unix)]
#[test]
fn a_full_filesystem_keeps_a_non_empty_directory_swapped_in_after_the_check() {
    // After the capture was found to be ours, a writer moves it away and
    // puts a directory holding a file at the random name. `REMOVEDIR` cannot
    // remove it: it is put back under the level's name, contents intact.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::no_room::a_full_filesystem_keeps_a_non_empty_directory_swapped_in_after_the_check",
        );
        assert!(
            stderr.contains("another entry has taken its name"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("no-room-swap-after");
    let (level, away) = (dir.join("new"), dir.join("away"));
    let created = super::create_directories(&level).unwrap();
    let base = dir.path.clone();
    undo_failing(
        &created,
        rustix::io::Errno::NOSPC,
        &mut || {},
        &mut |stage, _| {
            if stage == CaptureStage::Verified {
                let aside = the_aside_name(&base);
                fs::rename(&aside, &away).unwrap();
                fs::create_dir(&aside).unwrap();
                fs::write(aside.join("mine"), b"x").unwrap();
            }
        },
    );
    assert_eq!(fs::read(level.join("mine")).unwrap(), b"x");
    assert!(away.is_dir());
    assert_eq!(entry_names(&dir.path), ["away", "new"]);
}

#[cfg(unix)]
#[test]
fn a_full_filesystem_puts_a_level_back_beside_a_name_taken_meanwhile() {
    // The level holds a file, so it cannot be removed; its name is taken
    // while it is aside, so it is put back as `new.restored-0`.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::no_room::a_full_filesystem_puts_a_level_back_beside_a_name_taken_meanwhile",
        );
        assert!(
            stderr.contains("was recreated during cleanup; its contents are in "),
            "stderr:\n{stderr}"
        );
        assert!(stderr.contains("/new.restored-0"), "stderr:\n{stderr}");
        return;
    }
    let dir = TempDir::new("no-room-restored");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    fs::write(level.join("data"), b"theirs").unwrap();
    undo_failing(
        &created,
        rustix::io::Errno::NOSPC,
        &mut || {},
        &mut |stage, _| {
            if stage == CaptureStage::BeforeRestore {
                fs::create_dir(dir.join("new")).unwrap();
            }
        },
    );
    assert_eq!(entry_names(&dir.path), ["new", "new.restored-0"]);
    assert_eq!(
        fs::read(dir.join("new.restored-0").join("data")).unwrap(),
        b"theirs"
    );
}

#[cfg(unix)]
#[test]
fn a_full_filesystem_names_where_a_level_stays_when_nothing_takes_it_back() {
    // Its name and every `.restored-<n>` are taken while it is aside, so it
    // stays under the random name, and the warning says where.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::no_room::a_full_filesystem_names_where_a_level_stays_when_nothing_takes_it_back",
        );
        assert!(
            stderr.contains("its contents remain in ") && stderr.contains("/.undo-"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("no-room-stranded");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    fs::write(level.join("data"), b"theirs").unwrap();
    undo_failing(
        &created,
        rustix::io::Errno::NOSPC,
        &mut || {},
        &mut |stage, _| {
            if stage == CaptureStage::BeforeRestore {
                fs::create_dir(dir.join("new")).unwrap();
                for n in 0..16 {
                    fs::create_dir(dir.join(&format!("new.restored-{n}"))).unwrap();
                }
            }
        },
    );
    let aside = the_aside_name(&dir.path);
    assert_eq!(fs::read(aside.join("data")).unwrap(), b"theirs");
}

#[cfg(unix)]
#[test]
fn a_full_filesystem_names_a_swapped_in_directory_that_nothing_takes_back() {
    // After the check, a writer moves the level away, puts a directory
    // holding a file at the random name, and takes the level's name and every
    // `.restored-<n>`. Nothing can put the stranger back, so it stays under
    // the random name, and the warning names that path without claiming
    // the contents are the level's.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::no_room::a_full_filesystem_names_a_swapped_in_directory_that_nothing_takes_back",
        );
        assert!(
            stderr.contains("a directory that is not the one rollback made could not be put back and remains at ")
                && stderr.contains("/.undo-"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("no-room-stranded-stranger");
    let (level, away) = (dir.join("new"), dir.join("away"));
    let created = super::create_directories(&level).unwrap();
    let base = dir.path.clone();
    undo_failing(
        &created,
        rustix::io::Errno::NOSPC,
        &mut || {},
        &mut |stage, _| {
            if stage == CaptureStage::Verified {
                let aside = the_aside_name(&base);
                fs::rename(&aside, &away).unwrap();
                fs::create_dir(&aside).unwrap();
                fs::write(aside.join("mine"), b"x").unwrap();
                fs::create_dir(base.join("new")).unwrap();
                for n in 0..16 {
                    fs::create_dir(base.join(format!("new.restored-{n}"))).unwrap();
                }
            }
        },
    );
    let aside = the_aside_name(&dir.path);
    assert_eq!(fs::read(aside.join("mine")).unwrap(), b"x");
    assert!(away.is_dir());
}

#[cfg(target_os = "linux")]
#[test]
fn a_level_on_another_filesystem_is_taken_back_aside_when_its_quarantine_has_no_room() {
    use std::os::unix::fs::MetadataExt as _;
    // As the cross-device test: `more` is on /dev/shm, so the shared
    // quarantine's rename fails with EXDEV and a quarantine of its own is
    // asked for beside it. That `mkdirat` finds no room (the second call);
    // `more` goes aside on its own filesystem, and `new` still gets a real
    // quarantine.
    let shm = std::path::Path::new("/dev/shm");
    let dir = TempDir::new("no-room-cross-device");
    let Ok(shm_meta) = fs::metadata(shm) else {
        panic!("this test needs /dev/shm");
    };
    assert_ne!(
        shm_meta.dev(),
        fs::metadata(&dir.path).unwrap().dev(),
        "this test needs /dev/shm on another filesystem than the temp dir"
    );
    let other = shm.join(format!(
        "pokeemerald-rs-no-room-cross-device-{}",
        std::process::id()
    ));
    fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(&other, dir.join("m")).unwrap();
    let target = dir.join("new").join("..").join("m").join("more");
    let created = super::create_directories(&target).unwrap();
    assert!(other.join("more").is_dir(), "more was made on /dev/shm");

    let mut calls = 0;
    undo_created_directories_making_with(
        &created,
        &mut || {},
        &mut |_, _| {},
        &mut |home, name| {
            calls += 1;
            if calls == 2 {
                Err(rustix::io::Errno::NOSPC)
            } else {
                rustix::fs::mkdirat(home, name, rustix::fs::Mode::RWXU)
            }
        },
    );

    let on_shm = entry_names(&other);
    let in_base = entry_names(&dir.path);
    fs::remove_dir_all(&other).unwrap();
    assert!(on_shm.is_empty(), "left on /dev/shm: {on_shm:?}");
    assert_eq!(in_base, ["m"], "left in the base: {in_base:?}");
    assert!(calls >= 3, "new asked for a quarantine again: {calls}");
}
