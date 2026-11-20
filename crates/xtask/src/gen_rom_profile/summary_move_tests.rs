//! Summary-move locator tests over a synthetic ROM and pack.
//!
//! The fixture plants the two pointer tables with their strings, then the two
//! struct tables at the strides the importer reads (8 and 4 bytes), with filler
//! in the combo and padding bytes the pack leaves out, and builds the pack
//! entry the extractor would write for the same data. Each failure test breaks
//! one fact: a late pointer or row, a second copy, a stride the ROM does not
//! use, or a malformed or surplus entry.

use assets::pack::{
    ContestEffectRecord, ContestMoveRecord, SummaryMoveTable, CONTEST_EFFECT_COUNT,
    CONTEST_MOVE_COUNT, MOVE_DESCRIPTION_COUNT,
};
use pack_format::{image_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::ROM_BASE;

use super::error::GenRomProfileError;
use super::plan::{ReportLine, Resolution, SummaryMovePlan, SymbolExpectation};
use super::summary_moves::locate;
use super::tests::with_context;

const PACK_ID: &str = rom_import::SUMMARY_MOVE_PACK_ID;
const DESCRIPTIONS: usize = 0x1000;
const DESCRIPTION_STRINGS: usize = 0x2000;
const CONTEST_MOVES: usize = 0x8000;
const CONTEST_EFFECTS: usize = 0xA000;
const EFFECT_DESCRIPTIONS: usize = 0xA800;
const EFFECT_STRINGS: usize = 0xB000;
const CELL: usize = 16;
const FILLER: u8 = 0xEE;
/// Two slots that share one text, and one that is a bare `EOS`.
const SHARED: (usize, usize) = (20, 21);
const EMPTY_SLOT: usize = 9;

fn byte(index: usize) -> u8 {
    u8::try_from(index % 200).expect("under 200") + 1
}

/// A slot's description: unique, longer than the search's anchor, except for
/// the shared pair and the empty entry.
fn description(slot: usize) -> Vec<u8> {
    let slot = if slot == SHARED.1 { SHARED.0 } else { slot };
    if slot == EMPTY_SLOT {
        return vec![0xFF];
    }
    vec![
        0x11,
        0x12,
        0x13,
        0x14,
        0x15,
        0x16,
        0x17,
        0x18,
        byte(slot),
        byte(slot / 200),
        0x19,
        0xFF,
    ]
}

fn effect_text(id: usize) -> Vec<u8> {
    vec![
        0x21,
        0x22,
        0x23,
        0x24,
        0x25,
        0x26,
        0x27,
        0x28,
        byte(id),
        0x29,
        0xFF,
    ]
}

fn effect_type(id: usize) -> u8 {
    [0, 1, 2, 3, 4, 5, 6, 8][id % 8]
}

fn move_effect(index: usize) -> u8 {
    u8::try_from((index * 7) % CONTEST_EFFECT_COUNT).expect("under 48")
}

fn move_category(index: usize) -> u8 {
    u8::try_from((index * 3) % 5).expect("under 5")
}

fn payload() -> Vec<u8> {
    let descriptions: Vec<(u16, Vec<u8>)> = (0..MOVE_DESCRIPTION_COUNT)
        .map(|slot| (u16::try_from(slot).unwrap(), description(slot)))
        .collect();
    let moves: Vec<ContestMoveRecord> = (0..CONTEST_MOVE_COUNT)
        .map(|index| ContestMoveRecord {
            move_id: u16::try_from(index).unwrap(),
            effect: move_effect(index),
            category: move_category(index),
        })
        .collect();
    let texts: Vec<Vec<u8>> = (0..CONTEST_EFFECT_COUNT).map(effect_text).collect();
    let effects: Vec<ContestEffectRecord<'_>> = (0..CONTEST_EFFECT_COUNT)
        .map(|id| ContestEffectRecord {
            id: u16::try_from(id).unwrap(),
            effect_type: effect_type(id),
            appeal: u8::try_from(id * 5).unwrap(),
            jam: u8::try_from(id).unwrap(),
            description: &texts[id],
        })
        .collect();
    SummaryMoveTable::from_entries(
        descriptions.iter().map(|(slot, s)| (*slot, s.as_slice())),
        moves,
        effects,
    )
    .expect("the fixture data is valid")
    .encode()
}

fn pack() -> Vec<PackEntry> {
    vec![raw_entry(PACK_ID.to_owned(), payload())]
}

fn bus(offset: usize) -> u32 {
    ROM_BASE + u32::try_from(offset).expect("fits")
}

fn plant_pointers(
    mut rom: RomFixture,
    table: usize,
    strings: usize,
    streams: &[Vec<u8>],
) -> RomFixture {
    for (slot, stream) in streams.iter().enumerate() {
        let cell = strings + slot * CELL;
        rom = rom
            .write(table + slot * 4, &bus(cell).to_le_bytes())
            .write(cell, stream);
    }
    rom
}

fn plant_moves(mut rom: RomFixture, base: usize, stride: usize) -> RomFixture {
    for index in 0..CONTEST_MOVE_COUNT {
        let mut row = vec![FILLER; stride];
        row[0] = move_effect(index);
        row[1] = 0xF8 | move_category(index);
        rom = rom.write(base + index * stride, &row);
    }
    rom
}

fn plant_effects(mut rom: RomFixture, base: usize) -> RomFixture {
    for id in 0..CONTEST_EFFECT_COUNT {
        let row = [
            effect_type(id),
            u8::try_from(id * 5).unwrap(),
            u8::try_from(id).unwrap(),
            FILLER,
        ];
        rom = rom.write(base + id * 4, &row);
    }
    rom
}

fn full_rom() -> RomFixture {
    let descriptions: Vec<Vec<u8>> = (0..MOVE_DESCRIPTION_COUNT).map(description).collect();
    let texts: Vec<Vec<u8>> = (0..CONTEST_EFFECT_COUNT).map(effect_text).collect();
    let rom = plant_pointers(
        RomFixture::new().emerald_header(),
        DESCRIPTIONS,
        DESCRIPTION_STRINGS,
        &descriptions,
    );
    let rom = plant_pointers(rom, EFFECT_DESCRIPTIONS, EFFECT_STRINGS, &texts);
    plant_effects(plant_moves(rom, CONTEST_MOVES, 8), CONTEST_EFFECTS)
}

type Located = Result<(SummaryMovePlan, Vec<ReportLine>), GenRomProfileError>;

fn locate_in(name: &str, rom: RomFixture, entries: Vec<PackEntry>) -> Located {
    with_context(name, &rom.finish(), entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).map(|plan| (plan, report))
    })
}

fn refused(name: &str, rom: RomFixture) -> GenRomProfileError {
    locate_in(name, rom, pack()).expect_err("the ROM disagrees with the pack")
}

fn mismatch_id(err: &GenRomProfileError) -> &str {
    match err {
        GenRomProfileError::StructMismatch { id, .. } => id,
        other => panic!("expected StructMismatch, got {other:?}"),
    }
}

#[test]
fn all_four_tables_are_located_with_their_global_symbols() {
    let (plan, report) = locate_in("sm-ok", full_rom(), pack()).expect("matching tables");
    assert_eq!(plan.descriptions, bus(DESCRIPTIONS));
    assert_eq!(plan.contest_moves, bus(CONTEST_MOVES));
    assert_eq!(plan.contest_effects, bus(CONTEST_EFFECTS));
    assert_eq!(plan.effect_descriptions, bus(EFFECT_DESCRIPTIONS));

    let expected = [
        ("gMoveDescriptionPointers", DESCRIPTIONS, 354 * 4),
        ("gContestMoves", CONTEST_MOVES, 355 * 8),
        ("gContestEffects", CONTEST_EFFECTS, 48 * 4),
        (
            "gContestEffectDescriptionPointers",
            EFFECT_DESCRIPTIONS,
            48 * 4,
        ),
    ];
    assert_eq!(report.len(), expected.len());
    for (line, (symbol, offset, len)) in report.iter().zip(expected) {
        assert_eq!(line.id, format!("{PACK_ID}#{symbol}"));
        assert_eq!(line.addr, bus(offset));
        assert_eq!(line.len, u32::try_from(len).unwrap());
        assert_eq!(line.resolution, Resolution::StructDerived);
        assert_eq!(line.symbol, SymbolExpectation::Exact(symbol.to_owned()));
    }
}

#[test]
fn repeated_and_empty_texts_need_no_unique_string() {
    // Slots 20 and 21 share their text and slot 9 is a bare `EOS`; the table is
    // still found through the other slots' unique texts.
    assert_eq!(description(SHARED.0), description(SHARED.1));
    assert_eq!(description(EMPTY_SLOT), [0xFF]);
    locate_in("sm-shared", full_rom(), pack()).expect("shared and empty texts are fine");
}

#[test]
fn a_late_description_pointer_that_disagrees_is_refused() {
    let rom = full_rom().write(
        DESCRIPTIONS + 353 * 4,
        &bus(DESCRIPTION_STRINGS).to_le_bytes(),
    );
    let err = refused("sm-late-description", rom);
    assert_eq!(
        mismatch_id(&err),
        format!("{PACK_ID}#gMoveDescriptionPointers")
    );
}

#[test]
fn a_late_effect_description_pointer_that_disagrees_is_refused() {
    let rom = full_rom().write(
        EFFECT_DESCRIPTIONS + 47 * 4,
        &bus(EFFECT_STRINGS).to_le_bytes(),
    );
    let err = refused("sm-late-effect-text", rom);
    assert_eq!(
        mismatch_id(&err),
        format!("{PACK_ID}#gContestEffectDescriptionPointers")
    );
}

#[test]
fn a_late_contest_move_that_disagrees_is_refused() {
    let wrong = move_effect(354) ^ 1;
    let rom = full_rom().write(CONTEST_MOVES + 354 * 8, &[wrong]);
    let err = refused("sm-late-move", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gContestMoves"));

    let wrong = 0xF8 | ((move_category(200) + 1) % 5);
    let rom = full_rom().write(CONTEST_MOVES + 200 * 8 + 1, &[wrong]);
    let err = refused("sm-late-category", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gContestMoves"));
}

#[test]
fn a_late_contest_effect_that_disagrees_is_refused() {
    let rom = full_rom().write(CONTEST_EFFECTS + 47 * 4 + 2, &[99]);
    let err = refused("sm-late-effect", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gContestEffects"));
}

#[test]
fn a_stride_the_importer_does_not_read_matches_nowhere() {
    // Rows packed at the seven declared bytes, not the compiler's eight.
    let descriptions: Vec<Vec<u8>> = (0..MOVE_DESCRIPTION_COUNT).map(description).collect();
    let texts: Vec<Vec<u8>> = (0..CONTEST_EFFECT_COUNT).map(effect_text).collect();
    let rom = plant_pointers(
        RomFixture::new().emerald_header(),
        DESCRIPTIONS,
        DESCRIPTION_STRINGS,
        &descriptions,
    );
    let rom = plant_pointers(rom, EFFECT_DESCRIPTIONS, EFFECT_STRINGS, &texts);
    let rom = plant_effects(plant_moves(rom, CONTEST_MOVES, 7), CONTEST_EFFECTS);
    let err = refused("sm-stride", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gContestMoves"));
}

#[test]
fn a_second_copy_of_a_struct_table_is_ambiguous_not_guessed() {
    let rom = plant_moves(full_rom(), CONTEST_MOVES + 0x4000, 8);
    let err = refused("sm-twice-moves", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gContestMoves"));

    let rom = plant_effects(full_rom(), CONTEST_EFFECTS + 0x200);
    let err = refused("sm-twice-effects", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gContestEffects"));
}

#[test]
fn a_rom_without_the_tables_finds_none() {
    let err = refused("sm-absent", RomFixture::new().emerald_header());
    assert_eq!(
        mismatch_id(&err),
        format!("{PACK_ID}#gMoveDescriptionPointers")
    );
}

#[test]
fn a_pack_without_the_table_entry_is_refused() {
    let err = locate_in("sm-no-entry", full_rom(), Vec::new()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_table_entry_of_the_wrong_kind_is_refused() {
    let entry = image_entry(PACK_ID.to_owned(), 1, 1, 8, vec![0]).expect("an image entry");
    let err = locate_in("sm-wrong-kind", full_rom(), vec![entry]).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::WrongPackEntryKind { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_malformed_table_entry_is_refused_as_an_entry_shape() {
    let mut truncated = payload();
    truncated.truncate(truncated.len() - 3);
    let entries = vec![raw_entry(PACK_ID.to_owned(), truncated)];
    let err = locate_in("sm-truncated", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_pack_entry_the_domain_does_not_cover_is_refused() {
    let mut entries = pack();
    entries.push(raw_entry("summary-move/extra".to_owned(), vec![0xFF]));
    let err = locate_in("sm-extra", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "summary-move/*"),
        "{err}"
    );
}
