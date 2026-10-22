//! Starter-chooser locator tests over a synthetic ROM and pack.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::locate;
use crate::extract::scope::starter_chooser_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::Resolution;
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

fn palette_bytes(seed: u16, colors: u16) -> (Vec<u16>, Vec<u8>) {
    let colors: Vec<u16> = (0..colors).map(|n| n * 0x0421 + seed).collect();
    let bytes = colors.iter().flat_map(|c| c.to_le_bytes()).collect();
    (colors, bytes)
}

fn sheet(id: &str, tiles: &[u8]) -> PackEntry {
    image_entry_from_tiles(id.to_owned(), tiles, 4, 16, 16, None).expect("sheet entry")
}

fn tilemap(seed: u8) -> Vec<u8> {
    (0..64u8)
        .map(|n| n.wrapping_mul(7).wrapping_add(seed))
        .collect()
}

/// A ROM and pack holding all eight chooser roots, one distinct copy each.
fn fixture() -> (Vec<u8>, Vec<PackEntry>) {
    let tiles = [
        distinctive_tiles(4),
        distinctive_tiles(4).iter().map(|b| b ^ 0x11).collect(),
        distinctive_tiles(4)
            .iter()
            .map(|b| b ^ 0x22)
            .collect::<Vec<u8>>(),
    ];
    let (tiles_colors, tiles_bytes) = palette_bytes(0x1111, 32);
    let (ball_colors, ball_bytes) = palette_bytes(0x2222, 16);
    let (circle_colors, circle_bytes) = palette_bytes(0x3333, 16);
    let (bag_map, grass_map) = (tilemap(3), tilemap(9));
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &lz77_literals(&tiles[0]))
        .write(0x11_0000, &lz77_literals(&tiles[1]))
        .write(0x12_0000, &lz77_literals(&tiles[2]))
        .write(0x13_0000, &tiles_bytes)
        .write(0x14_0000, &ball_bytes)
        .write(0x15_0000, &circle_bytes)
        .write(0x16_0000, &lz77_literals(&bag_map))
        .write(0x17_0000, &lz77_literals(&grass_map))
        .finish();
    let entries = vec![
        sheet("starter-chooser/image/tiles", &tiles[0]),
        sheet("starter-chooser/image/pokeball_selection", &tiles[1]),
        sheet("starter-chooser/image/starter_circle", &tiles[2]),
        palette_entry("starter-chooser/palette/tiles".into(), &tiles_colors).expect("palette"),
        palette_entry(
            "starter-chooser/palette/pokeball_selection".into(),
            &ball_colors,
        )
        .expect("palette"),
        palette_entry(
            "starter-chooser/palette/starter_circle".into(),
            &circle_colors,
        )
        .expect("palette"),
        raw_entry("starter-chooser/raw/birch_bag".into(), bag_map),
        raw_entry("starter-chooser/raw/birch_grass".into(), grass_map),
    ];
    (rom, entries)
}

#[test]
fn the_fixed_inventory_is_three_sheets_three_palettes_and_two_tilemaps() {
    let ids = starter_chooser_ids();
    let count = |prefix: &str| ids.iter().filter(|id| id.starts_with(prefix)).count();
    assert_eq!(ids.len(), 8);
    assert_eq!(count("starter-chooser/image/"), 3);
    assert_eq!(count("starter-chooser/palette/"), 3);
    assert_eq!(count("starter-chooser/raw/"), 2);
}

#[test]
fn every_chooser_family_is_located_with_its_own_encoding() {
    let (rom, entries) = fixture();
    with_context("chooser-families", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let images: Vec<_> = plan
            .images
            .iter()
            .map(|i| {
                (
                    i.id.as_str(),
                    i.addr,
                    i.encoding,
                    i.rom_bit_depth,
                    i.pack_bit_depth,
                )
            })
            .collect();
        assert_eq!(
            images,
            [
                (
                    "starter-chooser/image/pokeball_selection",
                    0x0811_0000,
                    Encoding::Lz77,
                    4,
                    4
                ),
                (
                    "starter-chooser/image/starter_circle",
                    0x0812_0000,
                    Encoding::Lz77,
                    4,
                    4
                ),
                (
                    "starter-chooser/image/tiles",
                    0x0810_0000,
                    Encoding::Lz77,
                    4,
                    4
                ),
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
                    "starter-chooser/palette/pokeball_selection",
                    0x0814_0000,
                    Encoding::Raw,
                    16
                ),
                (
                    "starter-chooser/palette/starter_circle",
                    0x0815_0000,
                    Encoding::Raw,
                    16
                ),
                (
                    "starter-chooser/palette/tiles",
                    0x0813_0000,
                    Encoding::Raw,
                    32
                ),
            ]
        );
        let tilemaps: Vec<_> = plan
            .tilemaps
            .iter()
            .map(|t| (t.id.as_str(), t.addr, t.encoding, t.len))
            .collect();
        assert_eq!(
            tilemaps,
            [
                (
                    "starter-chooser/raw/birch_bag",
                    0x0816_0000,
                    Encoding::Lz77,
                    64
                ),
                (
                    "starter-chooser/raw/birch_grass",
                    0x0817_0000,
                    Encoding::Lz77,
                    64
                ),
            ]
        );
        assert_eq!(report.len(), 8);
        assert!(report
            .iter()
            .all(|line| line.resolution == Resolution::UniqueSignature));
    });
}

#[test]
fn a_compressed_palette_is_not_a_raw_chooser_palette() {
    let (rom, entries) = fixture();
    let (_, tiles_bytes) = palette_bytes(0x1111, 32);
    let mut rom = rom;
    // Replace the raw tiles palette with its LZ77 form: the root is raw.
    rom[0x13_0000..0x13_0000 + tiles_bytes.len()].fill(0);
    let packed = lz77_literals(&tiles_bytes);
    rom[0x13_0000..0x13_0000 + packed.len()].copy_from_slice(&packed);
    with_context("chooser-palette-compressed", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::NotFound { .. }), "{err}");
    });
}

#[test]
fn a_tilemap_present_twice_is_ambiguous() {
    let (rom, entries) = fixture();
    let mut rom = rom;
    let packed = lz77_literals(&tilemap(3));
    rom[0x18_0000..0x18_0000 + packed.len()].copy_from_slice(&packed);
    with_context("chooser-tilemap-twice", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GenRomProfileError::Ambiguous { .. }), "{err}");
    });
}

#[test]
fn a_surplus_or_missing_chooser_entry_is_refused_before_any_report_line() {
    let (rom, mut entries) = fixture();
    entries.push(raw_entry("starter-chooser/raw/extra".into(), vec![0]));
    with_context("chooser-surplus", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "starter-chooser/raw/extra"),
            "{err}"
        );
        assert!(report.is_empty());
    });

    let (rom, mut entries) = fixture();
    entries.retain(|entry| entry.id != "starter-chooser/raw/birch_bag");
    with_context("chooser-missing", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == "starter-chooser/raw/birch_bag"),
            "{err}"
        );
        assert!(report.is_empty());
    });
}

#[test]
fn a_wrong_kind_chooser_entry_is_refused() {
    let (rom, mut entries) = fixture();
    entries.retain(|entry| entry.id != "starter-chooser/palette/tiles");
    entries.push(raw_entry(
        "starter-chooser/palette/tiles".into(),
        vec![0; 64],
    ));
    with_context("chooser-wrong-kind", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(err, GenRomProfileError::WrongPackEntryKind { .. }),
            "{err}"
        );
    });
}
