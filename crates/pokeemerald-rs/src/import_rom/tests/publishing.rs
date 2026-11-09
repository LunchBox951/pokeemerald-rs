//! A successful import publishes the pack and leaves no temporary file behind.

#[cfg(target_os = "linux")]
use std::ffi::OsStr;
use std::fs;

use super::super::import_to_with;
#[cfg(unix)]
use super::super::ImportRomError;
use super::support::{fake_pack, file_names, SourceRom, TempDir};

#[test]
fn a_successful_import_publishes_the_pack_and_clears_the_temp_file() {
    let dir = TempDir::new("publish");
    // A directory that does not exist yet: importing has to create it.
    let pack_path = dir.join("nested").join("pokeemerald.pack");
    let source = SourceRom::new("publish-src");
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    })
    .expect("the import succeeds");

    assert_eq!(fs::read(&pack_path).unwrap(), b"pack bytes");
    assert_eq!(outcome.pack_path(), pack_path);
    assert_eq!(outcome.entry_count(), 7);
    assert_eq!(outcome.pack_bytes(), 10);
    // The temporary file is gone: the pack is the only file left.
    assert_eq!(
        file_names(pack_path.parent().unwrap()),
        ["pokeemerald.pack"]
    );
}

#[test]
fn an_import_creates_and_publishes_through_every_missing_level() {
    // The first import on a machine is the one that creates the data
    // directory, and it can be more than one level deep.
    let dir = TempDir::new("deep-publish");
    let pack_path = dir
        .join("data")
        .join("pokeemerald-rs")
        .join("pokeemerald.pack");

    let source = SourceRom::new("deep-publish-src");
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    })
    .expect("the import succeeds");

    assert_eq!(outcome.pack_path(), pack_path);
    assert_eq!(fs::read(&pack_path).unwrap(), b"pack bytes");
    assert_eq!(
        file_names(pack_path.parent().unwrap()),
        ["pokeemerald.pack"]
    );
}

#[test]
fn a_pack_directory_spelled_through_dotdot_still_imports() {
    // `directories_to_create` walks lexically (`Path::parent`), so a
    // destination spelled through `..` produces a level whose
    // `Path::file_name` is `None` -- it names no new component to create,
    // only an already-real ancestor `create_directories`'s Unix arm has to
    // recognize rather than mistake for an unnameable failure.
    let dir = TempDir::new("dotdot-level");
    let pack_path = dir.join("missing").join("..").join("pokeemerald.pack");

    let source = SourceRom::new("dotdot-level-src");
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    })
    .expect("a dotdot-spelled destination still imports");

    assert_eq!(outcome.pack_path(), pack_path);
    // `missing/..` resolves back to `dir`, so the pack lands there.
    assert_eq!(
        fs::read(dir.join("pokeemerald.pack")).unwrap(),
        b"pack bytes"
    );
    assert_eq!(fs::read(outcome.pack_path()).unwrap(), b"pack bytes");
}

#[test]
fn a_re_import_replaces_the_pack_that_already_held_the_name() {
    // Re-importing after a pack-format bump is the ordinary second run, so
    // publishing must *replace* the installed pack rather than refuse a
    // taken name. Both `publish` arms promise that: `renameat(2)` on Unix,
    // and `std::fs::rename` off it -- which is `MoveFileExW` with
    // replace-existing on Windows, not C `rename`. Runs on every OS in CI's
    // `cargo test --workspace` matrix, so the off-Unix arm is pinned by a
    // real Windows run and not by this comment.
    let dir = TempDir::new("re-import");
    let pack_path = dir.join("pokeemerald.pack");
    fs::write(&pack_path, b"the pack from the last release").expect("the old pack writes");

    let source = SourceRom::new("re-import-src");
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"rebuilt pack"))
    })
    .expect("the second import succeeds");

    assert_eq!(outcome.pack_path(), pack_path);
    assert_eq!(fs::read(&pack_path).unwrap(), b"rebuilt pack");
    // The replaced pack leaves no litter: no temporary file, no backup.
    assert_eq!(file_names(&dir.path), ["pokeemerald.pack"]);
}

#[test]
fn an_existing_regular_file_at_the_destination_is_still_replaced() {
    // The new directory check must fire only on an actual directory: a
    // plain file already occupying the name is the ordinary re-import
    // case (also covered end-to-end by
    // `a_re_import_replaces_the_pack_that_already_held_the_name`), and has
    // to keep being replaced rather than refused.
    let dir = TempDir::new("existing-file");
    let pack_path = dir.join("pokeemerald.pack");
    fs::write(&pack_path, b"an old pack").expect("the occupying file writes");

    let source = SourceRom::new("existing-file-src");
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"a new pack"))
    })
    .expect("the import succeeds");

    assert_eq!(outcome.pack_path(), pack_path);
    assert_eq!(fs::read(&pack_path).unwrap(), b"a new pack");
    assert_eq!(file_names(&dir.path), ["pokeemerald.pack"]);
}

#[cfg(unix)]
#[test]
fn a_destination_symlinked_to_a_directory_is_still_published() {
    // A symlink to a directory must not be refused: see
    // `Dest::name_is_directory` for why `publish` handles it like any
    // other occupied name.
    let dir = TempDir::new("symlink-to-directory");
    let target = dir.join("elsewhere");
    fs::create_dir(&target).expect("the symlink's target directory is created");
    let pack_path = dir.join("pokeemerald.pack");
    std::os::unix::fs::symlink(&target, &pack_path).expect("the destination symlink is created");

    let source = SourceRom::new("symlink-to-directory-src");
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    })
    .expect("the import succeeds");

    assert_eq!(outcome.pack_path(), pack_path);
    // The symlink was replaced by the finished pack, a regular file.
    assert!(!fs::symlink_metadata(&pack_path).unwrap().is_symlink());
    assert_eq!(fs::read(&pack_path).unwrap(), b"pack bytes");
    // What it used to point at is untouched.
    assert!(target.is_dir());
    assert!(fs::read_dir(&target).unwrap().next().is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn a_non_utf8_pack_name_is_published_byte_for_byte() {
    // The requested basename survives as an `OsStr` end to end: the file
    // published is the one the player named, not a lossy re-spelling.
    // Linux-only: APFS on macOS rejects non-UTF-8 names with EILSEQ at the
    // filesystem, so the premise cannot be constructed there.
    use std::os::unix::ffi::OsStrExt as _;

    let dir = TempDir::new("non-utf8-name");
    let name = OsStr::from_bytes(b"pok\xe9mon.pack");
    let pack_path = dir.path.join(name);

    let source = SourceRom::new("non-utf8-name-src");
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    })
    .expect("the import succeeds");

    assert_eq!(outcome.pack_path(), pack_path);
    assert_eq!(fs::read(&pack_path).unwrap(), b"pack bytes");
}

/// Set in the child [`a_re_import_under_an_owner_stripping_umask_publishes_an_owner_readable_pack`]
/// re-runs itself as, so only that child changes its own umask.
#[cfg(unix)]
const IMPORT_UMASK_CHILD: &str = "POKEEMERALD_IMPORT_UMASK_CHILD";

/// An import under a umask that strips the owner bits (`0444`) over an
/// installed, readable pack publishes a pack its owner can still read.
/// `create_new` asks for `0o666`, so the umask leaves `0o222`, and the rename
/// carries that mode onto the published name. The mode bits are asserted
/// because a root runner reads a `0o222` file regardless.
#[cfg(unix)]
#[test]
fn a_re_import_under_an_owner_stripping_umask_publishes_an_owner_readable_pack() {
    use std::os::unix::fs::PermissionsExt as _;

    let name = "import_rom::tests::publishing::\
                a_re_import_under_an_owner_stripping_umask_publishes_an_owner_readable_pack";
    if std::env::var_os(IMPORT_UMASK_CHILD).is_none() {
        // The umask is process-wide, so the 0o444 one is set only in a re-run
        // of this test binary that runs this test alone.
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", "umask 0444 && exec \"$0\" \"$@\""])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1"])
            .env(IMPORT_UMASK_CHILD, "1")
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

    let dir = TempDir::new("umask-publish");
    // The directory already exists and is accessible, so only the file's own
    // mode is in question.
    fs::set_permissions(&dir.path, fs::Permissions::from_mode(0o700)).unwrap();
    let pack_path = dir.join("pokeemerald.pack");
    fs::write(&pack_path, b"old pack").unwrap();
    fs::set_permissions(&pack_path, fs::Permissions::from_mode(0o600)).unwrap();
    let source = SourceRom::new("umask-publish-src");
    fs::set_permissions(source.path(), fs::Permissions::from_mode(0o600)).unwrap();

    import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    })
    .expect("the import succeeds");

    let mode = fs::metadata(&pack_path).unwrap().permissions().mode() & 0o7777;
    // 0666 & !0444 leaves 0222; only the owner read is restored, so the
    // group and other bits still honour the umask.
    assert_eq!(mode, 0o622, "published pack mode: {mode:o}");
    assert_eq!(fs::read(&pack_path).unwrap(), b"pack bytes");
    assert_eq!(file_names(&dir.path), ["pokeemerald.pack"]);
}

/// The umasks [`an_import_to_a_missing_nested_destination_keeps_owner_access_under_any_umask`]
/// imports under.
#[cfg(unix)]
const OWNER_STRIPPING_UMASKS: &[&str] = &["0444", "0277", "0477", "0022", "0177", "0777"];

/// Set in the child [`an_import_to_a_missing_nested_destination_keeps_owner_access_under_any_umask`]
/// re-runs itself as, holding the umask that child imports under.
#[cfg(unix)]
const NESTED_UMASK_CHILD: &str = "POKEEMERALD_IMPORT_NESTED_UMASK_CHILD";

/// Set in that child, on Linux and Android, to import with the
/// `/proc/self/fd` route unavailable.
#[cfg(any(target_os = "linux", target_os = "android"))]
const NESTED_NO_PROCFS_CHILD: &str = "POKEEMERALD_IMPORT_NESTED_NO_PROCFS_CHILD";

/// Whether a level `mkdirat(0777)` leaves under `mask` is one no inode-bound
/// route restores or pins on this target; the import then fails closed.
#[cfg(unix)]
fn import_refuses_level_mode(mask: u32, no_procfs: bool) -> bool {
    let left = 0o777 & !mask;
    if cfg!(target_os = "freebsd") {
        false
    } else if cfg!(any(target_os = "linux", target_os = "android")) {
        no_procfs && left == 0
    } else if cfg!(target_vendor = "apple") {
        left & 0o500 == 0
    } else {
        left & 0o400 == 0
    }
}

/// An import to a destination whose every level is missing leaves each level
/// it created with the owner's read, write and search, whatever the umask
/// took, and the group and other bits the umask allows. `mkdirat(0777)`
/// under `0444` leaves `0333`, under `0277` leaves `0500`, which the next
/// level cannot be created in. The modes are asserted because a root runner
/// is not stopped by either.
///
/// The umask is process-wide, so each case runs alone in a child that reads
/// its mask from the environment.
///
/// Every mask is tried on every Unix target, and on Linux and Android a
/// second time with the `/proc/self/fd` route unavailable. A level needs the
/// owner's read or search bit to be pinned (Apple: search; Unix systems that
/// pin with a read-mode open: read), and a mode with none left has no
/// inode-bound restore: `0777` on Apple and on Linux and Android without
/// procfs, `0444`, `0477` and `0777` on those other Unix systems (see
/// [`import_refuses_level_mode`]). Those imports must fail with the
/// diagnostic naming the umask and publish nothing, rather than `chmod` a
/// name; a privileged runner is never refused, so there they publish.
#[cfg(unix)]
#[test]
fn an_import_to_a_missing_nested_destination_keeps_owner_access_under_any_umask() {
    use std::os::unix::fs::PermissionsExt as _;

    let name = "import_rom::tests::publishing::\
                an_import_to_a_missing_nested_destination_keeps_owner_access_under_any_umask";
    let Some(mask) = std::env::var_os(NESTED_UMASK_CHILD) else {
        let procfs_variants: &[bool] = if cfg!(any(target_os = "linux", target_os = "android")) {
            &[false, true]
        } else {
            &[false]
        };
        for (mask, no_procfs) in OWNER_STRIPPING_UMASKS.iter().flat_map(|mask| {
            procfs_variants
                .iter()
                .map(move |no_procfs| (mask, *no_procfs))
        }) {
            let mut command = std::process::Command::new("/bin/sh");
            #[cfg(any(target_os = "linux", target_os = "android"))]
            if no_procfs {
                command.env(NESTED_NO_PROCFS_CHILD, "1");
            }
            let output = command
                .args(["-c", "umask \"$1\" && shift && exec \"$0\" \"$@\""])
                .arg(std::env::current_exe().unwrap())
                .arg(mask)
                .args(["--exact", name, "--test-threads=1"])
                .env(NESTED_UMASK_CHILD, mask)
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains("1 passed"),
                "umask {mask} (procfs unavailable: {no_procfs}) child failed:\n{stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    };
    let mask = u32::from_str_radix(mask.to_str().unwrap(), 8).unwrap();

    // The fixtures are made under an ordinary umask, so only the import's
    // own directories answer to the hostile one.
    let hostile = rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o022));
    let dir = TempDir::new("umask-nested");
    let source = SourceRom::new("umask-nested-src");
    rustix::process::umask(hostile);

    let outer = dir.join("one");
    let inner = outer.join("two");
    let pack_path = inner.join("pokeemerald.pack");
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let no_procfs = std::env::var_os(NESTED_NO_PROCFS_CHILD).is_some();
    // Only Linux and Android have a procfs route to withhold.
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let no_procfs = false;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let _no_procfs = no_procfs.then(super::super::dest::procfs_unavailable::make_unavailable);
    // Whether mode bits bind this process, asked of the outcome.
    let probe = dir.join("probe");
    fs::create_dir(&probe).unwrap();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o000)).unwrap();
    let privileged = fs::read_dir(&probe).is_ok();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir(&probe).unwrap();

    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    });
    if import_refuses_level_mode(mask, no_procfs) && !privileged {
        let err = outcome.expect_err("a level no inode-bound route restores fails the import");
        assert!(
            matches!(err, ImportRomError::CreateDirFailed { .. }),
            "umask {mask:04o}: {err:?}"
        );
        assert!(
            err.to_string().contains("umask"),
            "umask {mask:04o}: the diagnostic names the umask: {err}"
        );
        assert!(!pack_path.exists(), "nothing is published");
        // Whatever the failed import left stands closed (the levels, and a
        // quarantine its rollback could not remove); open it so the fixture
        // cleans up.
        let open_up = |path: &std::path::Path| {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
        };
        open_up(&outer);
        open_up(&inner);
        for entry in fs::read_dir(&dir.path).into_iter().flatten().flatten() {
            open_up(&entry.path());
        }
        return;
    }
    outcome.expect("the import creates the nested destination");

    // The owner's rwx is added to whatever `mkdirat(0777)` left, and nothing
    // else changes.
    let expected_dir = (0o777 & !mask) | 0o700;
    for level in [&outer, &inner] {
        let mode = fs::metadata(level).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, expected_dir, "{level:?} under umask {mask:04o}");
    }
    let mode = fs::metadata(&pack_path).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, (0o666 & !mask) | 0o600, "pack under umask {mask:04o}");
    assert_eq!(fs::read(&pack_path).unwrap(), b"pack bytes");
    assert_eq!(file_names(&inner), ["pokeemerald.pack"]);
}

// Same platform boundary as the traversal opens in `created_dirs`: every
// other Unix falls back to a read-requiring open.
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_vendor = "apple"
))]
#[test]
fn an_import_into_an_existing_destination_that_is_writable_but_not_readable_publishes() {
    use std::os::unix::fs::PermissionsExt as _;

    // Creating, renaming and removing a basename needs write and search on
    // the directory, never read: a `0o300` drop box takes the pack.
    let dir = TempDir::new("unreadable-destination");
    let drop_box = dir.join("drop-box");
    fs::create_dir(&drop_box).expect("the destination is created");
    let pack_path = drop_box.join("pokeemerald.pack");
    let source = SourceRom::new("unreadable-destination-src");

    fs::set_permissions(&drop_box, fs::Permissions::from_mode(0o300))
        .expect("the destination closes");
    // A privileged user ignores directory permissions, so the close-off does
    // not block them and there is nothing to assert. Asked of the outcome
    // rather than of the uid, so it needs no libc.
    let unreadable = fs::read_dir(&drop_box).is_err();
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    });
    fs::set_permissions(&drop_box, fs::Permissions::from_mode(0o755))
        .expect("the destination reopens");

    if unreadable {
        outcome.expect("a writable, searchable destination takes the pack");
        assert_eq!(fs::read(&pack_path).unwrap(), b"pack bytes");
        assert_eq!(file_names(&drop_box), ["pokeemerald.pack"]);
    } else {
        // A privileged user is not closed off by `0o300`, so the assertion
        // cannot run; say so rather than pass vacuously.
        eprintln!(
            "an_import_into_an_existing_destination_that_is_writable_but_not_readable_publishes: \
             skipped -- running privileged, 0o300 did not close the destination off"
        );
    }
}
