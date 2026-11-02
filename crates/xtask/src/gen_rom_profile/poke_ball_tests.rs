//! Poké Ball locator tests over a synthetic ROM and pack.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::{locate, POKE_BALL_TAG, SHEET_BYTES};
use crate::extract::scope::poke_ball_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::Resolution;
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

const SHEET: usize = 0x10_0000;
const PALETTE: usize = 0x11_0000;
const OPEN_SHEET: usize = 0x12_0000;
const SHEET_TABLE: usize = 0x13_0000;
const PALETTE_TABLE: usize = 0x14_0000;
/// A second copy of a stream, below the one its table row names.
const STRAY_PALETTE: usize = 0x0F_0000;
const STRAY_SHEET: usize = 0x0E_0000;

fn bus(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("a fixture offset fits a bus address")
}

fn sheet_tiles() -> Vec<u8> {
    distinctive_tiles(12)
}

fn open_tiles() -> Vec<u8> {
    distinctive_tiles(4)
        .iter()
        .map(|byte| byte ^ 0x5A)
        .collect()
}

fn colors() -> Vec<u16> {
    (0..16u16).map(|n| n * 0x0421 + 0x0123).collect()
}

fn color_bytes() -> Vec<u8> {
    colors().iter().flat_map(|c| c.to_le_bytes()).collect()
}

fn sheet_row(data: u32, size: u16, tag: u16) -> Vec<u8> {
    let mut row = data.to_le_bytes().to_vec();
    row.extend_from_slice(&size.to_le_bytes());
    row.extend_from_slice(&tag.to_le_bytes());
    row
}

fn palette_row(data: u32, tag: u16) -> Vec<u8> {
    let mut row = data.to_le_bytes().to_vec();
    row.extend_from_slice(&tag.to_le_bytes());
    row.extend_from_slice(&[0, 0]);
    row
}

fn entries() -> Vec<PackEntry> {
    vec![
        image_entry_from_tiles(
            "poke-ball/image/poke".to_owned(),
            &sheet_tiles(),
            4,
            16,
            48,
            None,
        )
        .expect("sheet"),
        palette_entry("poke-ball/palette/poke".to_owned(), &colors()).expect("palette"),
        image_entry_from_tiles(
            "poke-ball/image/open".to_owned(),
            &open_tiles(),
            4,
            16,
            16,
            None,
        )
        .expect("open sheet"),
    ]
}

/// A ROM holding the three streams and both tables' first rows.
fn fixture() -> RomFixture {
    RomFixture::new()
        .emerald_header()
        .write(SHEET, &lz77_literals(&sheet_tiles()))
        .write(PALETTE, &lz77_literals(&color_bytes()))
        .write(OPEN_SHEET, &lz77_literals(&open_tiles()))
        .write(
            SHEET_TABLE,
            &sheet_row(bus(SHEET), SHEET_BYTES, POKE_BALL_TAG),
        )
        .write(PALETTE_TABLE, &palette_row(bus(PALETTE), POKE_BALL_TAG))
}

#[test]
fn the_fixed_inventory_is_the_sheet_its_palette_and_the_open_sheet() {
    assert_eq!(
        poke_ball_ids(),
        [
            "poke-ball/image/poke",
            "poke-ball/palette/poke",
            "poke-ball/image/open"
        ]
    );
}

#[test]
fn the_sheets_palette_and_both_tables_are_found() {
    let rom = fixture().finish();
    with_context("poke-ball-found", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");

        assert_eq!(plan.sheet_table, bus(SHEET_TABLE));
        assert_eq!(plan.palette_table, bus(PALETTE_TABLE));
        assert_eq!(
            (plan.sheet.addr, plan.sheet.encoding, plan.sheet.tile_count),
            (bus(SHEET), Encoding::Lz77, 12)
        );
        assert_eq!((plan.sheet.width, plan.sheet.height), (16, 48));
        assert_eq!(
            (
                plan.palette.addr,
                plan.palette.encoding,
                plan.palette.color_count
            ),
            (bus(PALETTE), Encoding::Lz77, 16)
        );
        assert_eq!(
            (
                plan.open_sheet.addr,
                plan.open_sheet.encoding,
                plan.open_sheet.tile_count
            ),
            (bus(OPEN_SHEET), Encoding::Lz77, 4)
        );
        assert_eq!((plan.open_sheet.width, plan.open_sheet.height), (16, 16));
        // Three assets and two tables.
        assert_eq!(report.len(), 5);
    });
}

#[test]
fn the_report_names_every_symbol_a_map_can_confirm() {
    use crate::gen_rom_profile::plan::SymbolExpectation::{Contains, Exact};
    let rom = fixture().finish();
    with_context("poke-ball-symbols", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let symbol = |id: &str| {
            report
                .iter()
                .find(|line| line.id == id)
                .unwrap_or_else(|| panic!("no report line for {id}"))
                .symbol
                .clone()
        };
        assert_eq!(
            symbol("poke-ball/image/poke"),
            Contains(vec!["gBallGfx_Poke".into()])
        );
        assert_eq!(
            symbol("poke-ball/image/open"),
            Contains(vec!["gOpenPokeballGfx".into()])
        );
        assert_eq!(
            symbol("poke-ball/palette/poke"),
            Exact("gBallPal_Poke".into())
        );
        assert_eq!(
            symbol("gBallSpriteSheets"),
            Exact("gBallSpriteSheets".into())
        );
        assert_eq!(
            symbol("gBallSpritePalettes"),
            Exact("gBallSpritePalettes".into())
        );
    });
}

#[test]
fn a_palette_stored_twice_takes_the_copy_its_table_row_names() {
    let rom = fixture()
        .write(STRAY_PALETTE, &lz77_literals(&color_bytes()))
        .finish();
    with_context("poke-ball-stray-palette", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        assert_eq!(plan.palette.addr, bus(PALETTE));
        let line = report
            .iter()
            .find(|line| line.id == "poke-ball/palette/poke")
            .expect("a report line");
        assert_eq!(line.resolution, Resolution::StructDerived);
    });
}

#[test]
fn a_sheet_row_with_the_wrong_size_or_tag_pins_no_table() {
    for row in [
        sheet_row(bus(SHEET), SHEET_BYTES + 32, POKE_BALL_TAG),
        sheet_row(bus(SHEET), SHEET_BYTES, POKE_BALL_TAG + 1),
    ] {
        let rom = fixture().write(SHEET_TABLE, &row).finish();
        with_context("poke-ball-bad-sheet-row", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "gBallSpriteSheets"),
                "{err}"
            );
        });
    }
}

#[test]
fn a_palette_row_with_the_wrong_tag_pins_no_table() {
    let rom = fixture()
        .write(PALETTE_TABLE, &palette_row(bus(PALETTE), POKE_BALL_TAG + 1))
        .finish();
    with_context("poke-ball-bad-palette-row", &rom, entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "gBallSpritePalettes"),
            "{err}"
        );
    });
}

#[test]
fn art_the_rom_lacks_is_not_found() {
    for (name, at, id) in [
        ("poke-ball-no-sheet", SHEET, "poke-ball/image/poke"),
        ("poke-ball-no-palette", PALETTE, "poke-ball/palette/poke"),
        ("poke-ball-no-open", OPEN_SHEET, "poke-ball/image/open"),
    ] {
        let rom = fixture().write(at, &[0xFF; 4]).finish();
        with_context(name, &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::NotFound { id: missing } if missing == id),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn a_duplicated_open_sheet_is_reported_as_an_arbitrary_copy() {
    let rom = fixture()
        .write(STRAY_SHEET, &lz77_literals(&open_tiles()))
        .finish();
    with_context("poke-ball-two-open", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        // Nothing but the tiles anchors the opened sheet, so a copy is reported as arbitrary.
        let line = report
            .iter()
            .find(|line| line.id == "poke-ball/image/open")
            .expect("a report line");
        assert_eq!(line.resolution, Resolution::ArbitraryAmongIdentical);
        assert_eq!(plan.open_sheet.addr, bus(STRAY_SHEET));
    });
}

#[test]
fn a_surplus_or_missing_entry_is_refused_before_any_report_line() {
    let rom = fixture().finish();
    let mut surplus = entries();
    surplus.push(raw_entry("poke-ball/raw/extra".into(), vec![0]));
    with_context("poke-ball-surplus", &rom, surplus, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "poke-ball/raw/extra"),
            "{err}"
        );
        assert!(report.is_empty());
    });

    for missing in poke_ball_ids() {
        let mut partial = entries();
        partial.retain(|entry| entry.id != missing);
        with_context("poke-ball-missing", &rom, partial, |ctx| {
            let mut report = Vec::new();
            let err = locate(ctx, &mut report).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::MissingPackEntry(id) if *id == missing),
                "{err}"
            );
            assert!(report.is_empty());
        });
    }
}
