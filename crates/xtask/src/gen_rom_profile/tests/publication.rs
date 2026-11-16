//! Regression tests for where the generated profile is written. An output
//! that names the ROM is refused wherever the destination guard can tell, even
//! after a missing parent directory is created; a hard link the guard cannot
//! see off Unix costs only the alias, because publishing renames over it. A
//! publish leaves nothing but the output beside it unless its own cleanup
//! fails, which it then reports.

use std::path::Path;

use rom_import::fixture::RomFixture;

use super::super::error::GenRomProfileError;
use super::scratch;

/// Writes a ROM-shaped image to `path` and returns its bytes. The header is
/// valid but the build is not the supported one, so a run either stops at the
/// destination guard or, once past it, fails on the build check.
fn rom_shaped_file(path: &Path) -> Vec<u8> {
    let bytes = RomFixture::new().emerald_header().finish();
    std::fs::write(path, &bytes).expect("the stand-in ROM writes");
    bytes
}

/// One path given to both `--rom` and `--out` would cost a developer their
/// cartridge dump. The whole ROM is already in memory when the module is
/// written, so nothing downstream would notice.
#[test]
fn an_output_naming_the_rom_itself_is_refused_and_the_rom_survives() {
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

/// A symlink is a different path *string* for the same file: comparing the
/// two spellings would miss it, resolving them does not.
#[cfg(unix)]
#[test]
fn an_output_symlinked_to_the_rom_is_refused_and_the_rom_survives() {
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

/// A hard link is two directory entries for one inode. `canonicalize` reports
/// two distinct names for it, so only a device/inode comparison sees it.
#[cfg(unix)]
#[test]
fn an_output_hard_linked_to_the_rom_is_refused_and_the_rom_survives() {
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

/// A distinct output path gets past the destination guard and fails later,
/// on the ROM not being the supported build.
#[test]
fn an_ordinary_output_is_not_mistaken_for_the_rom() {
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

/// Publishing over a hard link to the ROM replaces the alias's directory
/// entry instead of truncating the inode both names share, so the ROM keeps
/// its bytes.
///
/// This is the hole the destination guard leaves off Unix: `std` exposes no
/// file identity on Windows, so `overwrites_rom` compares canonical paths
/// there, and two hard links canonicalize to two distinct names. The
/// property belongs to `publish` and holds on every platform; this test runs
/// on Unix only.
#[cfg(unix)]
#[test]
fn writing_through_a_hard_link_retires_the_alias_and_not_the_file() {
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

/// A publish whose cleanup also fails reports the write's own error and names
/// the temporary it left behind; `temp_sibling` draws a fresh name from the
/// process id, the clock and a random salt, so no later run revisits the path
/// to clean it up.
///
/// The injected write swaps the temporary file for a non-empty directory, so
/// `remove_file` fails for every user, root included, which a permission-based
/// seam would not. Mirrors `rom_import`'s
/// `a_failed_cleanup_names_the_partial_file_it_left_behind` and
/// `crates/xtask/src/extract/mod.rs`'s
/// `a_failed_staging_cleanup_names_the_artifact_it_left_behind`.
#[test]
#[cfg(unix)]
fn a_failed_cleanup_names_the_partial_profile_it_left_behind() {
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

/// A directory at the output path makes the rename fail after the temporary
/// is written; the write and the rename share one cleanup.
#[test]
fn a_failed_publish_leaves_no_temporary_beside_the_output() {
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

/// On Unix, `absent/..` resolves to nothing while `absent/` does not exist, so
/// the pre-check in `run` lets the path through. Creating the directory is what
/// makes the output name the ROM, and the publishing rename would land on it;
/// `write_module` asks again after creating it.
#[test]
fn an_output_reaching_the_rom_through_a_missing_directory_is_refused() {
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
    let stray: Vec<_> = std::fs::read_dir(&dir)
        .expect("listing")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .filter(|name| name != "emerald.gba" && name != "absent")
        .collect();
    assert!(
        stray.is_empty(),
        "a refused write may leave the directory it created to ask the question, \
         but no module and no temporary beside the ROM: {stray:?}"
    );

    let _ = std::fs::remove_dir(dir.join("absent"));
    let _ = std::fs::remove_file(&rom);
    let _ = std::fs::remove_dir(&dir);
}

/// Marks the re-run of
/// [`publish_under_an_owner_stripping_umask_keeps_the_module_owner_readable`]
/// that executes the test body, so only that child changes its own umask.
#[cfg(unix)]
const PUBLISH_UMASK_CHILD: &str = "POKEEMERALD_GEN_ROM_PROFILE_UMASK_CHILD";

/// A publish under a umask that strips the owner bits (`0444`) over a
/// readable module leaves a module its owner can still read. The temporary
/// file is created `0o666 & !umask`, which is `0o222`, and the rename carries
/// that mode onto the published name. The mode bits are asserted because a
/// root runner reads a `0o222` file regardless.
#[cfg(unix)]
#[test]
fn publish_under_an_owner_stripping_umask_keeps_the_module_owner_readable() {
    use std::os::unix::fs::PermissionsExt as _;

    let name = "gen_rom_profile::tests::publication::\
                publish_under_an_owner_stripping_umask_keeps_the_module_owner_readable";
    if std::env::var_os(PUBLISH_UMASK_CHILD).is_none() {
        // The umask is process-wide, so the 0o444 one is set only in a re-run
        // of this test binary that runs this test alone.
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", "umask 0444 && exec \"$0\" \"$@\""])
            .arg(std::env::current_exe().expect("test binary"))
            .args(["--exact", name, "--test-threads=1"])
            .env(PUBLISH_UMASK_CHILD, "1")
            .output()
            .expect("the child runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "umask 0444 child failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let dir = scratch("umask-0444-publish");
    let out = dir.join("bpee_rev0.rs");
    std::fs::write(&out, "pub const OLD: u32 = 0;\n").expect("existing module");
    std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o600)).expect("chmod");

    super::super::publish(&out, "pub const GENERATED: u32 = 0;\n").expect("the publish succeeds");

    let mode = std::fs::metadata(&out)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o7777;
    let text = std::fs::read(&out);
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_dir(&dir);
    assert_eq!(
        mode, 0o622,
        "0666 & !0444 leaves 0222 and only the owner's read is restored, so the \
         group and other bits still honour the umask: {mode:o}"
    );
    assert_eq!(
        text.expect("module reads back"),
        b"pub const GENERATED: u32 = 0;\n"
    );
}
