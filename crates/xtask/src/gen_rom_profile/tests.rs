//! Locator tests over synthetic ROMs, plus the real-ROM drift check.
//!
//! Every test here but the last builds its own ROM-shaped image with
//! `rom_import::fixture::RomFixture` and its own pack, so the suite needs
//! no copyrighted ROM and no real upstream checkout -- the audio tests
//! build their own synthetic stand-in for the `sound/` files that domain
//! reads. The last test needs both a real ROM and a real checkout and is
//! `#[ignore]`d: it regenerates the profile and asserts the committed file
//! is what a fresh run would write.

use std::path::{Path, PathBuf};

use pack_format::{image_entry, image_entry_from_tiles, palette_entry, PackWriter};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::audio;
use super::error::GenRomProfileError;
use super::images::{locate_images, ImageQuery};
use super::pack_source::PackSource;
use super::palettes::locate_unique;
use super::plan::{Resolution, SymbolExpectation};
use super::search::{Lz77Search, PointerIndex, RawSearch};
use super::Context;

/// A scratch directory unique to one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pokeemerald-rs-gen-rom-profile-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

/// Tile bytes that are distinctive enough to anchor a search.
fn distinctive_tiles(tile_count: usize) -> Vec<u8> {
    (0..tile_count * 32)
        .map(|index| {
            let value = u32::try_from(index).expect("small") * 37 + 11;
            u8::try_from(value % 251).expect("modulo 251 fits in u8")
        })
        .collect()
}

/// Run a body against a fixture ROM and a pack built from `entries`.
pub(super) fn with_context<T>(
    name: &str,
    rom: &[u8],
    entries: Vec<pack_format::PackEntry>,
    body: impl FnOnce(&Context<'_>) -> T,
) -> T {
    let dir = scratch(name);
    let pack_path = dir.join("test.pack");
    let mut writer = PackWriter::new();
    for entry in entries {
        writer.push(entry);
    }
    std::fs::write(&pack_path, writer.finish().expect("pack")).expect("write pack");
    let pack = PackSource::load(&pack_path).expect("load pack");
    let ctx = Context {
        rom,
        pack: &pack,
        raw: RawSearch::new(rom),
        lz77: Lz77Search::new(rom),
        pointers: PointerIndex::build(rom),
        upstream: dir.join("no-checkout"),
    };
    let out = body(&ctx);
    let _ = std::fs::remove_file(&pack_path);
    let _ = std::fs::remove_dir(&dir);
    out
}

#[test]
fn a_planted_sheet_is_found_once_with_the_shape_it_was_written_in() {
    // A 4x4-tile sheet written in gbagfx's 2x4 metatile walk. Only that
    // walk reproduces the planted bytes, so the locator has to discover it.
    let tiles = distinctive_tiles(16);
    let entry =
        image_entry_from_tiles("sprite/x".into(), &tiles, 4, 32, 32, Some((2, 4))).expect("entry");
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &tiles)
        .finish();

    with_context("one-sheet", &rom, vec![entry], |ctx| {
        let mut report = Vec::new();
        let plans = locate_images(ctx, &[ImageQuery::raw("sprite/x")], &mut report).expect("found");
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].addr, 0x0810_0000);
        assert_eq!(plans[0].encoding, Encoding::Raw);
        assert_eq!((plans[0].metatile_width, plans[0].metatile_height), (2, 4));
        assert_eq!(plans[0].tile_count, 16);
        assert_eq!(report.len(), 1);
        assert!(report[0].note.is_none());
    });
}

#[test]
fn art_that_is_nowhere_in_the_rom_is_not_found() {
    let tiles = distinctive_tiles(4);
    let entry = image_entry_from_tiles("sprite/x".into(), &tiles, 4, 16, 16, None).expect("entry");
    let rom = RomFixture::new().emerald_header().finish();

    with_context("missing-sheet", &rom, vec![entry], |ctx| {
        let mut report = Vec::new();
        let err = locate_images(ctx, &[ImageQuery::raw("sprite/x")], &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if id == "sprite/x"),
            "{err}"
        );
    });
}

#[test]
fn two_ids_sharing_art_take_one_copy_each() {
    // Upstream really does this: two object-event symbols hold identical
    // sheets. Both copies are valid, so the ids are paired off in address
    // order and both get a note.
    let tiles = distinctive_tiles(4);
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x20_0000, &tiles)
        .write(0x30_0000, &tiles)
        .finish();
    let entries = vec![
        image_entry_from_tiles("sprite/a".into(), &tiles, 4, 16, 16, None).expect("entry"),
        image_entry_from_tiles("sprite/b".into(), &tiles, 4, 16, 16, None).expect("entry"),
    ];

    with_context("shared-art", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plans = locate_images(
            ctx,
            &[ImageQuery::raw("sprite/a"), ImageQuery::raw("sprite/b")],
            &mut report,
        )
        .expect("found");
        assert_eq!(plans[0].addr, 0x0820_0000);
        assert_eq!(plans[1].addr, 0x0830_0000);
        assert!(report.iter().all(|line| line.note.is_some()), "{report:?}");
        // An arbitrary pick must never claim to be evidence-based.
        assert!(
            report
                .iter()
                .all(|line| line.resolution == Resolution::ArbitraryAmongIdentical),
            "{report:?}"
        );
    });
}

#[test]
fn one_id_with_two_copies_takes_the_lower_address() {
    let tiles = distinctive_tiles(4);
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x20_0000, &tiles)
        .write(0x30_0000, &tiles)
        .finish();
    let entries =
        vec![image_entry_from_tiles("sprite/a".into(), &tiles, 4, 16, 16, None).expect("entry")];

    with_context("duplicate-art", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plans = locate_images(ctx, &[ImageQuery::raw("sprite/a")], &mut report).expect("found");
        assert_eq!(plans[0].addr, 0x0820_0000);
        assert_eq!(report[0].resolution, Resolution::ArbitraryAmongIdentical);
        assert!(report[0]
            .note
            .as_deref()
            .is_some_and(|note| note.contains("identical bytes")));
    });
}

#[test]
fn a_compressed_sheet_is_found_by_its_decompressed_bytes() {
    let tiles = distinctive_tiles(4);
    let entry =
        image_entry_from_tiles("title/image/x".into(), &tiles, 4, 16, 16, None).expect("entry");
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x40_0000, &lz77_literals(&tiles))
        .finish();

    with_context("compressed-sheet", &rom, vec![entry], |ctx| {
        let mut report = Vec::new();
        let plans =
            locate_images(ctx, &[ImageQuery::lz77("title/image/x")], &mut report).expect("found");
        assert_eq!(plans[0].addr, 0x0840_0000);
        assert_eq!(plans[0].encoding, Encoding::Lz77);
        assert_eq!(plans[0].tile_count, 4);
    });
}

#[test]
fn a_planted_palette_is_found_and_a_duplicated_one_is_refused() {
    let colors: Vec<u16> = (0..16u16).map(|index| index * 0x0421 + 0x1234).collect();
    let mut bytes = Vec::new();
    for color in &colors {
        bytes.extend_from_slice(&color.to_le_bytes());
    }
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x50_0000, &bytes)
        .finish();
    let unique_entry = palette_entry("interface/palette/a".into(), &colors).expect("entry");

    with_context("one-palette", &rom, vec![unique_entry], |ctx| {
        let mut report = Vec::new();
        let plans = locate_unique(
            ctx,
            &["interface/palette/a".to_owned()],
            &|_| SymbolExpectation::Unnamed,
            &mut report,
        )
        .expect("found");
        assert_eq!(plans[0].addr, 0x0850_0000);
        assert_eq!(plans[0].color_count, 16);
    });

    let rom = RomFixture::new()
        .emerald_header()
        .write(0x50_0000, &bytes)
        .write(0x60_0000, &bytes)
        .finish();
    let entry = palette_entry("interface/palette/a".into(), &colors).expect("entry");
    with_context("duplicate-palette", &rom, vec![entry], |ctx| {
        let mut report = Vec::new();
        let err = locate_unique(
            ctx,
            &["interface/palette/a".to_owned()],
            &|_| SymbolExpectation::Unnamed,
            &mut report,
        )
        .unwrap_err();
        match err {
            GenRomProfileError::Ambiguous { addrs, .. } => {
                assert_eq!(addrs, vec![0x0850_0000, 0x0860_0000]);
            }
            other => panic!("expected Ambiguous, got {other}"),
        }
    });
}

/// Compress `data` as a literals-only LZ77 stream, which is what the
/// decompressor reads back.
fn lz77_literals(data: &[u8]) -> Vec<u8> {
    let len = u32::try_from(data.len()).expect("small");
    let size = len.to_le_bytes();
    let mut out = vec![0x10, size[0], size[1], size[2]];
    for chunk in data.chunks(8) {
        out.push(0);
        out.extend_from_slice(chunk);
    }
    out
}

#[test]
fn a_map_checks_names_where_they_are_derivable_and_presence_where_they_are_not() {
    use super::plan::ReportLine;

    let dir = scratch("map-check");
    let map_path = dir.join("pokeemerald.map");
    std::fs::write(
        &map_path,
        "                0x00000000083df704                gTileset_General
                        0x084975f8                gObjectEventPic_BrendanNormal
                        0x083ea284                LittlerootTown_Layout
         Memory Configuration
",
    )
    .expect("write map");

    let mut lines = vec![
        ReportLine::unique("tileset/general", 0x083D_F704, 24).symbol("gTileset_General"),
        ReportLine::unique("sprite/brendan/walking", 0x0849_75F8, 2304)
            .symbol_contains(["gObjectEventPic_"]),
        ReportLine::unique("layout/littleroot_town", 0x083E_A284, 24),
        ReportLine::unique("tileset/general/palette/01", 0x08DD_4E30, 32).interior(),
    ];
    let check = super::cross_check(Some(&map_path), &mut lines).expect("cross-check");
    assert_eq!(
        (check.named, check.confirmed, check.skipped, check.used),
        (2, 1, 1, true)
    );
    assert_eq!(lines[0].note.as_deref(), Some("map: gTileset_General"));
    assert!(lines[3].note.is_none(), "an interior address is skipped");

    // A name the map disagrees with is a failure, not an annotation.
    let mut wrong =
        vec![ReportLine::unique("tileset/general", 0x083D_F704, 24).symbol("gTileset_Petalburg")];
    let err = super::cross_check(Some(&map_path), &mut wrong).unwrap_err();
    let rendered = err.to_string();
    assert!(rendered.contains("gTileset_Petalburg"), "{rendered}");
    assert!(rendered.contains("gTileset_General"), "{rendered}");
    assert!(
        matches!(err, GenRomProfileError::MapMismatch { generated, .. } if generated == 0x083D_F704),
        "{rendered}"
    );

    // So is a loose convention the symbol at that address does not follow.
    let mut wrong_family =
        vec![ReportLine::unique("sprite/x", 0x083D_F704, 24).symbol_contains(["gObjectEventPic_"])];
    assert!(super::cross_check(Some(&map_path), &mut wrong_family).is_err());

    // And so is an address the map names nothing at.
    let mut absent = vec![ReportLine::unique("layout/x", 0x0812_3456, 24)];
    let err = super::cross_check(Some(&map_path), &mut absent).unwrap_err();
    assert!(err.to_string().contains("names nothing"), "{err}");

    let _ = std::fs::remove_file(&map_path);
    let _ = std::fs::remove_dir(&dir);
}

#[cfg(feature = "rom-drift")]
#[test]
#[ignore = "needs $POKEEMERALD_ROM, a pokeemerald/ checkout, and an extracted pack"]
fn the_committed_profile_matches_a_fresh_generation() {
    // The drift check: regenerating must reproduce the committed module
    // byte for byte, or the table and the generator have diverged.
    let Ok(rom) = std::env::var("POKEEMERALD_ROM") else {
        panic!("set POKEEMERALD_ROM to the supported cartridge image");
    };
    let dir = scratch("drift");
    let out = dir.join("bpee_rev0.rs");
    let report = super::run(&super::Options {
        rom: PathBuf::from(rom),
        out: Some(out.clone()),
        map: None,
    })
    .expect("generation should succeed");
    assert!(report.root_count() > 300, "{}", report.root_count());

    let generated = std::fs::read_to_string(&out).expect("read generated");
    let committed_path = crate::extract::repo_root().join(super::PROFILE_RELATIVE_PATH);
    let committed = std::fs::read_to_string(&committed_path).expect("read committed profile");
    assert_eq!(
        generated, committed,
        "the committed profile is out of date; rerun `cargo xtask gen-rom-profile`"
    );

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_dir(&dir);
    assert!(Path::new(&committed_path).is_file());
}

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

    let err = super::run(&super::Options {
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

    let err = super::run(&super::Options {
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

    let err = super::run(&super::Options {
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

    let err = super::run(&super::Options {
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

    super::publish(&alias, "pub const GENERATED: u32 = 0;\n").expect("the module writes");

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

    let err = super::publish_with(&out, |file, temp| {
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
    let super::GenRomProfileError::WriteFailed { reason, .. } = &err else {
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
    let result = super::publish(&out, "pub const GENERATED: u32 = 0;\n");
    assert!(
        matches!(result, Err(super::GenRomProfileError::WriteFailed { .. })),
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
    super::publish(&out, "pub const GENERATED: u32 = 0;\n").expect("the module writes");

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

    let err = super::write_module(&rom, &out, "pub const GENERATED: u32 = 0;\n")
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

/// A synthetic `pokeemerald/` holding just the `sound/` files
/// [`audio::locate`] reads, plus the `MUS_*` constants a multi-song pack
/// would name. Not a real upstream checkout: `title.inc` declares one slot,
/// enough to anchor the run [`audio_rom`] plants.
fn audio_upstream(dir: &Path) -> PathBuf {
    let upstream = dir.join("upstream");
    std::fs::create_dir_all(upstream.join("sound/voicegroups")).expect("voicegroup dir");
    std::fs::create_dir_all(upstream.join("include/constants")).expect("constants dir");
    std::fs::write(
        upstream.join("sound/voicegroups/voicegroup_title.inc"),
        "voice_group title\n\tvoice_directsound 60, 0, DirectSoundWaveData_demo, 255, 0, 255, 165\n",
    )
    .expect("write voicegroup");
    std::fs::write(upstream.join("sound/keysplit_tables.inc"), "").expect("write keysplits");
    std::fs::write(
        upstream.join("include/constants/songs.h"),
        "#define MUS_TITLE 1\n#define MUS_OTHER 2\n",
    )
    .expect("write songs.h");
    upstream
}

/// The pack payload of one `DirectSound` sample, in the wire format
/// `extract::audio_samples` writes: kind, freq, loop flag, loop start,
/// count, PCM.
fn audio_direct_sound_payload(frequency: u32, pcm: &[u8]) -> Vec<u8> {
    let mut payload = vec![0u8];
    payload.extend_from_slice(&frequency.to_le_bytes());
    payload.push(0);
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.extend_from_slice(&u32::try_from(pcm.len()).expect("small").to_le_bytes());
    payload.extend_from_slice(pcm);
    payload
}

/// A ROM holding one sample, `voicegroup_title`'s one slot, one song header
/// that plays through it, and the `gSongTable` entry at `MUS_TITLE`'s index.
fn audio_rom(pcm: &[u8]) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&0x2000u32.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&u32::try_from(pcm.len()).expect("small").to_le_bytes());

    let mut slot = vec![0x00, 60, 0, 0];
    slot.extend_from_slice(&0x0810_0000u32.to_le_bytes());
    slot.extend_from_slice(&[255, 0, 255, 165]);

    let mut song_header = vec![1u8, 0, 0, 0];
    song_header.extend_from_slice(&0x0820_0000u32.to_le_bytes());
    song_header.extend_from_slice(&0x0840_0000u32.to_le_bytes());

    RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &header)
        .write(0x10_0010, pcm)
        .write(0x20_0000, &slot)
        .write(0x30_0000, &song_header)
        // gSongTable at 0x0850_0000; MUS_TITLE is index 1.
        .write(0x50_0008, &0x0830_0000u32.to_le_bytes())
        .finish()
}

/// Run `body` against the [`audio_rom`]/[`audio_upstream`] fixture, with
/// `songs` as the pack's `audio/song/` ids.
fn with_audio_context<T>(name: &str, songs: &[&str], body: impl FnOnce(&Context<'_>) -> T) -> T {
    let pcm: Vec<u8> = (0..64u32)
        .map(|index| u8::try_from((index * 43 + 7) % 199).expect("modulo 199 fits in u8"))
        .collect();
    let rom = audio_rom(&pcm);
    let dir = scratch(name);
    let upstream = audio_upstream(&dir);

    let mut writer = PackWriter::new();
    writer.push(pack_format::raw_entry(
        "audio/sample/direct-sound/demo".to_owned(),
        audio_direct_sound_payload(0x2000, &pcm),
    ));
    for song in songs {
        writer.push(pack_format::raw_entry((*song).to_owned(), vec![1, 2, 3, 4]));
    }
    let pack_path = dir.join("test.pack");
    std::fs::write(&pack_path, writer.finish().expect("pack")).expect("write pack");
    let pack = PackSource::load(&pack_path).expect("load pack");

    let ctx = Context {
        rom: &rom,
        pack: &pack,
        raw: RawSearch::new(&rom),
        lz77: Lz77Search::new(&rom),
        pointers: PointerIndex::build(&rom),
        upstream,
    };
    let out = body(&ctx);
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[test]
fn the_audio_fixture_locates_its_one_song() {
    // The sanity check the refusal test below leans on: the fixture itself
    // resolves the header, the table, and exactly one voicegroup when the
    // pack holds only the one song this locator supports.
    with_audio_context("audio-one-song", &["audio/song/mus_title"], |ctx| {
        let mut report = Vec::new();
        let plan = audio::locate(ctx, &mut report).expect("the one-song pack locates");
        assert_eq!(plan.songs.len(), 1);
        assert_eq!(plan.songs[0].id, "audio/song/mus_title");
        assert_eq!(plan.songs[0].header, 0x0830_0000);
        assert_eq!(plan.song_table, 0x0850_0000);
        assert_eq!(plan.voicegroups.len(), 1);
    });
}

#[test]
fn a_second_song_is_refused_by_name_instead_of_mapped_through_mus_title() {
    // Pins the refusal naming both offending ids, before any root is
    // located.
    with_audio_context(
        "audio-two-songs",
        &["audio/song/mus_other", "audio/song/mus_title"],
        |ctx| {
            let mut report = Vec::new();
            let err = audio::locate(ctx, &mut report)
                .expect_err("a pack with more than the one supported song must be refused");
            let GenRomProfileError::StructMismatch { id, reason } = &err else {
                panic!("{err:?}");
            };
            assert_eq!(id, "audio/song/*");
            assert!(reason.contains("audio/song/mus_other"), "{reason}");
            assert!(reason.contains("audio/song/mus_title"), "{reason}");
            assert!(
                report.is_empty(),
                "refused before locating anything: {report:?}"
            );
        },
    );
}

#[test]
fn no_song_at_all_is_refused_by_the_same_singleton_check() {
    // Pins the same `StructMismatch` on an empty `audio/song/` id set.
    with_audio_context("audio-no-song", &[], |ctx| {
        let mut report = Vec::new();
        let err = audio::locate(ctx, &mut report)
            .expect_err("a pack with no audio/song/ entries must be refused");
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "audio/song/*"),
            "{err:?}"
        );
    });
}

#[test]
fn an_eight_bit_sheet_with_indices_above_fifteen_is_still_located() {
    // `title/image/pokemon_logo` is an 8-bit-indexed PNG using indices up
    // to 223. `rom_depths(8)` speculatively probes 4bpp as well, so the
    // 4bpp packing of this raster must not abort the whole search.
    let tiles: Vec<u8> = (0..4 * 64u32)
        .map(|index| u8::try_from(16 + index % 208).expect("fits in u8"))
        .collect();
    let entry =
        image_entry_from_tiles("title/image/x".into(), &tiles, 8, 16, 16, None).expect("entry");
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x50_0000, &tiles)
        .finish();

    with_context("wide-index-sheet", &rom, vec![entry], |ctx| {
        let mut report = Vec::new();
        let plans = locate_images(ctx, &[ImageQuery::raw("title/image/x")], &mut report)
            .expect("an 8bpp sheet is located despite the speculative 4bpp probe");
        assert_eq!(plans[0].addr, 0x0850_0000);
        assert_eq!(plans[0].rom_bit_depth, 8);
    });
}

#[test]
fn a_four_bit_sheet_with_an_index_above_fifteen_is_an_entry_shape_error() {
    // Unlike the 8bpp probe above, a 4bpp entry has only one candidate
    // depth (`rom_depths(4)` is `&[4]`), so this is never a speculative
    // probe: an out-of-range index here is a malformed pack, and must
    // surface as `EntryShape`, not read as the art simply not being
    // anywhere in the ROM.
    let mut pixels = vec![0u8; 8 * 8];
    pixels[0] = 16;
    let entry = image_entry("sprite/x".into(), 8, 8, 4, pixels).expect("entry");
    let rom = RomFixture::new().emerald_header().finish();

    with_context("wide-index-four-bit-sheet", &rom, vec![entry], |ctx| {
        let mut report = Vec::new();
        let err = locate_images(ctx, &[ImageQuery::raw("sprite/x")], &mut report)
            .expect_err("an out-of-range 4bpp index is a malformed pack, not a missing image");
        assert!(
            matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == "sprite/x"),
            "{err}"
        );
    });
}

#[test]
fn a_two_bit_sheet_with_an_index_above_fifteen_is_an_entry_shape_error() {
    // A 2bpp entry's sole ROM candidate is 4bpp too (`rom_depths(2)` is
    // `&[4]`), exactly like the 4bpp case above, and just as
    // non-speculative: there is no narrower depth being guessed at, so an
    // out-of-range index here must also reach `EntryShape`, not read as a
    // rejected speculative probe that leaves the art merely not found.
    let mut pixels = vec![0u8; 8 * 8];
    pixels[0] = 16;
    let entry = image_entry("sprite/x".into(), 8, 8, 2, pixels).expect("entry");
    let rom = RomFixture::new().emerald_header().finish();

    with_context("wide-index-two-bit-sheet", &rom, vec![entry], |ctx| {
        let mut report = Vec::new();
        let err = locate_images(ctx, &[ImageQuery::raw("sprite/x")], &mut report)
            .expect_err("an out-of-range index packed to 4bpp is a malformed pack");
        assert!(
            matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == "sprite/x"),
            "{err}"
        );
    });
}
