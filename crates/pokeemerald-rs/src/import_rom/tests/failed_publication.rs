//! A failed import or publication leaves the destination as it found it.

#[cfg(unix)]
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use rom_import::ImportError;

#[cfg(unix)]
use super::super::dest::Dest;
use super::super::{import_to_with, ImportRomError};
use super::support::{file_names, find_temp_path, SourceRom, TempDir};

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

#[cfg(unix)]
#[test]
fn a_failed_import_does_not_remove_a_directory_it_did_not_create() {
    // `undo_created_directories` looks `created`/`new` up in the pinned
    // parent and compares its device and inode against what
    // `create_directories` captured, rather than trusting the path
    // spelling again. Between the pin and the failure, another account
    // renames the level this run made out from under the name and puts its
    // own there -- the level it is about to write into. That replacement's
    // identity does not match the record, so it is left untouched;
    // removing it anyway would be the harm `create_directories` refuses
    // `create_dir_all` over, arriving by the other door.
    let dir = TempDir::new("undo-swapped");
    let created = dir.join("new");
    let moved = dir.join("moved");
    let pack_path = created.join("pokeemerald.pack");

    let source = SourceRom::new("undo-swapped-src");
    let err = import_to_with(source.path(), &pack_path, |_rom, _path| {
        fs::rename(&created, &moved).expect("the created level is renamed away");
        fs::create_dir(&created).expect("somebody else's level takes the name");
        Err(ImportError::EmptyPack)
    })
    .unwrap_err();

    assert!(matches!(err, ImportRomError::Import { .. }));
    assert!(
        created.is_dir(),
        "the cleanup took back a directory this run never created"
    );
    // The directory this run actually made is left standing too -- harmless
    // litter, not chased down under its new name.
    assert!(moved.is_dir());
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
