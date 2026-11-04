//! Battle-environment locator tests over a synthetic ROM and pack.

use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::{locate, RECORD_BYTES};
use crate::extract::scope::battle_environment_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{distinctive_tiles, lz77_literals, with_context};

const TABLE: usize = 0x10_0000;
/// Where a second copy of a stream lands, below the real ones.
const STRAY: usize = 0x08_0000;
const GRASS_BASE: usize = 0x20_0000;
const BUILDING_BASE: usize = 0x30_0000;
/// Members sit this far apart, in `BattleBackground` order.
const STEP: usize = 0x1_0000;

const SHEET_IDS: [&str; 2] = ["tiles", "anim_tiles"];

fn bus(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("a fixture offset fits a bus address")
}

/// The decoded bytes of member `k` (0 tiles, 1 map, 2 entry tiles, 3 entry
/// map, 4 palette) of the record whose seed is `seed`.
fn payload(seed: u8, k: usize) -> Vec<u8> {
    match k {
        0 | 2 => distinctive_tiles(256)
            .iter()
            .map(|byte| byte ^ seed ^ u8::try_from(k).unwrap())
            .collect(),
        1 => (0..4096usize)
            .map(|i| u8::try_from(i % 249).unwrap().wrapping_add(seed))
            .collect(),
        3 => (0..2048usize)
            .map(|i| u8::try_from(i % 247).unwrap().wrapping_add(seed))
            .collect(),
        _ => (0..48u16)
            .flat_map(|n| (n * 0x0421 + u16::from(seed)).to_le_bytes())
            .collect(),
    }
}

fn grass_payload(k: usize) -> Vec<u8> {
    payload(1, k)
}

fn building_payload(k: usize) -> Vec<u8> {
    payload(2, k)
}

/// Plain's palette is its own, not the building environment's.
fn plain_palette() -> Vec<u8> {
    payload(3, 4)
}

fn row(base: usize) -> Vec<u8> {
    (0..5)
        .flat_map(|k| bus(base + k * STEP).to_le_bytes())
        .collect()
}

fn plain_row() -> Vec<u8> {
    let mut row = row(BUILDING_BASE);
    row[16..20].copy_from_slice(&bus(BUILDING_BASE + 4 * STEP).to_le_bytes());
    row
}

fn entries() -> Vec<PackEntry> {
    let sheet = |id: &str, bytes: &[u8]| {
        image_entry_from_tiles(id.to_owned(), bytes, 4, 128, 128, None).expect("sheet")
    };
    let colors = |bytes: &[u8]| -> Vec<u16> {
        bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect()
    };
    let mut entries = Vec::new();
    for (dir, payload) in [
        ("tall_grass", grass_payload as fn(usize) -> Vec<u8>),
        ("building", building_payload),
    ] {
        for (id, k) in SHEET_IDS.iter().zip([0, 2]) {
            entries.push(sheet(
                &format!("battle-environment/image/{dir}/{id}"),
                &payload(k),
            ));
        }
        entries.push(raw_entry(
            format!("battle-environment/raw/{dir}/map"),
            payload(1),
        ));
        entries.push(raw_entry(
            format!("battle-environment/raw/{dir}/anim_map"),
            payload(3),
        ));
    }
    entries.push(
        palette_entry(
            "battle-environment/palette/tall_grass".to_owned(),
            &colors(&grass_payload(4)),
        )
        .expect("palette"),
    );
    entries.push(
        palette_entry(
            "battle-environment/palette/plain".to_owned(),
            &colors(&plain_palette()),
        )
        .expect("palette"),
    );
    entries
}

/// A ROM holding the ten streams and a table whose rows 0 and 9 name them.
fn fixture() -> RomFixture {
    let mut fixture = RomFixture::new()
        .emerald_header()
        .write(TABLE, &row(GRASS_BASE))
        .write(TABLE + 9 * RECORD_BYTES as usize, &plain_row());
    for k in 0..5 {
        fixture = fixture.write(GRASS_BASE + k * STEP, &lz77_literals(&grass_payload(k)));
    }
    for k in 0..4 {
        fixture = fixture.write(
            BUILDING_BASE + k * STEP,
            &lz77_literals(&building_payload(k)),
        );
    }
    fixture.write(BUILDING_BASE + 4 * STEP, &lz77_literals(&plain_palette()))
}

fn symbol(report: &[ReportLine], id: &str) -> SymbolExpectation {
    report
        .iter()
        .find(|line| line.id == id)
        .unwrap_or_else(|| panic!("no report line for {id}"))
        .symbol
        .clone()
}

#[test]
fn the_fixed_inventory_is_four_sheets_two_palettes_and_four_tilemaps() {
    let ids = battle_environment_ids();
    let count = |kind: &str| {
        ids.iter()
            .filter(|id| id.starts_with(&format!("battle-environment/{kind}/")))
            .count()
    };
    assert_eq!(
        (count("image"), count("palette"), count("raw"), ids.len()),
        (4, 2, 4, 10)
    );
}

#[test]
fn every_member_and_the_table_are_found_with_their_packing() {
    let rom = fixture().finish();
    with_context("battle-environment-found", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");

        assert_eq!(plan.table, bus(TABLE));
        for (record, base, palette_at) in [
            (&plan.tall_grass, GRASS_BASE, GRASS_BASE + 4 * STEP),
            (&plan.plain, BUILDING_BASE, BUILDING_BASE + 4 * STEP),
        ] {
            for (sheet, at) in [
                (&record.tiles, base),
                (&record.entry_tiles, base + 2 * STEP),
            ] {
                assert_eq!(sheet.addr, bus(at));
                assert_eq!(sheet.encoding, Encoding::Lz77);
                // Upstream's default 1x1 packing, the whole 256-tile sheet.
                assert_eq!(
                    (sheet.rom_bit_depth, sheet.pack_bit_depth, sheet.tile_count),
                    (4, 4, 256)
                );
                assert_eq!(
                    (
                        sheet.width,
                        sheet.height,
                        sheet.metatile_width,
                        sheet.metatile_height
                    ),
                    (128, 128, 1, 1)
                );
            }
            assert_eq!(
                (
                    record.tilemap.addr,
                    record.tilemap.len,
                    record.tilemap.encoding
                ),
                (bus(base + STEP), 4096, Encoding::Lz77)
            );
            assert_eq!(
                (
                    record.entry_tilemap.addr,
                    record.entry_tilemap.len,
                    record.entry_tilemap.encoding
                ),
                (bus(base + 3 * STEP), 2048, Encoding::Lz77)
            );
            assert_eq!(
                (
                    record.palette.addr,
                    record.palette.color_count,
                    record.palette.encoding
                ),
                (bus(palette_at), 48, Encoding::Lz77)
            );
        }
        // Ten members and the table.
        assert_eq!(report.len(), 11);
    });
}

#[test]
fn the_report_names_every_symbol_a_map_can_confirm() {
    use SymbolExpectation::{Contains, Exact, Local};
    let rom = fixture().finish();
    with_context("battle-environment-symbols", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).expect("located");
        let image = |id: &str, name: &str| {
            assert_eq!(
                symbol(&report, &format!("battle-environment/image/{id}")),
                Contains(vec![name.into()])
            );
        };
        image("tall_grass/tiles", "gBattleEnvironmentTiles_TallGrass");
        image(
            "tall_grass/anim_tiles",
            "gBattleEnvironmentAnimTiles_TallGrass",
        );
        image("building/tiles", "gBattleEnvironmentTiles_Building");
        image(
            "building/anim_tiles",
            "gBattleEnvironmentAnimTiles_Building",
        );
        for (id, name) in [
            ("palette/tall_grass", "gBattleEnvironmentPalette_TallGrass"),
            ("palette/plain", "gBattleEnvironmentPalette_Plain"),
            ("raw/tall_grass/map", "gBattleEnvironmentTilemap_TallGrass"),
            (
                "raw/tall_grass/anim_map",
                "gBattleEnvironmentAnimTilemap_TallGrass",
            ),
            ("raw/building/map", "gBattleEnvironmentTilemap_Building"),
            (
                "raw/building/anim_map",
                "gBattleEnvironmentAnimTilemap_Building",
            ),
        ] {
            assert_eq!(
                symbol(&report, &format!("battle-environment/{id}")),
                Exact(name.into())
            );
        }
        // `sBattleEnvironmentTable` is `static`, so a linker map cannot list it.
        assert_eq!(
            symbol(&report, "sBattleEnvironmentTable"),
            Local("sBattleEnvironmentTable".into())
        );
        let table = report
            .iter()
            .find(|line| line.id == "sBattleEnvironmentTable")
            .unwrap();
        assert_eq!(table.resolution, Resolution::StructDerived);
    });
}

#[test]
fn a_sheet_or_palette_stored_twice_is_refused_rather_than_guessed() {
    for (id, k) in [
        ("battle-environment/image/tall_grass/tiles", 0),
        ("battle-environment/palette/tall_grass", 4),
    ] {
        let rom = fixture()
            .write(STRAY, &lz77_literals(&grass_payload(k)))
            .finish();
        with_context("battle-environment-stray", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::Ambiguous { id: named, .. } if named == id),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn a_tilemap_stored_twice_takes_the_copy_its_table_row_names() {
    // The environments really do share identical tilemaps; below the real
    // streams and above them, only the row says which one is this record's.
    let stray_map = 0x40_0000;
    let rom = fixture()
        .write(STRAY, &lz77_literals(&grass_payload(1)))
        .write(stray_map, &lz77_literals(&grass_payload(3)))
        .finish();
    with_context("battle-environment-stray-map", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        assert_eq!(plan.tall_grass.tilemap.addr, bus(GRASS_BASE + STEP));
        assert_eq!(
            plan.tall_grass.entry_tilemap.addr,
            bus(GRASS_BASE + 3 * STEP)
        );
        for id in [
            "battle-environment/raw/tall_grass/map",
            "battle-environment/raw/tall_grass/anim_map",
        ] {
            let line = report.iter().find(|line| line.id == id).expect(id);
            assert_eq!(line.resolution, Resolution::StructDerived, "{id}");
        }
    });
}

#[test]
fn a_tilemap_copy_the_rows_do_not_name_cannot_stand_in_for_a_missing_one() {
    // The real grass map is gone and only a stray copy remains: no row names it.
    let rom = fixture()
        .write(GRASS_BASE + STEP, &[0xFF; 4])
        .write(STRAY, &lz77_literals(&grass_payload(1)))
        .finish();
    with_context("battle-environment-orphan-map", &rom, entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "sBattleEnvironmentTable"),
            "{err}"
        );
    });
}

#[test]
fn a_table_missing_either_row_pins_nothing() {
    let wrong_rows = [
        // Plain's row at index 8 instead of 9.
        fixture()
            .write(TABLE + 9 * RECORD_BYTES as usize, &[0; 20])
            .write(TABLE + 8 * RECORD_BYTES as usize, &plain_row()),
        // Plain's row naming the grass palette.
        fixture().write(
            TABLE + 9 * RECORD_BYTES as usize + 16,
            &bus(GRASS_BASE + 4 * STEP).to_le_bytes(),
        ),
        // Reversed main and entry tilemaps in the grass row.
        fixture().write(
            TABLE + 4,
            &[
                bus(GRASS_BASE + 3 * STEP).to_le_bytes(),
                bus(GRASS_BASE + 2 * STEP).to_le_bytes(),
                bus(GRASS_BASE + STEP).to_le_bytes(),
            ]
            .concat(),
        ),
        // Only the grass row is present.
        fixture().write(TABLE + 9 * RECORD_BYTES as usize, &[0; 20]),
    ];
    for (index, fixture) in wrong_rows.into_iter().enumerate() {
        let rom = fixture.finish();
        with_context("battle-environment-bad-table", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "sBattleEnvironmentTable"),
                "case {index}: {err}"
            );
        });
    }
}

#[test]
fn two_complete_tables_are_ambiguous() {
    let second = 0x18_0000;
    let rom = fixture()
        .write(second, &row(GRASS_BASE))
        .write(second + 9 * RECORD_BYTES as usize, &plain_row())
        .finish();
    with_context("battle-environment-two-tables", &rom, entries(), |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, reason } if id == "sBattleEnvironmentTable" && reason.contains("2 candidates")),
            "{err}"
        );
    });
}

#[test]
fn a_stray_decoy_row_does_not_disturb_the_table() {
    // A grass-only row elsewhere refers to the same tiles but is not a table.
    let rom = fixture().write(0x18_0000, &row(GRASS_BASE)).finish();
    with_context("battle-environment-decoy", &rom, entries(), |ctx| {
        let plan = locate(ctx, &mut Vec::new()).expect("located");
        assert_eq!(plan.table, bus(TABLE));
    });
}

#[test]
fn art_the_rom_lacks_is_not_found() {
    let streams = [
        ("battle-environment/image/tall_grass/tiles", GRASS_BASE),
        ("battle-environment/raw/tall_grass/map", GRASS_BASE + STEP),
        (
            "battle-environment/image/tall_grass/anim_tiles",
            GRASS_BASE + 2 * STEP,
        ),
        (
            "battle-environment/raw/tall_grass/anim_map",
            GRASS_BASE + 3 * STEP,
        ),
        (
            "battle-environment/palette/tall_grass",
            GRASS_BASE + 4 * STEP,
        ),
        ("battle-environment/image/building/tiles", BUILDING_BASE),
        ("battle-environment/raw/building/map", BUILDING_BASE + STEP),
        (
            "battle-environment/image/building/anim_tiles",
            BUILDING_BASE + 2 * STEP,
        ),
        (
            "battle-environment/raw/building/anim_map",
            BUILDING_BASE + 3 * STEP,
        ),
        ("battle-environment/palette/plain", BUILDING_BASE + 4 * STEP),
    ];
    for (id, at) in streams {
        let rom = fixture().write(at, &[0xFF; 4]).finish();
        with_context("battle-environment-missing-art", &rom, entries(), |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::NotFound { id: missing } if missing == id),
                "{id}: {err}"
            );
        });
    }
}

#[test]
fn a_surplus_or_missing_entry_is_refused_before_any_report_line() {
    let rom = fixture().finish();
    let mut surplus = entries();
    surplus.push(raw_entry("battle-environment/raw/extra".into(), vec![0]));
    with_context("battle-environment-surplus", &rom, surplus, |ctx| {
        let mut report = Vec::new();
        let err = locate(ctx, &mut report).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "battle-environment/raw/extra"),
            "{err}"
        );
        assert!(report.is_empty());
    });

    for missing in battle_environment_ids() {
        let mut partial = entries();
        partial.retain(|entry| entry.id != missing);
        with_context("battle-environment-absent", &rom, partial, |ctx| {
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
fn a_sheet_matching_only_under_a_real_metatile_walk_is_refused() {
    let id = "battle-environment/image/tall_grass/tiles";
    let mut metatiled = entries();
    metatiled.retain(|entry| entry.id != id);
    metatiled.push(
        image_entry_from_tiles(id.to_owned(), &grass_payload(0), 4, 128, 128, Some((2, 2)))
            .expect("sheet"),
    );
    let rom = fixture().finish();
    with_context("battle-environment-metatile", &rom, metatiled, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id: named, reason } if named == id && reason.contains("2x2")),
            "{err}"
        );
    });
}
