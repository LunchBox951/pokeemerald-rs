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
#[cfg(any(unix, windows))]
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
#[cfg(any(target_os = "linux", target_os = "android"))]
const UMASK_CHILD: &str = "POKEEMERALD_SCENARIO_UMASK_CHILD";

/// Linux only: other Unixes refuse, rather than restore, a scratch directory
/// the umask left without owner read (see `restore_private_mode`).
#[cfg(any(target_os = "linux", target_os = "android"))]
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
    // The child's umask made `base` 0o333, which `remove_dir_all` cannot
    // list; give the owner access back so the run leaves nothing behind.
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir_all(&base).unwrap();
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
fn restoring_the_private_mode_fixes_or_refuses_a_search_only_directory() {
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
    if cfg!(any(target_os = "linux", target_os = "android")) {
        result.unwrap();
        assert_eq!(mode, 0o700);
    } else {
        // No safe restore beyond Linux: refused, and the mode is unchanged.
        assert!(result.unwrap_err().contains("#2475"));
        assert_eq!(mode, 0o300);
    }
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
        std::fs::remove_dir_all(&base).unwrap();
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
        std::fs::remove_dir_all(&base).unwrap();
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
    std::fs::remove_dir_all(&base).unwrap();
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

#[cfg(unix)]
#[test]
fn cleanup_refuses_to_unlink_a_file_swapped_in_after_the_listing() {
    let base = private_base("cleanup-file-swap");
    let root = base.join("root");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("listed.sav"), b"ours").unwrap();
    let elsewhere = base.join("elsewhere.sav");
    std::fs::write(&elsewhere, b"keep me").unwrap();
    let result = remove_contents(&std::fs::File::open(&root).unwrap(), &mut |name| {
        if name.to_bytes() == b"listed.sav" {
            std::fs::rename(root.join("listed.sav"), base.join("aside.sav")).unwrap();
            std::fs::rename(&elsewhere, root.join("listed.sav")).unwrap();
        }
    });
    let kept = std::fs::read(root.join("listed.sav"));
    let _ = std::fs::remove_dir_all(&base);
    let error = result.expect_err("a swapped-in file must not be unlinked");
    assert!(
        error.to_string().contains("replaced during cleanup"),
        "{error}"
    );
    assert_eq!(kept.unwrap(), b"keep me");
}

/// Root bypasses mode bits, so the unreadable-directory cases cannot occur.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn running_as_root() -> bool {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::metadata("/proc/self").is_ok_and(|meta| meta.uid() == 0)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn cleanup_recovers_engine_created_unreadable_directories() {
    use std::os::unix::fs::PermissionsExt as _;
    if running_as_root() {
        eprintln!("skipped: root ignores directory modes, so 0333 stays readable");
        return;
    }
    let base = private_base("cleanup-unreadable");
    let root = base.join("root");
    let deep = root.join("save").join("deep");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("x.sav"), b"x").unwrap();
    std::fs::write(root.join("save").join("a.sav"), b"a").unwrap();
    let set = |path: &Path, mode| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    set(&root.join("save").join("a.sav"), 0o222);
    set(&deep, 0o333);
    set(&root.join("save"), 0o333);
    let result = remove_contents(&std::fs::File::open(&root).unwrap(), &mut |_| {});
    let left = std::fs::read_dir(&root).map(Iterator::count);
    let _ = std::fs::remove_dir_all(&base);
    result.unwrap();
    assert_eq!(left.unwrap(), 0, "the unreadable tree is fully removed");
}

/// Set in the child [`dropping_a_session_removes_save_media_created_under_an_owner_stripping_umask`]
/// re-runs itself as, so only that child changes its own umask.
#[cfg(any(target_os = "linux", target_os = "android"))]
const SAVE_UMASK_CHILD: &str = "POKEEMERALD_SCENARIO_SAVE_UMASK_CHILD";

/// The engine creates `save/` and its save file by pathname under the
/// process umask (`App::new_headless_real_at` through `SaveSlot::at_path`),
/// so under 0o444 they are 0o333 and 0o222. `create_dir_all` and `write`
/// under that same umask create the same media without a real pack; dropping
/// the session must then remove the whole scratch tree.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn dropping_a_session_removes_save_media_created_under_an_owner_stripping_umask() {
    use std::os::unix::fs::PermissionsExt as _;
    let name = "scenario::session::tests::\
                dropping_a_session_removes_save_media_created_under_an_owner_stripping_umask";
    if std::env::var_os(SAVE_UMASK_CHILD).is_none() {
        if running_as_root() {
            eprintln!("skipped: root ignores directory modes, so 0333 stays readable");
            return;
        }
        // The umask is process-wide, so the 0o444 one is set only in a
        // re-run of this test binary that runs this test alone.
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", "umask 0444 && exec \"$0\" \"$@\""])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1"])
            .env(SAVE_UMASK_CHILD, "1")
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
    let base = private_base("save-umask");
    let source = base.join("source.pack");
    std::fs::write(&source, b"not a real pack").unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o600)).unwrap();
    let session = Session::new_in(&PackSelection::Prepared(source), &base).unwrap();
    let scratch = session.scratch.path.clone();
    let save_dir = session.save.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(save_dir.join("deep")).unwrap();
    std::fs::write(save_dir.join("deep").join("x.sav"), b"x").unwrap();
    std::fs::write(&session.save, b"save").unwrap();
    let mode = |path: &Path| {
        std::fs::symlink_metadata(path)
            .unwrap()
            .permissions()
            .mode()
            & 0o7777
    };
    let modes = (
        mode(&save_dir),
        mode(&save_dir.join("deep")),
        mode(&session.save),
    );
    drop(session);
    let removed = !scratch.exists();
    // Leave nothing behind even if cleanup failed: give the owner access back
    // to every directory the umask stripped before removing the base.
    for dir in [
        base.clone(),
        scratch.clone(),
        save_dir.clone(),
        save_dir.join("deep"),
    ] {
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    std::fs::remove_dir_all(&base).unwrap();
    assert_eq!(
        modes,
        (0o333, 0o333, 0o222),
        "the umask stripped owner read"
    );
    assert!(
        removed,
        "dropping the session removes the whole scratch tree"
    );
}

/// The recovered descriptor is opened through the pin that was verified and
/// chmod-ed, not by re-resolving the name: a different directory installed at
/// the name between the chmod and the reopen is never the one returned, and
/// its mode is left alone. Runs as root too: it calls the recovery directly.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn recovered_unreadable_directory_is_read_through_the_verified_pin() {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let base = private_base("cleanup-unreadable-pin");
    let root = base.join("root");
    std::fs::create_dir_all(root.join("listed").join("inner")).unwrap();
    std::fs::set_permissions(root.join("listed"), std::fs::Permissions::from_mode(0o333)).unwrap();
    let listed = std::fs::symlink_metadata(root.join("listed")).unwrap();
    let parent = std::fs::File::open(&root).unwrap();
    let readable =
        restore_owner_access_with(&parent, c"listed", listed.dev(), listed.ino(), || {
            // The window the fix closes: the chmod-ed inode moves away and a
            // different directory takes its name before the reopen.
            std::fs::rename(root.join("listed"), root.join("moved")).unwrap();
            std::fs::create_dir(root.join("listed")).unwrap();
            std::fs::set_permissions(root.join("listed"), std::fs::Permissions::from_mode(0o555))
                .unwrap();
        });
    let mode = |name: &str| {
        std::fs::symlink_metadata(root.join(name))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777
    };
    let (moved_mode, replacement_mode) = (mode("moved"), mode("listed"));
    let replacement = std::fs::symlink_metadata(root.join("listed")).unwrap();
    for name in ["moved", "listed"] {
        let _ = std::fs::set_permissions(root.join(name), std::fs::Permissions::from_mode(0o700));
    }
    let _ = std::fs::remove_dir_all(&base);
    let readable = std::fs::File::from(readable.unwrap());
    let found = readable.metadata().unwrap();
    assert_ne!(
        (replacement.dev(), replacement.ino()),
        (listed.dev(), listed.ino()),
        "setup: the replacement is a different inode"
    );
    assert_eq!(
        (found.dev(), found.ino()),
        (listed.dev(), listed.ino()),
        "the readable descriptor is the pinned, listed inode, not the replacement"
    );
    assert_eq!(
        moved_mode, 0o700,
        "owner access was restored on the listed inode"
    );
    assert_eq!(
        replacement_mode, 0o555,
        "the replacement at the name is never chmod-ed"
    );
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn cleanup_never_chmods_an_unreadable_directory_swapped_in_after_the_listing() {
    use std::os::unix::fs::PermissionsExt as _;
    if running_as_root() {
        eprintln!("skipped: root ignores directory modes, so 0333 stays readable");
        return;
    }
    let base = private_base("cleanup-unreadable-swap");
    let root = base.join("root");
    std::fs::create_dir_all(root.join("listed")).unwrap();
    std::fs::set_permissions(root.join("listed"), std::fs::Permissions::from_mode(0o333)).unwrap();
    let elsewhere = base.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o333)).unwrap();
    let result = remove_contents(&std::fs::File::open(&root).unwrap(), &mut |name| {
        if name.to_bytes() == b"listed" {
            std::fs::rename(root.join("listed"), base.join("aside")).unwrap();
            std::fs::rename(&elsewhere, root.join("listed")).unwrap();
        }
    });
    let mode = std::fs::metadata(root.join("listed"))
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    for dir in [root.join("listed"), base.join("aside")] {
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let _ = std::fs::remove_dir_all(&base);
    let error = result.expect_err("a swapped-in directory must not be recovered");
    assert!(
        error.to_string().contains("replaced during cleanup"),
        "{error}"
    );
    assert_eq!(mode, 0o333, "a foreign directory is never chmod-ed");
}

#[cfg(windows)]
#[test]
fn windows_cleanup_empties_the_held_scratch_and_leaves_a_sibling_untouched() {
    let mut scratch = Scratch::create_in(&std::env::temp_dir()).unwrap();
    let sibling = foreign_dir("windows-sibling");
    let deep = scratch.path.join("save").join("deep");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("x.sav"), b"x").unwrap();
    std::fs::write(scratch.path.join(PACK_FILE_NAME), b"pack").unwrap();
    let path = scratch.path.clone();
    let result = scratch
        .remove_all_at(&path)
        .map_err(|error| error.to_string());
    let kept = std::fs::read(sibling.join("keep.txt"));
    let _ = std::fs::remove_dir_all(&sibling);
    result.unwrap();
    assert!(!path.exists(), "the emptied scratch root is removed");
    assert_eq!(kept.unwrap(), b"keep me");
}

#[cfg(windows)]
#[test]
fn windows_cleanup_refuses_a_directory_swapped_in_under_the_held_root() {
    let scratch = Scratch::create_in(&std::env::temp_dir()).unwrap();
    let foreign = foreign_dir("windows-swap");
    let listed = scratch.path.join("listed");
    std::fs::create_dir(&listed).unwrap();
    std::fs::write(listed.join("own.txt"), b"own").unwrap();
    let aside = foreign.with_extension("aside");
    let _ = std::fs::remove_dir_all(&aside);
    let mut swapped = false;
    let result = remove_contents(scratch.hold().unwrap(), &scratch.path, &mut |name| {
        if name == "listed" && !swapped {
            swapped = true;
            std::fs::rename(&listed, &aside).unwrap();
            std::fs::rename(&foreign, &listed).unwrap();
        }
    });
    let kept = std::fs::read(listed.join("keep.txt"));
    let moved = std::fs::read(aside.join("own.txt"));
    // Put the trees back where they were made so the cleanups can run.
    let _ = std::fs::remove_dir_all(&aside);
    let _ = std::fs::remove_dir_all(&listed);
    let error = result.expect_err("a swapped-in directory must not be removed");
    assert!(
        error.to_string().contains("replaced during cleanup"),
        "{error}"
    );
    assert_eq!(kept.unwrap(), b"keep me", "the foreign tree survives");
    assert_eq!(moved.unwrap(), b"own", "the original is not deleted");
}

#[cfg(windows)]
#[test]
fn a_held_scratch_directory_keeps_its_parent_from_being_renamed() {
    let base = private_base("windows-parent");
    let scratch = Scratch::create_in(&base).unwrap();
    let aside = base.with_extension("aside");
    let renamed = std::fs::rename(&base, &aside);
    if renamed.is_ok() {
        let _ = std::fs::rename(&aside, &base);
    }
    drop(scratch);
    let _ = std::fs::remove_dir_all(&base);
    assert!(
        renamed.is_err(),
        "NTFS refuses to rename a directory with an open handle in its subtree"
    );
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn a_failed_readable_open_restores_the_original_mode_and_propagates() {
    use std::os::unix::fs::PermissionsExt as _;
    let base = private_base("chmod-open-fails");
    let entry = base.join("entry");
    std::fs::create_dir(&entry).unwrap();
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o300)).unwrap();
    let result = hold::restore_private_mode_with(&entry, |_| {
        Err(std::io::Error::other("injected readable-open failure"))
    });
    let mode = std::fs::symlink_metadata(&entry)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    let _ = std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o700));
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(
        result.unwrap_err().to_string(),
        "injected readable-open failure"
    );
    assert_eq!(mode, 0o300, "the widened mode must be put back");
}
