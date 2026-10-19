use super::*;

fn missing(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pokeemerald-no-such-{}-{name}", std::process::id()))
}

#[test]
fn a_missing_source_pack_is_a_setup_error() {
    let error = Session::new(&PackSelection::Prepared(missing("pack")))
        .expect_err("no source pack must never prepare a session");
    assert!(matches!(error, ScenarioError::Setup(_)), "got {error:?}");
}

#[test]
fn a_missing_rom_is_a_setup_error() {
    let error = Session::new(&PackSelection::ImportedRom(missing("rom")))
        .expect_err("no ROM must never prepare a session");
    assert!(matches!(error, ScenarioError::Setup(_)), "got {error:?}");
}

#[test]
fn a_prepared_pack_is_copied_into_an_owned_scratch_directory() {
    let source = std::env::temp_dir().join(format!(
        "pokeemerald-scenario-source-{}.pack",
        std::process::id()
    ));
    std::fs::write(&source, b"not a real pack").unwrap();
    let first = Session::new(&PackSelection::Prepared(source.clone())).unwrap();
    let second = Session::new(&PackSelection::Prepared(source.clone())).unwrap();
    std::fs::remove_file(&source).unwrap();
    assert_ne!(first.pack, source);
    assert_ne!(first.pack, second.pack);
    assert_ne!(first.save, second.save);
    assert_eq!(std::fs::read(&first.pack).unwrap(), b"not a real pack");
    let scratch = first.pack.parent().unwrap().to_path_buf();
    drop(first);
    assert!(
        !scratch.exists(),
        "the scratch directory dies with its session"
    );
}

fn foreign_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-scenario-foreign-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("keep.txt"), b"keep me").unwrap();
    dir
}

#[test]
fn a_write_into_a_directory_that_is_not_the_held_one_is_refused() {
    let scratch = Scratch::create_in(&std::env::temp_dir()).unwrap();
    let foreign = foreign_dir("write");
    assert!(scratch.require_path(&foreign).is_err());
    assert!(scratch.require_path(&scratch.path).is_ok());
    let _ = std::fs::remove_dir_all(&foreign);
}

#[test]
fn cleanup_aimed_at_a_directory_that_is_not_the_held_one_removes_nothing() {
    let mut scratch = Scratch::create_in(&std::env::temp_dir()).unwrap();
    let foreign = foreign_dir("cleanup");
    assert!(scratch.remove_all_at(&foreign).is_err());
    assert_eq!(std::fs::read(foreign.join("keep.txt")).unwrap(), b"keep me");
    let _ = std::fs::remove_dir_all(&foreign);
}

/// A base directory private to one test, so a name vacated by a swap can
/// never be claimed by another test's scratch directory.
#[cfg(unix)]
fn private_base(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "pokeemerald-scenario-base-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir(&base).unwrap();
    base
}

#[cfg(unix)]
fn swap_in(scratch: &Scratch, replacement: impl FnOnce(&Path)) -> PathBuf {
    let aside = scratch.path.with_extension("aside");
    std::fs::rename(&scratch.path, &aside).unwrap();
    replacement(&scratch.path);
    aside
}

#[cfg(unix)]
#[test]
fn a_scratch_directory_under_a_setgid_base_is_claimed() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = std::env::temp_dir().join(format!(
        "pokeemerald-setgid-base-{}-{:016x}",
        std::process::id(),
        unpredictable()
    ));
    std::fs::create_dir(&base).unwrap();
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o2700)).unwrap();
    let claimed = Scratch::create_in(&base);
    let result = claimed.as_ref().map(|_| ()).map_err(ToString::to_string);
    drop(claimed);
    let _ = std::fs::remove_dir_all(&base);
    result.unwrap();
}

/// Set in the child [`a_session_under_an_owner_stripping_umask_is_private_and_readable`]
/// re-runs itself as, so only that child changes its own umask.
#[cfg(unix)]
const UMASK_CHILD: &str = "POKEEMERALD_SCENARIO_UMASK_CHILD";

#[cfg(unix)]
#[test]
fn a_session_under_an_owner_stripping_umask_is_private_and_readable() {
    use std::os::unix::fs::PermissionsExt as _;
    let name = "scenario::session::tests::\
                a_session_under_an_owner_stripping_umask_is_private_and_readable";
    if std::env::var_os(UMASK_CHILD).is_none() {
        // The umask is process-wide, so the 0o444 one is set only in a
        // re-run of this test binary that runs this test alone.
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", "umask 0444 && exec \"$0\" \"$@\""])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1"])
            .env(UMASK_CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "umask 0444 child failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let base = private_base("umask");
    let source = base.join("source.pack");
    std::fs::write(&source, b"not a real pack").unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o600)).unwrap();
    let session = Session::new_in(&PackSelection::Prepared(source), &base);
    let observed = session
        .as_ref()
        .map_err(ToString::to_string)
        .map(|session| {
            let mode = |path: &Path| {
                std::fs::symlink_metadata(path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o7777
            };
            (
                mode(&session.scratch.path),
                mode(&session.pack),
                std::fs::read(&session.pack).unwrap(),
            )
        });
    drop(session);
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(
        observed.unwrap(),
        (0o700, 0o600, b"not a real pack".to_vec())
    );
}

#[cfg(unix)]
#[test]
fn restoring_the_private_mode_never_follows_a_swapped_in_symlink() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = private_base("chmod-link");
    let target = base.join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    // The scratch entry, created and then replaced by a link before its
    // mode is restored.
    let entry = base.join("entry");
    std::fs::create_dir(&entry).unwrap();
    std::fs::remove_dir(&entry).unwrap();
    std::os::unix::fs::symlink(&target, &entry).unwrap();
    let result = restore_private_mode(&entry);
    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
    let _ = std::fs::remove_dir_all(&base);
    assert!(
        result.is_err(),
        "a symlink must not be treated as the scratch"
    );
    assert_eq!(mode, 0o755, "the link's target must keep its mode");
}

#[cfg(unix)]
#[test]
fn restoring_the_private_mode_fixes_a_search_only_directory() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = private_base("chmod-dir");
    let entry = base.join("entry");
    std::fs::create_dir(&entry).unwrap();
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o300)).unwrap();
    let result = restore_private_mode(&entry).map_err(|error| error.to_string());
    let mode = std::fs::symlink_metadata(&entry)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    let _ = std::fs::remove_dir_all(&base);
    result.unwrap();
    assert_eq!(mode, 0o700);
}

#[cfg(unix)]
#[test]
fn a_replaced_scratch_directory_receives_no_pack_write() {
    for symlink in [false, true] {
        let base = private_base("write");
        let scratch = Scratch::create_in(&base).unwrap();
        let target = foreign_dir("write-target");
        let aside = swap_in(&scratch, |path| {
            if symlink {
                std::os::unix::fs::symlink(&target, path).unwrap();
            } else {
                std::fs::create_dir(path).unwrap();
            }
        });
        let error = scratch
            .write_new(PACK_FILE_NAME, |_| Ok(()))
            .expect_err("a replaced scratch directory must refuse the write");
        assert!(error.to_string().contains("no longer matches"), "{error}");
        assert!(!scratch.path.join(PACK_FILE_NAME).exists());
        assert!(!target.join(PACK_FILE_NAME).exists());
        assert!(!aside.join(PACK_FILE_NAME).exists());
        let _ = std::fs::remove_dir_all(&target);
        let _ = std::fs::remove_dir_all(&aside);
        let _ = std::fs::remove_file(&scratch.path);
        let _ = std::fs::remove_dir(&scratch.path);
    }
}

#[cfg(unix)]
#[test]
fn dropping_a_session_never_removes_a_tree_that_replaced_its_scratch() {
    for symlink in [false, true] {
        let target = foreign_dir("drop-target");
        let source = std::env::temp_dir().join(format!(
            "pokeemerald-scenario-replaced-{}-{symlink}.pack",
            std::process::id()
        ));
        std::fs::write(&source, b"not a real pack").unwrap();
        let base = private_base("drop");
        let session = Session::new_in(&PackSelection::Prepared(source.clone()), &base).unwrap();
        std::fs::remove_file(&source).unwrap();
        let scratch_path = session.scratch.path.clone();
        let aside = swap_in(&session.scratch, |path| {
            if symlink {
                std::os::unix::fs::symlink(&target, path).unwrap();
            } else {
                std::fs::create_dir(path).unwrap();
                std::fs::write(path.join("keep.txt"), b"keep me").unwrap();
            }
        });
        drop(session);
        let survived = if symlink {
            target.join("keep.txt").exists()
        } else {
            scratch_path.join("keep.txt").exists()
        };
        // The aside tree is the real scratch: it keeps the pack.
        let kept_pack = aside.join(PACK_FILE_NAME).exists();
        let _ = std::fs::remove_dir_all(&target);
        let _ = std::fs::remove_dir_all(&aside);
        let _ = std::fs::remove_file(&scratch_path);
        let _ = std::fs::remove_dir_all(&scratch_path);
        assert!(survived, "cleanup removed content it did not create");
        assert!(kept_pack, "cleanup resolved the pathname, not the hold");
    }
}

#[cfg(unix)]
#[test]
fn a_session_whose_scratch_was_replaced_refuses_to_start() {
    let source = std::env::temp_dir().join(format!(
        "pokeemerald-scenario-nostart-{}.pack",
        std::process::id()
    ));
    std::fs::write(&source, b"not a real pack").unwrap();
    let base = private_base("nostart");
    let session = Session::new_in(&PackSelection::Prepared(source.clone()), &base).unwrap();
    std::fs::remove_file(&source).unwrap();
    let aside = swap_in(&session.scratch, |path| std::fs::create_dir(path).unwrap());
    let Err(error) = session.start() else {
        panic!("a replaced scratch directory must not start");
    };
    assert!(matches!(error, ScenarioError::Start(_)), "got {error:?}");
    let path = session.scratch.path.clone();
    drop(session);
    let _ = std::fs::remove_dir_all(&aside);
    let _ = std::fs::remove_dir_all(&path);
}

#[cfg(windows)]
#[test]
fn a_held_scratch_directory_cannot_be_renamed_away() {
    let scratch = Scratch::create_in(&std::env::temp_dir()).unwrap();
    let aside = scratch.path.with_extension("aside");
    assert!(
        std::fs::rename(&scratch.path, &aside).is_err(),
        "the hold must deny renaming the directory"
    );
    assert!(scratch.require_path(&scratch.path).is_ok());
}

#[cfg(unix)]
#[test]
fn a_directory_swapped_in_before_the_claim_is_never_adopted_or_emptied() {
    let path =
        std::env::temp_dir().join(format!("pokeemerald-scenario-swap-{}", std::process::id()));
    let aside = path.with_extension("aside");
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::remove_dir_all(&aside);
    create_private_dir(&path).unwrap();
    std::fs::rename(&path, &aside).unwrap();
    let foreign = foreign_dir("swap");
    std::fs::rename(&foreign, &path).unwrap();
    let claimed = Scratch::claim(path.clone());
    let adopted = claimed.is_ok();
    drop(claimed);
    let survived = path.join("keep.txt").exists();
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::remove_dir_all(&aside);
    assert!(
        !adopted,
        "claim adopted a directory this session did not create"
    );
    assert!(
        survived,
        "dropping the adopted scratch deleted foreign content"
    );
}

#[test]
fn a_session_that_cannot_load_its_pack_reports_start_failure_not_success() {
    let source = std::env::temp_dir().join(format!(
        "pokeemerald-scenario-garbage-{}.pack",
        std::process::id()
    ));
    std::fs::write(&source, b"not a real pack").unwrap();
    let session = Session::new(&PackSelection::Prepared(source.clone())).unwrap();
    std::fs::remove_file(&source).unwrap();
    let Err(error) = session.start() else {
        panic!("a garbage pack must not boot");
    };
    assert!(matches!(error, ScenarioError::Start(_)), "got {error:?}");
    assert!(
        !session.save.exists(),
        "a failed boot must not leave a save behind"
    );
}

#[cfg(unix)]
#[test]
fn the_held_directory_and_the_pack_are_not_inherited_by_child_processes() {
    let base = private_base("cloexec");
    let scratch = Scratch::create_in(&base).unwrap();
    let pack = scratch.create_new("pack").unwrap();
    let hold_flags = rustix::io::fcntl_getfd(scratch.hold().unwrap()).unwrap();
    let pack_flags = rustix::io::fcntl_getfd(&pack).unwrap();
    drop((pack, scratch));
    let _ = std::fs::remove_dir_all(&base);
    assert!(hold_flags.contains(rustix::io::FdFlags::CLOEXEC));
    assert!(pack_flags.contains(rustix::io::FdFlags::CLOEXEC));
}

#[cfg(unix)]
#[test]
fn restoring_the_private_mode_leaves_a_swapped_in_shared_directory_untouched() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = private_base("chmod-shared");
    let entry = base.join("entry");
    std::fs::create_dir(&entry).unwrap();
    std::fs::write(entry.join("keep.txt"), b"keep me").unwrap();
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o755)).unwrap();
    let result = restore_private_mode(&entry);
    let mode = std::fs::symlink_metadata(&entry)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    let _ = std::fs::remove_dir_all(&base);
    assert!(
        result.is_err(),
        "a directory with group or other access is not ours"
    );
    assert_eq!(mode, 0o755);
}

#[cfg(unix)]
#[test]
fn restoring_the_private_mode_leaves_a_swapped_in_nonempty_private_directory_as_it_was() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = private_base("chmod-nonempty");
    let entry = base.join("entry");
    std::fs::create_dir(&entry).unwrap();
    std::fs::write(entry.join("keep.txt"), b"keep me").unwrap();
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = restore_private_mode(&entry);
    let mode = std::fs::symlink_metadata(&entry)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o700)).unwrap();
    let kept = std::fs::read(entry.join("keep.txt")).unwrap();
    let _ = std::fs::remove_dir_all(&base);
    assert!(
        result.is_err(),
        "a non-empty directory is not the one just created"
    );
    assert_eq!(mode, 0o500);
    assert_eq!(kept, b"keep me");
}

#[cfg(unix)]
#[test]
fn cleanup_refuses_to_descend_into_a_directory_swapped_in_after_the_listing() {
    for swap in [false, true] {
        let base = private_base(if swap {
            "cleanup-swap"
        } else {
            "cleanup-plain"
        });
        let root = base.join("root");
        std::fs::create_dir_all(root.join("listed")).unwrap();
        std::fs::write(root.join("listed").join("owned.txt"), b"ours").unwrap();
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("keep.txt"), b"keep me").unwrap();
        let result = remove_contents(&std::fs::File::open(&root).unwrap(), &mut |name| {
            if swap && name.to_bytes() == b"listed" {
                std::fs::rename(root.join("listed"), base.join("aside")).unwrap();
                std::fs::rename(&elsewhere, root.join("listed")).unwrap();
            }
        });
        let kept = std::fs::read(root.join("listed").join("keep.txt"));
        let left = std::fs::read_dir(&root).unwrap().count();
        let _ = std::fs::remove_dir_all(&base);
        if swap {
            let error = result.expect_err("a swapped-in directory must not be emptied");
            assert!(
                error.to_string().contains("replaced during cleanup"),
                "{error}"
            );
            assert_eq!(kept.unwrap(), b"keep me");
        } else {
            result.unwrap();
            assert_eq!(left, 0, "without a swap the contents are removed");
        }
    }
}
