//! Interface locator tests over a synthetic ROM and pack.

use pack_format::{image_entry_from_tiles, palette_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::locate;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{cross_check_ld_map, distinctive_tiles, with_context};

const ARROW: &str = "interface/image/down_arrow";
const PALETTE: &str = "interface/palette/main_menu_bg";
/// `down_arrow.png` is one column of six tiles.
const ARROW_TILES: usize = 6;
const ARROW_AT: usize = 0x10_0000;
const PALETTE_AT: usize = 0x11_0000;

fn arrow_tiles() -> Vec<u8> {
    distinctive_tiles(ARROW_TILES)
}

fn palette_colors() -> Vec<u16> {
    (0..16u16).map(|n| n * 0x0421 + 0x0111).collect()
}

fn arrow_entry() -> PackEntry {
    image_entry_from_tiles(ARROW.to_owned(), &arrow_tiles(), 4, 8, 48, None).expect("arrow entry")
}

fn entries() -> Vec<PackEntry> {
    vec![
        arrow_entry(),
        palette_entry(PALETTE.to_owned(), &palette_colors()).expect("palette"),
    ]
}

fn rom_with(arrow_copies: &[usize]) -> Vec<u8> {
    let palette: Vec<u8> = palette_colors()
        .iter()
        .flat_map(|color| color.to_le_bytes())
        .collect();
    let mut rom = RomFixture::new().emerald_header();
    for &at in arrow_copies {
        rom = rom.write(at, &arrow_tiles());
    }
    rom.write(PALETTE_AT, &palette).finish()
}

#[test]
fn the_down_arrow_is_located_as_raw_four_bit_tiles() {
    with_context(
        "interface-arrow",
        &rom_with(&[ARROW_AT]),
        entries(),
        |ctx| {
            let mut report = Vec::new();
            let plan = locate(ctx, &mut report).expect("located");

            assert_eq!(plan.images.len(), 1);
            let image = &plan.images[0];
            assert_eq!(image.id, ARROW);
            assert_eq!(image.addr, 0x0810_0000);
            assert_eq!(image.encoding, Encoding::Raw);
            assert_eq!((image.rom_bit_depth, image.pack_bit_depth), (4, 4));
            assert_eq!((image.width, image.height), (8, 48));
            assert_eq!(image.tile_count, 6);
            assert_eq!(plan.palettes.len(), 1);

            assert_eq!(report.len(), 2);
            assert!(report
                .iter()
                .all(|line| line.resolution == Resolution::UniqueSignature));
        },
    );
}

#[test]
fn the_down_arrow_is_a_translation_unit_local_static() {
    with_context(
        "interface-arrow-map",
        &rom_with(&[ARROW_AT]),
        entries(),
        |ctx| {
            let mut report = Vec::new();
            locate(ctx, &mut report).expect("located");
            let line = report.iter().find(|line| line.id == ARROW).expect(ARROW);
            assert_eq!(
                line.symbol,
                SymbolExpectation::Local("sDownArrowTiles".to_owned())
            );
            // A map with no symbol at all is unreadable, so give it one global root.
            report.push(ReportLine::unique("global/root", 0x0820_0000, 4));
            let check = cross_check_ld_map("interface-arrow-map", &mut report, &[])
                .expect("a static the map cannot list must not fail the cross-check");
            assert_eq!((check.local, check.confirmed), (2, 1));
        },
    );
}

#[test]
fn an_arrow_the_rom_does_not_hold_is_not_found() {
    with_context("interface-arrow-absent", &rom_with(&[]), entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::NotFound { .. }), "{err}");
    });
}

#[test]
fn a_second_copy_of_the_arrow_is_ambiguous_rather_than_paired() {
    let rom = rom_with(&[ARROW_AT, ARROW_AT + 0x1000]);
    with_context("interface-arrow-twin", &rom, entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::Ambiguous { .. }), "{err}");
    });
}
