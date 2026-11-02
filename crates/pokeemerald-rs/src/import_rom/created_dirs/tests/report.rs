//! Where rollback reports a level or quarantine it left standing or that was moved
//! away, read through the held descriptors.

use super::*;

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_quarantine_whose_removal_fails_is_reported_under_its_homes_current_name() {
    // The quarantine's home `a` is renamed to `b` after the quarantine is
    // made, then something lands in it so its removal fails; the warning
    // must name `b/.undo-r`, where it is, not `a/.undo-r`.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::a_quarantine_whose_removal_fails_is_reported_under_its_homes_current_name",
        );
        let warned = warned_path(
            &stderr,
            "warning: could not remove cleanup directory ",
            ": ",
        );
        assert_names_marked_path(&stderr, warned, "quarantine at: ");
        return;
    }
    let dir = TempDir::new("quarantine-home-renamed");
    let created = super::create_directories(&dir.join("a").join("new")).unwrap();
    let level = created.last().unwrap();
    let parent = level.parent.as_deref().unwrap();
    let moved = dir.join("b");
    let proceeded = super::take_back::take_back_level_with_hooks(
        parent,
        level,
        ".undo-r",
        &mut || fs::rename(dir.join("a"), &moved).unwrap(),
        &mut || fs::write(moved.join(".undo-r").join("held"), b"x").unwrap(),
    );
    assert!(proceeded, "the level itself was taken back");
    assert!(moved.join(".undo-r").join("held").is_file());
    mark_path("quarantine at: ", &moved.join(".undo-r"));
}

#[cfg(unix)]
#[test]
fn a_level_whose_name_was_retaken_before_rollback_is_reported_left_in_place() {
    // The name is swapped before rollback starts, so the first check fails:
    // the replacement and the levels above stay, and rollback says so
    // without calling the replacement the level it made.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::a_level_whose_name_was_retaken_before_rollback_is_reported_left_in_place",
        );
        let warned = warned_path(
            &stderr,
            "warning: ",
            " is left in place: another entry has taken its name; the levels above it are left too",
        );
        assert_names_marked_path(&stderr, warned, "level at: ");
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let moved = warned_path(&stderr, "warning: ", " is the directory rollback made for ");
            assert_names_marked_path(&stderr, moved, "original at: ");
        }
        return;
    }
    let dir = TempDir::new("undo-retaken-before-reported");
    let level = dir.join("new");
    let moved = dir.join("original");
    let created = super::create_directories(&level).unwrap();
    fs::rename(&level, &moved).unwrap();
    fs::create_dir(&level).unwrap();
    super::undo_created_directories(&created);
    assert!(level.is_dir() && moved.is_dir());
    mark_path("level at: ", &level);
    mark_path("original at: ", &moved);
}

#[cfg(unix)]
#[test]
fn a_level_rollback_no_longer_holds_is_reported_left_in_place() {
    // The reopen failed, so no pin vouches for the level: it is left
    // standing, and the warning says why.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::a_level_rollback_no_longer_holds_is_reported_left_in_place",
        );
        let warned = warned_path(
            &stderr,
            "warning: ",
            " is left in place: rollback no longer holds it open to confirm it is the directory this import made",
        );
        assert_names_marked_path(&stderr, warned, "level at: ");
        return;
    }
    let dir = TempDir::new("undo-unheld-reported");
    let level = dir.join("new");
    let (created, _source) = super::create_directories_with_hooks(
        &level,
        super::MAX_PINNED_LEVELS,
        &mut || {},
        &mut || Some(std::io::Error::other("reopen forced to fail for the test")),
        &mut || None,
    )
    .expect_err("a forced reopen failure fails the create");
    super::undo_created_directories(&created);
    assert!(level.is_dir());
    mark_path("level at: ", &level);
}

#[cfg(unix)]
#[test]
fn a_level_that_cannot_be_examined_is_reported_left_in_place() {
    // The name check fails with something other than ENOENT (here
    // ENAMETOOLONG, standing in for EACCES or EMFILE): the level is left
    // standing, and the warning carries the error.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::a_level_that_cannot_be_examined_is_reported_left_in_place",
        );
        assert!(
            stderr.contains(" is left in place: it could not be examined: ")
                && stderr.contains("; the levels above it are left too"),
            "the unexamined level is not reported; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("undo-unexamined-reported");
    let level = dir.join("new");
    let mut created = super::create_directories(&level).unwrap();
    created[0].name = std::ffi::OsString::from("n".repeat(4096));
    super::undo_created_directories(&created);
    assert!(level.is_dir());
}

#[cfg(unix)]
#[test]
fn an_unpinned_level_is_reported_left_in_place() {
    // A pin budget of one sheds the outer levels' pins as the inner ones
    // are made: rollback takes back the pinned innermost level and reports
    // the unpinned one it leaves, rather than skipping it silently.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::an_unpinned_level_is_reported_left_in_place",
        );
        let warned = warned_path(
            &stderr,
            "warning: ",
            " is left in place: it was not held open, so rollback could not confirm it is the directory this import made; the levels above it are left too",
        );
        assert_names_marked_path(&stderr, warned, "level at: ");
        return;
    }
    let dir = TempDir::new("undo-unpinned-reported");
    let outer = dir.join("a");
    let target = outer.join("b").join("c");
    let created =
        super::create_directories_with_hooks(&target, 1, &mut || {}, &mut || None, &mut || None)
            .expect("every level is created");
    assert!(created[0].parent.is_none() && created[1].parent.is_none());
    assert!(created[2].parent.is_some());
    super::undo_created_directories(&created);
    assert!(!target.exists(), "the pinned level is taken back");
    assert!(
        outer.join("b").is_dir(),
        "the unpinned level is left standing"
    );
    mark_path("level at: ", &outer.join("b"));
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_level_moved_away_after_validation_is_reported_where_it_was_moved() {
    // The validated level is renamed away with nothing put in its place,
    // so its take-back finds the name gone; the directory rollback made
    // still stands where it was moved, and the warning names that place.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::a_level_moved_away_after_validation_is_reported_where_it_was_moved",
        );
        let warned = warned_path(&stderr, "warning: ", " is the directory rollback made for ");
        assert_names_marked_path(&stderr, warned, "original at: ");
        assert!(
            stderr.contains(", moved away before rollback could take it back; it is left in place"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("undo-moved-away-reported");
    let level = dir.join("new");
    let moved = dir.join("original");
    let created = super::create_directories(&level).unwrap();
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        fs::rename(&level, &moved).unwrap();
    });
    assert!(!level.exists() && moved.is_dir());
    assert_eq!(
        entry_names(&dir.path),
        ["original"],
        "no quarantine is left behind"
    );
    mark_path("original at: ", &moved);
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_level_removed_by_another_writer_after_validation_is_not_reported() {
    // Removed outright, not moved: nothing of it is left, so rollback
    // reports nothing.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::a_level_removed_by_another_writer_after_validation_is_not_reported",
        );
        assert!(!stderr.contains("warning:"), "stderr:\n{stderr}");
        return;
    }
    let dir = TempDir::new("undo-removed-silent");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        fs::remove_dir(&level).unwrap();
    });
    assert!(entry_names(&dir.path).is_empty());
}

#[cfg(unix)]
#[test]
fn an_unpinned_level_missing_from_its_path_is_reported() {
    // A pin budget of one leaves `a` and `a/b` unpinned; `a` is renamed
    // while the pinned `c` is taken back, so neither recorded path resolves
    // and `b` stands under the new name. Rollback cannot tell that from a
    // removal, so it says the level may be left elsewhere rather than
    // skipping it silently.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::an_unpinned_level_missing_from_its_path_is_reported",
        );
        let warned = warned_path(
            &stderr,
            "warning: ",
            " is no longer at its recorded path, and rollback did not hold it open; if it was moved rather than removed, the directory this import made is left elsewhere",
        );
        assert_names_marked_path(&stderr, warned, "level at: ");
        return;
    }
    let dir = TempDir::new("undo-unpinned-moved-reported");
    let outer = dir.join("a");
    let target = outer.join("b").join("c");
    let created =
        super::create_directories_with_hooks(&target, 1, &mut || {}, &mut || None, &mut || None)
            .expect("every level is created");
    assert!(created[0].parent.is_none() && created[1].parent.is_none());
    let moved = dir.join("moved");
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        fs::rename(&outer, &moved).unwrap();
    });
    assert!(
        moved.join("b").is_dir(),
        "the unpinned level stands elsewhere"
    );
    assert!(
        !moved.join("b").join("c").exists(),
        "the pinned level is taken back"
    );
    eprintln!(
        "level at: {} | {}",
        outer.join("b").display(),
        outer.join("b").display()
    );
}

#[cfg(unix)]
#[test]
fn a_level_removed_and_replaced_after_validation_is_not_reported_as_moved() {
    // The validated level is removed outright and another directory made at
    // its name: the replacement is restored, nothing of the original is
    // left, so rollback must not claim it was moved and still stands; it
    // reports only the replacement that leaves the levels above standing.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::a_level_removed_and_replaced_after_validation_is_not_reported_as_moved",
        );
        assert!(
            !stderr.contains(" is the directory rollback made for ")
                && !stderr.contains("moved away")
                && !stderr.contains("(deleted)"),
            "a removed original is reported as moved; stderr:\n{stderr}"
        );
        let warned = warned_path(
            &stderr,
            "warning: ",
            " is left in place: another entry has taken its name; the levels above it are left too",
        );
        assert_names_marked_path(&stderr, warned, "level at: ");
        return;
    }
    let dir = TempDir::new("undo-removed-replaced");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        fs::remove_dir(&level).unwrap();
        fs::create_dir(&level).unwrap();
    });
    assert!(level.is_dir(), "the replacement is kept");
    assert_eq!(
        entry_names(&dir.path),
        ["new"],
        "no quarantine is left behind"
    );
    mark_path("level at: ", &level);
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_level_swapped_out_of_its_quarantine_before_put_back_is_reported_where_it_went() {
    // The level gains contents, so it cannot be removed; before it is put
    // back another writer moves it out of the quarantine and puts an empty
    // directory there, which the put-back then restores under the level's
    // name. The original must be reported where it went, and the name as
    // taken by another entry, not the replacement as the level that could
    // not be removed.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::a_level_swapped_out_of_its_quarantine_before_put_back_is_reported_where_it_went",
        );
        let warned = warned_path(&stderr, "warning: ", " is the directory rollback made for ");
        assert_names_marked_path(&stderr, warned, "moved to: ");
        let taken = warned_path(
            &stderr,
            "warning: ",
            " is left in place: another entry has taken its name; the levels above it are left too",
        );
        assert_names_marked_path(&stderr, taken, "level at: ");
        assert!(
            !stderr.contains("it could not be removed"),
            "the replacement is reported as the level; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("undo-swapped-before-put-back");
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
            }
        },
    );
    assert!(moved.join("keep").is_file(), "the original is kept");
    assert!(level.is_dir(), "the replacement is put back");
    assert_eq!(
        entry_names(&dir.path),
        ["moved", "new"],
        "no quarantine is left behind"
    );
    mark_path("moved to: ", &moved);
    mark_path("level at: ", &level);
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_level_swapped_out_of_its_quarantine_before_its_unlink_is_reported_where_it_went() {
    // The capture is found to be this level; before its unlink another
    // writer moves it out of the quarantine and puts an empty directory
    // there, which the unlink removes. The original still stands where it
    // was moved, and must be reported there.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::a_level_swapped_out_of_its_quarantine_before_its_unlink_is_reported_where_it_went",
        );
        let warned = warned_path(&stderr, "warning: ", " is the directory rollback made for ");
        assert_names_marked_path(&stderr, warned, "moved to: ");
        return;
    }
    let dir = TempDir::new("undo-swapped-before-unlink");
    let level = dir.join("new");
    let moved = dir.join("moved");
    let created = super::create_directories(&level).unwrap();
    let mut swapped = 0;
    super::take_back::undo_created_directories_with_capture_hooks(
        &created,
        &mut || {},
        &mut |stage, held| {
            if stage == super::take_back::CaptureStage::Verified {
                swapped += 1;
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
            }
        },
    );
    assert_eq!(swapped, 1, "the hook ran once");
    assert!(moved.is_dir(), "the original is kept");
    assert_eq!(
        entry_names(&dir.path),
        ["moved"],
        "the replacement and the quarantine are removed"
    );
    mark_path("moved to: ", &moved);
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_quarantine_renamed_before_its_removal_is_reported_where_it_went() {
    // Before the quarantine's re-check it is renamed, leaving its name
    // vacant. The warning must name the quarantine's new place, read
    // through its held descriptor, not the vacated `.undo-*` path.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::report::a_quarantine_renamed_before_its_removal_is_reported_where_it_went",
        );
        let warned = warned_path(
            &stderr,
            "warning: ",
            " is the cleanup directory rollback made, moved away from ",
        );
        assert_names_marked_path(&stderr, warned, "quarantine at: ");
        assert!(
            !stderr.contains("no longer resolves"),
            "the vacated name is reported; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("quarantine-renamed-before-removal");
    let level = dir.join("new");
    let (name, away) = (dir.join(".undo-r"), dir.join("away"));
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let proceeded = super::take_back::take_back_level_with_hooks(
        parent,
        &created[0],
        ".undo-r",
        &mut || {},
        &mut || fs::rename(&name, &away).unwrap(),
    );
    assert!(proceeded, "the level itself was taken back");
    assert!(!level.exists() && !name.exists());
    assert!(entry_names(&away).is_empty(), "the moved quarantine stays");
    mark_path("quarantine at: ", &away);
}
