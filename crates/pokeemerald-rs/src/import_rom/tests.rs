//! Unit tests for the `--import-rom` write path.
//!
//! No real ROM is involved anywhere. The importer is injected where the
//! test is about *publishing* the pack, and `rom_import::fixture` supplies
//! a synthetic cartridge image where the test is about the real wiring.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rom_import::fixture::RomFixture;
use rom_import::{ImportError, ImportReport};

use super::{import_to, import_to_with, pack_directory, temp_path, ImportOutcome, ImportRomError};

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
    let outcome = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom, out| {
        fs::write(out, b"pack bytes").expect("the fake importer writes");
        Ok(ImportReport::new(out.to_path_buf(), "fixture", 7, 10))
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
fn a_failed_import_leaves_neither_a_pack_nor_a_partial_file() {
    let dir = TempDir::new("fail-closed");
    let pack_path = dir.join("pokeemerald.pack");
    // A domain reader can fail after the pack has been serialized, so the
    // write path must survive the importer having already written bytes.
    let err = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom, out| {
        fs::write(out, b"half a pack").expect("the fake importer writes");
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(
        err,
        ImportRomError::Import(ImportError::EmptyPack)
    ));
    assert!(!pack_path.exists());
    assert!(file_names(&dir.path).is_empty());
}

#[test]
fn a_failed_import_removes_the_directory_it_created() {
    let dir = TempDir::new("undo-dir");
    let created = dir.join("pokeemerald-rs");
    let pack_path = created.join("pokeemerald.pack");

    let err = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom, _out| {
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(err, ImportRomError::Import(_)));
    // A failed import leaves no trace: not even an empty data directory
    // that would look like a half-installed game.
    assert!(!created.exists());
    assert!(file_names(&dir.path).is_empty());
}

#[test]
fn an_import_into_an_existing_directory_leaves_it_alone() {
    let dir = TempDir::new("keep-dir");
    let pack_path = dir.join("pokeemerald.pack");

    let err = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom, _out| {
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(err, ImportRomError::Import(_)));
    // The directory was already there, so the cleanup must not touch it.
    assert!(dir.path.is_dir());
}

#[test]
fn an_existing_pack_survives_a_failed_import() {
    let dir = TempDir::new("keep-old");
    let pack_path = dir.join("pokeemerald.pack");
    fs::write(&pack_path, b"the pack that already worked").expect("the old pack writes");

    let err = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom, out| {
        fs::write(out, b"half a pack").expect("the fake importer writes");
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(err, ImportRomError::Import(_)));
    assert_eq!(
        fs::read(&pack_path).unwrap(),
        b"the pack that already worked"
    );
    assert_eq!(file_names(&dir.path), ["pokeemerald.pack"]);
}

#[test]
fn the_import_error_renders_the_importers_own_message() {
    let err = ImportRomError::Import(ImportError::EmptyPack);
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
            ImportRomError::Import(ImportError::UnsupportedRevision { .. })
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
        matches!(err, ImportRomError::Import(ImportError::ReadFailed { .. })),
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

#[test]
fn the_temp_file_sits_beside_the_pack() {
    let pack_path = Path::new("/data/pokeemerald-rs/pokeemerald.pack");
    let dir = pack_directory(pack_path);
    assert_eq!(dir, Path::new("/data/pokeemerald-rs"));
    let temp = temp_path(pack_path, &dir);
    // Same directory, so the rename that publishes it is atomic.
    assert_eq!(temp.parent().unwrap(), dir);
    let name = temp.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with(".pokeemerald.pack."), "temp name: {name}");
    assert_eq!(temp.extension().unwrap(), "tmp", "temp name: {name}");
}

#[test]
fn a_bare_pack_name_lands_in_the_current_directory() {
    let pack_path = Path::new("pokeemerald.pack");
    assert_eq!(pack_directory(pack_path), Path::new("."));
    assert_eq!(
        temp_path(pack_path, &pack_directory(pack_path))
            .parent()
            .unwrap(),
        Path::new(".")
    );
}

#[test]
fn a_pack_destination_pointing_at_the_rom_is_refused_with_the_rom_intact() {
    // `$POKEEMERALD_PACK` can name any path, including the file passed to
    // `--import-rom`. The temporary file is written fine and it is the
    // *rename* that would drop the pack on the player's cartridge image,
    // so the importer's own same-file guard never sees this one.
    let dir = TempDir::new("pack-is-rom");
    let rom_path = write_fixture_rom(&dir);
    let before = fs::read(&rom_path).expect("the fixture reads back");

    let err = import_to_with(&rom_path, &rom_path, |_rom, out| {
        fs::write(out, b"pack bytes").expect("the fake importer writes");
        Ok(ImportReport::new(out.to_path_buf(), "fixture", 7, 10))
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
