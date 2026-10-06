//! How the importer names its temporary files and the pack beside them.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use super::super::dest::Dest;
#[cfg(unix)]
use super::super::import_to_with;
use super::super::{pack_directory, pack_name};
use super::support::TempDir;
#[cfg(unix)]
use super::support::{fake_pack, file_names, SourceRom};

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

#[test]
fn a_bare_pack_name_lands_in_the_current_directory() {
    let pack_path = Path::new("pokeemerald.pack");
    assert_eq!(pack_directory(pack_path), Path::new("."));
    assert_eq!(pack_name(pack_path), Some(OsStr::new("pokeemerald.pack")));
}
