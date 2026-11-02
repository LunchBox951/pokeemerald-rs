//! Taking a level back when its name, or the capture in its quarantine, is swapped
//! by another writer between validation and the take-back.

use super::*;

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

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_level_replaced_after_validation_is_reported_where_it_was_moved() {
    // The same swap as `undo_preserves_a_replacement_swapped_after_validation`:
    // the replacement is restored silently, but the directory rollback made
    // still stands where it was moved, so the warning names that place,
    // read through the held original.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::races::a_level_replaced_after_validation_is_reported_where_it_was_moved",
        );
        let warned = warned_path(&stderr, "warning: ", " is the directory rollback made for ");
        assert_names_marked_path(&stderr, warned, "original at: ");
        return;
    }
    let dir = TempDir::new("undo-replaced-reported");
    let level = dir.join("new");
    let moved = dir.join("original");
    let created = super::create_directories(&level).unwrap();
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        fs::rename(&level, &moved).unwrap();
        fs::create_dir(&level).unwrap();
    });
    assert!(level.is_dir() && moved.is_dir());
    mark_path("original at: ", &moved);
}

#[cfg(unix)]
#[test]
fn a_level_that_gained_contents_is_reported_left_in_place() {
    // The level is put back under its own name, so restoration itself has
    // nothing to warn about; the warning must still say the level stands
    // and why it could not be removed.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::races::a_level_that_gained_contents_is_reported_left_in_place",
        );
        let warned = warned_path(
            &stderr,
            "warning: ",
            " is left in place: it could not be removed: ",
        );
        assert_names_marked_path(&stderr, warned, "level at: ");
        return;
    }
    let dir = TempDir::new("undo-gained-contents-reported");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        fs::write(level.join("keep"), b"x").unwrap();
    });
    assert!(level.join("keep").is_file());
    mark_path("level at: ", &level);
}

#[cfg(unix)]
#[test]
fn a_capture_that_cannot_be_examined_is_reported_left_in_place() {
    // The capture is hidden only while its identity is checked, so that
    // check fails, and is back before it is put back: restoration under its
    // own name has nothing to warn about, but the walk stops there, so the
    // warning must say the level and those above it are left.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::races::a_capture_that_cannot_be_examined_is_reported_left_in_place",
        );
        let warned = warned_path(
            &stderr,
            "warning: ",
            " is left in place: it could not be examined in its cleanup directory: ",
        );
        assert_names_marked_path(&stderr, warned, "level at: ");
        assert!(
            stderr.contains("the levels above it are left too"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("undo-unexamined-capture");
    let level = dir.join("above").join("new");
    let created = super::create_directories(&level).unwrap();
    let mut stages = Vec::new();
    super::take_back::undo_created_directories_with_capture_hooks(
        &created,
        &mut || {},
        &mut |stage, held| {
            stages.push(stage);
            match stage {
                super::take_back::CaptureStage::BeforeCheck => {
                    rustix::fs::renameat(held, super::take_back::QUARANTINE_ENTRY, held, "hidden")
                        .unwrap();
                }
                super::take_back::CaptureStage::BeforeRestore => {
                    rustix::fs::renameat(held, "hidden", held, super::take_back::QUARANTINE_ENTRY)
                        .unwrap();
                }
                super::take_back::CaptureStage::Verified => {}
            }
        },
    );
    assert_eq!(
        stages,
        [
            super::take_back::CaptureStage::BeforeCheck,
            super::take_back::CaptureStage::BeforeRestore
        ]
    );
    assert!(level.is_dir(), "the capture is put back under its name");
    assert_eq!(
        entry_names(&dir.path),
        ["above"],
        "no quarantine is left behind"
    );
    mark_path("level at: ", &level);
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_level_moved_out_of_its_quarantine_before_restoration_is_reported_where_it_went() {
    // The level gains contents, so it cannot be removed; another writer then
    // moves it out of the quarantine before it is put back. The warning must
    // name where it went, read through the held original, not the
    // quarantine it left.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::races::a_level_moved_out_of_its_quarantine_before_restoration_is_reported_where_it_went",
        );
        let warned = warned_path(&stderr, "warning: ", " is the directory rollback made for ");
        assert_names_marked_path(&stderr, warned, "moved to: ");
        assert!(
            !stderr.contains("its contents remain in"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("undo-moved-out-of-quarantine");
    let level = dir.join("new");
    let moved = dir.join("moved");
    let created = super::create_directories(&level).unwrap();
    super::take_back::undo_created_directories_with_capture_hooks(
        &created,
        &mut || fs::write(level.join("keep"), b"x").unwrap(),
        &mut |stage, held| {
            if stage == super::take_back::CaptureStage::BeforeRestore {
                rustix::fs::renameat(
                    held,
                    super::take_back::QUARANTINE_ENTRY,
                    rustix::fs::CWD,
                    &moved,
                )
                .unwrap();
            }
        },
    );
    assert!(moved.join("keep").is_file());
    assert_eq!(
        entry_names(&dir.path),
        ["moved"],
        "no quarantine is left behind"
    );
    mark_path("moved to: ", &moved);
}

#[cfg(unix)]
#[test]
fn a_level_swapped_in_its_quarantine_before_a_put_back_aside_is_not_placed_there() {
    // The level gains contents, so it cannot be removed; before it is put
    // back another writer moves it out of the quarantine, leaves an empty
    // directory there, and retakes its name, so what goes back lands at
    // `new.restored-0`. That is not the level: the warning must not say its
    // contents are there, and the level is reported where it went.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::races::a_level_swapped_in_its_quarantine_before_a_put_back_aside_is_not_placed_there",
        );
        assert!(!stderr.contains("its contents are in"), "stderr:\n{stderr}");
        assert!(
            stderr.contains("but it is not the directory rollback made"),
            "stderr:\n{stderr}"
        );
        // Only Linux and Android can tell a moved held level from a removed
        // one and say where it went; elsewhere the warning is hedged
        // (`Standing::Unknown`, #2576).
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let warned = warned_path(&stderr, "warning: ", " is the directory rollback made for ");
            assert_names_marked_path(&stderr, warned, "moved to: ");
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        assert!(
            stderr.contains(" is no longer under its name; if it was moved rather than removed"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("undo-swapped-before-aside");
    let level = dir.join("new");
    let moved = dir.join("moved");
    let created = super::create_directories(&level).unwrap();
    super::take_back::undo_created_directories_with_capture_hooks(
        &created,
        &mut || fs::write(level.join("keep"), b"x").unwrap(),
        &mut |stage, held| {
            if stage == super::take_back::CaptureStage::BeforeRestore {
                rustix::fs::renameat(
                    held,
                    super::take_back::QUARANTINE_ENTRY,
                    rustix::fs::CWD,
                    &moved,
                )
                .unwrap();
                rustix::fs::mkdirat(
                    held,
                    super::take_back::QUARANTINE_ENTRY,
                    rustix::fs::Mode::RWXU,
                )
                .unwrap();
                fs::create_dir(&level).unwrap();
            }
        },
    );
    assert!(moved.join("keep").is_file());
    assert!(dir.join("new.restored-0").is_dir());
    mark_path("moved to: ", &moved);
}

#[cfg(unix)]
#[test]
fn a_level_swapped_in_its_quarantine_before_a_failed_put_back_is_not_placed_there() {
    // As above, but every name a put-back could use is taken, so what is in
    // the quarantine stays there. It is not the level: the warning must not
    // say the level's contents remain there.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::races::a_level_swapped_in_its_quarantine_before_a_failed_put_back_is_not_placed_there",
        );
        assert!(
            !stderr.contains("its contents remain in"),
            "stderr:\n{stderr}"
        );
        // Only Linux and Android can tell a moved held level from a removed
        // one and say where it went; elsewhere the warning is hedged
        // (`Standing::Unknown`, #2576).
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let warned = warned_path(&stderr, "warning: ", " is the directory rollback made for ");
            assert_names_marked_path(&stderr, warned, "moved to: ");
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        assert!(
            stderr.contains(" is no longer under its name; if it was moved rather than removed"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("undo-swapped-before-stranding");
    let level = dir.join("new");
    let moved = dir.join("moved");
    let created = super::create_directories(&level).unwrap();
    super::take_back::undo_created_directories_with_capture_hooks(
        &created,
        &mut || fs::write(level.join("keep"), b"x").unwrap(),
        &mut |stage, held| {
            if stage == super::take_back::CaptureStage::BeforeRestore {
                rustix::fs::renameat(
                    held,
                    super::take_back::QUARANTINE_ENTRY,
                    rustix::fs::CWD,
                    &moved,
                )
                .unwrap();
                rustix::fs::mkdirat(
                    held,
                    super::take_back::QUARANTINE_ENTRY,
                    rustix::fs::Mode::RWXU,
                )
                .unwrap();
                fs::create_dir(&level).unwrap();
                for n in 0..16 {
                    fs::create_dir(dir.join(&format!("new.restored-{n}"))).unwrap();
                }
            }
        },
    );
    assert!(moved.join("keep").is_file());
    mark_path("moved to: ", &moved);
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
