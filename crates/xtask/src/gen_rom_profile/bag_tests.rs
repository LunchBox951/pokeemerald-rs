//! Bag locator tests over a synthetic ROM and pack.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::locate_expected;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::Resolution;
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

const ICON_TILES: usize = 9;

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

fn palette_bytes(seed: u16) -> (Vec<u16>, Vec<u8>) {
    let colors: Vec<u16> = (0..16u16).map(|n| n * 0x0421 + seed).collect();
    let bytes = colors.iter().flat_map(|c| c.to_le_bytes()).collect();
    (colors, bytes)
}

fn icon(id: &str, tiles: &[u8]) -> PackEntry {
    image_entry_from_tiles(id.to_owned(), tiles, 4, 24, 24, None).expect("icon entry")
}

fn blob() -> Vec<u8> {
    (0..64u8)
        .map(|n| n.wrapping_mul(7).wrapping_add(3))
        .collect()
}

#[test]
fn every_bag_family_is_located_with_its_own_encoding() {
    let tiles = distinctive_tiles(ICON_TILES);
    let (bag_colors, bag_bytes) = palette_bytes(0x1111);
    let (ball_colors, ball_bytes) = palette_bytes(0x2222);
    let (icon_colors, icon_bytes) = palette_bytes(0x3333);
    let blob = blob();
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &lz77_literals(&tiles))
        .write(0x11_0000, &lz77_literals(&bag_bytes))
        .write(0x12_0000, &ball_bytes)
        .write(0x13_0000, &lz77_literals(&icon_bytes))
        .write(0x14_0000, &lz77_literals(&blob))
        .finish();
    let entries = vec![
        icon("bag/item-icon/image/potion", &tiles),
        palette_entry("bag/palette/bag".into(), &bag_colors).expect("palette"),
        palette_entry("bag/palette/rotating_ball".into(), &ball_colors).expect("palette"),
        palette_entry("bag/item-icon/palette/potion".into(), &icon_colors).expect("palette"),
        raw_entry("bag/raw/menu".into(), blob.clone()),
    ];
    let expected = ids(&[
        "bag/raw/menu",
        "bag/palette/rotating_ball",
        "bag/item-icon/palette/potion",
        "bag/palette/bag",
        "bag/item-icon/image/potion",
    ]);

    with_context("bag-families", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate_expected(ctx, &expected, &mut report).expect("located");
        assert_eq!(plan.images.len(), 1);
        assert_eq!(plan.images[0].addr, 0x0810_0000);
        assert_eq!(plan.images[0].encoding, Encoding::Lz77);
        assert_eq!(plan.images[0].tile_count, 9);
        let palettes: Vec<_> = plan
            .palettes
            .iter()
            .map(|p| (p.id.as_str(), p.addr, p.encoding, p.color_count))
            .collect();
        assert_eq!(
            palettes,
            [
                (
                    "bag/item-icon/palette/potion",
                    0x0813_0000,
                    Encoding::Lz77,
                    16
                ),
                ("bag/palette/bag", 0x0811_0000, Encoding::Lz77, 16),
                ("bag/palette/rotating_ball", 0x0812_0000, Encoding::Raw, 16),
            ]
        );
        assert_eq!(plan.raws.len(), 1);
        assert_eq!(plan.raws[0].addr, 0x0814_0000);
        assert_eq!(plan.raws[0].encoding, Encoding::Lz77);
        assert_eq!(plan.raws[0].len, 64);
        assert_eq!(report.len(), 5);
        assert!(report
            .iter()
            .all(|line| line.resolution == Resolution::UniqueSignature));
    });
}

#[test]
fn an_uncompressed_bag_image_is_found_verbatim() {
    let tiles = distinctive_tiles(4);
    let entry =
        image_entry_from_tiles("bag/image/hm".into(), &tiles, 4, 16, 16, None).expect("entry");
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &tiles)
        .finish();
    with_context("bag-raw-image", &rom, vec![entry], |ctx| {
        let mut report = Vec::new();
        let plan = locate_expected(ctx, &ids(&["bag/image/hm"]), &mut report).expect("located");
        assert_eq!(plan.images[0].encoding, Encoding::Raw);
        assert_eq!(plan.images[0].addr, 0x0810_0000);
    });
}

#[test]
fn byte_identical_icons_in_two_copies_are_reported_as_identical() {
    let tiles = distinctive_tiles(ICON_TILES);
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &lz77_literals(&tiles))
        .write(0x11_0000, &lz77_literals(&tiles))
        .finish();
    let entries = vec![
        icon("bag/item-icon/image/antidote", &tiles),
        icon("bag/item-icon/image/status_heal", &tiles),
    ];
    let expected = ids(&[
        "bag/item-icon/image/antidote",
        "bag/item-icon/image/status_heal",
    ]);
    with_context("bag-identical", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate_expected(ctx, &expected, &mut report).expect("located");
        let addrs: Vec<u32> = plan.images.iter().map(|image| image.addr).collect();
        assert_eq!(addrs, [0x0810_0000, 0x0811_0000]);
        assert!(report
            .iter()
            .all(|line| line.resolution == Resolution::ArbitraryAmongIdentical));
    });
}

#[test]
fn identical_icons_sharing_one_copy_are_ambiguous() {
    let tiles = distinctive_tiles(ICON_TILES);
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &lz77_literals(&tiles))
        .finish();
    let entries = vec![
        icon("bag/item-icon/image/antidote", &tiles),
        icon("bag/item-icon/image/status_heal", &tiles),
    ];
    let expected = ids(&[
        "bag/item-icon/image/antidote",
        "bag/item-icon/image/status_heal",
    ]);
    with_context("bag-one-copy", &rom, entries, |ctx| {
        let err = locate_expected(ctx, &expected, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::Ambiguous { .. }), "{err}");
    });
}

#[test]
fn a_palette_absent_from_the_rom_or_stored_the_wrong_way_fails_closed() {
    let (colors, bytes) = palette_bytes(0x4444);
    let entry = || palette_entry("bag/item-icon/palette/potion".into(), &colors).expect("palette");
    let expected = ids(&["bag/item-icon/palette/potion"]);

    // Present only uncompressed: an icon palette is an LZ77 root.
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &bytes)
        .finish();
    with_context("bag-palette-raw", &rom, vec![entry()], |ctx| {
        let err = locate_expected(ctx, &expected, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::NotFound { .. }), "{err}");
    });

    // Present twice: ambiguity is an error, never a first match.
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &lz77_literals(&bytes))
        .write(0x11_0000, &lz77_literals(&bytes))
        .finish();
    with_context("bag-palette-twice", &rom, vec![entry()], |ctx| {
        let err = locate_expected(ctx, &expected, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::Ambiguous { .. }), "{err}");
    });
}

#[test]
fn a_wrong_kind_or_a_stray_expected_id_is_refused() {
    let rom = RomFixture::new().emerald_header().finish();
    let wrong = raw_entry("bag/palette/bag".into(), vec![0; 32]);
    with_context("bag-wrong-kind", &rom, vec![wrong], |ctx| {
        let err = locate_expected(ctx, &ids(&["bag/palette/bag"]), &mut Vec::new()).unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::WrongPackEntryKind { .. }),
            "{err}"
        );
    });

    let stray = raw_entry("bag/other/thing".into(), vec![0]);
    with_context("bag-stray", &rom, vec![stray], |ctx| {
        let err = locate_expected(ctx, &ids(&["bag/other/thing"]), &mut Vec::new()).unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::StructMismatch { .. }),
            "{err}"
        );
    });
}

#[test]
fn a_missing_bag_entry_is_refused_before_any_report_line() {
    let rom = RomFixture::new().emerald_header().finish();
    with_context("bag-missing", &rom, vec![], |ctx| {
        let mut report = Vec::new();
        let err = locate_expected(ctx, &ids(&["bag/raw/menu"]), &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == "bag/raw/menu"),
            "{err}"
        );
        assert!(report.is_empty());
    });
}

#[test]
fn static_bag_roots_survive_a_map_that_omits_locals() {
    use crate::gen_rom_profile::plan::SymbolExpectation;
    use crate::gen_rom_profile::tests::assert_survives_ld_map;

    // `locate_expected` (through `locate_images` and `locate_palettes`) builds
    // every report line, so the test fails if either stops marking an upstream
    // `static` root as local.
    let ball_tiles = distinctive_tiles(4);
    let select_tiles: Vec<u8> = ball_tiles.iter().map(|byte| byte ^ 0x5a).collect();
    let (ball_colors, ball_bytes) = palette_bytes(0x2222);
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &ball_tiles)
        .write(0x11_0000, &select_tiles)
        .write(0x12_0000, &ball_bytes)
        .finish();
    let entries = vec![
        image_entry_from_tiles(
            "bag/image/rotating_ball".into(),
            &ball_tiles,
            4,
            16,
            16,
            None,
        )
        .expect("image"),
        image_entry_from_tiles(
            "bag/image/select_button".into(),
            &select_tiles,
            4,
            16,
            16,
            None,
        )
        .expect("image"),
        palette_entry("bag/palette/rotating_ball".into(), &ball_colors).expect("palette"),
    ];
    let expected = ids(&[
        "bag/image/rotating_ball",
        "bag/image/select_button",
        "bag/palette/rotating_ball",
    ]);

    with_context("bag-statics", &rom, entries, |ctx| {
        let mut report = Vec::new();
        locate_expected(ctx, &expected, &mut report).expect("located");
        assert_eq!(report.len(), 3);
        assert!(report
            .iter()
            .any(|line| line.id == "bag/image/select_button"
                && line.symbol == SymbolExpectation::Local("sRegisteredSelect_Gfx".to_owned())));
        assert_survives_ld_map("bag-statics-map", 0x0818_0000, &mut report);
    });
    // A global bag palette keeps its map check.
    assert_eq!(
        super::palette_symbol("bag/palette/bag"),
        SymbolExpectation::Unnamed
    );
}

#[test]
fn the_scroll_indicator_and_swap_line_sheets_keep_their_upstream_encodings() {
    use crate::gen_rom_profile::plan::SymbolExpectation;

    // `list_menu.c:289-290`: LZ77 sheet, uncompressed `static` palette.
    // `graphics.c:1151-1152`: LZ77 sheet and LZ77 palette, both global.
    let arrow_tiles = distinctive_tiles(8);
    let line_tiles: Vec<u8> = arrow_tiles.iter().map(|byte| byte ^ 0x5a).collect();
    let (red_colors, red_bytes) = palette_bytes(0x5555);
    let (line_colors, line_bytes) = palette_bytes(0x6666);
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &lz77_literals(&arrow_tiles))
        .write(0x11_0000, &red_bytes)
        .write(0x12_0000, &lz77_literals(&line_tiles))
        .write(0x13_0000, &lz77_literals(&line_bytes))
        .finish();
    let sheet = |id: &str, tiles: &[u8]| {
        image_entry_from_tiles(id.into(), tiles, 4, 16, 32, None).expect("image")
    };
    let entries = vec![
        sheet("bag/image/scroll_indicator", &arrow_tiles),
        palette_entry("bag/palette/scroll_indicator".into(), &red_colors).expect("palette"),
        sheet("bag/image/swap_line", &line_tiles),
        palette_entry("bag/palette/swap_line".into(), &line_colors).expect("palette"),
    ];
    let expected = ids(&[
        "bag/palette/swap_line",
        "bag/image/swap_line",
        "bag/palette/scroll_indicator",
        "bag/image/scroll_indicator",
    ]);

    with_context("bag-scroll-swap", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate_expected(ctx, &expected, &mut report).expect("located");
        let images: Vec<_> = plan
            .images
            .iter()
            .map(|p| (p.id.as_str(), p.addr, p.encoding, p.tile_count))
            .collect();
        assert_eq!(
            images,
            [
                ("bag/image/scroll_indicator", 0x0810_0000, Encoding::Lz77, 8),
                ("bag/image/swap_line", 0x0812_0000, Encoding::Lz77, 8),
            ]
        );
        let palettes: Vec<_> = plan
            .palettes
            .iter()
            .map(|p| (p.id.as_str(), p.addr, p.encoding, p.color_count))
            .collect();
        assert_eq!(
            palettes,
            [
                (
                    "bag/palette/scroll_indicator",
                    0x0811_0000,
                    Encoding::Raw,
                    16
                ),
                ("bag/palette/swap_line", 0x0813_0000, Encoding::Lz77, 16),
            ]
        );
        let symbol = |id: &str| {
            report
                .iter()
                .find(|line| line.id == id)
                .unwrap()
                .symbol
                .clone()
        };
        assert_eq!(
            symbol("bag/image/scroll_indicator"),
            SymbolExpectation::Local("sScrollIndicator_Gfx".to_owned())
        );
        assert_eq!(
            symbol("bag/palette/scroll_indicator"),
            SymbolExpectation::Local("sRedInterface_Pal".to_owned())
        );
        assert_eq!(symbol("bag/image/swap_line"), SymbolExpectation::Unnamed);
        assert_eq!(symbol("bag/palette/swap_line"), SymbolExpectation::Unnamed);
    });
}

#[test]
fn the_menu_info_sheet_and_palettes_are_located_uncompressed() {
    use crate::gen_rom_profile::plan::SymbolExpectation;

    // `graphics.c:1264-1267`: global `.4bpp` sheet and three `.gbapal` palettes,
    // all linked uncompressed.
    let tiles = distinctive_tiles(256);
    let (first_colors, first_bytes) = palette_bytes(0x7001);
    let (second_colors, second_bytes) = palette_bytes(0x7002);
    let (third_colors, third_bytes) = palette_bytes(0x7003);
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &first_bytes)
        .write(0x10_0020, &second_bytes)
        .write(0x10_0040, &third_bytes)
        .write(0x10_0060, &tiles)
        .finish();
    let entries = vec![
        image_entry_from_tiles("bag/image/menu_info".into(), &tiles, 4, 128, 128, None)
            .expect("image"),
        palette_entry("bag/palette/menu_info1".into(), &first_colors).expect("palette"),
        palette_entry("bag/palette/menu_info2".into(), &second_colors).expect("palette"),
        palette_entry("bag/palette/menu_info3".into(), &third_colors).expect("palette"),
    ];
    let expected = ids(&[
        "bag/palette/menu_info3",
        "bag/image/menu_info",
        "bag/palette/menu_info1",
        "bag/palette/menu_info2",
    ]);

    with_context("bag-menu-info", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate_expected(ctx, &expected, &mut report).expect("located");
        let images: Vec<_> = plan
            .images
            .iter()
            .map(|p| (p.id.as_str(), p.addr, p.encoding, p.tile_count))
            .collect();
        assert_eq!(
            images,
            [("bag/image/menu_info", 0x0810_0060, Encoding::Raw, 256)]
        );
        let palettes: Vec<_> = plan
            .palettes
            .iter()
            .map(|p| (p.id.as_str(), p.addr, p.encoding, p.color_count))
            .collect();
        assert_eq!(
            palettes,
            [
                ("bag/palette/menu_info1", 0x0810_0000, Encoding::Raw, 16),
                ("bag/palette/menu_info2", 0x0810_0020, Encoding::Raw, 16),
                ("bag/palette/menu_info3", 0x0810_0040, Encoding::Raw, 16),
            ]
        );
        assert!(report
            .iter()
            .all(|line| line.symbol == SymbolExpectation::Unnamed));
    });
}
