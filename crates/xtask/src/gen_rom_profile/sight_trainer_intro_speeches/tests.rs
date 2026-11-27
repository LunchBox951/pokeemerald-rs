//! Sight-trainer intro-speech locator tests over a synthetic ROM and pack.
//!
//! The fixture plants a distinct stream per script after an `EOS`, a
//! `trainerbattle` command per script naming it (deliberately unaligned), and
//! rematch commands naming verbatim copies of Amy's and Liv's streams. Each
//! failure test breaks one fact: a missing, duplicated or misleading command,
//! or a malformed or surplus pack entry.

use assets::{SightTrainerIntroSpeechTable, SIGHT_TRAINER_INTRO_SCRIPTS};
use pack_format::{image_entry, raw_entry, PackEntry};
use rom_import::fixture::RomFixture;
use rom_import::{ROM_BASE, ROM_SIZE};

use super::super::error::GenRomProfileError;
use super::super::plan::{ReportLine, Resolution, SightTrainerIntroSpeechPlan, SymbolExpectation};
use super::super::tests::with_context;
use super::locate;

const PACK_ID: &str = rom_import::SIGHT_TRAINER_INTRO_SPEECH_PACK_ID;
const AMY: usize = 0;
const LIV: usize = 4;
/// Where the rematch copies of Amy's and Liv's streams sit.
const REMATCH_COPIES: [usize; 2] = [0x6000, 0x6100];
/// `TRAINER_BATTLE_REMATCH_DOUBLE`: a rematch command's type byte.
const REMATCH_DOUBLE: u8 = 7;

const fn stream_offset(position: usize) -> usize {
    0x2000 + position * 0x40
}

/// Where the script's command is planted, unaligned.
const fn command_offset(position: usize) -> usize {
    0x1001 + position * 0x20
}

fn stream_of(position: usize) -> Vec<u8> {
    let base = 0xC0 + u8::try_from(position).unwrap();
    vec![base, base + 1, base + 2, 0xFF]
}

fn payload() -> Vec<u8> {
    let streams: Vec<Vec<u8>> = (0..SIGHT_TRAINER_INTRO_SCRIPTS.len())
        .map(stream_of)
        .collect();
    SightTrainerIntroSpeechTable::from_entries(
        SIGHT_TRAINER_INTRO_SCRIPTS
            .into_iter()
            .zip(streams.iter().map(Vec::as_slice)),
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

fn command(script: &str, intro: u32) -> Vec<u8> {
    let mut bytes = rom_import::sight_trainer_intro_witness_head(script)
        .expect("every inventory script has a witness")
        .to_vec();
    bytes.extend_from_slice(&intro.to_le_bytes());
    bytes.extend_from_slice(&bus(0x7000).to_le_bytes());
    bytes
}

/// A string at `offset`, after an `EOS` that ends the string before it.
fn with_string(rom: RomFixture, offset: usize, stream: &[u8]) -> RomFixture {
    rom.write(offset - 1, &[0xFF]).write(offset, stream)
}

fn full_rom() -> RomFixture {
    let mut rom = RomFixture::new().emerald_header();
    for (position, script) in SIGHT_TRAINER_INTRO_SCRIPTS.into_iter().enumerate() {
        rom = with_string(rom, stream_offset(position), &stream_of(position)).write(
            command_offset(position),
            &command(script, bus(stream_offset(position))),
        );
    }
    // A rematch command for each twin: same trainer, other battle type.
    for ((position, copy), at) in [AMY, LIV]
        .into_iter()
        .zip(REMATCH_COPIES)
        .zip([0x4001, 0x4021])
    {
        let mut rematch = command(SIGHT_TRAINER_INTRO_SCRIPTS[position], bus(copy));
        rematch[1] = REMATCH_DOUBLE;
        rom = with_string(rom, copy, &stream_of(position)).write(at, &rematch);
    }
    rom
}

type Located = Result<(SightTrainerIntroSpeechPlan, Vec<ReportLine>), GenRomProfileError>;

fn locate_in(name: &str, rom: RomFixture, entries: Vec<PackEntry>) -> Located {
    with_context(name, &rom.finish(), entries, |ctx| {
        let mut report = Vec::new();
        locate(ctx, &mut report).map(|plan| (plan, report))
    })
}

#[test]
fn every_script_is_located_through_its_command() {
    let (plan, report) = locate_in("stis-ok", full_rom(), pack()).expect("a matching ROM");
    assert_eq!(plan.speeches.len(), SIGHT_TRAINER_INTRO_SCRIPTS.len());
    assert_eq!(report.len(), SIGHT_TRAINER_INTRO_SCRIPTS.len());
    for (position, script) in SIGHT_TRAINER_INTRO_SCRIPTS.into_iter().enumerate() {
        let speech = &plan.speeches[position];
        let line = &report[position];
        assert_eq!(speech.symbol, script);
        assert_eq!(speech.addr, bus(stream_offset(position)));
        assert_eq!(speech.script, bus(command_offset(position)));
        assert_eq!(line.id, PACK_ID);
        assert_eq!(line.symbol, SymbolExpectation::Local(script.to_owned()));
        assert_eq!(
            usize::try_from(line.len).unwrap(),
            stream_of(position).len()
        );
        assert_eq!(line.resolution, Resolution::StructDerived);
        assert_eq!(line.addr, speech.addr);
    }
}

#[test]
fn a_twin_is_told_from_its_sibling_and_from_rematch_copies_by_its_command() {
    let (plan, _) = locate_in("stis-twins", full_rom(), pack()).expect("a matching ROM");
    assert_ne!(plan.speeches[AMY].script, plan.speeches[LIV].script);
    assert_eq!(plan.speeches[AMY].addr, bus(stream_offset(AMY)));
    assert_eq!(plan.speeches[LIV].addr, bus(stream_offset(LIV)));
}

#[test]
fn a_command_naming_the_siblings_text_leaves_the_script_not_found() {
    let rom = full_rom().write(
        command_offset(AMY),
        &command(SIGHT_TRAINER_INTRO_SCRIPTS[AMY], bus(stream_offset(LIV))),
    );
    let err = locate_in("stis-swapped", rom, pack()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == SIGHT_TRAINER_INTRO_SCRIPTS[AMY]),
        "{err}"
    );
}

#[test]
fn a_missing_command_is_not_found() {
    for (position, script) in SIGHT_TRAINER_INTRO_SCRIPTS.into_iter().enumerate() {
        let rom = full_rom().write(command_offset(position), &[0xFF; 14]);
        let err = locate_in("stis-no-command", rom, pack()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if id == script),
            "{script}: {err}"
        );
    }
}

#[test]
fn a_command_of_another_trainer_or_battle_type_is_not_the_witness() {
    let good = command(SIGHT_TRAINER_INTRO_SCRIPTS[7], bus(stream_offset(7)));
    // Opcode, battle type, either trainer byte, either local-ID byte.
    for index in 0..rom_import::SIGHT_TRAINER_INTRO_POINTER_OFFSET {
        let mut bad = good.clone();
        bad[index] ^= 1;
        let rom = full_rom().write(command_offset(7), &bad);
        let err = locate_in("stis-head", rom, pack()).unwrap_err();
        assert!(
            matches!(&err, GenRomProfileError::NotFound { id } if id == SIGHT_TRAINER_INTRO_SCRIPTS[7]),
            "{index}: {err}"
        );
    }
}

#[test]
fn two_commands_naming_the_text_are_ambiguous() {
    let rom = full_rom().write(
        0x5001,
        &command(SIGHT_TRAINER_INTRO_SCRIPTS[3], bus(stream_offset(3))),
    );
    let err = locate_in("stis-two-commands", rom, pack()).unwrap_err();
    match err {
        GenRomProfileError::Ambiguous { id, addrs } => {
            assert_eq!(id, SIGHT_TRAINER_INTRO_SCRIPTS[3]);
            assert_eq!(addrs, [bus(command_offset(3)), bus(0x5001)]);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

#[test]
fn a_command_naming_a_suffix_that_does_not_start_a_string_is_not_the_witness() {
    let rom = full_rom()
        .write(0x7000, &[0xAD])
        .write(0x7001, &stream_of(2))
        .write(
            command_offset(2),
            &command(SIGHT_TRAINER_INTRO_SCRIPTS[2], bus(0x7001)),
        );
    let err = locate_in("stis-suffix", rom, pack()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == SIGHT_TRAINER_INTRO_SCRIPTS[2]),
        "{err}"
    );
}

#[test]
fn a_pointer_off_the_cartridge_or_a_cut_command_is_not_the_witness() {
    let rom = full_rom().write(
        command_offset(2),
        &command(SIGHT_TRAINER_INTRO_SCRIPTS[2], 0x0200_0000),
    );
    let err = locate_in("stis-off-cartridge", rom, pack()).unwrap_err();
    assert!(matches!(&err, GenRomProfileError::NotFound { .. }), "{err}");

    // The head fits in the last eight bytes; the pointer after it does not.
    let rom = full_rom().write(command_offset(2), &[0xFF; 14]).write(
        ROM_SIZE - 8,
        &command(SIGHT_TRAINER_INTRO_SCRIPTS[2], bus(stream_offset(2)))[..8],
    );
    let err = locate_in("stis-cut", rom, pack()).unwrap_err();
    assert!(matches!(&err, GenRomProfileError::NotFound { .. }), "{err}");
}

#[test]
fn a_pack_without_the_table_entry_is_refused() {
    let err = locate_in("stis-no-entry", full_rom(), Vec::new()).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::MissingPackEntry(id) if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_table_entry_of_the_wrong_kind_is_refused() {
    let entry = image_entry(PACK_ID.to_owned(), 1, 1, 8, vec![0]).expect("an image entry");
    let err = locate_in("stis-wrong-kind", full_rom(), vec![entry]).unwrap_err();
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
    let err = locate_in("stis-truncated", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::EntryShape { id, .. } if id == PACK_ID),
        "{err}"
    );
}

#[test]
fn a_pack_entry_the_domain_does_not_cover_is_refused() {
    let mut entries = pack();
    entries.push(raw_entry(
        "sight-trainer-intro-speech/extra".to_owned(),
        vec![0xFF],
    ));
    let err = locate_in("stis-extra", full_rom(), entries).unwrap_err();
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id, .. } if id == "sight-trainer-intro-speech/*"),
        "{err}"
    );
}

#[test]
fn a_failed_location_appends_no_report_lines() {
    let rom = full_rom().write(command_offset(8), &[0xFF; 14]);
    let result = with_context("stis-no-lines", &rom.finish(), pack(), |ctx| {
        let mut report = Vec::new();
        let result = locate(ctx, &mut report);
        (result.is_err(), report.len())
    });
    assert_eq!(result, (true, 0));
}
