//! Door-animation locator tests over a synthetic ROM and pack.
//!
//! The locator's own logic is the table: it is found by the pointers to its
//! first row's sheet and accepted only when every row, the sheets and bank
//! arrays they point at, and the terminator agree with the pack's table. So
//! the cases are a well-formed ROM and, per way a table can fail to pin, a ROM
//! that breaks it.

use assets::DoorAnimationTable;
use pack_format::{image_entry_from_tiles, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::Encoding;

use super::{locate, packing, ROW_BYTES};
use crate::extract::door_animation::records;
use crate::extract::scope::door_animation_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{DoorAnimationPlan, Resolution, SymbolExpectation};
use crate::gen_rom_profile::tests::{distinctive_tiles, with_context};

const SHEETS: usize = 0x10_0000;
const SHEET_STRIDE: usize = 0x800;
const BANKS: usize = 0x20_0000;
const TABLE: usize = 0x30_0000;
/// A second copy of the table, further up the ROM.
const SECOND_TABLE: usize = 0x38_0000;
/// A second copy of the first sheet, below the one the table names.
const STRAY_SHEET: usize = 0x0E_0000;

fn bus(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("a fixture offset fits a bus address")
}

/// The stems in the order the table first names them.
fn stems() -> Vec<String> {
    let mut stems: Vec<String> = Vec::new();
    for record in records() {
        if !stems.contains(&record.sheet) {
            stems.push(record.sheet);
        }
    }
    stems
}

fn sheet_offset(stem: &str) -> usize {
    SHEETS + stems().iter().position(|s| s == stem).unwrap() * SHEET_STRIDE
}

fn sheet_height(stem: &str) -> u32 {
    if stem.starts_with("battle_tent") {
        128
    } else {
        96
    }
}

fn sheet_width(stem: &str) -> u32 {
    if packing(stem) == (2, 4) {
        32
    } else {
        16
    }
}

/// The tiles the ROM stores for `stem`, distinct for every sheet.
fn sheet_tiles(stem: &str) -> Vec<u8> {
    let tile_count = (sheet_width(stem) / 8 * (sheet_height(stem) / 8)) as usize;
    let salt = u8::try_from(stems().iter().position(|s| s == stem).unwrap() + 1).unwrap();
    distinctive_tiles(tile_count)
        .into_iter()
        .map(|byte| byte ^ salt)
        .collect()
}

fn entries() -> Vec<PackEntry> {
    let mut entries: Vec<PackEntry> = stems()
        .iter()
        .map(|stem| {
            image_entry_from_tiles(
                format!("door-animation/image/{stem}"),
                &sheet_tiles(stem),
                4,
                sheet_width(stem),
                sheet_height(stem),
                Some(packing(stem)),
            )
            .expect("a sheet")
        })
        .collect();
    let table = DoorAnimationTable::new(records()).unwrap();
    entries.push(raw_entry("door-animation/table".to_owned(), table.encode()));
    entries
}

fn row_bytes(index: usize, tiles: u32, banks: u32) -> Vec<u8> {
    let record = &records()[index];
    let mut row = record.metatile.to_le_bytes().to_vec();
    row.extend_from_slice(&[record.sound.tag(), record.size.tag()]);
    row.extend_from_slice(&tiles.to_le_bytes());
    row.extend_from_slice(&banks.to_le_bytes());
    row
}

/// The table at `base`, every row pointing at its own sheet and its own bank
/// array, with the terminator behind it.
fn table_at(mut fixture: RomFixture, base: usize) -> RomFixture {
    let mut table = Vec::new();
    for (index, record) in records().iter().enumerate() {
        let banks = BANKS + index * 0x10;
        fixture = fixture.write(banks, &record.palette_slots);
        table.extend(row_bytes(
            index,
            bus(sheet_offset(&record.sheet)),
            bus(banks),
        ));
    }
    table.extend([0; ROW_BYTES as usize]);
    fixture.write(base, &table)
}

fn fixture() -> RomFixture {
    let mut fixture = RomFixture::new().emerald_header();
    for stem in stems() {
        fixture = fixture.write(sheet_offset(&stem), &sheet_tiles(&stem));
    }
    table_at(fixture, TABLE)
}

fn located(name: &str, rom: &[u8]) -> Result<DoorAnimationPlan, GenRomProfileError> {
    with_context(name, rom, entries(), |ctx| locate(ctx, &mut Vec::new()))
}

fn assert_table_not_pinned(result: Result<DoorAnimationPlan, GenRomProfileError>) {
    match result {
        Err(GenRomProfileError::StructMismatch { id, .. }) => {
            assert_eq!(id, "door-animation/table");
        }
        other => panic!("expected the table to be unpinned, got {other:?}"),
    }
}

#[test]
fn the_fixed_inventory_is_every_referenced_sheet_and_the_table() {
    let ids = door_animation_ids();
    assert_eq!(ids.len(), 52);
    assert!(ids.contains(&"door-animation/table".to_owned()));
    assert!(ids.contains(&"door-animation/image/littleroot".to_owned()));
}

#[test]
fn the_table_its_rows_and_every_sheet_are_found() {
    let rom = fixture().finish();
    with_context("door-animation-found", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");

        assert_eq!(plan.table, bus(TABLE));
        assert_eq!(plan.rows.len(), 53);
        assert_eq!(plan.images.len(), 51);
        for (index, (row, record)) in plan.rows.iter().zip(records()).enumerate() {
            assert_eq!(row.metatile, record.metatile);
            assert_eq!(row.sheet, record.sheet);
            // Each row's own array, though several hold identical banks.
            assert_eq!(row.palette_slots, bus(BANKS + index * 0x10));
        }
        let littleroot = plan
            .images
            .iter()
            .find(|image| image.id == "door-animation/image/littleroot")
            .expect("littleroot");
        assert_eq!(
            (littleroot.addr, littleroot.encoding, littleroot.tile_count),
            (bus(sheet_offset("littleroot")), Encoding::Raw, 24)
        );
        assert_eq!((littleroot.width, littleroot.height), (16, 96));
        assert_eq!(
            (littleroot.metatile_width, littleroot.metatile_height),
            (1, 1)
        );
        // The large door is stored in 2x4 blocks; a tent is longer.
        let large = plan
            .images
            .iter()
            .find(|image| image.id == "door-animation/image/battle_tower_multi_corridor")
            .expect("the large door");
        assert_eq!(
            (large.width, large.metatile_width, large.metatile_height),
            (32, 2, 4)
        );
        assert_eq!(large.tile_count, 48);
        let tent = plan
            .images
            .iter()
            .find(|image| image.id == "door-animation/image/battle_tent")
            .expect("the tent");
        assert_eq!((tent.height, tent.tile_count), (128, 32));
        // 51 sheets and the table.
        assert_eq!(report.len(), 52);
    });
}

#[test]
fn the_report_names_the_static_symbols_a_map_cannot_list() {
    let rom = fixture().finish();
    with_context("door-animation-symbols", &rom, entries(), |ctx| {
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
            symbol("door-animation/image/birchs_lab"),
            SymbolExpectation::Local("sDoorAnimTiles_BirchsLab".into())
        );
        assert_eq!(
            symbol("door-animation/table"),
            SymbolExpectation::Local("sDoorAnimGraphicsTable".into())
        );
    });
}

#[test]
fn a_sheet_stored_twice_takes_the_copy_its_row_names() {
    let rom = fixture()
        .write(STRAY_SHEET, &sheet_tiles("general"))
        .finish();
    with_context("door-animation-stray-sheet", &rom, entries(), |ctx| {
        let mut report = Vec::new();
        let plan = locate(ctx, &mut report).expect("located");
        let general = plan
            .images
            .iter()
            .find(|image| image.id == "door-animation/image/general")
            .expect("general");
        assert_eq!(general.addr, bus(sheet_offset("general")));
        let line = report
            .iter()
            .find(|line| line.id == "door-animation/image/general")
            .expect("a report line");
        assert_eq!(line.addr, bus(sheet_offset("general")));
        assert_eq!(line.resolution, Resolution::StructDerived);
    });
}

#[test]
fn a_second_complete_table_pins_nothing() {
    let rom = table_at(fixture(), SECOND_TABLE).finish();
    assert_table_not_pinned(located("door-animation-two-tables", &rom));
}

#[test]
fn a_rom_without_the_table_pins_nothing() {
    let mut rom = fixture().finish();
    rom[TABLE..TABLE + 53 * ROW_BYTES as usize].fill(0xFF);
    assert_table_not_pinned(located("door-animation-no-table", &rom));
}

#[test]
fn one_row_that_disagrees_pins_nothing() {
    let sheet = |stem: &str| bus(sheet_offset(stem));
    let broken: [(usize, Vec<u8>); 4] = [
        // A wrong metatile.
        (4, {
            let mut row = row_bytes(4, sheet("littleroot"), bus(BANKS + 4 * 0x10));
            row[0] ^= 1;
            row
        }),
        // A wrong sound.
        (4, {
            let mut row = row_bytes(4, sheet("littleroot"), bus(BANKS + 4 * 0x10));
            row[2] = 1;
            row
        }),
        // Another sheet's tiles.
        (4, row_bytes(4, sheet("birchs_lab"), bus(BANKS + 4 * 0x10))),
        // Another door's banks.
        (4, row_bytes(4, sheet("littleroot"), bus(BANKS + 5 * 0x10))),
    ];
    for (index, row) in broken {
        let rom = fixture()
            .write(TABLE + index * ROW_BYTES as usize, &row)
            .finish();
        assert_table_not_pinned(located("door-animation-broken-row", &rom));
    }
}

#[test]
fn a_table_without_its_terminator_pins_nothing() {
    let rom = fixture()
        .write(TABLE + 53 * ROW_BYTES as usize, &[1])
        .finish();
    assert_table_not_pinned(located("door-animation-no-terminator", &rom));
}

#[test]
fn rows_that_share_a_sheet_must_name_one_copy() {
    // The Battle Tent doors are the last of three rows on one sheet; point
    // the second at a copy of the art instead.
    let tent_rows: Vec<usize> = records()
        .iter()
        .enumerate()
        .filter(|(_, record)| record.sheet == "battle_tent")
        .map(|(index, _)| index)
        .collect();
    assert_eq!(tent_rows.len(), 3);
    let copy = 0x3C_0000;
    let rom = fixture()
        .write(copy, &sheet_tiles("battle_tent"))
        .write(
            TABLE + tent_rows[1] * ROW_BYTES as usize,
            &row_bytes(tent_rows[1], bus(copy), bus(BANKS + tent_rows[1] * 0x10)),
        )
        .finish();
    let err = located("door-animation-split-sheet", &rom).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "door-animation/image/battle_tent"),
        "{err}"
    );
}

#[test]
fn a_sheet_the_rom_lacks_is_not_found() {
    let mut rom = fixture().finish();
    let at = sheet_offset("poke_center");
    rom[at..at + 768].fill(0xFF);
    let err = located("door-animation-missing-sheet", &rom).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == "door-animation/image/poke_center"),
        "{err}"
    );
}

/// A pack whose table is `records`, with every sheet still present.
fn pack_with_table(records: Vec<assets::DoorAnimationRecord>) -> Vec<PackEntry> {
    let mut pack: Vec<PackEntry> = entries()
        .into_iter()
        .filter(|e| e.id != "door-animation/table")
        .collect();
    pack.push(raw_entry(
        "door-animation/table".to_owned(),
        DoorAnimationTable::new(records).unwrap().encode(),
    ));
    pack
}

#[test]
fn a_table_naming_only_a_suffix_of_the_rows_is_refused() {
    // The ROM's last row followed by its terminator is a valid one-row table,
    // so only the pack's own table can reject it.
    let rom = fixture().finish();
    let last = records().last().cloned().unwrap();
    with_context(
        "door-animation-suffix-table",
        &rom,
        pack_with_table(vec![last]),
        |ctx| {
            let result = locate(ctx, &mut Vec::new());
            if let Ok(plan) = &result {
                panic!(
                    "a one-row suffix table was accepted: table {:#x}, {} rows, {} images",
                    plan.table,
                    plan.rows.len(),
                    plan.images.len()
                );
            }
            assert!(
                matches!(&result, Err(GenRomProfileError::StructMismatch { id, .. }) if id == "door-animation/table"),
                "{result:?}"
            );
        },
    );
}

#[test]
fn a_table_missing_any_one_row_is_refused() {
    let rom = fixture().finish();
    // Dropping a middle row, or the second of the three Battle Tent rows,
    // keeps every sheet named; the table is still not the checkout's.
    for dropped in [0, 7, 40, 52] {
        let mut rows = records();
        rows.remove(dropped);
        with_context(
            "door-animation-dropped-row",
            &rom,
            pack_with_table(rows),
            |ctx| {
                let result = locate(ctx, &mut Vec::new());
                assert!(
                    matches!(&result, Err(GenRomProfileError::StructMismatch { id, .. }) if id == "door-animation/table"),
                    "row {dropped}: {result:?}"
                );
            },
        );
    }
}

#[test]
fn a_pack_with_a_missing_or_surplus_door_entry_is_refused() {
    let rom = fixture().finish();
    let ids = door_animation_ids();
    for id in &ids {
        let pack: Vec<PackEntry> = entries().into_iter().filter(|e| &e.id != id).collect();
        with_context("door-animation-missing-entry", &rom, pack, |ctx| {
            let err = locate(ctx, &mut Vec::new()).unwrap_err();
            assert!(
                matches!(&err, GenRomProfileError::MissingPackEntry(missing) if missing == id),
                "{id}: {err}"
            );
        });
    }
    let mut surplus = entries();
    surplus.push(raw_entry(
        "door-animation/image/unused_top".to_owned(),
        vec![0; 4],
    ));
    with_context("door-animation-surplus-entry", &rom, surplus, |ctx| {
        let err = locate(ctx, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "door-animation/image/unused_top"),
            "{err}"
        );
    });
}
