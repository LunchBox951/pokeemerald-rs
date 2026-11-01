//! The stderr warnings rollback prints when a held directory, a quarantine name, or
//! a put-back capture cannot be examined at all.

use super::*;

#[cfg(unix)]
#[test]
fn a_quarantine_whose_held_descriptor_cannot_be_examined_is_reported_with_the_error() {
    // The quarantine's name still resolves, but its held descriptor cannot
    // be examined (EIO through a test seam): it is left in place, and the
    // warning carries the error, not a claim that its name no longer
    // resolves to it.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::unexamined::a_quarantine_whose_held_descriptor_cannot_be_examined_is_reported_with_the_error",
        );
        let io = std::io::Error::from(rustix::io::Errno::IO).to_string();
        assert!(
            stderr.contains("warning: cleanup directory ")
                && stderr.contains(&format!(
                    " could not be examined: {io}; it is left in place"
                )),
            "the unexamined quarantine is not reported with its error; stderr:\n{stderr}"
        );
        assert!(
            !stderr.contains("no longer resolves") && !stderr.contains("moved away"),
            "the examination error is reported as a moved quarantine; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("quarantine-held-unexamined");
    let level = dir.join("new");
    let name = dir.join(".undo-r");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let proceeded = super::take_back::take_back_level_held_stat_failing(
        parent,
        &created[0],
        ".undo-r",
        rustix::io::Errno::IO,
        &mut || {},
    );
    assert!(proceeded, "the level itself was taken back");
    assert!(!level.exists(), "the level is removed");
    assert!(
        entry_names(&name).is_empty(),
        "the quarantine is left, empty"
    );
}

#[cfg(unix)]
#[test]
fn a_quarantine_whose_name_cannot_be_examined_is_reported_with_the_error() {
    // The quarantine's re-check fails with something other than ENOENT
    // (here ENAMETOOLONG, standing in for EACCES on its parent): it is left
    // in place, and the warning carries the error, not a claim that its
    // name no longer resolves to it or that it was moved away.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::unexamined::a_quarantine_whose_name_cannot_be_examined_is_reported_with_the_error",
        );
        assert!(
            stderr.contains("warning: cleanup directory ")
                && stderr.contains(" could not be examined: ")
                && stderr.contains("; it is left in place"),
            "the unexamined quarantine is not reported; stderr:\n{stderr}"
        );
        assert!(
            !stderr.contains("no longer resolves") && !stderr.contains("moved away"),
            "the lookup error is reported as a moved quarantine; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("quarantine-unexamined");
    let level = dir.join("new");
    let name = dir.join(".undo-r");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let proceeded = super::take_back::take_back_level_looking_up(
        parent,
        &created[0],
        ".undo-r",
        &"n".repeat(4096),
    );
    assert!(proceeded, "the level itself was taken back");
    assert!(!level.exists(), "the level is removed");
    assert!(
        entry_names(&name).is_empty(),
        "the quarantine is left, empty"
    );
}

#[cfg(unix)]
#[test]
fn a_level_put_back_that_cannot_be_examined_is_reported_with_the_error() {
    // The level gained contents, so it is put back under its name; the
    // re-check of what was put back fails (EIO through a test seam). The
    // warning carries that error; it does not claim another entry took the
    // name or that the level was moved away.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::unexamined::a_level_put_back_that_cannot_be_examined_is_reported_with_the_error",
        );
        let io = std::io::Error::from(rustix::io::Errno::IO).to_string();
        let warned = warned_path(
            &stderr,
            "warning: ",
            " is left in place: it could not be removed: ",
        );
        assert_names_marked_path(&stderr, warned, "level at: ");
        assert!(
            stderr.contains(&format!(
                "; it was put back under its name, where it could not be examined: {io}; the levels above it are left too"
            )),
            "the unexamined put-back is not reported with its error; stderr:\n{stderr}"
        );
        assert!(
            !stderr.contains("another entry has taken its name") && !stderr.contains("moved away"),
            "the examination error is reported as an identity mismatch; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("put-back-unexamined");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    fs::write(level.join("keep"), b"x").unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let proceeded = super::take_back::take_back_level_revalidation_failing(
        parent,
        &created[0],
        ".undo-r",
        rustix::io::Errno::IO,
        &mut |_, _| {},
    );
    assert!(!proceeded, "the walk stops at the level put back");
    assert!(level.join("keep").is_file(), "the level is back in place");
    assert_eq!(entry_names(&dir.path), ["new"], "no quarantine is left");
    mark_path("level at: ", &level);
}

#[cfg(unix)]
#[test]
fn a_level_put_back_aside_that_cannot_be_examined_keeps_its_recovery_warning() {
    // The level gained contents and its name is retaken before put-back,
    // so it goes to `new.restored-0`; the re-check there fails (EIO). The
    // recovery warning naming `new.restored-0` is kept, the error is
    // reported, and the put-back is not called another writer's directory.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::unexamined::a_level_put_back_aside_that_cannot_be_examined_keeps_its_recovery_warning",
        );
        let io = std::io::Error::from(rustix::io::Errno::IO).to_string();
        assert!(
            stderr.contains(" was recreated during cleanup; its contents are in ")
                && stderr.contains("/new.restored-0\n"),
            "the recovery warning is missing; stderr:\n{stderr}"
        );
        assert!(
            stderr.contains(&format!(
                "/new.restored-0 was put back during cleanup but could not be examined there: {io}; the levels above it are left too"
            )),
            "the unexamined put-back is not reported with its error; stderr:\n{stderr}"
        );
        assert!(
            !stderr.contains("is not the directory rollback made")
                && !stderr.contains("another entry has taken its name")
                && !stderr.contains("moved away"),
            "the examination error is reported as an identity mismatch; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("put-back-aside-unexamined");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    fs::write(level.join("keep"), b"x").unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let proceeded = super::take_back::take_back_level_revalidation_failing(
        parent,
        &created[0],
        ".undo-r",
        rustix::io::Errno::IO,
        &mut |stage, _| {
            if stage == super::take_back::CaptureStage::BeforeRestore {
                fs::create_dir(&level).unwrap();
            }
        },
    );
    assert!(!proceeded, "the walk stops at the level put back");
    assert!(dir.join("new.restored-0").join("keep").is_file());
}

#[cfg(unix)]
#[test]
fn a_moved_level_whose_held_original_cannot_be_examined_is_reported_with_the_error() {
    // The level is no longer under its name, and its held original's
    // `fstat` fails (EIO through a test seam): that is not proof it was
    // removed, so the warning says it may stand elsewhere, with the error.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::unexamined::a_moved_level_whose_held_original_cannot_be_examined_is_reported_with_the_error",
        );
        let io = std::io::Error::from(rustix::io::Errno::IO).to_string();
        assert!(
            stderr.contains(&format!(
                "/new is no longer under its name, and the directory rollback made for it could not be examined: {io}; if it was moved rather than removed, it is left elsewhere"
            )),
            "the unexamined original is not reported; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("moved-unexamined");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    fs::rename(&level, dir.join("original")).unwrap();
    super::take_back::warn_if_moved_away_held_stat_failing(&created[0], rustix::io::Errno::IO);
}

#[cfg(unix)]
#[test]
fn a_moved_quarantine_that_cannot_be_examined_is_reported_with_the_error() {
    // The quarantine is renamed away before its removal, leaving its name
    // vacant, and the held quarantine's `fstat` fails (EIO through a test
    // seam): it is reported as left in place with the error, not passed
    // over as removed.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::unexamined::a_moved_quarantine_that_cannot_be_examined_is_reported_with_the_error",
        );
        let io = std::io::Error::from(rustix::io::Errno::IO).to_string();
        assert!(
            stderr.contains("/.undo-r no longer resolves to the one rollback made; it is left in place")
                && stderr.contains(&format!(
                    "/.undo-r could not be examined: {io}; if it was moved rather than removed, it is left elsewhere"
                )),
            "the unexamined quarantine is not reported with its error; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("quarantine-moved-unexamined");
    let level = dir.join("new");
    let name = dir.join(".undo-r");
    let away = dir.join("away");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let proceeded = super::take_back::take_back_level_held_stat_failing(
        parent,
        &created[0],
        ".undo-r",
        rustix::io::Errno::IO,
        &mut || fs::rename(&name, &away).unwrap(),
    );
    assert!(proceeded, "the level itself was taken back");
    assert!(!level.exists() && !name.exists());
    assert!(entry_names(&away).is_empty(), "the moved quarantine stays");
}

#[cfg(unix)]
#[test]
fn a_capture_whose_held_original_cannot_be_examined_is_reported_with_the_error() {
    // The capture's lookup matches the level's identity, but its held
    // original's `fstat` fails (EIO through a test seam) while it is
    // checked: the level is put back under its name and the walk stops with
    // that error as the cause, not a claim that another entry took the name
    // or that the level was moved away.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::unexamined::a_capture_whose_held_original_cannot_be_examined_is_reported_with_the_error",
        );
        let io = std::io::Error::from(rustix::io::Errno::IO).to_string();
        assert!(
            stderr.contains(&format!(
                "/new is left in place: it could not be examined in its cleanup directory: {io}; the levels above it are left too"
            )),
            "the unexamined capture is not reported with its error; stderr:\n{stderr}"
        );
        assert!(
            !stderr.contains("another entry has taken its name") && !stderr.contains("moved away"),
            "the examination error is reported as a replacement or move; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("capture-held-unexamined");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let proceeded = super::take_back::take_back_level_capture_check_failing(
        parent,
        &created[0],
        ".undo-r",
        rustix::io::Errno::IO,
    );
    assert!(!proceeded, "the walk stops at the unexamined level");
    assert!(level.is_dir(), "the level is put back under its name");
    assert_eq!(entry_names(&dir.path), ["new"], "no quarantine is left");
}
