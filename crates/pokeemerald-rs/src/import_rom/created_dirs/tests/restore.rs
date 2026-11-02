//! Putting a captured level back under its name, or beside it as
//! `<name>.restored-<n>`, and reporting where a capture that stays behind is.

use super::*;

#[cfg(unix)]
#[test]
fn restore_capture_puts_contents_beside_a_retaken_name() {
    // Deterministic form of the retake race: the level sits in a quarantine
    // holding `keep`, and its name was re-created meanwhile. The capture must
    // come back beside it, not stay only under the hidden quarantine.
    let dir = TempDir::new("restore-retaken");
    let created = super::create_directories(&dir.join("new")).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let quarantine_path = dir.join(".undo-held");
    fs::create_dir(&quarantine_path).unwrap();
    let quarantine = rustix::fs::openat(
        parent,
        ".undo-held",
        rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .unwrap();
    fs::rename(
        dir.join("new"),
        quarantine_path.join(super::take_back::QUARANTINE_ENTRY),
    )
    .unwrap();
    fs::write(
        quarantine_path
            .join(super::take_back::QUARANTINE_ENTRY)
            .join("keep"),
        b"x",
    )
    .unwrap();
    fs::create_dir(dir.join("new")).unwrap();

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &super::take_back::located_beside(parent, &created[0], std::ffi::OsStr::new(".undo-held")),
        parent,
        &created[0],
    );

    assert_eq!(
        warning.as_deref(),
        Some(
            format!(
                "warning: {} was recreated during cleanup; its contents are in {}",
                dir.join("new").display(),
                dir.join("new.restored-0").display()
            )
            .as_str()
        )
    );
    assert!(dir.join("new.restored-0").join("keep").is_file());
    assert!(!quarantine_path
        .join(super::take_back::QUARANTINE_ENTRY)
        .exists());
    assert_eq!(fs::read_dir(dir.join("new")).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn a_capture_no_name_takes_back_is_reported_at_its_exact_quarantine_path() {
    // The level's name and all sixteen `.restored-<n>` names are taken, so
    // the capture stays in the quarantine; the warning must name that exact
    // directory, not a `.undo-*` pattern among other quarantines.
    let dir = TempDir::new("restore-all-taken");
    let created = super::create_directories(&dir.join("new")).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let quarantine_path = dir.join(".undo-held");
    fs::create_dir(&quarantine_path).unwrap();
    let quarantine = rustix::fs::openat(
        parent,
        ".undo-held",
        rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .unwrap();
    let capture = quarantine_path.join(super::take_back::QUARANTINE_ENTRY);
    fs::rename(dir.join("new"), &capture).unwrap();
    fs::write(capture.join("keep"), b"x").unwrap();
    fs::create_dir(dir.join("new")).unwrap();
    for n in 0..16 {
        fs::create_dir(dir.join(&format!("new.restored-{n}"))).unwrap();
    }
    fs::create_dir(dir.join(".undo-other")).unwrap();

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &super::take_back::located_beside(parent, &created[0], std::ffi::OsStr::new(".undo-held")),
        parent,
        &created[0],
    );

    assert!(capture.join("keep").is_file(), "the capture stays put");
    assert_eq!(
        warning.as_deref(),
        Some(
            format!(
                "warning: could not put {} back; its contents remain in {}",
                dir.join("new").display(),
                capture.display()
            )
            .as_str()
        )
    );
}

/// Stages a capture in `<parent>/.undo-held/level` holding `keep`, with the
/// level's name and all sixteen `.restored-<n>` names taken, so
/// `restore_capture` must leave it there and report it.
#[cfg(unix)]
fn stage_unrestorable_capture(
    parent_path: &std::path::Path,
    parent: &std::os::fd::OwnedFd,
) -> std::os::fd::OwnedFd {
    let quarantine_path = parent_path.join(".undo-held");
    fs::create_dir(&quarantine_path).unwrap();
    let quarantine = rustix::fs::openat(
        parent,
        ".undo-held",
        rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .unwrap();
    let capture = quarantine_path.join(super::take_back::QUARANTINE_ENTRY);
    fs::create_dir(&capture).unwrap();
    fs::write(capture.join("keep"), b"x").unwrap();
    for n in 0..16 {
        fs::create_dir(parent_path.join(format!("new.restored-{n}"))).unwrap();
    }
    quarantine
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_stranded_capture_is_reported_under_its_parents_current_name() {
    // The level's parent `a` is renamed to `b` after creation; rollback acts
    // through the held parent, so the warning must name `b`, not `a`.
    let dir = TempDir::new("restore-renamed-parent");
    let created = super::create_directories(&dir.join("a").join("new")).unwrap();
    let level = created.last().unwrap();
    let parent = level.parent.as_deref().unwrap();
    fs::rename(dir.join("a"), dir.join("b")).unwrap();
    let moved = fs::canonicalize(dir.join("b")).unwrap();
    let quarantine = stage_unrestorable_capture(&moved, parent);

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &super::take_back::located_beside(parent, level, std::ffi::OsStr::new(".undo-held")),
        parent,
        level,
    );

    assert_eq!(
        warning.as_deref(),
        Some(
            format!(
                "warning: could not put {} back; its contents remain in {}",
                moved.join("new").display(),
                moved
                    .join(".undo-held")
                    .join(super::take_back::QUARANTINE_ENTRY)
                    .display()
            )
            .as_str()
        )
    );
}

#[cfg(unix)]
#[test]
fn a_cleanup_warning_keeps_a_control_character_in_a_path_on_one_line() {
    let dir = TempDir::new("restore-newline");
    let parent_path = dir.join("line\nbreak");
    fs::create_dir(&parent_path).unwrap();
    let created = super::create_directories(&parent_path.join("new")).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let quarantine = stage_unrestorable_capture(&parent_path, parent);

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &super::take_back::located_beside(parent, &created[0], std::ffi::OsStr::new(".undo-held")),
        parent,
        &created[0],
    )
    .expect("the capture is reported");

    assert!(!warning.contains('\n'), "{warning:?}");
    assert!(warning.contains("line\\nbreak"), "{warning:?}");
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_stranded_capture_is_reported_in_its_quarantines_current_place() {
    // The quarantine was made in `a`, so the location recorded then is
    // `a/.undo-held`; `a` is renamed to `b` before the capture is given up
    // on, and the warning must name where it is now, under `b`.
    let dir = TempDir::new("restore-renamed-quarantine-home");
    let created = super::create_directories(&dir.join("a").join("new")).unwrap();
    let level = created.last().unwrap();
    let parent = level.parent.as_deref().unwrap();
    let recorded = dir.join("a").join(".undo-held");
    fs::rename(dir.join("a"), dir.join("b")).unwrap();
    let moved = fs::canonicalize(dir.join("b")).unwrap();
    let quarantine = stage_unrestorable_capture(&moved, parent);

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &recorded,
        parent,
        level,
    );

    assert_eq!(
        warning.as_deref(),
        Some(
            format!(
                "warning: could not put {} back; its contents remain in {}",
                moved.join("new").display(),
                moved
                    .join(".undo-held")
                    .join(super::take_back::QUARANTINE_ENTRY)
                    .display()
            )
            .as_str()
        )
    );
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_stranded_capture_is_reported_under_the_quarantines_own_current_name() {
    // The quarantine itself, not its home, is renamed after it is made:
    // `.undo-held` becomes `.undo-moved`, and the warning must name the
    // capture under `.undo-moved`, where it is, read through the held
    // quarantine descriptor rather than rebuilt from its original name.
    let dir = TempDir::new("restore-renamed-quarantine");
    let created = super::create_directories(&dir.join("a").join("new")).unwrap();
    let level = created.last().unwrap();
    let parent = level.parent.as_deref().unwrap();
    let home = fs::canonicalize(dir.join("a")).unwrap();
    let recorded = home.join(".undo-held");
    let quarantine = stage_unrestorable_capture(&home, parent);
    fs::rename(&recorded, home.join(".undo-moved")).unwrap();

    let warning = super::take_back::restore_capture(
        std::os::fd::AsFd::as_fd(&quarantine),
        std::os::fd::AsFd::as_fd(parent),
        ".undo-held",
        &recorded,
        parent,
        level,
    );

    assert_eq!(
        warning.as_deref(),
        Some(
            format!(
                "warning: could not put {} back; its contents remain in {}",
                home.join("new").display(),
                home.join(".undo-moved")
                    .join(super::take_back::QUARANTINE_ENTRY)
                    .display()
            )
            .as_str()
        )
    );
}
