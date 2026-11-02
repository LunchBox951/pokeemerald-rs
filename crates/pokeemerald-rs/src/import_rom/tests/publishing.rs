//! A successful import publishes the pack and leaves no temporary file behind.

#[cfg(target_os = "linux")]
use std::ffi::OsStr;
use std::fs;

use super::super::import_to_with;
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
    // The `..` lands the pack beside `missing`, so record its name too.
    let _ = dir.join("pokeemerald.pack");
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
    let pack_path = dir.join(name);

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
