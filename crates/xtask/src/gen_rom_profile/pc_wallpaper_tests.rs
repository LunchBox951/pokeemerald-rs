//! PC wallpaper locator tests over a synthetic ROM and pack.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::locate;
use crate::extract::scope::{pc_wallpaper_ids, PC_WALLPAPERS};
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::with_context;

const TILES: usize = 0x10_0000;
const TILEMAPS: usize = 0x20_0000;
const PALETTES: usize = 0x30_0000;
const TABLE: usize = 0x40_0000;
/// A second table, identical to the first, for the ambiguity case.
const OTHER_TABLE: usize = 0x41_0000;
/// Room for copies of one wallpaper's art below the real copy.
const STRAY: usize = 0x08_0000;
const RECORD: usize = 12;

fn bus(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("a fixture offset fits a bus address")
}

fn sheet_height(index: usize) -> u32 {
    let source = &PC_WALLPAPERS[index];
    (source.frame.kept_tiles + source.background.kept_tiles) * 8
}

fn sheet_tiles(index: usize) -> Vec<u8> {
    let tile_count = usize::try_from(sheet_height(index) / 8).expect("small");
    let seed = u8::try_from(index).expect("twelve wallpapers") * 7 + 1;
    (0..tile_count * 32)
        .map(|n| {
            let value = u32::try_from(n).expect("small") * 37 + u32::from(seed) * 11;
            u8::try_from(value % 251).expect("modulo 251 fits in u8") ^ seed
        })
        .collect()
}

fn tilemap(index: usize) -> Vec<u8> {
    (0..720usize)
        .map(|n| u8::try_from((n * 13 + index * 29 + 5) % 251).expect("fits"))
        .collect()
}

fn colors(index: usize, bank: u16) -> Vec<u16> {
    let index = u16::try_from(index).expect("small");
    (0..16u16)
        .map(|n| n * 0x0421 + 0x0100 * (index + 1) + bank * 0x0033)
        .collect()
}

fn palette_pair(index: usize) -> Vec<u8> {
    (0..2)
        .flat_map(|bank| colors(index, bank))
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn lz77(data: &[u8]) -> Vec<u8> {
    crate::gen_rom_profile::tests::lz77_literals(data)
}

fn row_of(index: usize) -> usize {
    usize::from(PC_WALLPAPERS[index].wallpaper.index()) * RECORD
}

fn record(tiles: usize, tilemap: usize, palettes: usize) -> Vec<u8> {
    [tiles, tilemap, palettes]
        .into_iter()
        .flat_map(|at| bus(at).to_le_bytes())
        .collect()
}

fn tiles_at(index: usize) -> usize {
    TILES + index * 0x1000
}

fn tilemap_at(index: usize) -> usize {
    TILEMAPS + index * 0x1000
}

fn palettes_at(index: usize) -> usize {
    PALETTES + index * 0x100
}

fn entries() -> Vec<PackEntry> {
    let mut entries = Vec::new();
    for (index, source) in PC_WALLPAPERS.iter().enumerate() {
        let id = source.wallpaper;
        entries.push(
            image_entry_from_tiles(
                id.tiles_id(),
                &sheet_tiles(index),
                4,
                8,
                sheet_height(index),
                None,
            )
            .expect("sheet"),
        );
        entries.push(raw_entry(id.tilemap_id(), tilemap(index)));
        entries.push(palette_entry(id.frame_palette_id(), &colors(index, 0)).expect("frame"));
        entries.push(
            palette_entry(id.background_palette_id(), &colors(index, 1)).expect("background"),
        );
    }
    entries
}

/// A ROM holding every wallpaper's art and a table row naming it.
fn fixture() -> RomFixture {
    let mut fixture = RomFixture::new().emerald_header();
    for index in 0..PC_WALLPAPERS.len() {
        fixture = fixture
            .write(tiles_at(index), &lz77(&sheet_tiles(index)))
            .write(tilemap_at(index), &lz77(&tilemap(index)))
            .write(palettes_at(index), &palette_pair(index))
            .write(
                TABLE + row_of(index),
                &record(tiles_at(index), tilemap_at(index), palettes_at(index)),
            );
    }
    fixture
}

fn line<'a>(report: &'a [ReportLine], id: &str) -> &'a ReportLine {
    report
        .iter()
        .find(|line| line.id == id)
        .unwrap_or_else(|| panic!("no report line for {id}"))
}

fn expect_refused(name: &str, rom: &[u8], check: impl FnOnce(&GenRomProfileError)) {
    with_context(name, rom, entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        check(&err);
    });
}

#[test]
fn the_fixed_inventory_is_four_entries_for_each_of_twelve_wallpapers() {
    let ids = pc_wallpaper_ids();
    assert_eq!(ids.len(), 48);
    assert_eq!(ids[0], "pc-wallpaper/crag/tiles");
    assert_eq!(ids[47], "pc-wallpaper/plain/palette/background");
}

#[test]
fn every_wallpaper_art_and_the_table_are_found() {
    let rom = fixture().finish();
    with_context("pc-wallpaper-found", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        assert_eq!(plan.table, bus(TABLE));
        assert_eq!(plan.wallpapers.len(), 12);
        for (index, paper) in plan.wallpapers.iter().enumerate() {
            let id = PC_WALLPAPERS[index].wallpaper;
            assert_eq!(usize::from(paper.wallpaper), 4 + index);
            assert_eq!(paper.tiles.id, id.tiles_id());
            assert_eq!(
                (paper.tiles.addr, paper.tiles.encoding),
                (bus(tiles_at(index)), Encoding::Lz77)
            );
            assert_eq!(
                (paper.tiles.width, paper.tiles.height),
                (8, sheet_height(index))
            );
            assert_eq!(paper.tiles.tile_count, sheet_height(index) / 8);
            assert_eq!(
                (
                    paper.tilemap.addr,
                    paper.tilemap.encoding,
                    paper.tilemap.len
                ),
                (bus(tilemap_at(index)), Encoding::Lz77, 720)
            );
            assert_eq!(paper.frame_palette.id, id.frame_palette_id());
            assert_eq!(
                (paper.frame_palette.addr, paper.frame_palette.encoding),
                (bus(palettes_at(index)), Encoding::Raw)
            );
            assert_eq!(paper.background_palette.id, id.background_palette_id());
            assert_eq!(paper.background_palette.addr, bus(palettes_at(index) + 32));
            assert_eq!(paper.background_palette.color_count, 16);
        }
        // The table, then four roots for each of twelve wallpapers.
        assert_eq!(report.len(), 1 + 48);
    });
}

#[test]
fn the_report_names_the_static_symbols_a_map_cannot_list() {
    let rom = fixture().finish();
    with_context("pc-wallpaper-symbols", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let local = |name: &str| SymbolExpectation::Local(name.to_owned());
        assert_eq!(line(&report, "sWallpapers").symbol, local("sWallpapers"));
        assert_eq!(
            line(&report, "pc-wallpaper/crag/tiles").symbol,
            local("sWallpaperTiles_Crag")
        );
        assert_eq!(
            line(&report, "pc-wallpaper/crag/tilemap").symbol,
            local("sWallpaperTilemap_Crag")
        );
        assert_eq!(
            line(&report, "pc-wallpaper/crag/palette/frame").symbol,
            local("sWallpaperPalettes_Crag")
        );
        assert_eq!(
            line(&report, "pc-wallpaper/crag/palette/background").symbol,
            SymbolExpectation::Interior
        );
        // Upstream spells this one with a capital D.
        assert_eq!(
            line(&report, "pc-wallpaper/polkadot/tiles").symbol,
            local("sWallpaperTiles_PolkaDot")
        );
        assert_eq!(
            line(&report, "pc-wallpaper/pokecenter/palette/frame").symbol,
            local("sWallpaperPalettes_Pokecenter")
        );
    });
}

#[test]
fn art_stored_twice_takes_the_copy_its_row_names() {
    let crag = 0;
    let rom = fixture()
        .write(STRAY, &lz77(&sheet_tiles(crag)))
        .write(STRAY + 0x1000, &lz77(&tilemap(crag)))
        .write(STRAY + 0x2000, &palette_pair(crag))
        .finish();
    with_context("pc-wallpaper-stray-copies", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let paper = &plan.wallpapers[crag];
        assert_eq!(paper.tiles.addr, bus(tiles_at(crag)));
        assert_eq!(paper.tilemap.addr, bus(tilemap_at(crag)));
        assert_eq!(paper.frame_palette.addr, bus(palettes_at(crag)));
        for id in [
            "pc-wallpaper/crag/tiles",
            "pc-wallpaper/crag/tilemap",
            "pc-wallpaper/crag/palette/frame",
        ] {
            assert_eq!(
                line(&report, id).resolution,
                Resolution::StructDerived,
                "{id}"
            );
        }
    });
}

#[test]
fn a_tilemap_or_palette_named_after_many_lower_copies_is_still_found() {
    let mut fixture = fixture();
    for copy in 0..20usize {
        fixture = fixture
            .write(STRAY + copy * 0x1000, &lz77(&tilemap(3)))
            .write(STRAY + 0x2_0000 + copy * 0x100, &palette_pair(3));
    }
    let rom = fixture.finish();
    with_context("pc-wallpaper-many-copies", &rom, entries(), |ctx| {
        let plan = locate(ctx, &mut Vec::new()).expect("located");
        assert_eq!(plan.wallpapers[3].tilemap.addr, bus(tilemap_at(3)));
        assert_eq!(plan.wallpapers[3].frame_palette.addr, bus(palettes_at(3)));
    });
}

#[test]
fn a_sheet_repeated_past_the_search_cap_is_refused_not_truncated() {
    let mut fixture = fixture();
    for copy in 0..17usize {
        fixture = fixture.write(STRAY + copy * 0x1000, &lz77(&sheet_tiles(5)));
    }
    let rom = fixture.finish();
    expect_refused("pc-wallpaper-capped-sheet", &rom, |err| {
        assert!(
            matches!(err, GenRomProfileError::Ambiguous { id, .. } if id == "pc-wallpaper/seafloor/tiles"),
            "{err}"
        );
    });
}

#[test]
fn art_the_rom_lacks_is_not_found() {
    for (name, id, wipe) in [
        ("sheet", "pc-wallpaper/cave/tiles", tiles_at(3)),
        ("tilemap", "pc-wallpaper/cave/tilemap", tilemap_at(3)),
        (
            "palettes",
            "pc-wallpaper/cave/palette/frame",
            palettes_at(3),
        ),
    ] {
        let rom = fixture().write(wipe, &[0xFF; 8]).finish();
        expect_refused(&format!("pc-wallpaper-no-{name}"), &rom, |err| {
            assert!(
                matches!(err, GenRomProfileError::NotFound { id: missing } if missing == id),
                "{name}: {err}"
            );
        });
    }
}

#[test]
fn a_malformed_or_truncated_stream_is_not_found() {
    let first_byte_wrong = [0x20, 0x00, 0x00, 0x00];
    let truncated = &lz77(&tilemap(0))[..100];
    for (name, id, at, bytes) in [
        (
            "wrong-type",
            "pc-wallpaper/crag/tilemap",
            tilemap_at(0),
            &first_byte_wrong[..],
        ),
        (
            "truncated",
            "pc-wallpaper/crag/tilemap",
            tilemap_at(0),
            truncated,
        ),
        (
            "sheet-type",
            "pc-wallpaper/crag/tiles",
            tiles_at(0),
            &first_byte_wrong[..],
        ),
    ] {
        let rom = fixture().write(at, &[0xFF; 1000]).write(at, bytes).finish();
        expect_refused(&format!("pc-wallpaper-{name}"), &rom, |err| {
            assert!(
                matches!(err, GenRomProfileError::NotFound { id: missing } if missing == id),
                "{name}: {err}"
            );
        });
    }
}

#[test]
fn a_background_palette_stored_apart_from_its_frame_is_not_found() {
    let rom = fixture()
        .write(palettes_at(2) + 32, &[0u8; 32])
        .write(palettes_at(2) + 0x80, &palette_pair(2)[32..])
        .finish();
    expect_refused("pc-wallpaper-apart-palettes", &rom, |err| {
        assert!(
            matches!(err, GenRomProfileError::NotFound { id } if id == "pc-wallpaper/snow/palette/frame"),
            "{err}"
        );
    });
}

#[test]
fn a_row_naming_the_wrong_art_pins_no_table() {
    let swapped = record(tiles_at(1), tilemap_at(0), palettes_at(0));
    let stray_map = record(tiles_at(0), tilemap_at(1), palettes_at(0));
    let stray_palettes = record(tiles_at(0), tilemap_at(0), palettes_at(1));
    for (name, row) in [
        ("tiles", swapped),
        ("tilemap", stray_map),
        ("palettes", stray_palettes),
    ] {
        let rom = fixture().write(TABLE + row_of(0), &row).finish();
        expect_refused(&format!("pc-wallpaper-bad-row-{name}"), &rom, |err| {
            assert!(
                matches!(err, GenRomProfileError::StructMismatch { id, .. } if id == "sWallpapers"),
                "{name}: {err}"
            );
        });
    }
}

#[test]
fn every_wallpaper_row_must_hold_not_just_the_first() {
    let rom = fixture()
        .write(
            TABLE + row_of(11),
            &record(tiles_at(10), tilemap_at(11), palettes_at(11)),
        )
        .finish();
    expect_refused("pc-wallpaper-bad-last-row", &rom, |err| {
        assert!(
            matches!(err, GenRomProfileError::StructMismatch { id, .. } if id == "sWallpapers"),
            "{err}"
        );
    });
}

#[test]
fn a_row_pointing_outside_the_cartridge_pins_no_table() {
    let rom = fixture()
        .write(TABLE + row_of(4) + 4, &0x0200_0000u32.to_le_bytes())
        .finish();
    expect_refused("pc-wallpaper-outside-pointer", &rom, |err| {
        assert!(
            matches!(err, GenRomProfileError::StructMismatch { id, .. } if id == "sWallpapers"),
            "{err}"
        );
    });
}

#[test]
fn a_table_running_past_the_rom_pins_no_table() {
    let end = 0x0100_0000;
    let table = end - 100;
    let mut fixture = fixture();
    fixture = fixture.write(TABLE + row_of(0), &[0; 12]);
    for index in 0..12 {
        let at = table + row_of(index);
        if at + RECORD <= end {
            fixture = fixture.write(
                at,
                &record(tiles_at(index), tilemap_at(index), palettes_at(index)),
            );
        }
    }
    let rom = fixture.finish();
    expect_refused("pc-wallpaper-truncated-table", &rom, |err| {
        assert!(
            matches!(err, GenRomProfileError::StructMismatch { id, .. } if id == "sWallpapers"),
            "{err}"
        );
    });
}

#[test]
fn two_tables_that_both_hold_are_refused_rather_than_guessed() {
    let mut fixture = fixture();
    for index in 0..12 {
        fixture = fixture.write(
            OTHER_TABLE + row_of(index),
            &record(tiles_at(index), tilemap_at(index), palettes_at(index)),
        );
    }
    let rom = fixture.finish();
    expect_refused("pc-wallpaper-two-tables", &rom, |err| {
        assert!(
            matches!(err, GenRomProfileError::StructMismatch { id, reason } if id == "sWallpapers" && reason.contains("2 candidates")),
            "{err}"
        );
    });
}

#[test]
fn a_surplus_or_missing_entry_is_refused_before_any_report_line() {
    let rom = fixture().finish();
    let mut surplus = entries();
    surplus.push(raw_entry("pc-wallpaper/forest/tiles".into(), vec![0]));
    with_context("pc-wallpaper-surplus", &rom, surplus, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "pc-wallpaper/forest/tiles"),
            "{err}"
        );
        assert!(report.is_empty());
    });

    for missing in pc_wallpaper_ids() {
        let mut partial = entries();
        partial.retain(|entry| entry.id != missing);
        with_context("pc-wallpaper-missing", &rom, partial, |ctx| {
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

#[test]
fn a_palette_that_is_not_sixteen_colours_is_refused() {
    let rom = fixture().finish();
    let mut short = entries();
    let frame = "pc-wallpaper/crag/palette/frame";
    let at = short
        .iter()
        .position(|entry| entry.id == frame)
        .expect("frame");
    short[at] = palette_entry(frame.to_owned(), &colors(0, 0)[..8]).expect("palette");
    with_context("pc-wallpaper-short-palette", &rom, short, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == frame),
            "{err}"
        );
    });
}
