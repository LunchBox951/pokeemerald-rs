//! The quarantine a rollback makes: claiming it under an unguessable name, refusing
//! one that is not ours, taking a level back through it, and removing it.

use super::*;

#[cfg(unix)]
#[test]
fn a_quarantine_whose_check_fails_is_reported_with_the_error() {
    // The freshly made quarantine's check fails with an error (an `EIO`
    // listing or `fchmod`, injected through the seam). That says nothing
    // about whose directory it is: the warning gives the error, not an
    // ownership swap, and both the level and the quarantine are left.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::quarantine::a_quarantine_whose_check_fails_is_reported_with_the_error",
        );
        assert!(
            !stderr.contains("is not the private cleanup directory rollback made"),
            "stderr:\n{stderr}"
        );
        assert!(
            stderr.contains("could not be checked: "),
            "stderr:\n{stderr}"
        );
        assert!(
            stderr.contains(&std::io::Error::from(rustix::io::Errno::IO).to_string()),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("quarantine-check-fails");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let proceeded =
        super::take_back::take_back_level_vouched_by(parent, &created[0], ".undo-check", |_| {
            Err(rustix::io::Errno::IO)
        });
    assert!(!proceeded, "rollback must not proceed");
    assert!(level.is_dir(), "the level is left standing");
    assert!(dir.join(".undo-check").is_dir(), "the quarantine is left");
}

#[cfg(unix)]
#[test]
fn rollback_refuses_a_quarantine_name_already_taken() {
    // The quarantine name is injected so a directory (or symlink) is already
    // there when rollback tries to claim it. `mkdirat` must refuse it: the
    // staged entry is never filled, emptied, or removed, and the level stays.
    for staged_kind in ["empty-0700-dir", "dir-with-data", "symlink"] {
        let dir = TempDir::new(&format!("quarantine-taken-{staged_kind}"));
        let level = dir.join("new");
        let staged = dir.join(".undo-staged");
        let created = super::create_directories(&level).unwrap();
        match staged_kind {
            "empty-0700-dir" => fs::create_dir(&staged).unwrap(),
            "dir-with-data" => {
                fs::create_dir(&staged).unwrap();
                fs::write(staged.join("mine"), b"x").unwrap();
            }
            _ => std::os::unix::fs::symlink(&level, &staged).unwrap(),
        }
        let parent = created[0].parent.as_deref().unwrap();
        assert!(
            !super::take_back::take_back_level_via(parent, &created[0], ".undo-staged", &mut || {}),
            "{staged_kind}: rollback must not proceed"
        );
        assert!(level.is_dir(), "{staged_kind}: the level is left standing");
        assert!(
            fs::symlink_metadata(&staged).is_ok(),
            "{staged_kind}: the staged entry is left in place"
        );
        if staged_kind == "dir-with-data" {
            assert!(staged.join("mine").is_file(), "its contents are untouched");
        }
        assert_eq!(entry_names(&dir.path), [".undo-staged", "new"]);
    }
}

#[cfg(unix)]
#[test]
fn rollback_refuses_a_quarantine_swapped_between_mkdir_and_open() {
    use std::os::unix::fs::PermissionsExt;
    // Between rollback's `mkdirat` and `openat` the name is swapped for a
    // directory that is not a fresh private quarantine. It must be neither
    // filled nor removed, and the level must stay.
    for kind in ["non-empty-0700", "empty-0755"] {
        let dir = TempDir::new(&format!("quarantine-swap-{kind}"));
        let level = dir.join("new");
        let (name, away) = (dir.join(".undo-swap"), dir.join("away"));
        let created = super::create_directories(&level).unwrap();
        let parent = created[0].parent.as_deref().unwrap();
        let mut swapped = 0;
        let proceeded =
            super::take_back::take_back_level_via(parent, &created[0], ".undo-swap", &mut || {
                swapped += 1;
                fs::rename(&name, &away).unwrap();
                fs::create_dir(&name).unwrap();
                if kind == "non-empty-0700" {
                    fs::write(name.join("mine"), b"x").unwrap();
                } else {
                    fs::set_permissions(&name, fs::Permissions::from_mode(0o755)).unwrap();
                }
            });
        assert_eq!(swapped, 1, "{kind}: the hook ran once");
        assert!(!proceeded, "{kind}: rollback must not proceed");
        assert!(level.is_dir(), "{kind}: the level is left standing");
        assert!(name.is_dir(), "{kind}: the replacement was removed");
        assert_eq!(
            fs::read_dir(&name).unwrap().count(),
            usize::from(kind == "non-empty-0700"),
            "{kind}: the replacement was filled or emptied"
        );
        assert!(
            away.is_dir(),
            "{kind}: the original quarantine is untouched"
        );
    }
}

#[cfg(unix)]
#[test]
fn quarantine_names_are_random_and_hidden() {
    let a = super::take_back::unguessable_quarantine_name().unwrap();
    let b = super::take_back::unguessable_quarantine_name().unwrap();
    assert_ne!(a, b);
    assert!(a.starts_with(".undo-") && a.len() == ".undo-".len() + 16);
}

#[cfg(unix)]
#[test]
fn rollback_takes_back_a_level_when_the_quarantine_open_takes_the_last_descriptor() {
    // The quarantine's `openat` takes the table's last free slot. Checking
    // that quarantine must not cost a second descriptor, or rollback strands
    // both the level and an empty `.undo-*` beside it.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::quarantine::rollback_takes_back_a_level_when_the_quarantine_open_takes_the_last_descriptor",
        );
        return;
    }
    let dir = TempDir::new("quarantine-last-descriptor");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let mut fillers = None;
    let proceeded =
        super::take_back::take_back_level_via(parent, &created[0], ".undo-last", &mut || {
            fillers = Some(fill_descriptor_table_leaving(1));
        });
    drop(fillers);
    assert!(proceeded, "the level is taken back");
    assert!(!level.exists(), "the level is removed");
    assert!(entry_names(&dir.path).is_empty(), "no quarantine is left");
}

#[cfg(unix)]
#[test]
fn a_swapped_quarantine_is_not_removed_when_its_open_fails() {
    // Between `mkdirat` and `openat` the quarantine name is swapped for
    // someone else's empty directory, and the descriptor table is full, so
    // the open fails. Nothing then proves what the name denotes: it must not
    // be unlinked by name.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::quarantine::a_swapped_quarantine_is_not_removed_when_its_open_fails",
        );
        return;
    }
    let dir = TempDir::new("swap-open-fails");
    let level = dir.join("new");
    let (name, away) = (dir.join(".undo-v"), dir.join("away"));
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let mut fillers = None;
    let proceeded =
        super::take_back::take_back_level_via(parent, &created[0], ".undo-v", &mut || {
            fs::rename(&name, &away).unwrap();
            fs::create_dir(&name).unwrap(); // someone else's empty directory
            fillers = Some(fill_descriptor_table_leaving(0));
        });
    drop(fillers);
    assert!(!proceeded);
    assert!(level.is_dir());
    assert!(
        name.is_dir(),
        "the replacement directory was unlinked by its mutable name"
    );
}

#[cfg(unix)]
const UMASK_CHILD: &str = "POKEEMERALD_IMPORT_ROLLBACK_UMASK_CHILD";

#[cfg(unix)]
#[test]
fn rollback_takes_back_a_level_under_a_umask_that_masks_owner_bits() {
    // Under umask 0177 the quarantine's `mkdirat(0700)` yields 0600: still
    // private, but without the search bit the capture's rename needs. The
    // umask is process-wide, so the body runs alone in a child.
    const TEST: &str = "import_rom::created_dirs::tests::quarantine::rollback_takes_back_a_level_under_a_umask_that_masks_owner_bits";
    if std::env::var_os(UMASK_CHILD).is_none() {
        let exe = std::env::current_exe().unwrap();
        let output = std::process::Command::new(exe)
            .args([TEST, "--exact", "--nocapture"])
            .env(UMASK_CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("1 passed"),
            "child status {:?}\nstdout:\n{stdout}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let dir = TempDir::new("umask-0177");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    let previous = rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o177));
    super::undo_created_directories(&created);
    rustix::process::umask(previous);
    assert!(!level.exists(), "the level is taken back");
    assert!(entry_names(&dir.path).is_empty(), "no quarantine is left");
}

#[cfg(unix)]
#[test]
fn a_quarantine_that_cannot_be_made_reports_the_level_left_standing() {
    // A file already sits at the quarantine name, so `mkdirat` fails with
    // EEXIST; the warning names the level rollback leaves and the cause.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::quarantine::a_quarantine_that_cannot_be_made_reports_the_level_left_standing",
        );
        assert!(
            stderr.contains("/new is left in place: could not create cleanup directory ")
                && stderr.contains("/.undo-f: "),
            "the retained level is not reported; stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("quarantine-mkdir-fails");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    fs::write(dir.join(".undo-f"), b"theirs").unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    assert!(!super::take_back::take_back_level_via(
        parent,
        &created[0],
        ".undo-f",
        &mut || {}
    ));
    assert!(level.is_dir(), "the level is left standing");
    assert_eq!(fs::read(dir.join(".undo-f")).unwrap(), b"theirs");
}

#[cfg(unix)]
#[test]
fn a_taken_back_level_removes_its_quarantine_without_a_warning() {
    // The happy path leaves no trace: the level and its quarantine are both
    // gone, and nothing is printed.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::quarantine::a_taken_back_level_removes_its_quarantine_without_a_warning",
        );
        assert!(!stderr.contains("warning:"), "stderr:\n{stderr}");
        return;
    }
    let dir = TempDir::new("quarantine-removed");
    let level = dir.join("new");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    assert!(super::take_back::take_back_level_via(
        parent,
        &created[0],
        ".undo-q",
        &mut || {}
    ));
    assert!(entry_names(&dir.path).is_empty(), "no trace is left");
}

#[cfg(unix)]
#[test]
fn a_quarantine_whose_removal_fails_is_reported_at_its_path() {
    // Something lands in the quarantine before its removal, so
    // `unlinkat(REMOVEDIR)` refuses it; it stays, reported at its path.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::quarantine::a_quarantine_whose_removal_fails_is_reported_at_its_path",
        );
        let warned = warned_path(
            &stderr,
            "warning: could not remove cleanup directory ",
            ": ",
        );
        assert_names_marked_path(&stderr, warned, "quarantine at: ");
        assert!(
            stderr.contains("; it is left in place"),
            "stderr:\n{stderr}"
        );
        return;
    }
    let dir = TempDir::new("quarantine-unlink-fails");
    let level = dir.join("new");
    let name = dir.join(".undo-r");
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let proceeded = super::take_back::take_back_level_with_hooks(
        parent,
        &created[0],
        ".undo-r",
        &mut || {},
        &mut || fs::write(name.join("held"), b"x").unwrap(),
    );
    assert!(proceeded, "the level itself was taken back");
    assert!(
        name.join("held").is_file(),
        "the quarantine is left, with what it holds"
    );
    mark_path("quarantine at: ", &name);
}

#[cfg(unix)]
#[test]
fn a_directory_swapped_in_at_the_quarantine_name_is_left_standing_and_reported() {
    // Before the quarantine's re-check, it is moved away and a directory of
    // someone else's, holding data, takes its name. The name no longer
    // resolves to the held quarantine, so neither is removed, and the
    // warning names the quarantine's path.
    if !in_rollback_stderr_child() {
        let stderr = rerun_alone_for_stderr(
            "import_rom::created_dirs::tests::quarantine::a_directory_swapped_in_at_the_quarantine_name_is_left_standing_and_reported",
        );
        let warned = warned_path(
            &stderr,
            "warning: cleanup directory ",
            " no longer resolves to the one rollback made; it is left in place",
        );
        assert_names_marked_path(&stderr, warned, "name at: ");
        return;
    }
    let dir = TempDir::new("swap-at-quarantine-name");
    let level = dir.join("new");
    let (name, away) = (dir.join(".undo-r"), dir.join("away"));
    let created = super::create_directories(&level).unwrap();
    let parent = created[0].parent.as_deref().unwrap();
    let mut swapped = 0;
    let proceeded = super::take_back::take_back_level_with_hooks(
        parent,
        &created[0],
        ".undo-r",
        &mut || {},
        &mut || {
            swapped += 1;
            fs::rename(&name, &away).unwrap();
            fs::create_dir(&name).unwrap();
            fs::write(name.join("theirs"), b"x").unwrap();
        },
    );
    assert_eq!(swapped, 1, "the hook ran once");
    assert!(proceeded, "the level itself was taken back");
    assert!(!level.exists(), "the level is removed");
    assert!(
        name.join("theirs").is_file(),
        "the replacement or its data was removed"
    );
    assert!(
        entry_names(&away).is_empty(),
        "the moved quarantine stays, empty"
    );
    mark_path("name at: ", &name);
}

#[cfg(unix)]
#[test]
fn a_deep_rollback_takes_back_every_level_and_its_quarantine() {
    // Three levels are taken back through one quarantine made beside the
    // outermost, which is then removed: no trace is left.
    let dir = TempDir::new("deep-one-quarantine");
    let created = super::create_directories(&dir.join("a").join("b").join("c")).unwrap();
    assert_eq!(created.len(), 3);
    super::undo_created_directories(&created);
    assert!(
        entry_names(&dir.path).is_empty(),
        "{:?}",
        entry_names(&dir.path)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_level_on_another_filesystem_is_taken_back_through_a_quarantine_beside_it() {
    // `new/../m/more`: `new` is made in the base directory, then `m`, a
    // symlink to a directory on /dev/shm, is descended through and `more`
    // is made there. A rename of `more` into the quarantine beside `new`
    // fails with EXDEV; `more` must be taken back through a quarantine on
    // its own filesystem, and `new` still through the shared one.
    use std::os::unix::fs::MetadataExt as _;
    let shm = std::path::Path::new("/dev/shm");
    let dir = TempDir::new("cross-device");
    let Ok(shm_meta) = fs::metadata(shm) else {
        panic!("this test needs /dev/shm");
    };
    assert_ne!(
        shm_meta.dev(),
        fs::metadata(&dir.path).unwrap().dev(),
        "this test needs /dev/shm on another filesystem than the temp dir"
    );
    let other = shm.join(format!(
        "pokeemerald-rs-cross-device-{}",
        std::process::id()
    ));
    fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(&other, dir.join("m")).unwrap();
    let target = dir.join("new").join("..").join("m").join("more");
    let created = super::create_directories(&target).unwrap();
    assert!(other.join("more").is_dir(), "more was made on /dev/shm");

    super::undo_created_directories(&created);

    let on_shm = entry_names(&other);
    let in_base = entry_names(&dir.path);
    fs::remove_dir_all(&other).unwrap();
    assert!(on_shm.is_empty(), "left on /dev/shm: {on_shm:?}");
    assert_eq!(in_base, ["m"], "left in the base: {in_base:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn a_level_on_another_filesystem_is_taken_back_when_one_descriptor_is_free() {
    use std::os::unix::fs::MetadataExt as _;
    // As above, but rollback starts with one free descriptor slot. The
    // shared quarantine beside `new` takes it, so the cross-device fallback
    // for `more` must free that quarantine first, or it cannot read
    // `/dev/urandom` and strands `more` and `new` both.
    if !in_descriptor_pressure_child() {
        run_under_descriptor_pressure(
            "import_rom::created_dirs::tests::quarantine::a_level_on_another_filesystem_is_taken_back_when_one_descriptor_is_free",
        );
        return;
    }
    let shm = std::path::Path::new("/dev/shm");
    let dir = TempDir::new("cross-device-pressure");
    let Ok(shm_meta) = fs::metadata(shm) else {
        panic!("this test needs /dev/shm");
    };
    assert_ne!(
        shm_meta.dev(),
        fs::metadata(&dir.path).unwrap().dev(),
        "this test needs /dev/shm on another filesystem than the temp dir"
    );
    let other = shm.join(format!(
        "pokeemerald-rs-cross-device-pressure-{}",
        std::process::id()
    ));
    fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(&other, dir.join("m")).unwrap();
    let target = dir.join("new").join("..").join("m").join("more");
    let created = super::create_directories(&target).unwrap();
    assert!(other.join("more").is_dir(), "more was made on /dev/shm");

    let mut fillers = None;
    super::take_back::undo_created_directories_with_hooks(&created, &mut || {
        if fillers.is_none() {
            fillers = Some(fill_descriptor_table_leaving(1));
        }
    });
    drop(fillers);

    let on_shm = entry_names(&other);
    let in_base = entry_names(&dir.path);
    fs::remove_dir_all(&other).unwrap();
    assert!(on_shm.is_empty(), "left on /dev/shm: {on_shm:?}");
    assert_eq!(in_base, ["m"], "left in the base: {in_base:?}");
}
