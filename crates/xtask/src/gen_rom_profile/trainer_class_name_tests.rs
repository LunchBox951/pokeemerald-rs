//! Trainer class-name locator tests over a synthetic ROM and pack.
//!
//! The fixture plants the whole 858-byte fixed-width array and builds the pack
//! entry the extractor would write for it. The array repeats names (the four
//! `{PKMN} TRAINER` classes, `COOLTRAINER`, `TUBER`), so the locator has to
//! find the array as a whole. Each failure test breaks one fact: a missing,
//! duplicated, or altered array, a malformed or surplus pack entry.

use assets::TrainerClassNameTable;
use pack_format::{image_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::{ROM_BASE, ROM_SIZE};

use super::error::GenRomProfileError;
use super::plan::{ReportLine, Resolution, SymbolExpectation, TrainerClassNamePlan};
use super::tests::with_context;
use super::trainer_class_names::locate;

const PACK_ID: &str = rom_import::TRAINER_CLASS_NAME_PACK_ID;
const TABLE: usize = 0x2001;
const COPY: usize = 0x6000;

/// The rival's name: `{PKMN} TRAINER`.
const RIVAL: [u8; 11] = [
    0x53, 0x54, 0x00, 0xCE, 0xCC, 0xBB, 0xC3, 0xC8, 0xBF, 0xCC, 0xFF,
];

fn stream_of(id: usize) -> Vec<u8> {
    match id {
        0 | 1 | 50 | 65 => RIVAL.to_vec(),
        5 | 48 => vec![
            0xBD, 0xC9, 0xC9, 0xC6, 0xCE, 0xCC, 0xBB, 0xC3, 0xC8, 0xBF, 0xCC, 0xFF,
        ],
        18 | 19 => vec![0xCE, 0xCF, 0xBC, 0xBF, 0xCC, 0xFF],
        _ => vec![0xA1 + u8::try_from(id).unwrap(), 0xBB, 0xFF],
    }
}

fn streams() -> Vec<Vec<u8>> {
    (0..66).map(stream_of).collect()
}

fn rows() -> Vec<u8> {
    let mut out = Vec::new();
    for stream in streams() {
        out.extend_from_slice(&stream);
        out.resize(out.len() + 13 - stream.len(), 0);
    }
    out
}

fn payload() -> Vec<u8> {
    let streams = streams();
    TrainerClassNameTable::from_entries(
        streams
            .iter()
            .enumerate()
            .map(|(id, s)| (u16::try_from(id).unwrap(), s.as_slice())),
    )
    .expect("the fixture streams are valid")
    .encode()
}

fn pack() -> Vec<PackEntry> {
    vec![raw_entry(PACK_ID.to_owned(), payload())]
}

fn bus(offset: usize) -> u32 {
    ROM_BASE + u32::try_from(offset).expect("fits")
}

fn full_rom() -> RomFixture {
    RomFixture::new().emerald_header().write(TABLE, &rows())
}

type Located = Result<(TrainerClassNamePlan, Vec<ReportLine>), GenRomProfileError>;

fn locate_in(name: &str, rom: RomFixture, entries: Vec<PackEntry>) -> Located {
    with_context(name, &rom.finish(), entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).map(|plan| (plan, report))
    })
}

#[test]
fn the_whole_array_is_located_once_with_its_global_symbol() {
    let (plan, report) = locate_in("tcn-ok", full_rom(), pack()).expect("a matching ROM");
    assert_eq!(plan.addr, bus(TABLE));
    let [line] = report.as_slice() else {
        panic!("one report line, got {}", report.len())
    };
    assert_eq!(line.id, PACK_ID);
    assert_eq!(line.addr, bus(TABLE));
    assert_eq!(line.len, 858);
    assert_eq!(line.resolution, Resolution::UniqueSignature);
    assert_eq!(
        line.symbol,
        SymbolExpectation::Exact("gTrainerClassNames".to_owned())
    );
}

#[test]
fn a_duplicate_individual_name_elsewhere_is_not_ambiguous() {
    // Loose copies of repeated names must not matter; only the array does.
    let rom = full_rom()
        .write(0x5000, &RIVAL)
        .write(0x5100, &stream_of(5));
    let (plan, _) = locate_in("tcn-loose", rom, pack()).expect("only the array counts");
    assert_eq!(plan.addr, bus(TABLE));
}

#[test]
fn an_absent_array_is_not_found() {
    let mut altered = rows();
    // One name changed by a single glyph.
    altered[13 * 20] ^= 0x01;
    let rom = RomFixture::new().emerald_header().write(TABLE, &altered);
    let err = locate_in("tcn-absent", rom, pack()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == "gTrainerClassNames"),
        "{err}"
    );
}

#[test]
fn an_array_with_nonzero_padding_is_not_found() {
    let mut altered = rows();
    altered[13 * 7 + 12] = 0x01;
    let rom = RomFixture::new().emerald_header().write(TABLE, &altered);
    let err = locate_in("tcn-padding", rom, pack()).unwrap_err();
    assert!(matches!(&err, GenRomProfileError::NotFound { .. }), "{err}");
}

#[test]
fn two_whole_arrays_are_ambiguous_not_guessed() {
    let rom = full_rom().write(COPY, &rows());
    let err = locate_in("tcn-twice", rom, pack()).unwrap_err();
    match err {
        GenRomProfileError::Ambiguous { id, addrs } => {
            assert_eq!(id, "gTrainerClassNames");
            assert_eq!(addrs, [bus(TABLE), bus(COPY)]);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

#[test]
fn an_array_cut_by_the_end_of_the_image_is_not_found() {
    let rom = RomFixture::new()
        .emerald_header()
        .write(ROM_SIZE - 100, &rows()[..100]);
    let err = locate_in("tcn-cut", rom, pack()).unwrap_err();
    assert!(matches!(&err, GenRomProfileError::NotFound { .. }), "{err}");
}

#[test]
fn a_pack_without_the_table_entry_is_refused() {
    let err = locate_in("tcn-no-entry", full_rom(), Vec::new()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_table_entry_of_the_wrong_kind_is_refused() {
    let entry = image_entry(PACK_ID.to_owned(), 1, 1, 8, vec![0]).expect("an image entry");
    let err = locate_in("tcn-wrong-kind", full_rom(), vec![entry]).unwrap_err();
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
    let err = locate_in("tcn-truncated", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_pack_entry_the_domain_does_not_cover_is_refused() {
    let mut entries = pack();
    entries.push(raw_entry("trainer-class-name/extra".to_owned(), vec![0xFF]));
    let err = locate_in("tcn-extra", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "trainer-class-name/*"),
        "{err}"
    );
}
