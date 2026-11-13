//! Species-icon locator tests over a synthetic ROM and pack.
//!
//! Every sheet gets its own bytes at its production shape, and the tables are
//! planted as upstream lays them out. The witness tests plant decoy copies and
//! broken tables beside them: a pointer table, not the bytes, must decide.

use std::sync::atomic::{AtomicUsize, Ordering};

use assets::species_icons::{
    encode_palette_table, encode_species_sheets, palette_pack_id, SpeciesIconSheetId,
    PALETTE_INDICES_ID, PALETTE_TABLE_ID, SHEETS, SPECIES_SHEETS_ID, SPECIES_SLOTS,
};
use pack_format::{image_entry_from_tiles, palette_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;

use super::locate;
use crate::extract::scope::species_icon_ids;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SpeciesIconPlan, SymbolExpectation};
use crate::gen_rom_profile::tests::with_context;

const TABLE: usize = 0x10_0000;
const PALETTE_TABLE: usize = 0x10_1000;
const BANKS: usize = 0x10_2000;
const INDICES: usize = 0x10_3000;
const SHEET_BASE: usize = 0x20_0000;
const SHEET_STRIDE: usize = 0x1000;
/// Copies planted below every real sheet.
const STRAY: usize = 0x0A_0000;

fn bus(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).expect("a fixture offset fits a bus address")
}

/// Where sheet `number` sits: the Deoxys speed sheet directly after the Deoxys
/// sheet, as `GetMonIconTiles` reaches it, every other sheet in its own slot.
fn sheet_offset(number: u16) -> usize {
    if number == 415 {
        sheet_offset(385) + 0x400
    } else {
        SHEET_BASE + usize::from(number) * SHEET_STRIDE
    }
}

fn size_of(number: u16) -> (u32, u32) {
    SpeciesIconSheetId::new(number).unwrap().size()
}

/// The sheet species slot `slot` points at: slots 0 and 1 share sheet 0, slots
/// 2 to 26 share sheet 1, and every later slot has the sheet numbered 25 below
/// it, so all 415 table sheets are named and Deoxys (410) has sheet 385.
fn sheet_of(slot: usize) -> u16 {
    match slot {
        0 | 1 => 0,
        2..=26 => 1,
        _ => u16::try_from(slot - 25).expect("below 415"),
    }
}

fn species_sheets() -> Vec<u16> {
    (0..SPECIES_SLOTS).map(sheet_of).collect()
}

fn palette_indices() -> Vec<u8> {
    (0..SPECIES_SLOTS)
        .map(|slot| u8::try_from(slot % 3).unwrap())
        .collect()
}

/// The 4bpp tile bytes of sheet `number`, distinct from every other sheet's.
fn tiles(number: u16) -> Vec<u8> {
    let (width, height) = size_of(number);
    (0..width / 8 * (height / 8) * 32)
        .map(|byte| {
            let mixed = (u32::from(number) + 1)
                .wrapping_mul(2_654_435_761)
                .wrapping_add(byte.wrapping_mul(40_503));
            u8::try_from((mixed >> 11) & 0xFF).unwrap()
        })
        .collect()
}

fn bank_colors(bank: u16) -> Vec<u16> {
    (0..16).map(|color| 0x100 * (bank + 1) + color).collect()
}

fn sheet_entry(number: u16) -> PackEntry {
    let id = SpeciesIconSheetId::new(number).unwrap();
    let (width, height) = id.size();
    image_entry_from_tiles(id.pack_id(), &tiles(number), 4, width, height, None).expect("image")
}

fn entries() -> Vec<PackEntry> {
    let mut entries: Vec<PackEntry> = (0..SHEETS).map(sheet_entry).collect();
    for bank in 0..3u8 {
        entries.push(
            palette_entry(palette_pack_id(bank), &bank_colors(u16::from(bank))).expect("palette"),
        );
    }
    entries.push(raw_entry(
        SPECIES_SHEETS_ID.to_owned(),
        encode_species_sheets(&species_sheets()).unwrap(),
    ));
    entries.push(raw_entry(PALETTE_INDICES_ID.to_owned(), palette_indices()));
    entries.push(raw_entry(
        PALETTE_TABLE_ID.to_owned(),
        encode_palette_table(),
    ));
    entries
}

fn entries_without(id: &str) -> Vec<PackEntry> {
    entries().into_iter().filter(|e| e.id != id).collect()
}

fn entries_replacing(replacement: PackEntry) -> Vec<PackEntry> {
    let mut entries = entries_without(&replacement.id);
    entries.push(replacement);
    entries
}

fn write_table(fixture: RomFixture, base: usize, sheets: &[usize]) -> RomFixture {
    sheets
        .iter()
        .enumerate()
        .fold(fixture, |fixture, (slot, &at)| {
            fixture.write(base + slot * 4, &bus(at).to_le_bytes())
        })
}

fn write_palettes(fixture: RomFixture, banks: usize, table: usize) -> RomFixture {
    let mut fixture = fixture;
    for bank in 0..3u16 {
        let bytes: Vec<u8> = bank_colors(bank)
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        fixture = fixture.write(banks + usize::from(bank) * 32, &bytes);
    }
    for row in 0..6usize {
        fixture = fixture
            .write(table + row * 8, &bus(banks + row * 32).to_le_bytes())
            .write(
                table + row * 8 + 4,
                &(56000 + u16::try_from(row).unwrap()).to_le_bytes(),
            );
    }
    fixture
}

/// Every sheet, both tables, the banks and the indices, as upstream lays them out.
fn fixture() -> RomFixture {
    let mut fixture = RomFixture::new().emerald_header();
    for number in 0..SHEETS {
        fixture = fixture.write(sheet_offset(number), &tiles(number));
    }
    let slots: Vec<usize> = species_sheets().into_iter().map(sheet_offset).collect();
    let fixture = write_table(fixture, TABLE, &slots);
    write_palettes(fixture, BANKS, PALETTE_TABLE).write(INDICES, &palette_indices())
}

fn located(fixture: RomFixture) -> Result<(SpeciesIconPlan, Vec<ReportLine>), GenRomProfileError> {
    located_with(fixture, entries())
}

fn located_with(
    fixture: RomFixture,
    entries: Vec<PackEntry>,
) -> Result<(SpeciesIconPlan, Vec<ReportLine>), GenRomProfileError> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let name = format!("species-icons-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let rom = fixture.finish();
    with_context(&name, &rom, entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).map(|plan| (plan, report))
    })
}

fn line<'a>(report: &'a [ReportLine], id: &str) -> &'a ReportLine {
    report
        .iter()
        .find(|line| line.id == id)
        .unwrap_or_else(|| panic!("no report line for {id}"))
}

fn assert_struct_mismatch(err: &GenRomProfileError, id: &str) {
    assert!(
        matches!(err, GenRomProfileError::StructMismatch { id: found, .. } if found == id),
        "{err}"
    );
}

#[test]
fn the_roots_are_the_checkouts_fixed_inventory() {
    let mut planted: Vec<String> = entries().into_iter().map(|e| e.id).collect();
    planted.sort();
    let mut expected = species_icon_ids();
    expected.sort();
    assert_eq!(planted, expected);
    assert_eq!(expected.len(), 423);
}

#[test]
fn every_sheet_table_bank_and_index_is_found_with_its_production_shape() {
    let (plan, report) = located(fixture()).expect("located");

    assert_eq!(plan.sheet_table, bus(TABLE));
    assert_eq!(plan.palette_table, bus(PALETTE_TABLE));
    assert_eq!(plan.palette_indices.addr, bus(INDICES));
    assert_eq!(plan.palette_indices.len, 440);
    assert_eq!(plan.sheets.len(), 417);
    assert_eq!(plan.palettes.len(), 3);
    for (number, sheet) in (0..SHEETS).zip(&plan.sheets) {
        let (width, height) = size_of(number);
        assert_eq!(sheet.id, SpeciesIconSheetId::new(number).unwrap().pack_id());
        assert_eq!(sheet.addr, bus(sheet_offset(number)), "{}", sheet.id);
        assert_eq!(
            (sheet.width, sheet.height, sheet.tile_count),
            (width, height, width / 8 * (height / 8))
        );
        assert_eq!((sheet.rom_bit_depth, sheet.pack_bit_depth), (4, 4));
        assert_eq!((sheet.metatile_width, sheet.metatile_height), (1, 1));
    }
    for (bank, palette) in plan.palettes.iter().enumerate() {
        assert_eq!(palette.addr, bus(BANKS + bank * 32));
        assert_eq!(palette.color_count, 16);
    }
    // 417 sheets, three banks, three tables.
    assert_eq!(report.len(), 417 + 3 + 3);
    assert_eq!(
        line(&report, "gMonIconTable").symbol,
        SymbolExpectation::Exact("gMonIconTable".to_owned())
    );
    assert_eq!(line(&report, "gMonIconTable").len, 1760);
    assert_eq!(line(&report, "gMonIconPaletteTable").len, 48);
    assert_eq!(
        line(&report, "species-icon/palette/0").symbol,
        SymbolExpectation::Exact("gMonIconPalettes".to_owned())
    );
    assert_eq!(
        line(&report, "species-icon/palette/2").symbol,
        SymbolExpectation::Interior
    );
    assert_eq!(
        line(&report, "species-icon/raw/palette-indices").resolution,
        Resolution::UniqueSignature
    );
    // The Deoxys speed sheet is derived and named; the wide sheet is unique.
    let speed = line(&report, "species-icon/image/415");
    assert_eq!(speed.addr, bus(sheet_offset(385) + 0x400));
    assert_eq!(
        speed.symbol,
        SymbolExpectation::Exact("gMonIcon_DeoxysSpeed".to_owned())
    );
    assert_eq!(
        line(&report, "species-icon/image/416").resolution,
        Resolution::UniqueSignature
    );
}

#[test]
fn the_table_not_the_lowest_copy_names_each_sheet() {
    // Seventeen copies of sheet 0's bytes sit below the real sheet; the table
    // names the real one, which is not the lowest.
    let mut rom = fixture();
    for copy in 0..17 {
        rom = rom.write(STRAY + copy * SHEET_STRIDE, &tiles(0));
    }
    let (plan, report) = located(rom).expect("located");
    assert_eq!(plan.sheets[0].addr, bus(sheet_offset(0)));
    let note = line(&report, "species-icon/image/000")
        .note
        .clone()
        .unwrap();
    assert!(note.contains("18 addresses"), "{note}");

    // And the lowest copy when the table names it.
    let mut slots: Vec<usize> = species_sheets().into_iter().map(sheet_offset).collect();
    let mut rom = fixture().write(STRAY, &tiles(0));
    for slot in &mut slots {
        if *slot == sheet_offset(0) {
            *slot = STRAY;
        }
    }
    rom = write_table(rom, TABLE, &slots);
    let (plan, _) = located(rom).expect("located");
    assert_eq!(plan.sheets[0].addr, bus(STRAY));
}

#[test]
fn distinct_symbols_with_equal_bytes_keep_their_own_rows() {
    // Sheets 40 and 41 hold the same art at two addresses; the table gives
    // each slot its own object.
    let (offset_40, offset_41) = (sheet_offset(40), sheet_offset(41));
    let rom = fixture().write(offset_41, &tiles(40));
    let mut entries = entries_without("species-icon/image/041");
    let (width, height) = size_of(41);
    entries.push(
        image_entry_from_tiles(
            "species-icon/image/041".to_owned(),
            &tiles(40),
            4,
            width,
            height,
            None,
        )
        .unwrap(),
    );
    let (plan, _) = located_with(rom, entries).expect("located");
    assert_eq!(plan.sheets[40].addr, bus(offset_40));
    assert_eq!(plan.sheets[41].addr, bus(offset_41));
}

#[test]
fn two_symbols_pointed_at_one_object_are_refused() {
    // The sheets are equal, but the table points both slots at one copy: one
    // symbol per object, so the table is not upstream's.
    let (offset_40, offset_41) = (sheet_offset(40), sheet_offset(41));
    let mut slots: Vec<usize> = species_sheets().into_iter().map(sheet_offset).collect();
    for slot in &mut slots {
        if *slot == offset_41 {
            *slot = offset_40;
        }
    }
    let rom = write_table(fixture().write(offset_41, &tiles(40)), TABLE, &slots);
    let mut entries = entries_without("species-icon/image/041");
    let (width, height) = size_of(41);
    entries.push(
        image_entry_from_tiles(
            "species-icon/image/041".to_owned(),
            &tiles(40),
            4,
            width,
            height,
            None,
        )
        .unwrap(),
    );
    let err = located_with(rom, entries).unwrap_err();
    assert_struct_mismatch(&err, "gMonIconTable");
}

#[test]
fn a_shared_symbol_split_across_copies_is_refused() {
    // Slots 0 and 1 share a symbol, so they must name the same copy.
    let mut slots: Vec<usize> = species_sheets().into_iter().map(sheet_offset).collect();
    slots[1] = STRAY;
    let rom = write_table(fixture().write(STRAY, &tiles(0)), TABLE, &slots);
    let err = located(rom).unwrap_err();
    assert_struct_mismatch(&err, "gMonIconTable");
}

#[test]
fn a_table_that_is_partial_wrong_or_unreferenced_is_refused() {
    // One row naming another sheet.
    let rom = fixture().write(TABLE + 100 * 4, &bus(sheet_offset(7)).to_le_bytes());
    assert_struct_mismatch(&located(rom).unwrap_err(), "gMonIconTable");

    // A row that names no pointer at all.
    let rom = fixture().write(TABLE + 439 * 4, &[0, 0, 0, 0]);
    assert_struct_mismatch(&located(rom).unwrap_err(), "gMonIconTable");

    // A table at an unaligned offset is not a compiler-emitted pointer table.
    let slots: Vec<usize> = species_sheets().into_iter().map(sheet_offset).collect();
    let mut rom = fixture().write(TABLE, &vec![0xFF; SPECIES_SLOTS * 4]);
    rom = write_table(rom, TABLE + 0x2001, &slots);
    assert_struct_mismatch(&located(rom).unwrap_err(), "gMonIconTable");

    // A table with an 8-byte stride is not upstream's.
    let eight: Vec<usize> = (0..SPECIES_SLOTS * 2)
        .map(|n| {
            if n % 2 == 0 {
                sheet_offset(sheet_of(n / 2))
            } else {
                0
            }
        })
        .collect();
    let rom = write_table(
        fixture().write(TABLE, &vec![0xFF; SPECIES_SLOTS * 4]),
        TABLE + 0x4000,
        &eight,
    );
    assert_struct_mismatch(&located(rom).unwrap_err(), "gMonIconTable");
}

#[test]
fn two_complete_tables_are_ambiguous() {
    let slots: Vec<usize> = species_sheets().into_iter().map(sheet_offset).collect();
    let rom = write_table(fixture(), TABLE + 0x8000, &slots);
    assert_struct_mismatch(&located(rom).unwrap_err(), "gMonIconTable");
}

#[test]
fn the_wrong_row_pointer_with_a_right_sheet_elsewhere_still_fails() {
    // The sheet's bytes exist, but no table names them.
    let mut rom = RomFixture::new().emerald_header();
    for number in 0..SHEETS {
        rom = rom.write(sheet_offset(number), &tiles(number));
    }
    let rom = write_palettes(rom, BANKS, PALETTE_TABLE).write(INDICES, &palette_indices());
    assert_struct_mismatch(&located(rom).unwrap_err(), "gMonIconTable");
}

#[test]
fn a_missing_sheet_names_its_id() {
    let mut rom = fixture();
    // Overwrite sheet 200's bytes everywhere.
    rom = rom.write(sheet_offset(200), &vec![0xAA; 1024]);
    let err = located(rom).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == "species-icon/image/200"),
        "{err}"
    );
}

#[test]
fn the_speed_sheet_must_sit_where_the_deoxys_offset_reaches() {
    // The speed bytes exist, but not 0x400 past the Deoxys sheet.
    let rom = fixture()
        .write(sheet_offset(385) + 0x400, &vec![0xFF; 1024])
        .write(0x50_0000, &tiles(415));
    let err = located(rom).unwrap_err();
    assert_struct_mismatch(&err, "species-icon/image/415");
}

#[test]
fn the_wide_sheet_must_occur_exactly_once() {
    let err = located(fixture().write(0x51_0000, &tiles(416))).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::Ambiguous { id, addrs }
            if id == "species-icon/image/416" && addrs.len() == 2),
        "{err}"
    );
    let err = located(fixture().write(sheet_offset(416), &vec![0xEE; 4096])).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == "species-icon/image/416"),
        "{err}"
    );
}

#[test]
fn the_banks_are_accepted_only_with_a_table_whose_rows_and_tags_agree() {
    // An unreferenced lower copy of the banks does not matter.
    let mut rom = fixture();
    for bank in 0..3u16 {
        let bytes: Vec<u8> = bank_colors(bank)
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        rom = rom.write(STRAY + usize::from(bank) * 32, &bytes);
    }
    let (plan, report) = located(rom).expect("located");
    assert_eq!(plan.palettes[0].addr, bus(BANKS));
    assert!(line(&report, "species-icon/palette/0")
        .note
        .as_deref()
        .unwrap()
        .contains("2 addresses"));

    // A wrong tag in any row.
    for row in 0..6usize {
        let rom = fixture().write(PALETTE_TABLE + row * 8 + 4, &56100u16.to_le_bytes());
        assert_struct_mismatch(&located(rom).unwrap_err(), "gMonIconPaletteTable");
    }
    // A wrong pointer in any row, including the three past the banks.
    for row in 0..6usize {
        let rom = fixture().write(
            PALETTE_TABLE + row * 8,
            &bus(BANKS + row * 32 + 2).to_le_bytes(),
        );
        assert_struct_mismatch(&located(rom).unwrap_err(), "gMonIconPaletteTable");
    }
    // A lone pointer to the banks with no table behind it.
    let mut rom = fixture();
    for row in 1..6usize {
        rom = rom.write(PALETTE_TABLE + row * 8, &[0xFF; 4]);
    }
    assert_struct_mismatch(&located(rom).unwrap_err(), "gMonIconPaletteTable");
    // Two complete tables over the banks.
    let mut rom = fixture();
    for row in 0..6usize {
        rom = rom
            .write(
                PALETTE_TABLE + 0x100 + row * 8,
                &bus(BANKS + row * 32).to_le_bytes(),
            )
            .write(
                PALETTE_TABLE + 0x100 + row * 8 + 4,
                &(56000 + u16::try_from(row).unwrap()).to_le_bytes(),
            );
    }
    assert_struct_mismatch(&located(rom).unwrap_err(), "gMonIconPaletteTable");
}

#[test]
fn the_palette_indices_are_the_one_copy_or_the_one_referenced() {
    // A second, unreferenced copy is ambiguous.
    let err = located(fixture().write(STRAY, &palette_indices())).unwrap_err();
    assert_struct_mismatch(&err, "species-icon/raw/palette-indices");

    // Referenced from code, one copy wins whichever address is lower.
    let rom = fixture()
        .write(STRAY, &palette_indices())
        .write(0x40_0000, &bus(INDICES).to_le_bytes());
    let (plan, report) = located(rom).expect("located");
    assert_eq!(plan.palette_indices.addr, bus(INDICES));
    assert_eq!(
        line(&report, "species-icon/raw/palette-indices").resolution,
        Resolution::StructDerived
    );

    // No copy.
    let err = located(fixture().write(INDICES, &vec![1u8; 440])).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == "species-icon/raw/palette-indices"),
        "{err}"
    );
}

#[test]
fn an_incomplete_or_surplus_inventory_is_refused_before_any_search() {
    for id in species_icon_ids() {
        let err = located_with(fixture(), entries_without(&id)).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::MissingPackEntry(missing) if *missing == id),
            "{id}: {err}"
        );
    }
    let mut surplus = entries();
    surplus.push(raw_entry("species-icon/raw/extra".to_owned(), vec![0]));
    assert_struct_mismatch(
        &located_with(fixture(), surplus).unwrap_err(),
        "species-icon/raw/extra",
    );
}

#[test]
fn a_malformed_entry_is_refused_with_its_id() {
    // Wrong kind.
    for id in ["species-icon/image/003", "species-icon/palette/1"] {
        let err = located_with(
            fixture(),
            entries_replacing(raw_entry(id.to_owned(), vec![0; 8])),
        )
        .unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::WrongPackEntryKind { id: found, .. } if found == id),
            "{id}: {err}"
        );
    }
    for id in [SPECIES_SHEETS_ID, PALETTE_INDICES_ID, PALETTE_TABLE_ID] {
        let err = located_with(
            fixture(),
            entries_replacing(palette_entry(id.to_owned(), &[0; 16]).unwrap()),
        )
        .unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::WrongPackEntryKind { id: found, .. } if found == id),
            "{id}: {err}"
        );
    }
    // Wrong shapes.
    let crop = image_entry_from_tiles(
        "species-icon/image/003".to_owned(),
        &tiles(3)[..512],
        4,
        32,
        32,
        None,
    )
    .unwrap();
    let narrow_wide = image_entry_from_tiles(
        "species-icon/image/416".to_owned(),
        &tiles(3),
        4,
        32,
        64,
        None,
    )
    .unwrap();
    let short_bank = palette_entry(palette_pack_id(1), &bank_colors(1)[..15]).unwrap();
    let mut bad_sheets = species_sheets();
    bad_sheets[9] = 415;
    let mut sheets_table = bad_sheets
        .iter()
        .flat_map(|s| s.to_le_bytes())
        .collect::<Vec<u8>>();
    sheets_table.truncate(878);
    let mut bad_indices = palette_indices();
    bad_indices[3] = 3;
    let mut bad_records = encode_palette_table();
    bad_records[2] = 1;
    let cases = [
        crop,
        narrow_wide,
        short_bank,
        raw_entry(
            SPECIES_SHEETS_ID.to_owned(),
            bad_sheets.iter().flat_map(|s| s.to_le_bytes()).collect(),
        ),
        raw_entry(SPECIES_SHEETS_ID.to_owned(), sheets_table),
        raw_entry(PALETTE_INDICES_ID.to_owned(), bad_indices),
        raw_entry(PALETTE_INDICES_ID.to_owned(), vec![0; 439]),
        raw_entry(PALETTE_TABLE_ID.to_owned(), bad_records),
    ];
    for (n, entry) in cases.into_iter().enumerate() {
        let id = entry.id.clone();
        let err = located_with(fixture(), entries_replacing(entry)).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::EntryShape { id: found, .. } if *found == id),
            "case {n}: {err}"
        );
    }
}
