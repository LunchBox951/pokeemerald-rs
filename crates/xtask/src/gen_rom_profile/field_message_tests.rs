//! Field-message locator tests over a synthetic ROM and pack.
//!
//! The fixture plants the upstream string `gText_PkmnFainted_FldPsn` after an
//! `EOS` and builds the pack entry the extractor would write for it. Each
//! failure test breaks one fact: a missing or duplicated string, a malformed
//! or surplus entry.

use assets::pack::{FieldMessage, FieldMessageTable};
use pack_format::{image_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::ROM_BASE;

use super::error::GenRomProfileError;
use super::field_messages::locate;
use super::plan::{FieldMessagePlan, ReportLine, Resolution, SymbolExpectation};
use super::tests::with_context;

const PACK_ID: &str = rom_import::FIELD_MESSAGE_PACK_ID;
const STRING: usize = 0x2000;

/// `{STR_VAR_1} fainted…\p\n` and its `EOS`, spelled through `charmap.txt`.
const FAINTED: [u8; 14] = [
    0xFD, 0x02, 0x00, 0xDA, 0xD5, 0xDD, 0xE2, 0xE8, 0xD9, 0xD8, 0xB0, 0xFB, 0xFE, 0xFF,
];

fn payload(stream: &[u8]) -> Vec<u8> {
    FieldMessageTable::from_entries([(FieldMessage::PkmnFaintedFldPsn.id(), stream)])
        .expect("the fixture stream is valid")
        .encode()
}

fn pack() -> Vec<PackEntry> {
    vec![raw_entry(PACK_ID.to_owned(), payload(&FAINTED))]
}

/// The string at `offset`, after an `EOS` that ends the string before it.
fn with_string(rom: RomFixture, offset: usize) -> RomFixture {
    rom.write(offset - 1, &[0xFF]).write(offset, &FAINTED)
}

fn full_rom() -> RomFixture {
    with_string(RomFixture::new().emerald_header(), STRING)
}

type Located = Result<(FieldMessagePlan, Vec<ReportLine>), GenRomProfileError>;

fn locate_in(name: &str, rom: RomFixture, entries: Vec<PackEntry>) -> Located {
    with_context(name, &rom.finish(), entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).map(|plan| (plan, report))
    })
}

fn bus(offset: usize) -> u32 {
    ROM_BASE + u32::try_from(offset).expect("fits")
}

#[test]
fn the_string_is_located_once_with_its_global_symbol() {
    let (plan, report) = locate_in("fm-ok", full_rom(), pack()).expect("a matching string");
    let [message] = plan.messages.as_slice() else {
        panic!("one message, got {}", plan.messages.len())
    };
    assert_eq!(message.symbol, "gText_PkmnFainted_FldPsn");
    assert_eq!(message.addr, bus(STRING));

    let [line] = report.as_slice() else {
        panic!("one report line, got {}", report.len())
    };
    assert_eq!(line.id, PACK_ID);
    assert_eq!(line.addr, bus(STRING));
    assert_eq!(line.len, 14);
    assert_eq!(line.resolution, Resolution::UniqueSignature);
    assert_eq!(
        line.symbol,
        SymbolExpectation::Exact("gText_PkmnFainted_FldPsn".to_owned())
    );
}

#[test]
fn a_longer_string_ending_in_the_stream_is_not_a_second_start() {
    // "…" then the stream: it matches as a suffix but does not begin a string.
    let rom = full_rom()
        .write(STRING + 0x100, &[0xAD])
        .write(STRING + 0x101, &FAINTED);
    let (plan, _) = locate_in("fm-suffix", rom, pack()).expect("only the string start counts");
    assert_eq!(plan.messages[0].addr, bus(STRING));
}

#[test]
fn a_string_absent_from_the_rom_is_not_found() {
    let err = locate_in("fm-absent", RomFixture::new().emerald_header(), pack()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == "gText_PkmnFainted_FldPsn"),
        "{err}"
    );
}

#[test]
fn a_string_found_twice_is_ambiguous_not_guessed() {
    let rom = with_string(full_rom(), STRING + 0x800);
    let err = locate_in("fm-twice", rom, pack()).unwrap_err();
    match err {
        GenRomProfileError::Ambiguous { id, addrs } => {
            assert_eq!(id, "gText_PkmnFainted_FldPsn");
            assert_eq!(addrs, [bus(STRING), bus(STRING + 0x800)]);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

#[test]
fn a_pack_without_the_table_entry_is_refused() {
    let err = locate_in("fm-no-entry", full_rom(), Vec::new()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_table_entry_of_the_wrong_kind_is_refused() {
    let entry = image_entry(PACK_ID.to_owned(), 1, 1, 8, vec![0]).expect("an image entry");
    let err = locate_in("fm-wrong-kind", full_rom(), vec![entry]).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::WrongPackEntryKind { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_malformed_table_entry_is_refused_as_an_entry_shape() {
    let mut truncated = payload(&FAINTED);
    truncated.truncate(truncated.len() - 3);
    let entries = vec![raw_entry(PACK_ID.to_owned(), truncated)];
    let err = locate_in("fm-truncated", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_pack_entry_the_domain_does_not_cover_is_refused() {
    let mut entries = pack();
    entries.push(raw_entry("field-message/extra".to_owned(), vec![0xFF]));
    let err = locate_in("fm-extra", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "field-message/*"),
        "{err}"
    );
}
