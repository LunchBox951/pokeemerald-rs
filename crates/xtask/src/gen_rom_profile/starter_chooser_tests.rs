//! Starter-chooser locator tests over a synthetic ROM and pack.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::{
    locate, POKEBALL_SELECT_TAG, SHEET_SIZE, SHEET_SIZE_FIELD, SHEET_TAG_FIELD, STARTER_CIRCLE_TAG,
};
use crate::extract::scope::starter_chooser_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{
    cross_check_ld_map, distinctive_tiles, lz77_literals, with_context,
};

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

const POKEBALL_SHEET: &str = "starter-chooser/image/pokeball_selection";
const CIRCLE_SHEET: &str = "starter-chooser/image/starter_circle";
const POKEBALL_ADDR: u32 = 0x0811_0000;
const CIRCLE_ADDR: u32 = 0x0812_0000;
/// Where the fixture writes the two records that name the sheets.
const POKEBALL_RECORD: usize = 0x19_0000;
const CIRCLE_RECORD: usize = 0x19_0100;

/// A `CompressedSpriteSheet` row `{data, size, tag}` followed by the empty
/// terminator row.
fn sheet_record(data: u32, size: u16, tag: u16) -> Vec<u8> {
    let mut row = data.to_le_bytes().to_vec();
    row.extend_from_slice(&size.to_le_bytes());
    row.extend_from_slice(&tag.to_le_bytes());
    row.extend_from_slice(&[0; 8]);
    row
}

/// A ROM and pack holding all eight chooser roots, one distinct copy each,
/// and the records upstream names the two sprite sheets by.
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
        .write(
            POKEBALL_RECORD,
            &sheet_record(POKEBALL_ADDR, SHEET_SIZE, POKEBALL_SELECT_TAG),
        )
        .write(
            CIRCLE_RECORD,
            &sheet_record(CIRCLE_ADDR, SHEET_SIZE, STARTER_CIRCLE_TAG),
        )
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

/// The chooser roots upstream declares `static`, with their symbols
/// (`starter_choose.c:55-56,61`).
const STATIC_ROOTS: [(&str, &str); 3] = [
    ("starter-chooser/image/starter_circle", "sStarterCircle_Gfx"),
    (
        "starter-chooser/palette/pokeball_selection",
        "sPokeballSelection_Pal",
    ),
    (
        "starter-chooser/palette/starter_circle",
        "sStarterCircle_Pal",
    ),
];

/// Every other chooser root is global (`starter_choose.c:54,57-60`).
const GLOBAL_ROOTS: [&str; 5] = [
    "starter-chooser/image/tiles",
    "starter-chooser/image/pokeball_selection",
    "starter-chooser/palette/tiles",
    "starter-chooser/raw/birch_bag",
    "starter-chooser/raw/birch_grass",
];

#[test]
fn static_chooser_roots_survive_a_real_ld_map() {
    let (rom, entries) = fixture();
    with_context("chooser-static-map", &rom, entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        assert_eq!(report.len(), 8);
        for (id, symbol) in STATIC_ROOTS {
            let line = report.iter().find(|line| line.id == id).expect(id);
            assert_eq!(
                line.symbol,
                SymbolExpectation::Local(symbol.to_owned()),
                "{id}"
            );
        }
        for id in GLOBAL_ROOTS {
            let line = report.iter().find(|line| line.id == id).expect(id);
            assert_eq!(line.symbol, SymbolExpectation::Unnamed, "{id}");
        }
        let check = cross_check_ld_map("chooser-static-map", &mut report, &[])
            .expect("a correct ROM and an ordinary ld map must pass");
        assert_eq!((check.local, check.confirmed), (3, 5));
    });
}

#[test]
fn a_global_chooser_root_still_needs_its_map_symbol() {
    for id in GLOBAL_ROOTS {
        let (rom, entries) = fixture();
        with_context("chooser-global-map", &rom, entries, |ctx| {
            let mut report = Vec::new();
            locate(ctx, &mut report).expect("located");
            let err = cross_check_ld_map("chooser-global-map", &mut report, &[id])
                .expect_err("a global root missing from the map is a mismatch");
            assert!(
                matches!(&err, GenRomProfileError::MapMismatch { id: bad, .. } if bad == id),
                "{id}: {err}"
            );
        });
    }
}

/// A fixture with an identical stray copy of `id`'s sheet at `stray`, a
/// lower file offset than the copy the record names.
fn with_stray(id: &str, stray: usize) -> (Vec<u8>, Vec<PackEntry>) {
    let (mut rom, entries) = fixture();
    let xor = if id == POKEBALL_SHEET { 0x11 } else { 0x22 };
    let art: Vec<u8> = distinctive_tiles(4).iter().map(|b| b ^ xor).collect();
    let packed = lz77_literals(&art);
    rom[stray..stray + packed.len()].copy_from_slice(&packed);
    (rom, entries)
}

fn image_addr(plan: &super::StarterChooserPlan, id: &str) -> u32 {
    plan.images.iter().find(|i| i.id == id).expect(id).addr
}

#[test]
fn a_lower_identical_stray_does_not_displace_the_sheet_record_copy() {
    let (rom, entries) = fixture();
    let mut rom = rom;
    let ball: Vec<u8> = distinctive_tiles(4).iter().map(|b| b ^ 0x11).collect();
    // A stray identical copy below the one the sheet record names.
    let packed = lz77_literals(&ball);
    rom[0x0F_0000..0x0F_0000 + packed.len()].copy_from_slice(&packed);
    // sSpriteSheet_PokeballSelect[0]: {data = 0x08110000, size = 0x0800, tag = 0x1000}.
    let mut record = Vec::new();
    record.extend_from_slice(&0x0811_0000u32.to_le_bytes());
    record.extend_from_slice(&0x0800u16.to_le_bytes());
    record.extend_from_slice(&0x1000u16.to_le_bytes());
    record.extend_from_slice(&[0; 8]);
    rom[0x18_0000..0x18_0000 + record.len()].copy_from_slice(&record);
    with_context("chooser-sheet-record", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let ball = plan
            .images
            .iter()
            .find(|i| i.id == "starter-chooser/image/pokeball_selection")
            .expect("ball sheet");
        let line = report
            .iter()
            .find(|l| l.id == "starter-chooser/image/pokeball_selection")
            .expect("ball line");
        eprintln!("chosen addr {:#x}, resolution {:?}", ball.addr, line.resolution);
        assert_eq!(ball.addr, 0x0811_0000, "the record names this copy");
        assert_ne!(line.resolution, Resolution::ArbitraryAmongIdentical);
    });
}

#[test]
fn each_sheet_resolves_to_its_record_copy_over_a_lower_stray() {
    for (id, addr) in [(POKEBALL_SHEET, POKEBALL_ADDR), (CIRCLE_SHEET, CIRCLE_ADDR)] {
        let (rom, entries) = with_stray(id, 0x0F_0000);
        with_context("chooser-sheet-stray", &rom, entries, |ctx| {
            let mut report = Vec::new();
            let plan = locate(ctx, &mut report).expect("located");
            assert_eq!(image_addr(&plan, id), addr, "{id}");
            let line = report.iter().find(|l| l.id == id).expect(id);
            assert_eq!(line.resolution, Resolution::StructDerived, "{id}");
            assert_eq!(line.addr, addr, "{id}");
        });
    }
}

#[test]
fn a_record_naming_the_lower_copy_picks_it_and_the_other_stays_unchosen() {
    // The record, not address order, decides: here it names the low copy.
    let (mut rom, entries) = with_stray(POKEBALL_SHEET, 0x0F_0000);
    rom[POKEBALL_RECORD..POKEBALL_RECORD + 16].copy_from_slice(&sheet_record(
        0x080F_0000,
        SHEET_SIZE,
        POKEBALL_SELECT_TAG,
    ));
    with_context("chooser-sheet-low", &rom, entries, |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        assert_eq!(image_addr(&plan, POKEBALL_SHEET), 0x080F_0000);
        let line = report.iter().find(|l| l.id == POKEBALL_SHEET).unwrap();
        assert_eq!(line.resolution, Resolution::StructDerived);
    });
}

#[test]
fn a_record_with_the_wrong_size_or_tag_names_nothing() {
    for (id, record_at, addr, tag, other_tag) in [
        (
            POKEBALL_SHEET,
            POKEBALL_RECORD,
            POKEBALL_ADDR,
            POKEBALL_SELECT_TAG,
            STARTER_CIRCLE_TAG,
        ),
        (
            CIRCLE_SHEET,
            CIRCLE_RECORD,
            CIRCLE_ADDR,
            STARTER_CIRCLE_TAG,
            POKEBALL_SELECT_TAG,
        ),
    ] {
        for (label, decoy) in [
            ("wrong size", sheet_record(addr, SHEET_SIZE + 0x20, tag)),
            ("wrong tag", sheet_record(addr, SHEET_SIZE, tag + 2)),
            ("the other sheet's tag", sheet_record(addr, SHEET_SIZE, other_tag)),
            ("no record", vec![0; 16]),
        ] {
            let (mut rom, entries) = fixture();
            rom[record_at..record_at + decoy.len()].copy_from_slice(&decoy);
            with_context("chooser-sheet-wrong", &rom, entries, |ctx| {
                let err = locate(ctx, &mut Vec::new()).unwrap_err();
                assert!(
                    matches!(&err, GenRomProfileError::StructMismatch { id: bad, .. } if bad == id),
                    "{id} with {label}: {err}"
                );
            });
        }
    }
}

#[test]
fn a_size_and_tag_at_the_wrong_offsets_name_nothing() {
    let mut shifted = POKEBALL_ADDR.to_le_bytes().to_vec();
    shifted.extend_from_slice(&[0, 0]);
    shifted.extend_from_slice(&SHEET_SIZE.to_le_bytes());
    shifted.extend_from_slice(&POKEBALL_SELECT_TAG.to_le_bytes());
    let (mut rom, entries) = fixture();
    rom[POKEBALL_RECORD..POKEBALL_RECORD + shifted.len()].copy_from_slice(&shifted);
    with_context("chooser-sheet-shifted", &rom, entries, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == POKEBALL_SHEET),
            "{err}"
        );
    });
}

#[test]
fn two_records_that_name_different_copies_settle_nothing() {
    for (id, addr_tag) in [
        (POKEBALL_SHEET, POKEBALL_SELECT_TAG),
        (CIRCLE_SHEET, STARTER_CIRCLE_TAG),
    ] {
        let (mut rom, entries) = with_stray(id, 0x0F_0000);
        let second = sheet_record(0x080F_0000, SHEET_SIZE, addr_tag);
        rom[0x1A_0000..0x1A_0000 + second.len()].copy_from_slice(&second);
        with_context("chooser-sheet-disagree", &rom, entries, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::StructMismatch { id: bad, .. } if bad == id),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn the_sheet_record_layout_is_the_one_upstream_writes() {
    // `starter_choose.c:33-34,284-302`; `struct CompressedSpriteSheet`.
    assert_eq!((POKEBALL_SELECT_TAG, STARTER_CIRCLE_TAG), (0x1000, 0x1001));
    assert_eq!(SHEET_SIZE, 0x0800);
    assert_eq!((SHEET_SIZE_FIELD, SHEET_TAG_FIELD), (4, 6));
}
