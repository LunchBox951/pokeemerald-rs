//! The real wiring: `import_to` against a synthetic cartridge image.

use rom_import::ImportError;

use super::super::{import_to, ImportRomError};
use super::support::{file_names, write_fixture_rom, TempDir};

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
