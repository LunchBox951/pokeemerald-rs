//! Cry locator tests over a synthetic ROM and pack.
//!
//! The ROM plants both 388-record tables and 388 DPCM `WaveData` objects; the
//! pack holds the table and the samples those objects decode to.

use assets::audio::{
    CryTable, DirectSoundSample, Sample, SampleId, CRY_TABLE_LEN, CRY_TABLE_PACK_ID,
};
use pack_format::{raw_entry, PackEntry};
use rom_import::fixture::RomFixture;

use super::locate_cry;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::plan::ReportLine;
use crate::gen_rom_profile::tests::with_context;

const NORMAL: usize = 0x8000;
const REVERSE: usize = 0xA000;
const SAMPLES: usize = 0x1_0000;
const STRIDE: usize = 0x40;
const FREQ: u32 = 0x00A4_4000;
/// A first sample of 120, then deltas `+1 +4 +49 -64 -1`.
const PAYLOAD: [u8; 4] = [0x78, 0xF1, 0x27, 0x8F];
const DECODED: [i8; 6] = [120, 121, 125, -82, 110, 109];

fn id(index: usize) -> String {
    format!("audio/sample/direct-sound/cry/c{index}")
}

fn bus(offset: usize) -> u32 {
    0x0800_0000 + u32::try_from(offset).unwrap()
}

fn record(kind: u8, wav: u32) -> [u8; 12] {
    let w = wav.to_le_bytes();
    [kind, 60, 0, 0, w[0], w[1], w[2], w[3], 0xFF, 0, 0xFF, 0]
}

fn header(size: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(1u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(FREQ.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(size.to_le_bytes());
    out
}

fn rom() -> Vec<u8> {
    let mut rom = RomFixture::new().emerald_header();
    for index in 0..CRY_TABLE_LEN {
        let at = SAMPLES + index * STRIDE;
        rom = rom
            .write(NORMAL + index * 12, &record(0x20, bus(at)))
            .write(REVERSE + index * 12, &record(0x30, bus(at)))
            .write(at, &header(5))
            .write(at + 16, &PAYLOAD);
    }
    rom.finish()
}

fn sample_entry(index: usize) -> PackEntry {
    let sample = DirectSoundSample::new(FREQ, None, 5, DECODED.to_vec()).unwrap();
    raw_entry(id(index), Sample::DirectSound(sample).encode())
}

fn table_entry(ids: Vec<String>) -> PackEntry {
    let table = CryTable::new(ids.into_iter().map(SampleId).collect()).unwrap();
    raw_entry(CRY_TABLE_PACK_ID.to_owned(), table.encode())
}

/// The table and every sample.
fn entries() -> Vec<PackEntry> {
    let mut entries = vec![table_entry((0..CRY_TABLE_LEN).map(id).collect())];
    entries.extend((0..CRY_TABLE_LEN).map(sample_entry));
    entries
}

fn locate(
    name: &str,
    rom: &[u8],
    entries: Vec<PackEntry>,
) -> (
    Result<Option<super::CryPlan>, GenRomProfileError>,
    Vec<ReportLine>,
) {
    with_context(name, rom, entries, |ctx| {
        let mut report = Vec::new();
        let result = locate_cry(ctx, &mut report);
        (result, report)
    })
}

#[test]
fn a_pack_without_cries_plans_none() {
    let (plan, report) = locate("cry-none", &rom(), Vec::new());
    assert!(plan.expect("no cries is not a fault").is_none());
    assert!(report.is_empty());
}

#[test]
fn both_tables_and_every_sample_are_planned_from_their_records() {
    let (plan, report) = locate("cry-planned", &rom(), entries());
    let plan = plan.expect("a well-formed ROM").expect("a cry plan");
    assert_eq!(plan.normal, bus(NORMAL));
    assert_eq!(plan.reverse, bus(REVERSE));
    assert_eq!(plan.samples.len(), CRY_TABLE_LEN);
    for (index, sample) in plan.samples.iter().enumerate() {
        assert_eq!(sample.id, id(index));
        assert_eq!(sample.addr, bus(SAMPLES + index * STRIDE));
        assert_eq!((sample.header_len, sample.data_len), (16, 5));
    }
    assert_eq!(report.len(), 2 + CRY_TABLE_LEN);
}

#[test]
fn cry_samples_without_a_table_are_refused() {
    let (plan, _) = locate("cry-no-table", &rom(), vec![sample_entry(0)]);
    let err = plan.expect_err("samples need their table");
    assert!(
        matches!(&err, GenRomProfileError::MissingPackEntry(found) if found == CRY_TABLE_PACK_ID),
        "{err}"
    );
}

#[test]
fn a_table_that_does_not_decode_is_refused() {
    let (plan, _) = locate(
        "cry-bad-table",
        &rom(),
        vec![raw_entry(CRY_TABLE_PACK_ID.to_owned(), vec![9])],
    );
    let err = plan.expect_err("a truncated table");
    assert!(
        matches!(err, GenRomProfileError::EntryShape { .. }),
        "{err}"
    );
}

#[test]
fn a_table_and_its_samples_must_agree() {
    // A named sample the pack lacks.
    let mut missing = entries();
    missing.remove(6);
    let err = locate("cry-missing", &rom(), missing)
        .0
        .expect_err("a sample is absent");
    assert!(
        matches!(err, GenRomProfileError::StructMismatch { .. }),
        "{err}"
    );

    // A pack sample the table does not name.
    let mut extra = entries();
    extra.push(sample_entry(CRY_TABLE_LEN));
    let err = locate("cry-extra", &rom(), extra)
        .0
        .expect_err("a sample is unnamed");
    assert!(
        matches!(err, GenRomProfileError::StructMismatch { .. }),
        "{err}"
    );

    // A table naming one sample twice.
    let mut ids: Vec<String> = (0..CRY_TABLE_LEN).map(id).collect();
    ids[1] = ids[0].clone();
    let mut repeated = entries();
    repeated[0] = table_entry(ids);
    let err = locate("cry-repeated", &rom(), repeated)
        .0
        .expect_err("a repeated sample");
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { reason, .. } if reason.contains("twice")),
        "{err}"
    );

    // A table naming a sample outside the cry family.
    let ids: Vec<String> = (0..CRY_TABLE_LEN)
        .map(|index| format!("audio/sample/direct-sound/other{index}"))
        .collect();
    let mut foreign = entries();
    foreign[0] = table_entry(ids);
    let err = locate("cry-foreign", &rom(), foreign)
        .0
        .expect_err("a foreign sample");
    assert!(
        matches!(err, GenRomProfileError::StructMismatch { .. }),
        "{err}"
    );
}

#[test]
fn a_rom_without_the_tables_is_not_found() {
    let mut blank = RomFixture::new().emerald_header().finish();
    blank[0x3000] = 0;
    let err = locate("cry-absent", &blank, entries())
        .0
        .expect_err("no table");
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == "gCryTable"),
        "{err}"
    );
}

#[test]
fn a_second_copy_of_a_table_is_ambiguous() {
    let mut twice = rom();
    let copy: Vec<u8> = twice[REVERSE..REVERSE + CRY_TABLE_LEN * 12].to_vec();
    twice[0x20_0000..0x20_0000 + copy.len()].copy_from_slice(&copy);
    let err = locate("cry-twice", &twice, entries())
        .0
        .expect_err("two reverse tables");
    assert!(
        matches!(&err, GenRomProfileError::Ambiguous { id, addrs } if id == "gCryTable_Reverse" && addrs.len() == 2),
        "{err}"
    );
}

#[test]
fn a_longer_run_of_records_is_no_table() {
    let mut longer = rom();
    let after = NORMAL + CRY_TABLE_LEN * 12;
    longer[after..after + 12].copy_from_slice(&record(0x20, bus(SAMPLES)));
    let err = locate("cry-long", &longer, entries())
        .0
        .expect_err("389 records");
    assert!(
        matches!(&err, GenRomProfileError::NotFound { id } if id == "gCryTable"),
        "{err}"
    );
}

#[test]
fn a_rom_the_importer_cannot_decode_is_refused() {
    // The reverse table names another sample at position 4.
    let mut unpaired = rom();
    let word = REVERSE + 4 * 12 + 4;
    unpaired[word..word + 4].copy_from_slice(&bus(SAMPLES + 9 * STRIDE).to_le_bytes());
    let err = locate("cry-unpaired", &unpaired, entries())
        .0
        .expect_err("unpaired tables");
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { reason, .. } if reason.contains("do not decode")),
        "{err}"
    );

    // A header whose size the pack does not share.
    let mut resized = rom();
    let size = SAMPLES + 3 * STRIDE + 12;
    resized[size..size + 4].copy_from_slice(&6u32.to_le_bytes());
    let err = locate("cry-resized", &resized, entries())
        .0
        .expect_err("a wrong size");
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { reason, .. } if reason.contains("do not decode")),
        "{err}"
    );
}

#[test]
fn a_decoded_sample_must_equal_the_pack_down_to_the_guard() {
    // A logical sample that differs.
    let mut pcm = rom();
    pcm[SAMPLES + 3 * STRIDE + 16 + 2] = 0x37;
    let err = locate("cry-pcm", &pcm, entries())
        .0
        .expect_err("different PCM");
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id: found, reason } if *found == id(3) && reason.contains("differs")),
        "{err}"
    );

    // Only the guard differs: index 5 is the low nibble of the last byte.
    let mut guard = rom();
    guard[SAMPLES + 5 * STRIDE + 16 + 3] = 0x8E;
    let err = locate("cry-guard", &guard, entries())
        .0
        .expect_err("a different guard");
    assert!(
        matches!(&err, GenRomProfileError::StructMismatch { id: found, reason } if *found == id(5) && reason.contains("differs")),
        "{err}"
    );
}

/// The committed profile's cry roots are what the locator derives from the
/// supported ROM and the extracted pack.
#[cfg(feature = "rom-drift")]
#[test]
#[ignore = "needs $POKEEMERALD_ROM and an extracted pack"]
fn the_committed_cry_roots_match_a_fresh_location() {
    use std::path::PathBuf;

    use crate::gen_rom_profile::pack_source::PackSource;
    use crate::gen_rom_profile::search::{Lz77Search, PointerIndex, RawSearch};
    use crate::gen_rom_profile::Context;

    let Ok(path) = std::env::var("POKEEMERALD_ROM") else {
        panic!("set POKEEMERALD_ROM to the supported cartridge image");
    };
    let rom = rom_import::Rom::load(&PathBuf::from(path)).expect("the supported ROM");
    let repo_root = crate::extract::repo_root();
    let pack =
        PackSource::load(&repo_root.join(pack_format::OUTPUT_RELATIVE_PATH)).expect("a pack");
    let ctx = Context {
        rom: rom.bytes(),
        pack: &pack,
        raw: RawSearch::new(rom.bytes()),
        lz77: Lz77Search::new(rom.bytes()),
        pointers: PointerIndex::build(rom.bytes()),
        upstream: repo_root.join("pokeemerald"),
        bag_ids: Vec::new(),
    };
    let plan = locate_cry(&ctx, &mut Vec::new())
        .expect("the supported ROM locates")
        .expect("the pack holds the cries");

    let committed = rom_import::EMERALD_US_REV0
        .roots
        .audio
        .cry
        .expect("the committed profile records the cries");
    assert_eq!(committed.normal.raw(), plan.normal);
    assert_eq!(committed.reverse.raw(), plan.reverse);
    assert_eq!(committed.samples.len(), plan.samples.len());
    for (root, planned) in committed.samples.iter().zip(&plan.samples) {
        assert_eq!(root.id, planned.id);
        assert_eq!(root.addr.raw(), planned.addr);
        assert_eq!(root.header_len, planned.header_len);
        assert_eq!(root.data_len, planned.data_len);
    }
}
