//! Destination and cleanup regression tests for the generated profile.

use std::path::Path;

use rom_import::fixture::RomFixture;

use super::super::error::GenRomProfileError;
use super::scratch;

/// A ROM-shaped image at `path`, structurally valid but not the supported
/// build, so a run reaches the destination guard and stops there.
fn rom_shaped_file(path: &Path) -> Vec<u8> {
    let bytes = RomFixture::new().emerald_header().finish();
    std::fs::write(path, &bytes).expect("the stand-in ROM writes");
    bytes
}

#[test]
fn an_output_naming_the_rom_itself_is_refused_and_the_rom_survives() {
    // The typo that costs a developer their cartridge dump: one path given
    // to both `--rom` and `--out`. The whole ROM is in memory by the time
    // the module is written, so nothing downstream would notice.
    let dir = scratch("out-is-rom");
    let rom = dir.join("emerald.gba");
    let before = rom_shaped_file(&rom);

    let err = crate::gen_rom_profile::run(&crate::gen_rom_profile::Options {
        rom: rom.clone(),
        out: Some(rom.clone()),
        map: None,
    })
    .expect_err("an output naming the ROM must be refused");

    assert!(
        matches!(err, GenRomProfileError::OutputIsRom { .. }),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read(&rom).expect("the ROM survives"),
        before,
        "the ROM must be byte-identical after a refused run"
    );

    let _ = std::fs::remove_file(&rom);
    let _ = std::fs::remove_dir(&dir);
}

#[cfg(unix)]
#[test]
fn an_output_symlinked_to_the_rom_is_refused_and_the_rom_survives() {
    // A different path *string*, the same file. Comparing the two spellings
    // would miss this; resolving them does not.
    let dir = scratch("out-links-to-rom");
    let rom = dir.join("emerald.gba");
    let before = rom_shaped_file(&rom);
    let alias = dir.join("profile.rs");
    std::os::unix::fs::symlink(&rom, &alias).expect("the alias links");

    let err = crate::gen_rom_profile::run(&crate::gen_rom_profile::Options {
        rom: rom.clone(),
        out: Some(alias.clone()),
        map: None,
    })
    .expect_err("a symlinked output naming the ROM must be refused");

    assert!(
        matches!(err, GenRomProfileError::OutputIsRom { .. }),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read(&rom).expect("the ROM survives"),
        before,
        "the ROM must be byte-identical after a refused run"
    );

    let _ = std::fs::remove_file(&alias);
    let _ = std::fs::remove_file(&rom);
    let _ = std::fs::remove_dir(&dir);
}

#[cfg(unix)]
#[test]
fn an_output_hard_linked_to_the_rom_is_refused_and_the_rom_survives() {
    // Two directory entries, one inode. `canonicalize` reports two distinct
    // names here, so only a device/inode comparison sees it.
    let dir = scratch("out-hardlinks-rom");
    let rom = dir.join("emerald.gba");
    let before = rom_shaped_file(&rom);
    let alias = dir.join("profile.rs");
    std::fs::hard_link(&rom, &alias).expect("the alias links");

    let err = crate::gen_rom_profile::run(&crate::gen_rom_profile::Options {
        rom: rom.clone(),
        out: Some(alias.clone()),
        map: None,
    })
    .expect_err("a hard-linked output naming the ROM must be refused");

    assert!(
        matches!(err, GenRomProfileError::OutputIsRom { .. }),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read(&rom).expect("the ROM survives"),
        before,
        "the ROM must be byte-identical after a refused run"
    );

    let _ = std::fs::remove_file(&alias);
    let _ = std::fs::remove_file(&rom);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn an_ordinary_output_is_not_mistaken_for_the_rom() {
    // The guard must not refuse the normal case: a distinct output path
    // gets past it and fails later, on the ROM not being the supported
    // build.
    let dir = scratch("out-is-not-rom");
    let rom = dir.join("emerald.gba");
    rom_shaped_file(&rom);

    let err = crate::gen_rom_profile::run(&crate::gen_rom_profile::Options {
        rom: rom.clone(),
        out: Some(dir.join("bpee_rev0.rs")),
        map: None,
    })
    .expect_err("a stand-in ROM is not the supported build");

    assert!(
        matches!(err, GenRomProfileError::RomUnusable { .. }),
        "{err:?}"
    );

    let _ = std::fs::remove_file(&rom);
    let _ = std::fs::remove_dir(&dir);
}

#[cfg(unix)]
#[test]
fn writing_through_a_hard_link_retires_the_alias_and_not_the_file() {
    // The residual the destination guard cannot close off Unix: `std`
    // exposes no file identity on Windows, so `overwrites_rom` there
    // compares canonical paths, and two hard links canonicalize to two
    // distinct names. Publishing by rename is what makes that survivable --
    // it replaces the alias's directory entry rather than truncating the
    // inode both names share.
    //
    // Exercised on Unix because that is where a hard link can be made in a
    // test; the property under test is `publish`'s, and it is the same on
    // every platform.
    let dir = scratch("write-through-hard-link");
    let rom = dir.join("emerald.gba");
    let before = rom_shaped_file(&rom);
    let alias = dir.join("bpee_rev0.rs");
    std::fs::hard_link(&rom, &alias).expect("the alias links");

    crate::gen_rom_profile::publish(&alias, "pub const GENERATED: u32 = 0;\n")
        .expect("the module writes");

    assert_eq!(
        std::fs::read(&rom).expect("the ROM survives"),
        before,
        "the ROM must be untouched under its own name"
    );
    assert_eq!(
        std::fs::read_to_string(&alias).expect("the module is there"),
        "pub const GENERATED: u32 = 0;\n"
    );

    let _ = std::fs::remove_file(&alias);
    let _ = std::fs::remove_file(&rom);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
#[cfg(unix)]
fn a_failed_cleanup_names_the_partial_profile_it_left_behind() {
    // `remove_after`'s own removal must also be able to fail -- silently
    // dropping that failure would leave a `.profile.*.tmp` directory behind
    // permanently: `temp_sibling`'s name is never chosen twice, so no later
    // run ever revisits this exact path to clean it up. The injected write
    // swaps the temporary file for a non-empty directory before returning
    // its error,
    // so `remove_after`'s `remove_file` fails deterministically
    // (`remove_file` refuses any directory, empty or not, regardless of the
    // runner's privileges -- unlike a permission-based seam, which root
    // bypasses). Mirrors `rom_import`'s own
    // `a_failed_cleanup_names_the_partial_file_it_left_behind` and
    // `crates/xtask/src/extract/mod.rs`'s
    // `a_failed_staging_cleanup_names_the_artifact_it_left_behind`.
    let dir = scratch("cleanup-fails");
    let out = dir.join("bpee_rev0.rs");
    let mut swapped_temp = None;

    let err = crate::gen_rom_profile::publish_with(&out, |file, temp| {
        use std::io::Write as _;
        file.write_all(b"partial")?;
        drop(std::fs::remove_file(temp));
        std::fs::create_dir(temp).expect("the directory takes the freed name");
        std::fs::write(temp.join("occupant"), b"occupant").expect("the occupant writes");
        swapped_temp = Some(temp.to_path_buf());
        Err(std::io::Error::new(
            std::io::ErrorKind::StorageFull,
            "no space left on device",
        ))
    })
    .unwrap_err();
    let swapped_temp = swapped_temp.expect("the write ran");

    // The write's own error kind survives the additional cleanup failure,
    // and the message names what cleanup left behind.
    let crate::gen_rom_profile::GenRomProfileError::WriteFailed { reason, .. } = &err else {
        panic!("{err:?}");
    };
    assert!(
        reason.contains("no space left on device"),
        "the original failure must survive: {reason}"
    );
    assert!(
        reason.contains(&swapped_temp.display().to_string()),
        "a failed cleanup must name the artifact it left behind: {reason}"
    );
    assert!(
        swapped_temp.is_dir(),
        "cleanup should have failed, leaving the directory behind"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_publish_leaves_no_temporary_beside_the_output() {
    // A directory at the output path makes the rename fail after the
    // temporary is written; the write and rename share one cleanup.
    let dir = scratch("failed-publish-leaves-no-litter");
    let out = dir.join("bpee_rev0.rs");
    std::fs::create_dir_all(&out).expect("the colliding directory");
    let result = crate::gen_rom_profile::publish(&out, "pub const GENERATED: u32 = 0;\n");
    assert!(
        matches!(
            result,
            Err(crate::gen_rom_profile::GenRomProfileError::WriteFailed { .. })
        ),
        "{result:?}"
    );

    let stray: Vec<_> = std::fs::read_dir(&dir)
        .expect("listing")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .filter(|name| name != "bpee_rev0.rs")
        .collect();
    assert!(stray.is_empty(), "{stray:?}");

    let _ = std::fs::remove_dir(&out);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn a_successful_write_leaves_only_the_output() {
    let dir = scratch("write-leaves-no-litter");
    let out = dir.join("bpee_rev0.rs");
    crate::gen_rom_profile::publish(&out, "pub const GENERATED: u32 = 0;\n")
        .expect("the module writes");

    let stray: Vec<_> = std::fs::read_dir(&dir)
        .expect("listing")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .filter(|name| name != "bpee_rev0.rs")
        .collect();
    assert!(stray.is_empty(), "{stray:?}");

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn an_output_reaching_the_rom_through_a_missing_directory_is_refused() {
    // The alias the pre-check cannot answer: `absent/` does not exist when
    // `run` asks, so `absent/..` resolves to nothing and the guard lets the
    // path through. Creating that directory is what makes the output name
    // the ROM, and the publishing rename is what would land on it.
    let dir = scratch("out-reaches-rom-through-missing-dir");
    let rom = dir.join("emerald.gba");
    let before = rom_shaped_file(&rom);
    let out = dir.join("absent").join("..").join("emerald.gba");
    // Windows collapses `..` lexically before touching the disk, so its
    // pre-check already answers; the unanswerable case is a unix one.
    #[cfg(unix)]
    assert!(
        !rom_import::overwrites_rom(&rom, &out),
        "the scenario under test is the one the pre-check cannot resolve"
    );

    let err = crate::gen_rom_profile::write_module(&rom, &out, "pub const GENERATED: u32 = 0;\n")
        .expect_err("an output that resolves to the ROM must be refused");

    assert!(
        matches!(err, GenRomProfileError::OutputIsRom { .. }),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read(&rom).expect("the ROM survives"),
        before,
        "the ROM must be byte-identical after a refused write"
    );
    // The refused write leaves the directory it had to create to ask the
    // question, and nothing else: no module, no temporary beside the ROM.
    let stray: Vec<_> = std::fs::read_dir(&dir)
        .expect("listing")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .filter(|name| name != "emerald.gba" && name != "absent")
        .collect();
    assert!(stray.is_empty(), "{stray:?}");

    let _ = std::fs::remove_dir(dir.join("absent"));
    let _ = std::fs::remove_file(&rom);
    let _ = std::fs::remove_dir(&dir);
}
