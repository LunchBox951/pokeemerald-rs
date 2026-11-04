//! Cry reader tests over a synthetic ROM.
//!
//! The fixture plants both 388-record tables and 388 one-sample-per-position
//! `WaveData` objects. Entries are decoded back through `assets`, so the
//! tests pin the field mapping and the DPCM arithmetic rather than wire bytes.

use assets::audio::{AudioError, CryTable, CRY_TABLE_LEN, CRY_TABLE_PACK_ID};
use assets::{Sample, SampleId};
use pack_format::{PackEntry, PackWriter};

use super::decode_cry_payloads;
use crate::domains::audio::write;
use crate::error::ImportError;
use crate::fixture::RomFixture;
use crate::reader::{GbaPtr, RomReader, ROM_BASE};
use crate::rom::{Rom, ROM_SIZE};
use crate::roots::{AudioRoots, CryRoots, Roots, SampleRoot};

const fn at(off: u32) -> GbaPtr {
    GbaPtr::at(ROM_BASE + off)
}

const NORMAL: u32 = 0x8000;
const REVERSE: u32 = 0xA000;
const SAMPLES: u32 = 0x1_0000;
/// Room for a header and the longest payload a test plants.
const STRIDE: u32 = 0x40;
const FREQ: u32 = 0x00A4_4000;
/// Four index bytes: a first sample of 120, then deltas `+1 +4 +49 -64 -1`.
/// The first index byte's high nibble (`f`) is skipped by the mixer.
const PAYLOAD: [u8; 4] = [0x78, 0xF1, 0x27, 0x8F];
/// `PAYLOAD` decoded: 5 logical samples and the guard, with `125 + 49` and
/// `-82 - 64` wrapping at 8 bits.
const DECODED: [i8; 6] = [120, 121, 125, -82, 110, 109];

fn sample_addr(index: usize) -> u32 {
    SAMPLES + u32::try_from(index).unwrap() * STRIDE
}

fn offset(index: usize) -> usize {
    usize::try_from(sample_addr(index)).unwrap()
}

/// A `WaveData` header.
fn header(kind: u16, status: u16, freq: u32, size: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(kind.to_le_bytes());
    out.extend(status.to_le_bytes());
    out.extend(freq.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(size.to_le_bytes());
    out
}

/// A `cry` (`0x20`) or `cry_reverse` (`0x30`) record.
fn record(kind: u8, wav: GbaPtr) -> [u8; 12] {
    let w = wav.raw().to_le_bytes();
    [kind, 60, 0, 0, w[0], w[1], w[2], w[3], 0xFF, 0, 0xFF, 0]
}

/// A full-size image holding both tables and every sample.
fn image() -> Vec<u8> {
    let mut rom = RomFixture::new().emerald_header();
    for index in 0..CRY_TABLE_LEN {
        let wav = at(sample_addr(index));
        rom = rom
            .write(
                usize::try_from(NORMAL).unwrap() + index * 12,
                &record(0x20, wav),
            )
            .write(
                usize::try_from(REVERSE).unwrap() + index * 12,
                &record(0x30, wav),
            )
            .write(offset(index), &header(1, 0, FREQ, 5))
            .write(offset(index) + 16, &PAYLOAD);
    }
    rom.finish()
}

fn poke(bytes: &mut [u8], off: usize, data: &[u8]) {
    bytes[off..off + data.len()].copy_from_slice(data);
}

fn id(index: usize) -> &'static str {
    Box::leak(format!("audio/sample/direct-sound/cry/c{index}").into_boxed_str())
}

fn sample_roots() -> Vec<SampleRoot> {
    (0..CRY_TABLE_LEN)
        .map(|index| SampleRoot {
            id: id(index),
            addr: at(sample_addr(index)),
            header_len: 16,
            data_len: 5,
        })
        .collect()
}

fn roots_of(samples: Vec<SampleRoot>) -> CryRoots {
    CryRoots {
        normal: at(NORMAL),
        reverse: at(REVERSE),
        samples: Box::leak(samples.into_boxed_slice()),
    }
}

fn roots() -> CryRoots {
    roots_of(sample_roots())
}

fn decode(bytes: &[u8], cry: &CryRoots) -> Result<Vec<PackEntry>, ImportError> {
    decode_cry_payloads(&RomReader::new(bytes), cry)
}

fn fault(bytes: &[u8], cry: &CryRoots) -> ImportError {
    decode(bytes, cry).expect_err("the ROM or profile is malformed")
}

fn data_of(entry: &PackEntry) -> (u32, Option<u32>, u32, Vec<i8>) {
    let Sample::DirectSound(sample) = Sample::decode(&entry.payload).unwrap() else {
        panic!("a DirectSound entry")
    };
    (
        sample.base_frequency,
        sample.loop_start(),
        sample.sample_count(),
        sample.data().to_vec(),
    )
}

#[test]
fn every_cry_position_becomes_a_table_id_and_a_sample() {
    let entries = decode(&image(), &roots()).expect("a well-formed table");
    assert_eq!(entries.len(), 1 + CRY_TABLE_LEN);
    assert_eq!(entries[0].id, CRY_TABLE_PACK_ID);
    let table = CryTable::decode(&entries[0].payload).expect("a canonical table");
    for (index, entry) in entries[1..].iter().enumerate() {
        assert_eq!(table.samples()[index], SampleId(id(index).to_owned()));
        assert_eq!(entry.id, id(index));
        // Identical PCM at different addresses is a different cry each.
        assert_eq!(
            data_of(entry),
            (FREQ, None, 5, DECODED.to_vec()),
            "sample {index}"
        );
    }
}

#[test]
fn the_domain_writes_the_table_and_every_sample() {
    let rom = Rom::from_bytes(image()).expect("a valid header");
    let roots = Roots {
        audio: AudioRoots {
            cry: Some(roots()),
            ..Roots::NONE.audio
        },
        ..Roots::NONE
    };
    let mut writer = PackWriter::new();
    write(&rom, &roots, &mut writer).expect("a well-formed table");
    assert_eq!(writer.len(), 1 + CRY_TABLE_LEN);
}

/// Replace the last sample with `count` logical samples of `payload`.
fn with_last_sample(count: u32, payload: &[u8]) -> (Vec<u8>, CryRoots) {
    let last = CRY_TABLE_LEN - 1;
    let mut bytes = image();
    poke(&mut bytes, offset(last), &header(1, 0, FREQ, count));
    poke(&mut bytes, offset(last) + 16, payload);
    let mut samples = sample_roots();
    samples[last].data_len = count;
    (bytes, roots_of(samples))
}

fn last_entry(bytes: &[u8], cry: &CryRoots) -> Vec<i8> {
    let entries = decode(bytes, cry).expect("a well-formed sample");
    data_of(&entries[CRY_TABLE_LEN]).3
}

#[test]
fn an_unpadded_last_block_takes_the_guard_from_the_next_rom_byte() {
    // `--no-pad` stores 4 samples in 3 bytes; the guard is index 4, whose
    // index byte is the byte another object starts with.
    let (mut bytes, cry) = with_last_sample(4, &[0x0A, 0xF1, 0x23]);
    let adjacent = offset(CRY_TABLE_LEN - 1) + 16 + 3;
    poke(&mut bytes, adjacent, &[0x40]);
    assert_eq!(last_entry(&bytes, &cry), [10, 11, 15, 24, 40]);
}

#[test]
fn a_guard_past_the_image_is_truncated_not_zero() {
    let (bytes, cry) = with_last_sample(4, &[0x0A, 0xF1, 0x23]);
    let end = offset(CRY_TABLE_LEN - 1) + 16 + 3;
    let err = fault(&bytes[..end], &cry);
    assert!(
        matches!(err, ImportError::Truncated { at, len: 4 } if at == end - 3),
        "{err}"
    );
}

#[test]
fn the_level_restarts_at_every_block() {
    let mut payload = vec![0u8; 34];
    payload[0] = 10;
    payload[33] = 0xFC;
    let (bytes, cry) = with_last_sample(64, &payload);
    let data = last_entry(&bytes, &cry);
    assert_eq!(data.len(), 65);
    assert!(data[..64].iter().all(|&sample| sample == 10));
    // The next block's first byte is a level, not a delta from 10.
    assert_eq!(data[64], -4);
}

#[test]
fn the_second_block_decodes_its_own_deltas() {
    // Block 0 is 33 zero bytes; block 1 starts at 100 and its first index byte
    // adds the table's `+9` (index 3) to reach the guard at index 65.
    let mut payload = vec![0u8; 33];
    payload.extend([100, 0x03]);
    let (bytes, cry) = with_last_sample(65, &payload);
    let data = last_entry(&bytes, &cry);
    assert_eq!(data[64], 100);
    assert_eq!(data[65], 109);
}

#[test]
fn a_table_past_the_image_is_truncated() {
    let mut cry = roots();
    cry.normal = at(u32::try_from(ROM_SIZE).unwrap() - 4);
    let err = fault(&image(), &cry);
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");

    let mut cry = roots();
    cry.reverse = at(u32::try_from(ROM_SIZE).unwrap() - 4);
    let err = fault(&image(), &cry);
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn a_pointer_outside_the_cartridge_is_refused_at_its_word() {
    let mut bytes = image();
    let word = usize::try_from(NORMAL).unwrap() + 3 * 12 + 4;
    poke(&mut bytes, word, &0u32.to_le_bytes());
    let err = fault(&bytes, &roots());
    assert!(
        matches!(err, ImportError::PointerOutOfRange { at, raw: 0 } if at == word),
        "{err}"
    );
}

#[test]
fn an_unrecorded_sample_pointer_is_unresolved() {
    let mut bytes = image();
    let ptr = at(0x3_0000);
    poke(
        &mut bytes,
        usize::try_from(NORMAL).unwrap() + 2 * 12 + 4,
        &ptr.raw().to_le_bytes(),
    );
    let err = fault(&bytes, &roots());
    assert!(
        matches!(
            err,
            ImportError::UnresolvedPointer { root: "gCryTable", slot: 2, ptr: found, .. }
                if found == ptr
        ),
        "{err}"
    );
}

#[test]
fn a_record_naming_another_positions_sample_is_a_mismatch() {
    let mut bytes = image();
    poke(
        &mut bytes,
        usize::try_from(NORMAL).unwrap() + 4 * 12 + 4,
        &at(sample_addr(9)).raw().to_le_bytes(),
    );
    let err = fault(&bytes, &roots());
    assert!(
        matches!(
            err,
            ImportError::CryPointerMismatch { table: "gCryTable", slot: 4, expected, actual }
                if expected == at(sample_addr(4)) && actual == at(sample_addr(9))
        ),
        "{err}"
    );
}

#[test]
fn a_repeated_normal_sample_is_refused() {
    let mut bytes = image();
    poke(
        &mut bytes,
        usize::try_from(NORMAL).unwrap() + 12 + 4,
        &at(sample_addr(0)).raw().to_le_bytes(),
    );
    let err = fault(&bytes, &roots());
    assert!(
        matches!(
            err,
            ImportError::RepeatedCrySample {
                first: 0,
                slot: 1,
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn the_reverse_table_must_pair_with_the_normal_table() {
    let mut bytes = image();
    poke(
        &mut bytes,
        usize::try_from(REVERSE).unwrap() + 2 * 12 + 4,
        &at(sample_addr(5)).raw().to_le_bytes(),
    );
    let err = fault(&bytes, &roots());
    assert!(
        matches!(
            err,
            ImportError::CryPointerMismatch { table: "gCryTable_Reverse", slot: 2, expected, actual }
                if expected == at(sample_addr(2)) && actual == at(sample_addr(5))
        ),
        "{err}"
    );
}

#[test]
fn a_record_of_the_wrong_type_is_not_a_cry() {
    // A plain, reverse-DirectSound, or reverse-cry record in the normal table.
    for actual in [0x00u8, 0x10, 0x30] {
        let mut bytes = image();
        poke(
            &mut bytes,
            usize::try_from(NORMAL).unwrap() + 7 * 12,
            &[actual],
        );
        let err = fault(&bytes, &roots());
        assert!(
            matches!(
                err,
                ImportError::CryToneData {
                    table: "gCryTable", slot: 7, field: "type", expected: 0x20, actual: found
                } if found == actual
            ),
            "{err}"
        );
    }
    // A normal-cry record in the reverse table.
    let mut bytes = image();
    poke(
        &mut bytes,
        usize::try_from(REVERSE).unwrap() + 7 * 12,
        &[0x20],
    );
    let err = fault(&bytes, &roots());
    assert!(
        matches!(
            err,
            ImportError::CryToneData {
                table: "gCryTable_Reverse",
                slot: 7,
                field: "type",
                expected: 0x30,
                actual: 0x20
            }
        ),
        "{err}"
    );
}

#[test]
fn every_other_tone_byte_must_be_the_macros() {
    let fields = [
        (1, "key", 60),
        (2, "length", 0),
        (3, "pan_sweep", 0),
        (8, "attack", 0xFF),
        (9, "decay", 0),
        (10, "sustain", 0xFF),
        (11, "release", 0),
    ];
    for (byte, name, expected) in fields {
        let mut bytes = image();
        poke(
            &mut bytes,
            usize::try_from(REVERSE).unwrap() + 11 * 12 + byte,
            &[expected ^ 0x55],
        );
        let err = fault(&bytes, &roots());
        assert!(
            matches!(
                err,
                ImportError::CryToneData {
                    table: "gCryTable_Reverse", slot: 11, field, expected: want, actual
                } if field == name && want == expected && actual == expected ^ 0x55
            ),
            "{name}: {err}"
        );
    }
}

#[test]
fn a_sample_that_is_not_dpcm_is_refused() {
    for kind in [0u16, 2] {
        let mut bytes = image();
        poke(&mut bytes, offset(3), &kind.to_le_bytes());
        let err = fault(&bytes, &roots());
        assert!(
            matches!(
                err,
                ImportError::CryWaveType { id: found, kind: wave } if found == id(3) && wave == kind
            ),
            "{err}"
        );
    }
}

#[test]
fn a_header_that_disagrees_with_its_root_is_refused() {
    for (field_offset, bytes_to_write, field) in [
        (2, &1u16.to_le_bytes()[..], "WaveData.status"),
        (2, &0x4000u16.to_le_bytes()[..], "WaveData.status"),
        (12, &6u32.to_le_bytes()[..], "WaveData.size"),
    ] {
        let mut bytes = image();
        poke(&mut bytes, offset(6) + field_offset, bytes_to_write);
        let err = fault(&bytes, &roots());
        assert!(
            matches!(
                err,
                ImportError::StructMismatch { root, field: found } if root == id(6) && found == field
            ),
            "{field}: {err}"
        );
    }
}

#[test]
fn a_header_past_the_image_is_truncated() {
    let last = CRY_TABLE_LEN - 1;
    let addr = u32::try_from(ROM_SIZE).unwrap() - 8;
    let mut bytes = image();
    let ptr = at(addr).raw().to_le_bytes();
    poke(
        &mut bytes,
        usize::try_from(NORMAL).unwrap() + last * 12 + 4,
        &ptr,
    );
    poke(
        &mut bytes,
        usize::try_from(REVERSE).unwrap() + last * 12 + 4,
        &ptr,
    );
    let mut samples = sample_roots();
    samples[last].addr = at(addr);
    let err = fault(&bytes, &roots_of(samples));
    assert!(
        matches!(err, ImportError::Truncated { at, len: 16 } if at == ROM_SIZE - 8),
        "{err}"
    );
}

#[test]
fn a_size_larger_than_the_rom_is_truncated_before_it_is_allocated() {
    let (bytes, cry) = with_last_sample(u32::MAX, &PAYLOAD);
    let err = fault(&bytes, &cry);
    assert!(matches!(err, ImportError::Truncated { .. }), "{err}");
}

#[test]
fn the_sample_list_must_be_388_distinct_headed_cries() {
    let mut short = sample_roots();
    short.pop();
    let err = fault(&image(), &roots_of(short));
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: CRY_TABLE_PACK_ID,
                field: "CryRoots.samples"
            }
        ),
        "{err}"
    );

    let mut repeated = sample_roots();
    repeated[1].id = repeated[0].id;
    let err = fault(&image(), &roots_of(repeated));
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: CRY_TABLE_PACK_ID,
                field: "CryRoots.samples.id"
            }
        ),
        "{err}"
    );

    let mut headless = sample_roots();
    headless[2].header_len = 15;
    let err = fault(&image(), &roots_of(headless));
    assert!(
        matches!(
            err,
            ImportError::StructMismatch {
                root: CRY_TABLE_PACK_ID,
                field: "CryRoots.samples.header_len"
            }
        ),
        "{err}"
    );

    let mut foreign = sample_roots();
    foreign[0].id = "audio/sample/programmable-wave/01";
    let err = fault(&image(), &roots_of(foreign));
    assert!(
        matches!(
            err,
            ImportError::Audio {
                id: CRY_TABLE_PACK_ID,
                source: AudioError::CrySampleNotDirectSound { position: 0 }
            }
        ),
        "{err}"
    );
}
