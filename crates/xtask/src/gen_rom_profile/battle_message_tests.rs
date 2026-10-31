//! Battle-message locator tests over a synthetic ROM and pack.
//!
//! The fixture plants a 369-pointer table, 74 special templates, and the
//! zero-padded type-table cells, then builds the pack entry the extractor
//! would write for exactly those streams. Each failure test breaks one fact:
//! a missing table, a missing or duplicated template, a malformed entry.

use assets::pack::{
    BattleMessageTable, BATTLE_MESSAGE_SLOT_COUNT, BATTLE_MESSAGE_SPECIAL_TEMPLATES,
    BATTLE_MESSAGE_TABLE_START,
};
use pack_format::{image_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::{GbaPtr, ROM_BASE};

use super::battle_messages::locate;
use super::error::GenRomProfileError;
use super::plan::{BattleMessagePlan, ReportLine, Resolution};
use super::tests::with_context;

const PACK_ID: &str = rom_import::BATTLE_MESSAGE_PACK_ID;
const TABLE: usize = 0x1000;
const STRINGS: usize = 0x4000;
const SPECIALS: usize = 0x1_0000;
const TYPE_ROWS: usize = 0x2_0000;
const STRING_CELL: usize = 16;
const SPECIAL_CELL: usize = 32;
const ROW_CELL: usize = 17;

/// Index of `sATypeMove_Table[TYPE_NORMAL]` in the special inventory.
fn first_type_row() -> usize {
    BATTLE_MESSAGE_SPECIAL_TEMPLATES
        .iter()
        .position(|template| template.symbol == "sATypeMove_Table[TYPE_NORMAL]")
        .expect("the inventory has the type table")
}

fn special_index(symbol: &str) -> usize {
    BATTLE_MESSAGE_SPECIAL_TEMPLATES
        .iter()
        .position(|template| template.symbol == symbol)
        .expect("a known template")
}

/// A table stream no other stream shares: eight plain bytes and the `EOS`.
fn table_stream(index: usize) -> Vec<u8> {
    vec![
        0x01 + byte(index % 200),
        0x01 + byte(index / 200),
        3,
        4,
        5,
        6,
        7,
        8,
        0xFF,
    ]
}

/// A special stream: unique, short for the type rows.
fn special_stream(index: usize) -> Vec<u8> {
    if index >= first_type_row() {
        vec![0x40 + byte(index - first_type_row()), 0x50, 0x51, 0xFF]
    } else {
        vec![0x02, 0x01 + byte(index), 10, 11, 12, 13, 14, 15, 0xFF]
    }
}

/// Where the default layout plants special `index`.
fn special_offset(index: usize) -> usize {
    if index >= first_type_row() {
        TYPE_ROWS + (index - first_type_row()) * ROW_CELL
    } else {
        SPECIALS + index * SPECIAL_CELL
    }
}

fn payload(specials: &[Vec<u8>]) -> Vec<u8> {
    let slots: Vec<Vec<u8>> = (0..BATTLE_MESSAGE_SLOT_COUNT).map(table_stream).collect();
    BattleMessageTable::from_entries(
        slots
            .iter()
            .enumerate()
            .map(|(index, stream)| (BATTLE_MESSAGE_TABLE_START + word(index), stream.as_slice())),
        specials
            .iter()
            .enumerate()
            .map(|(index, stream)| (word(index), stream.as_slice())),
    )
    .expect("the fixture streams are valid")
    .encode()
}

fn default_specials() -> Vec<Vec<u8>> {
    (0..BATTLE_MESSAGE_SPECIAL_TEMPLATES.len())
        .map(special_stream)
        .collect()
}

/// The table and its strings.
fn with_table(mut rom: RomFixture) -> RomFixture {
    for index in 0..BATTLE_MESSAGE_SLOT_COUNT {
        let at = STRINGS + index * STRING_CELL;
        rom = rom
            .write_ptr(
                TABLE + index * 4,
                GbaPtr::at(ROM_BASE + u32::try_from(at).expect("fits")),
            )
            .write(at, &table_stream(index));
    }
    rom
}

/// Special `index` at `offset`, zero-padded to a 17-byte cell for a type row.
fn with_special(rom: RomFixture, index: usize, stream: &[u8], offset: usize) -> RomFixture {
    let mut cell = stream.to_vec();
    if index >= first_type_row() {
        cell.resize(ROW_CELL, 0);
    }
    rom.write(offset, &cell)
}

fn with_default_specials(mut rom: RomFixture, specials: &[Vec<u8>]) -> RomFixture {
    for (index, stream) in specials.iter().enumerate() {
        rom = with_special(rom, index, stream, special_offset(index));
    }
    rom
}

fn full_rom() -> RomFixture {
    with_default_specials(
        with_table(RomFixture::new().emerald_header()),
        &default_specials(),
    )
}

fn pack(payload: Vec<u8>) -> Vec<PackEntry> {
    vec![raw_entry(PACK_ID.to_owned(), payload)]
}

type Located = Result<(BattleMessagePlan, Vec<ReportLine>), GenRomProfileError>;

fn locate_in(name: &str, rom: RomFixture, entries: Vec<PackEntry>) -> Located {
    with_context(name, &rom.finish(), entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).map(|plan| (plan, report))
    })
}

fn byte(index: usize) -> u8 {
    u8::try_from(index).expect("a fixture index fits in u8")
}

fn word(index: usize) -> u16 {
    u16::try_from(index).expect("a fixture index fits in u16")
}

fn bus(offset: usize) -> u32 {
    ROM_BASE + u32::try_from(offset).expect("fits")
}

#[test]
fn a_matching_table_and_inventory_are_located() {
    let (plan, report) = locate_in(
        "bm-matching",
        full_rom(),
        pack(payload(&default_specials())),
    )
    .expect("a consistent ROM and pack");
    assert_eq!(plan.table, bus(TABLE));
    assert_eq!(plan.specials.len(), BATTLE_MESSAGE_SPECIAL_TEMPLATES.len());
    for (index, special) in plan.specials.iter().enumerate() {
        assert_eq!(
            special.symbol,
            BATTLE_MESSAGE_SPECIAL_TEMPLATES[index].symbol
        );
        assert_eq!(
            special.addr,
            bus(special_offset(index)),
            "{}",
            special.symbol
        );
    }
    assert_eq!(report.len(), 1 + plan.specials.len());
    assert_eq!(report[0].id, PACK_ID);
    assert_eq!(report[0].resolution, Resolution::StructDerived);
    assert_eq!(
        report[0].symbol,
        super::plan::SymbolExpectation::Exact("gBattleStringsTable".to_owned())
    );
}

#[test]
fn the_plan_records_addresses_and_no_stream_bytes() {
    let (plan, report) = locate_in(
        "bm-no-bytes",
        full_rom(),
        pack(payload(&default_specials())),
    )
    .expect("located");
    let rendered = format!("{plan:?} {report:?}");
    for index in 0..BATTLE_MESSAGE_SPECIAL_TEMPLATES.len() {
        let stream = special_stream(index);
        assert!(
            !rendered.contains(&format!("{stream:?}")),
            "special {index}'s bytes leaked into the plan"
        );
    }
}

#[test]
fn a_rom_without_the_pointer_table_is_refused() {
    let rom = with_default_specials(
        // Strings only: nothing points at them.
        (0..BATTLE_MESSAGE_SLOT_COUNT).fold(RomFixture::new().emerald_header(), |rom, index| {
            rom.write(STRINGS + index * STRING_CELL, &table_stream(index))
        }),
        &default_specials(),
    );
    let err = locate_in("bm-no-table", rom, pack(payload(&default_specials()))).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "gBattleStringsTable"),
        "{err}"
    );
}

#[test]
fn a_table_slot_pointing_at_the_wrong_string_is_refused() {
    let wrong = GbaPtr::at(bus(STRINGS + 6 * STRING_CELL));
    let rom = full_rom().write_ptr(TABLE + 5 * 4, wrong);
    let err = locate_in("bm-wrong-slot", rom, pack(payload(&default_specials()))).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "gBattleStringsTable"),
        "{err}"
    );
}

#[test]
fn a_special_template_absent_from_the_rom_is_not_found() {
    let specials = default_specials();
    let missing = special_index("sText_GoPkmn");
    let rom = with_table(RomFixture::new().emerald_header());
    let rom = specials
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != missing)
        .fold(rom, |rom, (index, stream)| {
            with_special(rom, index, stream, special_offset(index))
        });
    let err = locate_in("bm-no-special", rom, pack(payload(&specials))).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id.contains("sText_GoPkmn")),
        "{err}"
    );
}

#[test]
fn a_special_template_found_twice_with_no_neighbour_is_ambiguous() {
    let specials = default_specials();
    let rom = with_special(full_rom(), 0, &specials[0], SPECIALS + 0x800);
    let err = locate_in("bm-twice", rom, pack(payload(&specials))).unwrap_err();
    match err {
        GenRomProfileError::Ambiguous { id, addrs } => {
            assert!(id.contains("sText_TwoTrainersWantToBattle"), "{id}");
            assert_eq!(addrs, [bus(SPECIALS), bus(SPECIALS + 0x800)]);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

#[test]
fn identical_templates_are_placed_by_their_declared_neighbours() {
    let wild = special_index("sText_WildPkmnAppeared");
    let legendary = special_index("sText_LegendaryPkmnAppeared");
    let pause = special_index("sText_WildPkmnAppearedPause");
    let mut specials = default_specials();
    specials[legendary] = specials[wild].clone();
    let len = specials[wild].len();

    // The two identical streams sit back to back, the pause stream after
    // them, as `battle_message.c` declares them; a decoy copy sits apart.
    let run = SPECIALS + 0x3000;
    let mut rom = with_default_specials(with_table(RomFixture::new().emerald_header()), &specials);
    for index in [wild, legendary, pause] {
        rom = rom.write(special_offset(index), &[0xFF; SPECIAL_CELL]);
    }
    let rom = with_special(rom, wild, &specials[wild], run);
    let rom = with_special(rom, legendary, &specials[legendary], run + len);
    let rom = with_special(rom, pause, &specials[pause], run + 2 * len);
    let rom = with_special(rom, wild, &specials[wild], SPECIALS + 0x4000);

    let (plan, report) =
        locate_in("bm-neighbours", rom, pack(payload(&specials))).expect("neighbours decide");
    assert_eq!(plan.specials[wild].addr, bus(run));
    assert_eq!(plan.specials[legendary].addr, bus(run + len));
    assert_eq!(plan.specials[pause].addr, bus(run + 2 * len));
    let line = report
        .iter()
        .find(|line| line.id.ends_with("sText_LegendaryPkmnAppeared"))
        .expect("a report line");
    assert_eq!(line.resolution, Resolution::StructDerived);
}

#[test]
fn identical_templates_with_no_adjacent_neighbour_are_refused() {
    let wild = special_index("sText_WildPkmnAppeared");
    let legendary = special_index("sText_LegendaryPkmnAppeared");
    let mut specials = default_specials();
    specials[legendary] = specials[wild].clone();
    // The default layout separates the copies by a full cell and puts the
    // pause stream elsewhere, so no candidate abuts it.
    let rom = with_default_specials(with_table(RomFixture::new().emerald_header()), &specials);
    let err = locate_in("bm-apart", rom, pack(payload(&specials))).unwrap_err();
    // The template that failed to place is blamed; the anchor it was measured
    // against is named only in the reason.
    match err {
        GenRomProfileError::StructMismatch { id, reason } => {
            assert_eq!(id, "battle-message/table#sText_LegendaryPkmnAppeared");
            assert!(reason.contains("sText_WildPkmnAppearedPause"), "{reason}");
        }
        other => panic!("expected StructMismatch, got {other:?}"),
    }
}

#[test]
fn a_type_row_without_zero_padding_is_refused() {
    let row = first_type_row() + 3;
    let rom = full_rom().write(special_offset(row) + ROW_CELL - 1, &[0x7F]);
    let err = locate_in("bm-padding", rom, pack(payload(&default_specials()))).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id.contains("TYPE_POISON")),
        "{err}"
    );
}

#[test]
fn a_type_row_off_the_table_stride_is_refused() {
    let row = first_type_row() + 2;
    let specials = default_specials();
    let mut rom = with_default_specials(with_table(RomFixture::new().emerald_header()), &specials);
    // Wipe the row's cell and move it one byte over.
    rom = rom.write(special_offset(row), &[0u8; ROW_CELL]);
    rom = with_special(rom, row, &specials[row], special_offset(row) + 1);
    let err = locate_in("bm-stride", rom, pack(payload(&specials))).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id.contains("TYPE_FLYING")),
        "{err}"
    );
}

#[test]
fn a_pack_without_the_table_entry_is_refused() {
    let err = locate_in("bm-no-entry", full_rom(), Vec::new()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_table_entry_of_the_wrong_kind_is_refused() {
    let entry = image_entry(PACK_ID.to_owned(), 1, 1, 8, vec![0]).expect("an image entry");
    let err = locate_in("bm-wrong-kind", full_rom(), vec![entry]).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::WrongPackEntryKind { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_malformed_table_entry_is_refused_as_an_entry_shape() {
    let mut truncated = payload(&default_specials());
    truncated.truncate(truncated.len() - 3);
    let err = locate_in("bm-truncated", full_rom(), pack(truncated)).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_pack_entry_the_domain_does_not_cover_is_refused() {
    let mut entries = pack(payload(&default_specials()));
    entries.push(raw_entry("battle-message/extra".to_owned(), vec![0xFF]));
    let err = locate_in("bm-extra", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "battle-message/*"),
        "{err}"
    );
}
