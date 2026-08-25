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

use super::dest::Dest;
use super::{
    import_to, import_to_with, pack_directory, pack_name, temp_name, ImportOutcome, ImportRomError,
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
    let outcome = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom| {
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
fn a_failed_import_leaves_neither_a_pack_nor_a_partial_file() {
    let dir = TempDir::new("fail-closed");
    let pack_path = dir.join("pokeemerald.pack");
    // The temporary file is created before the importer runs, so a failed
    // import has one to take with it even though no bytes ever reached it.
    let err = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom| {
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

    let err = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom| {
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

    let err = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom| {
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

    let err = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom| {
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
fn the_temp_name_is_the_packs_own_name() {
    let pack_path = Path::new("/data/pokeemerald-rs/pokeemerald.pack");
    assert_eq!(pack_directory(pack_path), Path::new("/data/pokeemerald-rs"));
    let name = pack_name(pack_path).expect("the path names a file");
    assert_eq!(name, "pokeemerald.pack");
    // A basename, never a path: it is resolved against the pinned
    // directory, and the rename that publishes it stays inside that one
    // directory, which is what makes it atomic.
    let temp = temp_name(name);
    let temp = temp.to_str().expect("a UTF-8 name stays UTF-8");
    assert!(
        !temp.contains(std::path::MAIN_SEPARATOR),
        "temp name: {temp}"
    );
    assert!(temp.starts_with(".pokeemerald.pack."), "temp name: {temp}");
    assert_eq!(
        Path::new(temp).extension().expect("a temp extension"),
        "tmp",
        "temp name: {temp}"
    );
}

#[test]
fn no_two_temp_names_are_the_same() {
    // The temporary file is created exclusively, so a repeated name is a
    // refused import. It also has to be a name nobody watching the process
    // can pre-create: the process id is on its own public and reusable,
    // which is why it is not the whole name.
    let name = pack_name(Path::new("/data/pokeemerald-rs/pokeemerald.pack"))
        .expect("the path names a file");
    let first = temp_name(name);
    let second = temp_name(name);

    assert_ne!(first, second);
    let predictable = format!(".pokeemerald.pack.{}.tmp", std::process::id());
    for candidate in [&first, &second] {
        assert_ne!(
            candidate.as_os_str(),
            OsStr::new(&predictable),
            "the pack name and the process id must not spell the whole name"
        );
    }
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
    let outcome = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom| {
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

    assert!(dest.is_same_file_as(OsStr::new("fixture.gba"), &rom_path));
    assert!(dest.is_same_file_as(OsStr::new("alias.gba"), &rom_path));
    assert!(!dest.is_same_file_as(OsStr::new("pokeemerald.pack"), &rom_path));
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

    let err = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom| {
        Ok(fake_pack(b"pack bytes"))
    })
    .unwrap_err();

    assert!(
        matches!(err, ImportRomError::DestinationNamesNoFile { .. }),
        "expected a no-file-name refusal, got: {err}"
    );
    assert!(file_names(&dir.path).is_empty());
}

#[cfg(unix)]
#[test]
fn a_non_utf8_pack_name_is_published_byte_for_byte() {
    // The requested basename survives as an `OsStr` end to end: the file
    // published is the one the player named, not a lossy re-spelling.
    use std::os::unix::ffi::OsStrExt as _;

    let dir = TempDir::new("non-utf8-name");
    let name = OsStr::from_bytes(b"pok\xe9mon.pack");
    let pack_path = dir.path.join(name);

    let outcome = import_to_with(Path::new("/roms/emerald.gba"), &pack_path, |_rom| {
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

    let err =
        import_to_with(&rom_path, &rom_path, |_rom| Ok(fake_pack(b"pack bytes"))).unwrap_err();

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
