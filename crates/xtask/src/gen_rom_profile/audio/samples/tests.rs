//! Locating samples by payload against synthetic ROMs.

use super::{locate_direct_sound, locate_programmable_wave, SampleCandidates, WAVE_STATUS_LOOP};
use crate::gen_rom_profile::tests::with_context;
use assets::audio::{ProgrammableWave, Sample};
use rom_import::fixture::RomFixture;

/// PCM distinctive enough to occur exactly once in a fixture image.
fn pcm() -> Vec<u8> {
    (0..64u32)
        .map(|index| u8::try_from((index * 37 + 11) % 251).expect("modulo 251 fits in u8"))
        .collect()
}

/// A distinctive byte appended past `data`'s declared count: the
/// retained interpolation guard `wav.rs` always keeps (module docs).
const GUARD_SAMPLE: u8 = 0xA5;

/// `data` with [`GUARD_SAMPLE`] appended, matching the buffer `wav.rs`
/// and the ROM importer both retain past the logical sample count.
fn with_guard(data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    out.push(GUARD_SAMPLE);
    out
}

/// A `DirectSound` pack payload: kind, freq, loop flag, loop start,
/// logical sample count, PCM, and the retained guard byte.
fn direct_sound_payload(freq: u32, loop_start: Option<u32>, data: &[u8]) -> Vec<u8> {
    let mut payload = vec![0u8];
    payload.extend_from_slice(&freq.to_le_bytes());
    payload.push(u8::from(loop_start.is_some()));
    payload.extend_from_slice(&loop_start.unwrap_or_default().to_le_bytes());
    payload.extend_from_slice(
        &u32::try_from(data.len())
            .expect("a sample fits in u32")
            .to_le_bytes(),
    );
    payload.extend_from_slice(&with_guard(data));
    payload
}

/// A ROM `WaveData` header: type, status, freq, loop start, size (the
/// logical sample count).
fn wave_header(wave_type: u16, status: u16, freq: u32, loop_start: u32, size: u32) -> Vec<u8> {
    let mut header = Vec::with_capacity(16);
    header.extend_from_slice(&wave_type.to_le_bytes());
    header.extend_from_slice(&status.to_le_bytes());
    header.extend_from_slice(&freq.to_le_bytes());
    header.extend_from_slice(&loop_start.to_le_bytes());
    header.extend_from_slice(&size.to_le_bytes());
    header
}

/// Plant one sample at `0x10_0000` and locate it against a one-entry
/// pack. `data` is the logical PCM; the retained guard byte is appended
/// to what lands in the ROM.
fn locate_one(
    name: &str,
    header: &[u8],
    payload: Vec<u8>,
    data: &[u8],
) -> Result<(), crate::gen_rom_profile::GenRomProfileError> {
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, header)
        .write(0x10_0010, &with_guard(data))
        .finish();
    let entry = pack_format::raw_entry("audio/sample/direct-sound/test".to_owned(), payload);
    with_context(name, &rom, vec![entry], |ctx| {
        locate_direct_sound(ctx).map(|_| ())
    })
}

#[test]
fn a_wave_header_whose_loop_status_disagrees_with_the_pack_is_rejected() {
    let data = pcm();
    let freq = 1 << 20;
    let size = u32::try_from(data.len()).expect("a sample fits in u32");
    // All currently checked fields agree: both numeric loop starts are
    // zero. Only the ROM status says the sample loops.
    let header = wave_header(0, WAVE_STATUS_LOOP, freq, 0, size);
    let payload = direct_sound_payload(freq, None, &data);

    let err = locate_one("loop-status-mismatch", &header, payload, &data).unwrap_err();
    assert!(
        matches!(
            &err,
            crate::gen_rom_profile::GenRomProfileError::StructMismatch { id, .. }
                if id == "audio/sample/direct-sound/test"
        ),
        "{err}"
    );
}

#[test]
fn a_wave_header_with_a_nonzero_type_is_rejected() {
    let data = pcm();
    let freq = 1 << 20;
    let size = u32::try_from(data.len()).expect("a sample fits in u32");
    let header = wave_header(1, 0, freq, 0, size);
    let payload = direct_sound_payload(freq, None, &data);

    let err = locate_one("nonzero-wave-type", &header, payload, &data).unwrap_err();
    assert!(
        matches!(
            &err,
            crate::gen_rom_profile::GenRomProfileError::StructMismatch { id, .. }
                if id == "audio/sample/direct-sound/test"
        ),
        "{err}"
    );
}

/// A cry sample's expanded PCM is absent from the ROM, which stores it
/// DPCM-compressed, so the instrument locator must not search for it.
#[test]
fn cry_samples_are_left_out_of_the_instrument_locator() {
    let data = pcm();
    let freq = 1 << 20;
    let size = u32::try_from(data.len()).expect("a sample fits in u32");
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, &wave_header(0, 0, freq, 0, size))
        .write(0x10_0010, &with_guard(&data))
        .finish();
    let absent: Vec<u8> = data.iter().rev().map(|byte| byte ^ 0x5A).collect();
    let entries = vec![
        pack_format::raw_entry(
            "audio/sample/direct-sound/test".to_owned(),
            direct_sound_payload(freq, None, &data),
        ),
        pack_format::raw_entry(
            "audio/sample/direct-sound/cry/groudon".to_owned(),
            direct_sound_payload(freq, None, &absent),
        ),
    ];
    let plans = with_context("cry-sample-excluded", &rom, entries, |ctx| {
        locate_direct_sound(ctx)
    })
    .expect("the instrument locates and the cry is not searched for");
    let ids: Vec<&str> = plans.iter().map(|plan| plan.id.as_str()).collect();
    assert_eq!(ids, ["audio/sample/direct-sound/test"]);
}

#[test]
fn a_pack_sample_whose_declared_count_disagrees_with_its_pcm_is_rejected() {
    let data = pcm();
    let freq = 1 << 20;
    let size = u32::try_from(data.len()).expect("a sample fits in u32");
    let header = wave_header(0, 0, freq, 0, size);
    let mut payload = direct_sound_payload(freq, None, &data);
    // Corrupt only the serialized sample_count (bytes 10..14). The
    // canonical decoder (`assets::audio::Sample::decode`) requires it
    // to agree with the retained PCM and rejects this payload before
    // the generator ever searches the ROM for it.
    payload[10..14].copy_from_slice(&(size + 7).to_le_bytes());

    let err = locate_one("declared-count-too-large", &header, payload, &data).unwrap_err();
    assert!(
        matches!(
            &err,
            crate::gen_rom_profile::GenRomProfileError::EntryShape { id, .. }
                if id == "audio/sample/direct-sound/test"
        ),
        "a non-canonical sample payload located successfully: {err}"
    );
}

#[test]
fn a_pack_sample_with_trailing_bytes_past_its_declared_count_is_rejected() {
    let data = pcm();
    let freq = 1 << 20;
    let size = u32::try_from(data.len()).expect("a sample fits in u32");
    let header = wave_header(0, 0, freq, 0, size);
    let mut payload = direct_sound_payload(freq, None, &data);
    // Shrink the declared sample_count so the retained buffer holds
    // bytes past what the canonical schema's `expect_eof` allows.
    payload[10..14].copy_from_slice(&(size - 1).to_le_bytes());

    let err = locate_one("declared-count-too-small", &header, payload, &data).unwrap_err();
    assert!(
        matches!(
            &err,
            crate::gen_rom_profile::GenRomProfileError::EntryShape { id, .. }
                if id == "audio/sample/direct-sound/test"
        ),
        "a non-canonical sample payload located successfully: {err}"
    );
}

const WAVE_ID: &str = "audio/sample/programmable-wave/01";
const WAVE_TABLE: [u8; 16] = [
    0x13, 0x57, 0x9B, 0xDF, 0x24, 0x68, 0xAC, 0xE0, 0x31, 0x75, 0xB9, 0xFD, 0x42, 0x86, 0xCA, 0x0E,
];

fn programmable_wave_payload() -> Vec<u8> {
    Sample::ProgrammableWave(ProgrammableWave { table: WAVE_TABLE }).encode()
}

fn locate_one_wave(
    name: &str,
    payload: Vec<u8>,
    planted: &[u8],
) -> Result<Vec<SampleCandidates>, crate::gen_rom_profile::GenRomProfileError> {
    let rom = RomFixture::new()
        .emerald_header()
        .write(0x10_0000, planted)
        .finish();
    let entry = pack_format::raw_entry(WAVE_ID.to_owned(), payload);
    with_context(name, &rom, vec![entry], locate_programmable_wave)
}

fn assert_wave_entry_shape(name: &str, payload: Vec<u8>, planted: &[u8]) {
    let err = locate_one_wave(name, payload, planted)
        .expect_err("a non-canonical programmable-wave entry must be refused");
    assert!(
        matches!(
            &err,
            crate::gen_rom_profile::GenRomProfileError::EntryShape { id, .. } if id == WAVE_ID
        ),
        "{err}"
    );
}

#[test]
fn a_valid_programmable_wave_table_locates() {
    let found = locate_one_wave("wave-valid", programmable_wave_payload(), &WAVE_TABLE)
        .expect("a valid table locates");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, WAVE_ID);
    assert_eq!(found[0].addrs, [0x0810_0000]);
    assert_eq!(found[0].header_len, 0);
    assert_eq!(found[0].data_len, 16);
}

#[test]
fn a_programmable_wave_payload_with_a_wrong_kind_tag_is_rejected() {
    for (name, tag) in [("wave-ds-tag", 0), ("wave-unknown-tag", u8::MAX)] {
        let mut payload = programmable_wave_payload();
        payload[0] = tag;
        assert_wave_entry_shape(name, payload, &WAVE_TABLE);
    }
}

#[test]
fn a_direct_sound_sample_under_a_programmable_wave_id_is_rejected() {
    let payload = direct_sound_payload(1 << 20, None, &pcm());
    let planted = payload[1..17].to_vec();
    assert_wave_entry_shape("wave-wrong-variant", payload, &planted);
}

#[test]
fn a_programmable_wave_payload_with_trailing_bytes_is_rejected() {
    let mut payload = programmable_wave_payload();
    payload.push(0xA5);
    assert_wave_entry_shape("wave-trailing", payload, &WAVE_TABLE);
}

#[test]
fn a_programmable_wave_payload_with_a_malformed_length_is_rejected() {
    // Short payloads were already refused by the raw slice; this pins that
    // refusal through the canonical decoder (the oversized case is the
    // trailing-bytes test, which is the regression).
    let mut payload = programmable_wave_payload();
    payload.pop();
    assert_wave_entry_shape("wave-truncated", payload, &WAVE_TABLE);
    assert_wave_entry_shape("wave-empty", Vec::new(), &WAVE_TABLE);
}
