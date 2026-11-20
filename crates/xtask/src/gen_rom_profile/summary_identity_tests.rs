//! Summary-identity locator tests over a synthetic ROM and pack.
//!
//! The fixture plants the nature pointer table and the 8-byte-stride region
//! rows with their strings, the three special strings back to back, and the
//! conversion array, and builds the pack entry the extractor would write for
//! the same data. Each failure test breaks one fact: a late pointer, a second
//! copy, a stride the ROM does not use, strings that are not adjacent, or a
//! malformed or surplus entry.

use assets::pack::{
    SpecialLocationNames, SummaryIdentityTable, NATURE_COUNT, REGION_MAP_SECTION_COUNT,
    SPECIES_TO_NATIONAL_COUNT,
};
use pack_format::{image_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::ROM_BASE;

use super::error::GenRomProfileError;
use super::plan::{ReportLine, Resolution, SummaryIdentityPlan, SymbolExpectation};
use super::summary_identity::locate;
use super::tests::with_context;

const PACK_ID: &str = rom_import::SUMMARY_IDENTITY_PACK_ID;
const NATURES: usize = 0x1000;
const NATURE_STRINGS: usize = 0x1400;
const REGIONS: usize = 0x2000;
const REGION_STRINGS: usize = 0x4000;
const SPECIALS: usize = 0x8000;
const CONVERSIONS: usize = 0x9000;
const CELL: usize = 16;
const FILLER: u8 = 0xEE;
/// The section whose name is a bare `EOS`.
const EMPTY_SECTION: usize = 87;

const FERRY: [u8; 3] = [0x31, 0x32, 0xFF];
const SECRET_BASE: [u8; 3] = [0x33, 0x34, 0xFF];
const HIDEOUT: [u8; 3] = [0x35, 0x36, 0xFF];

fn byte(index: usize) -> u8 {
    u8::try_from(index % 200).expect("under 200") + 1
}

/// Short on purpose: the byte search cannot anchor on these.
fn nature(id: usize) -> Vec<u8> {
    vec![byte(id), 0xFF]
}

fn section(id: usize) -> Vec<u8> {
    if id == EMPTY_SECTION {
        vec![0xFF]
    } else {
        vec![byte(id), byte(id / 200), 0xFF]
    }
}

fn national(species: usize) -> u16 {
    u16::try_from((species * 7) % 412).unwrap()
}

fn payload() -> Vec<u8> {
    let natures: Vec<(u16, Vec<u8>)> = (0..NATURE_COUNT)
        .map(|id| (u16::try_from(id).unwrap(), nature(id)))
        .collect();
    let sections: Vec<(u16, Vec<u8>)> = (0..REGION_MAP_SECTION_COUNT)
        .map(|id| (u16::try_from(id).unwrap(), section(id)))
        .collect();
    SummaryIdentityTable::from_entries(
        natures.iter().map(|(id, s)| (*id, s.as_slice())),
        sections.iter().map(|(id, s)| (*id, s.as_slice())),
        SpecialLocationNames {
            ferry: &FERRY,
            secret_base: &SECRET_BASE,
            hideout: &HIDEOUT,
        },
        (1..=SPECIES_TO_NATIONAL_COUNT)
            .map(|species| (u16::try_from(species).unwrap(), national(species))),
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

fn plant_natures(mut rom: RomFixture, table: usize) -> RomFixture {
    for id in 0..NATURE_COUNT {
        let cell = NATURE_STRINGS + id * CELL;
        rom = rom
            .write(table + id * 4, &bus(cell).to_le_bytes())
            .write(cell, &nature(id));
    }
    rom
}

fn plant_regions(mut rom: RomFixture, table: usize, stride: usize) -> RomFixture {
    for id in 0..REGION_MAP_SECTION_COUNT {
        let cell = REGION_STRINGS + id * CELL;
        let row = table + id * stride;
        rom = rom
            .write(row, &[FILLER; 4])
            .write(row + 4, &bus(cell).to_le_bytes())
            .write(cell, &section(id));
    }
    rom
}

fn plant_specials(rom: RomFixture, at: usize) -> RomFixture {
    rom.write(at, &FERRY)
        .write(at + FERRY.len(), &SECRET_BASE)
        .write(at + FERRY.len() + SECRET_BASE.len(), &HIDEOUT)
}

fn plant_conversions(mut rom: RomFixture, at: usize) -> RomFixture {
    for species in 1..=SPECIES_TO_NATIONAL_COUNT {
        rom = rom.write(at + (species - 1) * 2, &national(species).to_le_bytes());
    }
    rom
}

fn full_rom() -> RomFixture {
    let rom = plant_natures(RomFixture::new().emerald_header(), NATURES);
    let rom = plant_regions(rom, REGIONS, 8);
    plant_conversions(plant_specials(rom, SPECIALS), CONVERSIONS)
}

/// A report line's symbol, offset, length, and how its symbol is expected.
type Expected = (&'static str, usize, usize, fn(String) -> SymbolExpectation);

type Located = Result<(SummaryIdentityPlan, Vec<ReportLine>), GenRomProfileError>;

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
fn all_six_roots_are_located_with_their_symbols() {
    let (plan, report) = locate_in("si-ok", full_rom(), pack()).expect("matching tables");
    assert_eq!(plan.natures, bus(NATURES));
    assert_eq!(plan.region_map_entries, bus(REGIONS));
    assert_eq!(plan.ferry, bus(SPECIALS));
    assert_eq!(plan.secret_base, bus(SPECIALS + 3));
    assert_eq!(plan.hideout, bus(SPECIALS + 6));
    assert_eq!(plan.species_to_national, bus(CONVERSIONS));

    // Reports come in the order the locators ran: specials, then the tables.
    let expected: [Expected; 6] = [
        ("gText_Ferry", SPECIALS, 3, SymbolExpectation::Exact),
        (
            "gText_SecretBase",
            SPECIALS + 3,
            3,
            SymbolExpectation::Exact,
        ),
        ("gText_Hideout", SPECIALS + 6, 3, SymbolExpectation::Exact),
        (
            "gNatureNamePointers",
            NATURES,
            25 * 4,
            SymbolExpectation::Exact,
        ),
        (
            "gRegionMapEntries",
            REGIONS,
            213 * 8,
            SymbolExpectation::Exact,
        ),
        (
            "sSpeciesToNationalPokedexNum",
            CONVERSIONS,
            411 * 2,
            SymbolExpectation::Local,
        ),
    ];
    assert_eq!(report.len(), expected.len());
    for (line, (symbol, offset, len, kind)) in report.iter().zip(expected) {
        assert_eq!(line.id, format!("{PACK_ID}#{symbol}"));
        assert_eq!(line.addr, bus(offset));
        assert_eq!(line.len, u32::try_from(len).unwrap());
        assert_eq!(line.symbol, kind(symbol.to_owned()), "{symbol}");
    }
    let tables = [3, 4].map(|i| report[i].resolution);
    assert_eq!(tables, [Resolution::StructDerived; 2]);
}

#[test]
fn a_short_string_elsewhere_does_not_confuse_the_special_strings() {
    // `HIDEOUT` also ends several map names; only the adjacent trio is a signature.
    let rom = full_rom().write(0xC000, &HIDEOUT).write(0xC100, &FERRY);
    let (plan, _) = locate_in("si-short", rom, pack()).expect("the trio is still unique");
    assert_eq!(plan.hideout, bus(SPECIALS + 6));
}

#[test]
fn special_strings_that_are_not_adjacent_are_not_found() {
    let rom = full_rom().write(SPECIALS + 3, &[FILLER, FILLER, FILLER]);
    let err = refused("si-gap", rom);
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == &format!("{PACK_ID}#gText_Ferry")),
        "{err}"
    );
}

#[test]
fn a_late_nature_pointer_that_disagrees_is_refused() {
    let rom = full_rom().write(NATURES + 24 * 4, &bus(NATURE_STRINGS).to_le_bytes());
    let err = refused("si-late-nature", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gNatureNamePointers"));
}

#[test]
fn a_late_region_name_that_disagrees_is_refused() {
    let rom = full_rom().write(REGIONS + 212 * 8 + 4, &bus(REGION_STRINGS).to_le_bytes());
    let err = refused("si-late-region", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gRegionMapEntries"));
}

#[test]
fn a_stride_the_importer_does_not_read_matches_nowhere() {
    // Rows packed at twelve bytes, not the compiler's eight.
    let rom = plant_natures(RomFixture::new().emerald_header(), NATURES);
    let rom = plant_regions(rom, REGIONS, 12);
    let rom = plant_conversions(plant_specials(rom, SPECIALS), CONVERSIONS);
    let err = refused("si-stride", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gRegionMapEntries"));
}

#[test]
fn a_second_copy_of_a_table_is_ambiguous_not_guessed() {
    let rom = plant_regions(full_rom(), REGIONS + 0x1_0000, 8);
    let err = refused("si-twice-regions", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gRegionMapEntries"));

    let rom = plant_natures(full_rom(), NATURES + 0x200);
    let err = refused("si-twice-natures", rom);
    assert_eq!(mismatch_id(&err), format!("{PACK_ID}#gNatureNamePointers"));

    let rom = plant_conversions(full_rom(), CONVERSIONS + 0x2000);
    let err = refused("si-twice-conversions", rom);
    assert!(
        matches!(&err, GenRomProfileError::Ambiguous { id, .. } if id == &format!("{PACK_ID}#sSpeciesToNationalPokedexNum")),
        "{err}"
    );
}

#[test]
fn a_conversion_that_disagrees_is_not_found() {
    let wrong = (national(411) + 1).to_le_bytes();
    let rom = full_rom().write(CONVERSIONS + 410 * 2, &wrong);
    let err = refused("si-late-conversion", rom);
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == &format!("{PACK_ID}#sSpeciesToNationalPokedexNum")),
        "{err}"
    );
}

#[test]
fn a_rom_without_the_tables_finds_none() {
    let err = refused("si-absent", RomFixture::new().emerald_header());
    assert!(matches!(err, GenRomProfileError::NotFound { .. }), "{err}");
}

#[test]
fn a_pack_without_the_table_entry_is_refused() {
    let err = locate_in("si-no-entry", full_rom(), Vec::new()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_table_entry_of_the_wrong_kind_is_refused() {
    let entry = image_entry(PACK_ID.to_owned(), 1, 1, 8, vec![0]).expect("an image entry");
    let err = locate_in("si-wrong-kind", full_rom(), vec![entry]).unwrap_err();
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
    let err = locate_in("si-truncated", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_pack_entry_the_domain_does_not_cover_is_refused() {
    let mut entries = pack();
    entries.push(raw_entry("summary-identity/extra".to_owned(), vec![0xFF]));
    let err = locate_in("si-extra", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "summary-identity/*"),
        "{err}"
    );
}
