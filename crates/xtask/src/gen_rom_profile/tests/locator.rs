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
use super::super::plan::{ReportLine, Resolution, SymbolExpectation};
use super::{assert_survives_ld_map, distinctive_tiles, lz77_literals, scratch, with_context};

#[test]
fn a_planted_sheet_is_found_once_with_the_shape_it_was_written_in() {
    let metatile_shape = (2, 4);
    let tiles = distinctive_tiles(16);
    let entry = image_entry_from_tiles("sprite/x".into(), &tiles, 4, 32, 32, Some(metatile_shape))
        .expect("entry");
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
        assert_eq!(
            (plans[0].metatile_width, plans[0].metatile_height),
            metatile_shape,
            "the locator must recover the planted metatile walk"
        );
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
        assert!(
            report
                .iter()
                .all(|line| line.resolution == Resolution::ArbitraryAmongIdentical),
            "identical-byte pairing is arbitrary, not structural evidence: {report:?}"
        );
    });
}

const SHARING_IDS_OVER_HIT_CAP: usize = super::super::search::MATCH_LIMIT + 1;

/// Locates more ids than the locator's hit cap, all sharing one sheet planted
/// once per id in `encoding`, and requires each to take its own copy.
fn assert_every_sharing_id_takes_its_own_copy(name: &str, encoding: Encoding) {
    let tiles = distinctive_tiles(4);
    let stored = match encoding {
        Encoding::Raw => tiles.clone(),
        Encoding::Lz77 => lz77_literals(&tiles),
    };
    let mut fixture = RomFixture::new().emerald_header();
    let mut entries = Vec::new();
    let mut queries = Vec::new();
    for slot in 0..SHARING_IDS_OVER_HIT_CAP {
        fixture = fixture.write(0x20_0000 + slot * 0x100, &stored);
        let id = format!("sprite/{slot:02}");
        entries.push(image_entry_from_tiles(id.clone(), &tiles, 4, 16, 16, None).expect("entry"));
        queries.push(match encoding {
            Encoding::Raw => ImageQuery::raw(id),
            Encoding::Lz77 => ImageQuery::lz77(id),
        });
    }
    let rom = fixture.finish();

    with_context(name, &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plans = locate_images(ctx, &queries, &mut report).expect("every id has a copy");
        let addrs: Vec<u32> = plans.iter().map(|plan| plan.addr).collect();
        let expected: Vec<u32> = (0..SHARING_IDS_OVER_HIT_CAP)
            .map(|slot| 0x0820_0000 + u32::try_from(slot).expect("small") * 0x100)
            .collect();
        assert_eq!(
            addrs, expected,
            "every shared-art id gets its own copy in address order beyond the ordinary hit cap"
        );
        assert!(
            report
                .iter()
                .all(|line| line.resolution == Resolution::ArbitraryAmongIdentical),
            "{report:?}"
        );
    });
}

#[test]
fn more_raw_ids_than_the_hit_cap_each_take_one_copy() {
    assert_every_sharing_id_takes_its_own_copy("shared-art-raw-over-cap", Encoding::Raw);
}

#[test]
fn more_compressed_ids_than_the_hit_cap_each_take_one_copy() {
    assert_every_sharing_id_takes_its_own_copy("shared-art-lz77-over-cap", Encoding::Lz77);
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

    let mut wrong =
        vec![ReportLine::unique("tileset/general", 0x083D_F704, 24).symbol("gTileset_Petalburg")];
    let err = crate::gen_rom_profile::cross_check(Some(&map_path), &mut wrong)
        .expect_err("a different symbol name is rejected, not annotated");
    let rendered = err.to_string();
    assert!(rendered.contains("gTileset_Petalburg"), "{rendered}");
    assert!(rendered.contains("gTileset_General"), "{rendered}");
    assert!(
        matches!(err, GenRomProfileError::MapMismatch { generated, .. } if generated == 0x083D_F704),
        "{rendered}"
    );

    let mut wrong_family =
        vec![ReportLine::unique("sprite/x", 0x083D_F704, 24).symbol_contains(["gObjectEventPic_"])];
    assert!(
        crate::gen_rom_profile::cross_check(Some(&map_path), &mut wrong_family).is_err(),
        "a symbol from another family cannot confirm this root"
    );

    let mut absent = vec![ReportLine::unique("layout/x", 0x0812_3456, 24)];
    let err = crate::gen_rom_profile::cross_check(Some(&map_path), &mut absent)
        .expect_err("an address with no map symbol cannot confirm a root");
    assert!(err.to_string().contains("names nothing"), "{err}");

    let _ = std::fs::remove_file(&map_path);
    let _ = std::fs::remove_dir(&dir);
}

/// `main_menu.c:406` gives `sMainMenuBgPal` internal linkage, so GNU ld map
/// evidence is not required at this palette's address.
#[test]
fn the_static_main_menu_bg_palette_needs_no_linker_global_evidence() {
    let id = "interface/palette/main_menu_bg";
    let mut lines = [ReportLine::unique(id, 0x0850_0000, 32)
        .with_symbol(super::super::interface_palette_symbol(id))];
    assert_survives_ld_map("main-menu-static-map", 0x0850_0000, &mut lines);
    assert_eq!(
        super::super::interface_palette_symbol(id),
        SymbolExpectation::Local("sMainMenuBgPal".to_owned())
    );
}

/// `main_menu.c:407` makes the text palette `static` as well, but extraction
/// emits no pack entry for it, so other ids are not assumed local.
#[test]
fn an_unclassified_interface_palette_is_not_assumed_local() {
    assert_eq!(
        super::super::interface_palette_symbol("interface/palette/other"),
        SymbolExpectation::Unnamed
    );
}

#[cfg(feature = "rom-drift")]
#[test]
#[ignore = "needs $POKEEMERALD_ROM, a pokeemerald/ checkout, and an extracted pack"]
fn the_committed_profile_matches_a_fresh_generation() {
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

fn malformed_raster_error(name: &str, width: u32, height: u32) -> GenRomProfileError {
    let pixels = vec![1u8; width as usize * height as usize];
    let entry = image_entry("sprite/x".into(), width, height, 4, pixels).expect("entry");
    let rom = RomFixture::new().emerald_header().finish();
    with_context(name, &rom, vec![entry], |ctx| {
        let mut report = Vec::new();
        locate_images(ctx, &[ImageQuery::raw("sprite/x")], &mut report)
            .expect_err("a raster without whole 8x8 tiles is malformed, not located")
    })
}

#[test]
fn a_raster_that_is_not_whole_eight_pixel_tiles_is_an_entry_shape_error() {
    for (width, height) in [(4, 8), (7, 8), (8, 7), (12, 8), (0, 8), (8, 0), (0, 0)] {
        let err = malformed_raster_error("malformed-raster", width, height);
        assert!(
            matches!(&err, GenRomProfileError::EntryShape { id, reason }
                if id == "sprite/x" && reason.contains(&format!("{width}x{height}"))),
            "{width}x{height}: {err:?}"
        );
    }
}

fn entry_outside_missing_domains() -> pack_format::PackEntry {
    pack_format::raw_entry("sprite/unrelated".to_owned(), vec![1, 2, 3, 4])
}

#[test]
fn a_pack_missing_the_whole_font_domain_is_refused() {
    let rom = RomFixture::new().emerald_header().finish();

    with_context(
        "no-fonts",
        &rom,
        vec![entry_outside_missing_domains()],
        |ctx| {
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
        },
    );
}

#[test]
fn a_pack_missing_the_whole_tileset_domain_is_refused() {
    let rom = RomFixture::new().emerald_header().finish();

    with_context(
        "no-tilesets",
        &rom,
        vec![entry_outside_missing_domains()],
        |ctx| {
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
        },
    );
}

#[test]
fn a_pack_missing_the_whole_layout_domain_is_refused() {
    let rom = RomFixture::new().emerald_header().finish();

    with_context(
        "no-layouts",
        &rom,
        vec![entry_outside_missing_domains()],
        |ctx| {
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
        },
    );
}

#[test]
fn a_pack_missing_the_whole_interface_palette_domain_is_refused() {
    let rom = RomFixture::new().emerald_header().finish();

    with_context(
        "no-interface-palettes",
        &rom,
        vec![entry_outside_missing_domains()],
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
