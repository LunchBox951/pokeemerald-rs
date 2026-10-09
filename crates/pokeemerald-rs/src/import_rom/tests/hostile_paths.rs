//! Destinations and sources that are symlinks, swapped, or otherwise hostile.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(unix)]
use super::super::dest::Dest;
use super::super::{import_to_with, import_to_with_hooks, pack_name, ImportRomError};
use super::support::{fake_pack, file_names, write_fixture_rom, SourceRom, TempDir};

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
fn a_just_created_destination_swapped_before_acquisition_still_receives_the_pack() {
    use std::os::unix::fs::MetadataExt as _;

    // The window between creating the destination and acquiring it: the
    // new directory is renamed away and a different one takes its name.
    // Acquisition goes through the creation walk's descriptor, so the pack
    // lands in the directory this run made.
    let root = TempDir::new("swap-before-acquire");
    let new = root.join("new");
    let original = root.join("original");
    let pack_path = new.join("pokeemerald.pack");
    let source = SourceRom::new("swap-before-acquire-src");
    let mut original_ino = None;

    let outcome = import_to_with_hooks(
        source.path(),
        &pack_path,
        |_rom, _path| Ok(fake_pack(b"pack bytes")),
        |_dir| {
            assert!(file_names(&new).is_empty(), "the new directory is empty");
            original_ino = Some(fs::metadata(&new).expect("new exists").ino());
            fs::rename(&new, &original).expect("the directory is moved");
            fs::create_dir(&new).expect("a different directory takes the name");
        },
    )
    .expect("the import succeeds");

    assert_eq!(outcome.pack_path(), pack_path);
    assert_eq!(
        fs::metadata(&original).expect("original").ino(),
        original_ino.unwrap()
    );
    assert_ne!(
        fs::metadata(&new).expect("replacement").ino(),
        original_ino.unwrap()
    );
    assert_eq!(
        fs::read(original.join("pokeemerald.pack")).expect("the pack is in the original"),
        b"pack bytes"
    );
    assert_eq!(file_names(&original), ["pokeemerald.pack"]);
    assert!(
        file_names(&new).is_empty(),
        "the replacement received nothing"
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
