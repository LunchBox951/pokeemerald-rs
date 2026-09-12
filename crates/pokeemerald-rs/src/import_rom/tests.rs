//! Unit tests for the `--import-rom` write path.
//!
//! No real ROM is involved anywhere. The importer is injected where the
//! test is about *publishing* the pack, and `rom_import::fixture` supplies
//! a synthetic cartridge image where the test is about the real wiring.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rom_import::fixture::RomFixture;
use rom_import::{ImportError, ImportedPack};

use super::dest::{not_a_directory_error, Dest};
use super::{
    create_directories, directories_to_create, import_to, import_to_with, pack_directory,
    pack_name, ImportOutcome, ImportRomError,
};

/// A pack of `bytes` the injected importer hands back, standing in for a
/// real one.
fn fake_pack(bytes: &[u8]) -> ImportedPack {
    ImportedPack::new(7, bytes.to_vec())
}

/// A temporary directory that removes itself, so a failing test cannot
/// leave a 16 MiB fixture behind.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// A fresh, empty directory under the OS temporary directory.
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-import-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a temporary directory");
        Self { path }
    }

    /// A path inside this directory.
    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Every file directly inside `dir`, sorted, as plain names.
fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("the directory exists")
        .map(|entry| entry.expect("a readable entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The path of the one entry inside `dir` that [`Dest::temp_name`]'s
/// prefix names -- the temporary file the import is currently building,
/// found by content rather than threaded through as a parameter, since the
/// name itself is generated inside `import_to_with` and never handed back
/// on a path that panics part-way through the import.
fn find_temp_path(dir: &Path) -> PathBuf {
    fs::read_dir(dir)
        .expect("the directory exists")
        .map(|entry| entry.expect("a readable entry").path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&format!("{}.", super::TEMP_PREFIX)))
        })
        .expect("the temporary file the import just created")
}

/// A stand-in for the file the player passes to `--import-rom`.
///
/// It has to be a real, openable file even where the importer is injected:
/// the import opens the source once up front and hands that handle on, so
/// there is no longer such a thing as a source path nothing opens. It keeps
/// its own directory, so a test's assertions about the *destination*
/// directory's contents are unaffected by it.
struct SourceRom {
    /// The directory holding it, removed when this is dropped.
    _dir: TempDir,
    /// The path to hand to the import.
    path: PathBuf,
}

impl SourceRom {
    /// A source file whose bytes are nobody's business but the importer's.
    fn new(label: &str) -> Self {
        let dir = TempDir::new(label);
        let path = dir.join("emerald.gba");
        fs::write(&path, b"stand-in for the player's cartridge").expect("the source writes");
        Self { _dir: dir, path }
    }

    /// The path to hand to the import.
    fn path(&self) -> &Path {
        &self.path
    }
}

/// Write a synthetic Emerald-shaped ROM image and return its path.
///
/// The image has a valid cartridge header, so it gets past every structural
/// check and is rejected on identity alone: its SHA-1 matches no shipped
/// profile, which is exactly what a player pointing the flag at the wrong
/// file would hit.
fn write_fixture_rom(dir: &TempDir) -> PathBuf {
    let path = dir.join("fixture.gba");
    fs::write(&path, RomFixture::new().emerald_header().finish()).expect("the fixture writes");
    path
}

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
fn every_level_the_import_must_create_is_listed_outermost_first() {
    // Each level is a name in the one before it, and it is the *parent*
    // that has to be synced to make that name durable -- so the list has to
    // name every level, in the order they are created.
    let dir = TempDir::new("levels");
    let one = dir.join("one");
    let two = one.join("two");
    let three = two.join("three");

    assert_eq!(
        directories_to_create(&three),
        [one.clone(), two.clone(), three.clone()]
    );
    // A destination that is already there is created by nobody, which is
    // also how a failed import knows not to remove it.
    assert!(directories_to_create(&dir.path).is_empty());

    fs::create_dir_all(&one).expect("the first level is created");
    assert_eq!(directories_to_create(&three), [two.clone(), three.clone()]);
    fs::create_dir_all(&three).expect("the rest are created");
    assert!(directories_to_create(&three).is_empty());
}

#[test]
fn only_the_levels_the_run_created_come_back_as_its_own() {
    // Ownership is the create's answer, not an earlier look's: a level that
    // is already there when the create reaches it belongs to whoever made
    // it, and a failed import must leave it standing even while it is
    // empty. Another importer racing this one hits exactly that path.
    let dir = TempDir::new("created-levels");
    let one = dir.join("one");
    let two = one.join("two");
    fs::create_dir(&one).expect("the outer level is created");

    let created = create_directories(&two).expect("the missing level is created");
    assert_eq!(created, std::slice::from_ref(&two));
    assert!(two.is_dir());

    let again = create_directories(&two).expect("an existing destination is not a failure");
    assert!(again.is_empty(), "a run that created nothing owns nothing");
}

#[test]
fn a_creation_that_fails_part_way_hands_back_the_levels_it_made() {
    // The overlong component trips the create after `new/` is on disk, and
    // the prefix has to come back with the error for the caller to undo.
    let dir = TempDir::new("partial-create-levels");
    let outer = dir.join("new");
    let overlong = "x".repeat(300);

    let (created, _source) = create_directories(&outer.join(overlong))
        .expect_err("the overlong component cannot be created");

    assert_eq!(created, std::slice::from_ref(&outer));
    assert!(outer.is_dir());
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
fn a_failed_import_leaves_neither_a_pack_nor_a_partial_file() {
    let dir = TempDir::new("fail-closed");
    let pack_path = dir.join("pokeemerald.pack");
    // The temporary file is created before the importer runs, so a failed
    // import has one to take with it even though no bytes ever reached it.
    let source = SourceRom::new("fail-closed-src");
    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(
        err,
        ImportRomError::Import {
            source: ImportError::EmptyPack,
            ..
        }
    ));
    assert!(!pack_path.exists());
    assert!(file_names(&dir.path).is_empty());
}

#[test]
fn a_failed_import_names_the_partial_file_its_own_cleanup_left_behind() {
    // `dest.discard`'s own removal can fail too, and silently dropping
    // that (as the unfixed code did) leaves the temporary file's name
    // permanently taken with no clue why every retry then also fails. The
    // injected importer swaps the just-created temporary file for a
    // non-empty directory of the same name before failing, so
    // `dest.discard`'s removal fails deterministically -- a directory is
    // refused regardless of the runner's privileges, unlike a
    // permission-based seam, which root bypasses (the same shape as
    // `crates/xtask/src/extract/mod.rs`'s
    // `a_failed_staging_cleanup_names_the_artifact_it_left_behind`, and
    // `discard`'s own `a-directory` case in
    // `discard_reports_a_name_that_is_gone_and_one_it_could_not_remove`
    // below).
    let dir = TempDir::new("import-cleanup-fails");
    let pack_path = dir.join("pokeemerald.pack");
    let source = SourceRom::new("import-cleanup-fails-src");

    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        let temp_path = find_temp_path(&dir.path);
        fs::remove_file(&temp_path).expect("the temp file removes");
        fs::create_dir(&temp_path).expect("the directory takes the freed name");
        fs::write(temp_path.join("occupant"), b"occupant").expect("the occupant writes");
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(
        matches!(
            &err,
            ImportRomError::Import {
                source: ImportError::EmptyPack,
                temp_path: Some(_),
                temp_removed: false,
            }
        ),
        "expected a named, unremoved partial file: {err:?}"
    );
    assert!(
        err.to_string().contains("could not be removed"),
        "a failed cleanup must say so: {err}"
    );
    // `TempDir`'s own `Drop` removes whatever is left, directory included.
}

#[test]
fn a_failed_import_removes_the_directory_it_created() {
    let dir = TempDir::new("undo-dir");
    let created = dir.join("pokeemerald-rs");
    let pack_path = created.join("pokeemerald.pack");

    let source = SourceRom::new("undo-dir-src");
    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(err, ImportRomError::Import { .. }));
    // A failed import leaves no trace: not even an empty data directory
    // that would look like a half-installed game.
    assert!(!created.exists());
    assert!(file_names(&dir.path).is_empty());
}

#[test]
fn a_failed_import_removes_every_level_it_created() {
    // A `$POKEEMERALD_PACK` can point through more than one missing level,
    // and leaving the outer ones behind is the same litter as leaving the
    // innermost: an empty chain that looks like a half-installed game.
    let dir = TempDir::new("undo-levels");
    let outer = dir.join("new");
    let pack_path = outer.join("data").join("pokeemerald.pack");

    let source = SourceRom::new("undo-levels-src");
    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(err, ImportRomError::Import { .. }));
    assert!(
        !outer.exists(),
        "every created level goes, not just the leaf"
    );
    assert!(file_names(&dir.path).is_empty());
}

#[test]
fn a_failed_import_keeps_the_levels_it_did_not_create() {
    // The cleanup reaches only the levels this run made. A directory that
    // was already there is not this run's to remove, however empty it
    // happens to be.
    let dir = TempDir::new("undo-stops");
    let kept = dir.join("already-here");
    fs::create_dir_all(&kept).expect("the existing level is created");
    let created = kept.join("made-by-the-import");
    let pack_path = created.join("pokeemerald.pack");

    let source = SourceRom::new("undo-stops-src");
    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(err, ImportRomError::Import { .. }));
    assert!(!created.exists(), "the level the import made goes");
    assert!(kept.is_dir(), "the level that was already there stays");
}

#[test]
fn a_failed_directory_creation_removes_the_levels_it_created() {
    // `create_dir_all` itself can fail part-way: the overlong component
    // trips it after `new/` is already on disk. The prefix it created is
    // the same litter as any other failed import's.
    let dir = TempDir::new("undo-partial-create");
    let outer = dir.join("new");
    let overlong = "x".repeat(300);
    let pack_path = outer.join(&overlong).join("pokeemerald.pack");

    let source = SourceRom::new("undo-partial-create-src");
    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(err, ImportRomError::CreateDirFailed { .. }));
    assert!(!outer.exists(), "the level the failed creation made goes");
    assert!(file_names(&dir.path).is_empty());
}

#[test]
fn an_import_into_an_existing_directory_leaves_it_alone() {
    let dir = TempDir::new("keep-dir");
    let pack_path = dir.join("pokeemerald.pack");

    let source = SourceRom::new("keep-dir-src");
    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(err, ImportRomError::Import { .. }));
    // The directory was already there, so the cleanup must not touch it.
    assert!(dir.path.is_dir());
}

#[test]
fn an_existing_pack_survives_a_failed_import() {
    let dir = TempDir::new("keep-old");
    let pack_path = dir.join("pokeemerald.pack");
    fs::write(&pack_path, b"the pack that already worked").expect("the old pack writes");

    let source = SourceRom::new("keep-old-src");
    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(err, ImportRomError::Import { .. }));
    assert_eq!(
        fs::read(&pack_path).unwrap(),
        b"the pack that already worked"
    );
    assert_eq!(file_names(&dir.path), ["pokeemerald.pack"]);
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
fn the_import_error_renders_the_importers_own_message() {
    let err = ImportRomError::Import {
        source: ImportError::EmptyPack,
        temp_path: None,
        temp_removed: true,
    };
    assert_eq!(err.to_string(), ImportError::EmptyPack.to_string());
    assert!(err.to_string().contains("no pack was written"));
}

#[test]
fn a_synthetic_rom_is_rejected_on_identity_and_writes_nothing() {
    let dir = TempDir::new("fixture-rom");
    let rom_path = write_fixture_rom(&dir);
    let pack_path = dir.join("pokeemerald.pack");

    let err = import_to(&rom_path, &pack_path).unwrap_err();

    assert!(
        matches!(
            err,
            ImportRomError::Import {
                source: ImportError::UnsupportedRevision { .. },
                ..
            }
        ),
        "expected an unsupported-revision failure, got: {err}"
    );
    assert!(!pack_path.exists());
    // Only the fixture ROM is left: no pack, no temporary file.
    assert_eq!(file_names(&dir.path), ["fixture.gba"]);
}

#[test]
fn a_missing_rom_reports_a_read_failure_and_writes_nothing() {
    let dir = TempDir::new("missing-rom");
    let pack_path = dir.join("pokeemerald.pack");

    let err = import_to(&dir.join("not-here.gba"), &pack_path).unwrap_err();

    assert!(
        matches!(
            err,
            ImportRomError::Import {
                source: ImportError::ReadFailed { .. },
                ..
            }
        ),
        "expected a read failure, got: {err}"
    );
    assert!(!pack_path.exists());
    assert!(file_names(&dir.path).is_empty());
}

#[test]
fn the_outcome_renders_the_one_line_summary() {
    let outcome = ImportOutcome {
        pack_path: PathBuf::from("/data/pokeemerald.pack"),
        entry_count: 1234,
        pack_bytes: 5678,
    };
    assert_eq!(
        outcome.to_string(),
        "imported 1234 entries (5678 bytes) to /data/pokeemerald.pack"
    );
}

/// Every message here is one terminal row (module docs on
/// [`ImportRomError`]), and the paths they quote are user-supplied:
/// `--import-rom`'s argument and `$POKEEMERALD_PACK`. Control bytes in
/// either must be escaped before interpolation, which is what routing them
/// through `OneLinePath` (`crates/rom-import/src/one_line.rs`) buys.
#[test]
fn a_destination_carrying_control_bytes_stays_one_printable_row() {
    let rendered = ImportRomError::DestinationIsDirectory {
        pack_path: PathBuf::from("/data/\npokeemerald.pack\u{1b}]0;x\u{7}"),
    }
    .to_string();
    assert!(
        !rendered.chars().any(char::is_control),
        "the diagnosis must stay one printable row: {rendered:?}"
    );

    let summary = ImportOutcome {
        pack_path: PathBuf::from("/data/\npokeemerald.pack"),
        entry_count: 1,
        pack_bytes: 2,
    }
    .to_string();
    assert!(
        !summary.chars().any(char::is_control),
        "the summary must stay one printable row: {summary:?}"
    );
}

/// Every path-bearing [`ImportRomError`] variant escapes control bytes the
/// same way as [`a_destination_carrying_control_bytes_stays_one_printable_row`]
/// above, not just [`ImportRomError::DestinationIsDirectory`]: a newline or
/// an ESC reaching any of them from `$POKEEMERALD_PACK` or the
/// `--import-rom` argument must not reach the terminal, and the escaped
/// spelling must still be legible rather than silently dropped.
#[test]
fn every_path_bearing_variant_escapes_control_bytes_and_stays_legible() {
    let hostile = PathBuf::from("one\ntwo\u{1b}[2Kthree");
    let escaped = r"one\ntwo\u{1b}[2Kthree";
    let cases = [
        ImportRomError::CreateDirFailed {
            path: hostile.clone(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        },
        ImportRomError::OpenDirFailed {
            path: hostile.clone(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        },
        ImportRomError::Import {
            source: ImportError::EmptyPack,
            temp_path: Some(hostile.clone()),
            temp_removed: false,
        },
        ImportRomError::DestinationNamesNoFile {
            pack_path: hostile.clone(),
        },
        ImportRomError::DestinationIsDirectory {
            pack_path: hostile.clone(),
        },
        ImportRomError::DestinationIsSource {
            rom_path: hostile.clone(),
        },
        ImportRomError::TempFileFailed {
            temp_path: hostile.clone(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            temp_removed: false,
        },
        ImportRomError::PublishFailed {
            temp_path: hostile.clone(),
            pack_path: PathBuf::from("/data/pokeemerald.pack"),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            temp_removed: true,
        },
        ImportRomError::PublishFailed {
            temp_path: PathBuf::from("/data/temp.tmp"),
            pack_path: hostile.clone(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            temp_removed: false,
        },
    ];
    for case in cases {
        let text = case.to_string();
        assert!(!text.contains('\n'), "{text:?}");
        assert!(!text.contains('\u{1b}'), "{text:?}");
        assert!(text.contains(escaped), "escaped path missing from {text:?}");
    }
}

/// A hostile destination appears exactly once, escaped, in
/// [`ImportRomError::OpenDirFailed`]'s rendering of
/// [`not_a_directory_error`].
#[test]
fn a_non_directory_open_error_names_the_destination_once() {
    let hostile = PathBuf::from("one\ntwo\u{1b}[2Kthree");
    let rendered = ImportRomError::OpenDirFailed {
        path: hostile,
        source: not_a_directory_error(),
    }
    .to_string();
    assert_eq!(
        rendered,
        r"could not open `one\ntwo\u{1b}[2Kthree`: not a directory"
    );
    assert!(
        !rendered.chars().any(char::is_control),
        "the diagnosis must stay one printable row: {rendered:?}"
    );
}

/// The non-Unix [`Dest::open`] itself keeps `dir` out of the failure it
/// reports, not just [`not_a_directory_error`] built independently.
#[cfg(not(unix))]
#[test]
fn the_non_unix_open_of_a_non_directory_does_not_name_it() {
    let dir = TempDir::new("open-non-directory");
    let path = dir.join("pokeemerald.pack");
    fs::write(&path, b"a file, not a directory").expect("a file to open");

    let source = Dest::open(&path).err().expect("a file is not a directory");

    assert_eq!(source.to_string(), not_a_directory_error().to_string());
    assert!(
        !source.to_string().contains("pokeemerald"),
        "the inner error must not carry the path: {source}"
    );
}

/// An ordinary path -- a Windows one included, whose separators are
/// backslashes and whose player might have quoted a space in it -- still
/// prints as the literal string a player can copy back into a shell:
/// `OneLinePath` does not double `\` or `"`.
#[test]
fn an_ordinary_windows_path_prints_the_literal_a_player_can_copy_back() {
    let windows = PathBuf::from(r#"C:\Users\me\my "roms"\pokeemerald.pack"#);
    let rendered = ImportRomError::DestinationIsDirectory { pack_path: windows }.to_string();
    assert!(
        rendered.contains(r#"C:\Users\me\my "roms"\pokeemerald.pack"#),
        "{rendered}"
    );
}

/// The line separators `U+2028`/`U+2029` and the `Bidi_Control`
/// characters are outside `char::is_control`, so the control-byte tests
/// above cannot see them, in the success summary or the errors.
#[test]
fn a_separator_or_bidi_control_in_a_path_is_escaped_too() {
    let hostile = PathBuf::from(
        "a\u{2028}b\u{2029}c\u{202e}d\u{202a}e\u{202b}f\u{202c}g\u{202d}h\u{2066}i\u{2067}j\u{2068}k\u{2069}l\u{200e}m\u{200f}n\u{61c}o",
    );
    let escaped = r"a\u{2028}b\u{2029}c\u{202e}d\u{202a}e\u{202b}f\u{202c}g\u{202d}h\u{2066}i\u{2067}j\u{2068}k\u{2069}l\u{200e}m\u{200f}n\u{61c}o";
    let rendered = ImportRomError::DestinationIsDirectory {
        pack_path: hostile.clone(),
    }
    .to_string();
    assert!(rendered.contains(escaped), "{rendered:?}");
    let summary = ImportOutcome {
        pack_path: hostile,
        entry_count: 1,
        pack_bytes: 2,
    }
    .to_string();
    assert!(summary.contains(escaped), "{summary:?}");
}

#[test]
fn the_temp_name_is_a_bounded_name_in_the_packs_own_directory() {
    let pack_path = Path::new("/data/pokeemerald-rs/pokeemerald.pack");
    assert_eq!(pack_directory(pack_path), Path::new("/data/pokeemerald-rs"));
    let name = pack_name(pack_path).expect("the path names a file");
    assert_eq!(name, "pokeemerald.pack");
    let dir = TempDir::new("temp-name-shape");
    let dest = Dest::open(&dir.path).expect("the directory opens");
    // A basename, never a path: it is resolved against the pinned
    // directory, and the rename that publishes it stays inside that one
    // directory, which is what makes it atomic.
    let temp = dest.temp_name();
    let temp = temp.to_str().expect("a UTF-8 name stays UTF-8");
    assert!(
        !temp.contains(std::path::MAIN_SEPARATOR),
        "temp name: {temp}"
    );
    assert!(
        temp.starts_with(&format!("{}.", super::TEMP_PREFIX)),
        "temp name: {temp}"
    );
    assert_eq!(
        Path::new(temp).extension().expect("a temp extension"),
        "tmp",
        "temp name: {temp}"
    );
    // The destination's name is not in it, so no valid basename can push
    // this past a filesystem's 255-byte limit for one component.
    assert!(temp.len() <= 96, "temp name: {temp} ({} bytes)", temp.len());
}

#[test]
fn no_two_temp_names_are_the_same() {
    // The temporary file is created exclusively, so a repeated name is a
    // refused import. It also has to be a name nobody watching the process
    // can pre-create: the process id is on its own public and reusable,
    // which is why it is not the whole name.
    let dir = TempDir::new("temp-name-uniqueness");
    let dest = Dest::open(&dir.path).expect("the directory opens");
    let first = dest.temp_name();
    let second = dest.temp_name();

    assert_ne!(first, second);
    let predictable = format!("{}.{}.tmp", super::TEMP_PREFIX, std::process::id());
    for candidate in [&first, &second] {
        assert_ne!(
            candidate.as_os_str(),
            OsStr::new(&predictable),
            "the prefix and the process id must not spell the whole name"
        );
    }
}

#[test]
fn no_two_temp_names_are_the_same_across_separate_dest_instances() {
    // Every real import opens its own fresh `Dest` and calls `temp_name`
    // exactly once (`import_to_with`), so `Dest`'s own counter alone is
    // always its instance's first value in production: it cannot
    // disambiguate two *separate* `Dest`s that land on the same clock
    // reading (two racing import attempts, or -- as here -- two opened in
    // immediate succession). The per-call random salt is what still must
    // keep them apart.
    let dir = TempDir::new("temp-name-uniqueness-cross-dest");
    let first_dest = Dest::open(&dir.path).expect("the directory opens");
    let second_dest = Dest::open(&dir.path).expect("the directory opens");

    let first = first_dest.temp_name();
    let second = second_dest.temp_name();

    assert_ne!(
        first, second,
        "two freshly opened Dest instances must not hand out the same temp name"
    );
}

#[cfg(unix)]
#[test]
fn a_long_but_valid_pack_name_still_imports() {
    // 240 bytes is a legal component wherever the limit is the usual 255,
    // and the temporary file has to fit beside it: a temporary name built
    // out of the destination's would not, and the destination could never
    // be imported to. Unix-only because it is the *component* limit under
    // test, and Windows caps the whole path first.
    let dir = TempDir::new("long-name");
    let name = "p".repeat(240);
    let pack_path = dir.join(&name);

    let source = SourceRom::new("long-name-src");
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    })
    .expect("the import succeeds");

    assert_eq!(outcome.pack_path(), pack_path);
    assert_eq!(fs::read(&pack_path).unwrap(), b"pack bytes");
    // The pack is the only file left: the temporary one fit and is gone.
    assert_eq!(file_names(&dir.path), [name]);
}

#[test]
fn a_taken_temp_name_is_refused_and_its_file_is_left_alone() {
    // A name already taken is a file this run did not create: a leftover,
    // or a link somebody planted in a writable pack directory. Exclusive
    // creation refuses it having created nothing, so there is never a
    // cleanup that could remove it.
    let dir = TempDir::new("taken-name");
    fs::write(dir.join("taken"), b"not the importer's").expect("the squatter writes");
    let dest = Dest::open(&dir.path).expect("the directory opens");

    let err = dest.create_new(OsStr::new("taken")).unwrap_err();

    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "{err}");
    assert_eq!(
        fs::read(dir.join("taken")).expect("the squatter survives"),
        b"not the importer's"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_at_the_temp_name_is_refused_and_its_target_survives() {
    // The one attack a fresh, unpredictable name still has to answer:
    // whatever sits at that name, the create must not write through it.
    let dir = TempDir::new("planted-link");
    let victim = dir.join("save.sav");
    fs::write(&victim, b"the player's save").expect("the victim writes");
    std::os::unix::fs::symlink(&victim, dir.join("planted")).expect("the link is planted");
    let dest = Dest::open(&dir.path).expect("the directory opens");

    let err = dest.create_new(OsStr::new("planted")).unwrap_err();

    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "{err}");
    assert_eq!(
        fs::read(&victim).expect("the victim survives"),
        b"the player's save"
    );
}

#[cfg(unix)]
#[test]
fn a_redirected_directory_component_cannot_move_the_published_pack() {
    // The race the pinned handle closes: `$POKEEMERALD_PACK` runs through
    // a component another account controls, and that account redirects it
    // while the pack is being built. Everything after the open resolves
    // names against the descriptor, so the redirect moves nothing.
    let dir = TempDir::new("pinned-dir");
    let checked = dir.join("checked");
    let elsewhere = dir.join("elsewhere");
    fs::create_dir_all(&checked).expect("the checked directory");
    fs::create_dir_all(&elsewhere).expect("the other directory");
    let component = dir.join("component");
    std::os::unix::fs::symlink(&checked, &component).expect("the component links");

    let pack_path = component.join("pokeemerald.pack");
    let source = SourceRom::new("pinned-dir-src");
    let outcome = import_to_with(source.path(), &pack_path, |_rom, _path| {
        fs::remove_file(&component).expect("the component is removed");
        std::os::unix::fs::symlink(&elsewhere, &component).expect("the component is redirected");
        Ok(fake_pack(b"pack bytes"))
    })
    .expect("the import succeeds");

    assert_eq!(outcome.pack_path(), pack_path);
    assert_eq!(
        fs::read(checked.join("pokeemerald.pack")).expect("the pack is where it was checked"),
        b"pack bytes"
    );
    assert!(
        file_names(&elsewhere).is_empty(),
        "the redirected component must have received nothing"
    );
}

#[cfg(unix)]
#[test]
fn the_rom_is_recognized_through_the_pinned_directory() {
    // The refusal is a device and inode comparison against the pinned
    // directory, so a hard link to the ROM under another name is still the
    // ROM.
    let dir = TempDir::new("pinned-identity");
    let rom_path = write_fixture_rom(&dir);
    fs::hard_link(&rom_path, dir.join("alias.gba")).expect("the link is made");
    let dest = Dest::open(&dir.path).expect("the directory opens");
    let rom = fs::File::open(&rom_path).expect("the ROM opens");

    assert!(dest.is_same_file_as(OsStr::new("fixture.gba"), &rom, &rom_path));
    assert!(dest.is_same_file_as(OsStr::new("alias.gba"), &rom, &rom_path));
    assert!(!dest.is_same_file_as(OsStr::new("pokeemerald.pack"), &rom, &rom_path));
}

#[cfg(unix)]
#[test]
fn a_redirected_source_component_cannot_change_what_is_imported() {
    // The mirror of the destination race, on the file the player named.
    // The identity guard clears the source against the destination, and an
    // account owning a component of the source's path then redirects it at
    // the destination. Only the pinned handle keeps the two answers
    // together: what the guard cleared is what the importer reads, so the
    // pack can never be built from the file it is about to replace.
    use std::io::Read as _;

    let roms = TempDir::new("pinned-source");
    let named = roms.join("emerald.gba");
    fs::write(&named, b"the ROM the player named").expect("the ROM writes");
    let swapped = roms.join("swapped.gba");
    fs::write(&swapped, b"the file swapped in mid-import").expect("the swap writes");
    let component = roms.join("link.gba");
    std::os::unix::fs::symlink(&named, &component).expect("the source links");

    let dir = TempDir::new("pinned-source-dest");
    let pack_path = dir.join("pokeemerald.pack");

    let outcome = import_to_with(&component, &pack_path, |rom, _path| {
        fs::remove_file(&component).expect("the source is removed");
        std::os::unix::fs::symlink(&swapped, &component).expect("the source is redirected");

        let mut handle = rom;
        let mut bytes = Vec::new();
        handle
            .read_to_end(&mut bytes)
            .expect("the pinned ROM reads");
        assert_eq!(
            bytes, b"the ROM the player named",
            "the import must read the handle it checked, not the path again"
        );
        Ok(fake_pack(b"pack bytes"))
    })
    .expect("the import succeeds");

    assert_eq!(outcome.pack_path(), pack_path);
    assert_eq!(fs::read(&pack_path).unwrap(), b"pack bytes");
}

#[test]
fn a_bare_pack_name_lands_in_the_current_directory() {
    let pack_path = Path::new("pokeemerald.pack");
    assert_eq!(pack_directory(pack_path), Path::new("."));
    assert_eq!(pack_name(pack_path), Some(OsStr::new("pokeemerald.pack")));
}

#[test]
fn a_destination_naming_no_file_is_refused_before_anything_is_written() {
    // A `$POKEEMERALD_PACK` ending in `..` has no final component. The old
    // behaviour substituted `pokeemerald.pack` and published it while the
    // outcome reported the original path; now it is refused outright.
    let dir = TempDir::new("no-file-name");
    let pack_path = dir.join("..");

    let source = SourceRom::new("no-file-name-src");
    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    })
    .unwrap_err();

    assert!(
        matches!(err, ImportRomError::DestinationNamesNoFile { .. }),
        "expected a no-file-name refusal, got: {err}"
    );
    assert!(file_names(&dir.path).is_empty());
}

#[test]
fn a_destination_spelled_as_a_directory_names_no_pack_file() {
    // A trailing separator is not a component, so `file_name` hands back
    // the name in front of it — a name the path itself never asks for. `/`
    // is a separator on every supported platform.
    for spelled in [
        "/data/pokeemerald.pack/",
        "/data/pokeemerald.pack/.",
        "/data/pokeemerald.pack/./",
        "/data/pokeemerald.pack//",
    ] {
        assert_eq!(pack_name(Path::new(spelled)), None, "{spelled}");
    }
    // A dot *inside* the final component is part of the name, not a
    // directory spelling.
    assert_eq!(
        pack_name(Path::new("/data/pokeemerald.pack.")),
        Some(OsStr::new("pokeemerald.pack."))
    );
    assert_eq!(
        pack_name(Path::new("/data/pokeemerald.pack")),
        Some(OsStr::new("pokeemerald.pack"))
    );
}

#[test]
fn a_destination_spelled_as_a_directory_is_refused_with_that_name_intact() {
    // The harm is two-sided: the loader re-reads `$POKEEMERALD_PACK` with
    // the trailing separator still on it and cannot open a regular file
    // through one, so a "successful" import would be unreadable — and
    // publishing would have replaced whatever already held the name.
    let dir = TempDir::new("directory-spelling");
    let occupied = dir.join("pokeemerald.pack");
    fs::write(&occupied, b"the file that already held the name").expect("the occupant writes");
    let mut spelled = occupied.clone().into_os_string();
    spelled.push(std::path::MAIN_SEPARATOR_STR);

    let source = SourceRom::new("directory-spelling-src");
    let err = import_to_with(source.path(), Path::new(&spelled), |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    })
    .unwrap_err();

    assert!(
        matches!(err, ImportRomError::DestinationNamesNoFile { .. }),
        "expected a no-file-name refusal, got: {err}"
    );
    assert_eq!(
        fs::read(&occupied).expect("the occupant survives"),
        b"the file that already held the name"
    );
    assert_eq!(file_names(&dir.path), ["pokeemerald.pack"]);
}

#[test]
fn an_existing_directory_at_the_destination_is_refused_before_the_rom_is_read() {
    // No trailing separator -- an ordinary file name -- but a directory
    // already occupies it. See `Dest::name_is_directory` for why this is
    // refused before the import runs rather than at the publishing rename.
    let dir = TempDir::new("existing-directory");
    let pack_path = dir.join("pokeemerald.pack");
    fs::create_dir(&pack_path).expect("the occupying directory is created");

    let ran = AtomicU32::new(0);
    let source = SourceRom::new("existing-directory-src");
    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        ran.fetch_add(1, Ordering::Relaxed);
        Ok(fake_pack(b"pack bytes"))
    })
    .unwrap_err();

    assert!(
        matches!(err, ImportRomError::DestinationIsDirectory { .. }),
        "expected an existing-directory refusal, got: {err}"
    );
    assert_eq!(ran.load(Ordering::Relaxed), 0, "the import must not run");
    assert!(pack_path.is_dir(), "the occupying directory survives");
    // No temporary file was left beside it.
    assert_eq!(file_names(&dir.path), ["pokeemerald.pack"]);
}

#[test]
fn a_refused_directory_destination_says_which_variable_to_change() {
    let rendered = ImportRomError::DestinationIsDirectory {
        pack_path: PathBuf::from("/data/pokeemerald.pack"),
    }
    .to_string();
    assert!(rendered.contains("/data/pokeemerald.pack"), "{rendered}");
    assert!(rendered.contains(pack_format::PACK_PATH_ENV), "{rendered}");
    assert!(!rendered.contains('\n'), "{rendered}");
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

#[test]
fn a_refused_no_file_destination_says_which_variable_to_change() {
    let rendered = ImportRomError::DestinationNamesNoFile {
        pack_path: PathBuf::from("/data/.."),
    }
    .to_string();
    assert!(rendered.contains("/data/.."), "{rendered}");
    assert!(rendered.contains(pack_format::PACK_PATH_ENV), "{rendered}");
    assert!(!rendered.contains('\n'), "{rendered}");
}

#[test]
fn a_pack_destination_pointing_at_the_rom_is_refused_with_the_rom_intact() {
    // `$POKEEMERALD_PACK` can name any path, including the file passed to
    // `--import-rom`. The temporary file never shares the ROM's name and it
    // is the *rename* that would drop the pack on the player's cartridge
    // image, so the importer's own same-file guard never sees this one.
    let dir = TempDir::new("pack-is-rom");
    let rom_path = write_fixture_rom(&dir);
    let before = fs::read(&rom_path).expect("the fixture reads back");

    let err = import_to_with(&rom_path, &rom_path, |_rom, _path| {
        Ok(fake_pack(b"pack bytes"))
    })
    .unwrap_err();

    assert!(
        matches!(err, ImportRomError::DestinationIsSource { .. }),
        "expected a same-file refusal, got: {err}"
    );
    assert_eq!(
        fs::read(&rom_path).expect("the ROM survives"),
        before,
        "the ROM must be byte-identical after a refused import"
    );
    // Nothing was written and no temporary file was left behind.
    assert_eq!(file_names(&dir.path), ["fixture.gba"]);
}

#[test]
fn a_refused_same_file_destination_says_which_variable_to_change() {
    let rendered = ImportRomError::DestinationIsSource {
        rom_path: PathBuf::from("/roms/emerald.gba"),
    }
    .to_string();
    assert!(rendered.contains("/roms/emerald.gba"), "{rendered}");
    assert!(rendered.contains(pack_format::PACK_PATH_ENV), "{rendered}");
    assert!(!rendered.contains('\n'), "{rendered}");
}

#[test]
fn a_missing_destination_says_which_variable_to_set() {
    let rendered = ImportRomError::NoDestination.to_string();
    assert!(rendered.contains(pack_format::PACK_PATH_ENV), "{rendered}");
}

#[test]
fn a_failed_publish_says_whether_the_finished_pack_is_still_on_disk() {
    // `Dest::discard` swallows its own failure so it cannot displace the
    // publish diagnosis, which means the temporary file can outlive the
    // error. It holds a *finished* pack, so the message must not claim it
    // was removed when it was not.
    let removed = ImportRomError::PublishFailed {
        temp_path: PathBuf::from("/data/.pokeemerald-rs-import.1.2.3.tmp"),
        pack_path: PathBuf::from("/data/pokeemerald.pack"),
        source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        temp_removed: true,
    }
    .to_string();
    assert!(removed.contains("was removed"), "{removed}");

    let kept = ImportRomError::PublishFailed {
        temp_path: PathBuf::from("/data/.pokeemerald-rs-import.1.2.3.tmp"),
        pack_path: PathBuf::from("/data/pokeemerald.pack"),
        source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        temp_removed: false,
    }
    .to_string();
    assert!(!kept.contains("was removed"), "{kept}");
    assert!(
        kept.contains("still at `/data/.pokeemerald-rs-import.1.2.3.tmp`"),
        "{kept}"
    );
    // Both spellings stay one line: the binary prints them on one row.
    for rendered in [&removed, &kept] {
        assert!(!rendered.contains('\n'), "{rendered}");
    }
}

/// [`ImportRomError::TempFileFailed`]'s own `temp_removed`, mirroring
/// [`a_failed_publish_says_whether_the_finished_pack_is_still_on_disk`] for
/// [`ImportRomError::PublishFailed`]: the write-failure cleanup call
/// (`import_to_with`, the branch after `file.write_all`/`flush`/
/// `sync_all`) has no injectable seam to make the write itself fail
/// end-to-end (unlike the temp-file *creation* seam
/// `a_taken_temp_name_is_refused_and_its_file_is_left_alone` exercises, or
/// the importer's own return path
/// `a_failed_import_names_the_partial_file_its_own_cleanup_left_behind`
/// exercises), so this pins the surfaced message directly against both
/// values of the field the fix threads through instead of discarding.
#[test]
fn a_failed_temp_write_says_whether_the_partial_file_was_removed() {
    let removed = ImportRomError::TempFileFailed {
        temp_path: PathBuf::from("/data/.pokeemerald-rs-import.1.2.3.tmp"),
        source: std::io::Error::from(std::io::ErrorKind::StorageFull),
        temp_removed: true,
    }
    .to_string();
    assert!(!removed.contains("could not be removed"), "{removed}");

    let kept = ImportRomError::TempFileFailed {
        temp_path: PathBuf::from("/data/.pokeemerald-rs-import.1.2.3.tmp"),
        source: std::io::Error::from(std::io::ErrorKind::StorageFull),
        temp_removed: false,
    }
    .to_string();
    assert!(
        kept.contains("could not be removed"),
        "a failed cleanup must say so: {kept}"
    );
    for rendered in [&removed, &kept] {
        assert!(!rendered.contains('\n'), "{rendered}");
    }
}

#[cfg(unix)]
#[test]
fn discard_reports_a_name_that_is_gone_and_one_it_could_not_remove() {
    let dir = TempDir::new("discard-reports");
    std::fs::write(dir.join("present"), b"x").expect("the file writes");
    let dest = Dest::open(&dir.path).expect("the directory opens");

    assert!(
        dest.discard(OsStr::new("present")),
        "a removed name is gone"
    );
    // Already absent counts as gone: the caller asks whether a file is left
    // behind, not whether this call did the removing.
    assert!(dest.discard(OsStr::new("never-existed")), "absent is gone");
    // A directory is not something `unlink` will remove, so this is the
    // "could not remove it" answer without needing to break permissions.
    std::fs::create_dir(dir.join("a-directory")).expect("the directory writes");
    assert!(
        !dest.discard(OsStr::new("a-directory")),
        "a name that survives must report as still there"
    );
}
