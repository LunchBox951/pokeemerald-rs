//! Image, palette, map, committed-profile, image-depth and missing-domain regression tests.

#[cfg(feature = "rom-drift")]
use std::path::Path;
use std::path::PathBuf;

use pack_format::{image_entry, image_entry_from_tiles, palette_entry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::super::error::GenRomProfileError;
use super::super::images::{locate_images, ImageQuery};
use super::super::palettes::locate_unique;
use super::super::plan::{Resolution, SymbolExpectation};
use super::{distinctive_tiles, lz77_literals, scratch, with_context};

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
fn two_ids_sharing_one_copy_of_art_are_ambiguous() {
    // One copy, two ids: there is no copy each, so the locator must not
    // alias both ids onto the same address.
    let tiles = distinctive_tiles(4);
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x20_0000, &tiles)
        .finish();
    let entries = vec![
        image_entry_from_tiles("sprite/a".into(), &tiles, 4, 16, 16, None).expect("entry"),
        image_entry_from_tiles("sprite/b".into(), &tiles, 4, 16, 16, None).expect("entry"),
    ];

    with_context("shared-art-one-copy", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let result = locate_images(
            ctx,
            &[ImageQuery::raw("sprite/a"), ImageQuery::raw("sprite/b")],
            &mut report,
        );
        assert!(
            matches!(&result, Err(GenRomProfileError::Ambiguous { .. })),
            "{result:?}"
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

#[test]
fn the_map_summary_counts_interior_and_local_skips_apart() {
    let report = crate::gen_rom_profile::GenReport {
        out_path: PathBuf::from("profile.rs"),
        lines: Vec::new(),
        map_named: 3,
        map_confirmed: 2,
        map_skipped: 5,
        map_local: 1,
        map_used: true,
    };
    let summary = report.map_summary().expect("a map was used");
    assert!(
        summary.contains("5 interior address(es) skipped"),
        "{summary}"
    );
    assert!(
        summary.contains("1 translation-unit-local symbol(s) skipped"),
        "{summary}"
    );
    let unused = crate::gen_rom_profile::GenReport {
        map_used: false,
        ..report
    };
    assert!(unused.map_summary().is_none());
}

#[test]
fn a_map_checks_names_where_they_are_derivable_and_presence_where_they_are_not() {
    use crate::gen_rom_profile::plan::ReportLine;

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
        ReportLine::unique("sTextWindowPalettes", 0x0851_0000, 160)
            .local_symbol("sTextWindowPalettes"),
    ];
    let check =
        crate::gen_rom_profile::cross_check(Some(&map_path), &mut lines).expect("cross-check");
    assert_eq!(
        (
            check.named,
            check.confirmed,
            check.skipped,
            check.local,
            check.used
        ),
        (2, 1, 1, 1, true)
    );
    assert_eq!(lines[0].note.as_deref(), Some("map: gTileset_General"));
    assert!(lines[3].note.is_none(), "an interior address is skipped");

    // A name the map disagrees with is a failure, not an annotation.
    let mut wrong =
        vec![ReportLine::unique("tileset/general", 0x083D_F704, 24).symbol("gTileset_Petalburg")];
    let err = crate::gen_rom_profile::cross_check(Some(&map_path), &mut wrong).unwrap_err();
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
    assert!(crate::gen_rom_profile::cross_check(Some(&map_path), &mut wrong_family).is_err());

    // And so is an address the map names nothing at.
    let mut absent = vec![ReportLine::unique("layout/x", 0x0812_3456, 24)];
    let err = crate::gen_rom_profile::cross_check(Some(&map_path), &mut absent).unwrap_err();
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
    let report = crate::gen_rom_profile::run(&crate::gen_rom_profile::Options {
        rom: PathBuf::from(rom),
        out: Some(out.clone()),
        map: None,
    })
    .expect("generation should succeed");
    assert!(report.root_count() > 300, "{}", report.root_count());

    let generated = std::fs::read_to_string(&out).expect("read generated");
    let committed_path =
        crate::extract::repo_root().join(crate::gen_rom_profile::PROFILE_RELATIVE_PATH);
    let committed = std::fs::read_to_string(&committed_path).expect("read committed profile");
    assert_eq!(
        generated, committed,
        "the committed profile is out of date; rerun `cargo xtask gen-rom-profile`"
    );

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_dir(&dir);
    assert!(Path::new(&committed_path).is_file());
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

/// A pack entry outside every domain this file tests, so a refusal proves
/// the domain itself is missing rather than the whole pack being empty.
fn unrelated_entry() -> pack_format::PackEntry {
    pack_format::raw_entry("sprite/unrelated".to_owned(), vec![1, 2, 3, 4])
}

#[test]
fn a_pack_missing_the_whole_font_domain_is_refused() {
    let rom = RomFixture::new().emerald_header().finish();

    with_context("no-fonts", &rom, vec![unrelated_entry()], |ctx| {
        let mut report = Vec::new();
        let err = crate::gen_rom_profile::fonts::locate(ctx, &mut report)
            .expect_err("a pack with no font/small/glyphs entries must be refused");
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == "font/small/glyphs"),
            "{err:?}"
        );
        assert!(
            report.is_empty(),
            "refused before locating anything: {report:?}"
        );
    });
}

#[test]
fn a_pack_missing_the_whole_tileset_domain_is_refused() {
    let rom = RomFixture::new().emerald_header().finish();

    with_context("no-tilesets", &rom, vec![unrelated_entry()], |ctx| {
        let mut report = Vec::new();
        let err = crate::gen_rom_profile::tilesets::locate(ctx, &mut report)
            .expect_err("a pack with no fixed tileset roots must be refused");
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == "tileset/general/tiles"),
            "{err:?}"
        );
        assert!(
            report.is_empty(),
            "refused before locating anything: {report:?}"
        );
    });
}

#[test]
fn a_pack_missing_the_whole_layout_domain_is_refused() {
    let rom = RomFixture::new().emerald_header().finish();

    with_context("no-layouts", &rom, vec![unrelated_entry()], |ctx| {
        let mut report = Vec::new();
        let err = crate::gen_rom_profile::layouts::locate(ctx, &mut report)
            .expect_err("a pack with no layout/*/map entries must be refused");
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == "layout/littleroot_town/map"),
            "{err:?}"
        );
        assert!(
            report.is_empty(),
            "refused before locating anything: {report:?}"
        );
    });
}

#[test]
fn a_pack_missing_the_whole_interface_palette_domain_is_refused() {
    let rom = RomFixture::new().emerald_header().finish();

    with_context(
        "no-interface-palettes",
        &rom,
        vec![unrelated_entry()],
        |ctx| {
            let mut report = Vec::new();
            let err = crate::gen_rom_profile::locate_interface_palettes(ctx, &mut report)
                .expect_err("a pack with no interface/palette/* entries must be refused");
            assert!(
                matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == "interface/palette/*"),
                "{err:?}"
            );
            assert!(
                report.is_empty(),
                "refused before locating anything: {report:?}"
            );
        },
    );
}
