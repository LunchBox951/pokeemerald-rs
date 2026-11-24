//! Trainer defeat-speech locator tests over a synthetic ROM and pack.
//!
//! The fixture plants a distinct stream per context after an `EOS` and builds
//! the pack entry the extractor would write for them. Isabelle's short stream
//! is planted three times, with the `trainerbattle` command naming the middle
//! copy. Each failure test breaks one fact: a missing or duplicated string, a
//! missing, duplicated or misleading command, a malformed or surplus entry.

use assets::{TrainerDefeatSpeechContext as Context, TrainerDefeatSpeechTable};
use pack_format::{image_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::{ROM_BASE, ROM_SIZE};

use super::error::GenRomProfileError;
use super::plan::{ReportLine, Resolution, SymbolExpectation, TrainerDefeatSpeechPlan};
use super::tests::with_context;
use super::trainer_defeat_speeches::locate;

const PACK_ID: &str = rom_import::TRAINER_DEFEAT_SPEECH_PACK_ID;
const ISABELLE: usize = 9;
const ISABELLE_SYMBOL: &str = "Route103_Text_IsabelleDefeated";
/// Isabelle's three identical copies; the command names the middle one.
const COPIES: [usize; 3] = [0x6000, 0x6100, 0x6200];
/// The `trainerbattle` command, deliberately unaligned.
const COMMAND: usize = 0x1001;

const fn stream_offset(id: usize) -> usize {
    0x2000 + id * 0x40
}

/// Eight bytes per context, distinct from every other context's; Isabelle's
/// is seven, as short as upstream's `Groan…`.
fn stream_of(id: usize) -> Vec<u8> {
    let base = 0xC0 + u8::try_from(id).unwrap();
    let mut stream: Vec<u8> = (0..6).map(|i| base + i).collect();
    if id != ISABELLE {
        stream.push(base + 6);
    }
    stream.push(0xFF);
    stream
}

fn payload() -> Vec<u8> {
    let streams: Vec<Vec<u8>> = (0..Context::ALL.len()).map(stream_of).collect();
    TrainerDefeatSpeechTable::from_entries(
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

fn command(defeat: u32) -> Vec<u8> {
    let mut bytes = rom_import::trainer_defeat_speech_witness_head(Context::Route103Isabelle)
        .expect("Isabelle has a witness")
        .to_vec();
    bytes.extend_from_slice(&bus(0x7000).to_le_bytes());
    bytes.extend_from_slice(&defeat.to_le_bytes());
    bytes
}

/// A string at `offset`, after an `EOS` that ends the string before it.
fn with_string(rom: RomFixture, offset: usize, stream: &[u8]) -> RomFixture {
    rom.write(offset - 1, &[0xFF]).write(offset, stream)
}

fn full_rom() -> RomFixture {
    let mut rom = RomFixture::new().emerald_header();
    for id in (0..Context::ALL.len()).filter(|id| *id != ISABELLE) {
        rom = with_string(rom, stream_offset(id), &stream_of(id));
    }
    for copy in COPIES {
        rom = with_string(rom, copy, &stream_of(ISABELLE));
    }
    rom.write(COMMAND, &command(bus(COPIES[1])))
}

type Located = Result<(TrainerDefeatSpeechPlan, Vec<ReportLine>), GenRomProfileError>;

fn locate_in(name: &str, rom: RomFixture, entries: Vec<PackEntry>) -> Located {
    with_context(name, &rom.finish(), entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).map(|plan| (plan, report))
    })
}

#[test]
fn every_speech_is_located_with_its_symbol_and_isabelles_by_her_command() {
    let (plan, report) = locate_in("tds-ok", full_rom(), pack()).expect("a matching ROM");
    assert_eq!(plan.speeches.len(), Context::ALL.len());
    assert_eq!(report.len(), Context::ALL.len());
    for (id, context) in Context::ALL.into_iter().enumerate() {
        let speech = &plan.speeches[id];
        let line = &report[id];
        assert_eq!(speech.symbol, context.symbol());
        assert_eq!(line.id, PACK_ID);
        assert_eq!(
            line.symbol,
            SymbolExpectation::Local(context.symbol().to_owned())
        );
        assert_eq!(usize::try_from(line.len).unwrap(), stream_of(id).len());
        if id == ISABELLE {
            // The middle of three identical copies, found through the command.
            assert_eq!(speech.addr, bus(COPIES[1]));
            assert_eq!(speech.script, Some(bus(COMMAND)));
            assert_eq!(line.resolution, Resolution::StructDerived);
        } else {
            assert_eq!(speech.addr, bus(stream_offset(id)));
            assert_eq!(speech.script, None);
            assert_eq!(line.resolution, Resolution::UniqueSignature);
        }
        assert_eq!(line.addr, speech.addr);
    }
}

#[test]
fn a_longer_string_ending_in_a_stream_is_not_a_second_start() {
    // A non-`EOS` byte then context 0's stream: a suffix match, not a start.
    let rom = full_rom()
        .write(0x4000, &[0xAD])
        .write(0x4001, &stream_of(0));
    let (plan, _) = locate_in("tds-suffix", rom, pack()).expect("only the string start counts");
    assert_eq!(plan.speeches[0].addr, bus(stream_offset(0)));
}

#[test]
fn a_stream_absent_from_the_rom_is_not_found() {
    let rom = full_rom()
        .write(stream_offset(3) - 1, &[0xAD])
        .write(stream_offset(3), &[0xAD; 8]);
    let err = locate_in("tds-absent", rom, pack()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == Context::Route103AmyRematch.symbol()),
        "{err}"
    );
}

#[test]
fn a_stream_found_twice_is_ambiguous_not_guessed() {
    let rom = with_string(full_rom(), 0x5000, &stream_of(4));
    let err = locate_in("tds-twice", rom, pack()).unwrap_err();
    match err {
        GenRomProfileError::Ambiguous { id, addrs } => {
            assert_eq!(id, Context::Route103LivRematch.symbol());
            assert_eq!(addrs, [bus(stream_offset(4)), bus(0x5000)]);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

#[test]
fn suffix_matches_past_the_search_cap_do_not_hide_a_second_start() {
    // Fifteen suffix matches, then a second string start of context 5's stream.
    let mut rom = full_rom();
    for i in 0..15 {
        let at = 0x8000 + i * 0x20;
        rom = rom.write(at, &[0xAD]).write(at + 1, &stream_of(5));
    }
    let rom = with_string(rom, 0x9000, &stream_of(5));
    match locate_in("tds-capped", rom, pack()) {
        Err(GenRomProfileError::Ambiguous { addrs, .. }) => {
            assert_eq!(addrs, [bus(stream_offset(5)), bus(0x9000)]);
        }
        other => panic!("expected Ambiguous, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn sixteen_leading_suffix_matches_do_not_hide_the_only_start() {
    let mut rom = full_rom();
    for i in 0..16 {
        let at = 0x1100 + i * 0x20;
        rom = rom.write(at, &[0xAD]).write(at + 1, &stream_of(5));
    }
    let (plan, _) = locate_in("tds-lead", rom, pack()).expect("the only start is found");
    assert_eq!(plan.speeches[5].addr, bus(stream_offset(5)));
}

#[test]
fn a_pack_without_the_table_entry_is_refused() {
    let err = locate_in("tds-no-entry", full_rom(), Vec::new()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_table_entry_of_the_wrong_kind_is_refused() {
    let entry = image_entry(PACK_ID.to_owned(), 1, 1, 8, vec![0]).expect("an image entry");
    let err = locate_in("tds-wrong-kind", full_rom(), vec![entry]).unwrap_err();
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
    let err = locate_in("tds-truncated", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_pack_entry_the_domain_does_not_cover_is_refused() {
    let mut entries = pack();
    entries.push(raw_entry(
        "trainer-defeat-speech/extra".to_owned(),
        vec![0xFF],
    ));
    let err = locate_in("tds-extra", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "trainer-defeat-speech/*"),
        "{err}"
    );
}

#[test]
fn isabelles_speech_without_its_command_is_not_found() {
    let rom = full_rom().write(COMMAND, &[0xFF; 14]);
    let err = locate_in("tds-no-command", rom, pack()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == ISABELLE_SYMBOL),
        "{err}"
    );
}

#[test]
fn two_commands_are_ambiguous_even_when_both_name_identical_text() {
    // A second command selecting another copy of the same bytes.
    let rom = full_rom().write(0x1500, &command(bus(COPIES[2])));
    let err = locate_in("tds-two-commands", rom, pack()).unwrap_err();
    match err {
        GenRomProfileError::Ambiguous { id, addrs } => {
            assert_eq!(id, ISABELLE_SYMBOL);
            assert_eq!(addrs, [bus(COMMAND), bus(0x1500)]);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

fn struct_mismatch(name: &str, rom: RomFixture) -> String {
    match locate_in(name, rom, pack()) {
        Err(GenRomProfileError::StructMismatch { id, reason }) => {
            assert_eq!(id, ISABELLE_SYMBOL);
            reason
        }
        other => panic!("expected StructMismatch, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_command_naming_other_text_is_refused() {
    // The pointer addresses a string start holding different bytes.
    let rom = with_string(full_rom(), 0x7000, &[0xAB; 7])
        .write(0x7007, &[0xFF])
        .write(COMMAND, &command(bus(0x7000)));
    let reason = struct_mismatch("tds-other-text", rom);
    assert!(
        reason.contains("does not address the expected text"),
        "{reason}"
    );
}

#[test]
fn a_command_naming_a_suffix_that_does_not_start_a_string_is_refused() {
    let rom = full_rom()
        .write(0x7000, &[0xAD])
        .write(0x7001, &stream_of(ISABELLE))
        .write(COMMAND, &command(bus(0x7001)));
    let reason = struct_mismatch("tds-suffix-target", rom);
    assert!(
        reason.contains("does not address the expected text"),
        "{reason}"
    );
}

#[test]
fn a_command_pointing_off_the_cartridge_is_refused() {
    let rom = full_rom().write(COMMAND, &command(0x0200_0000));
    let reason = struct_mismatch("tds-off-cartridge", rom);
    assert!(reason.contains("not a cartridge address"), "{reason}");
}

#[test]
fn a_command_cut_by_the_end_of_the_image_is_refused() {
    // The head fits in the last eight bytes; the two text pointers do not.
    let cut = ROM_SIZE - 8;
    let rom = full_rom()
        .write(COMMAND, &[0xFF; 14])
        .write(cut, &command(bus(COPIES[1]))[..8]);
    let reason = struct_mismatch("tds-cut-command", rom);
    assert!(reason.contains("runs off the image"), "{reason}");
}
