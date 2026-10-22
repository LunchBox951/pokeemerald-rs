//! Locates `DirectSound` and programmable-wave samples by their payloads.

use assets::audio::{DirectSoundSample, ProgrammableWave, Sample};

use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::locate::{exactly_one, u16_at_addr, u32_at_addr};
use crate::gen_rom_profile::plan::{ReportLine, SamplePlan};
use crate::gen_rom_profile::Context;

use super::{
    raw_payload, DIRECT_SOUND_PREFIX, DIRECT_SOUND_SYMBOL, PROGRAMMABLE_WAVE_PREFIX,
    PROGRAMMABLE_WAVE_SYMBOL, WAVE_HEADER_BYTES, WAVE_STATUS_LOOP, WAVE_TABLE_BYTES,
};

/// Locates every `DirectSound` sample by its PCM and checks its `WaveData` header.
pub(super) fn locate_direct_sound(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<SamplePlan>, GenRomProfileError> {
    let ids = ctx.pack.ids_with_prefix(DIRECT_SOUND_PREFIX);
    let mut samples = Vec::with_capacity(ids.len());
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        let sample = direct_sound_of(ctx, id)?;
        needles.push(pcm_needle(&sample));
        samples.push(sample);
    }
    let hits = ctx.raw.find_all(&needles);

    let mut plans = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let pcm_addr = exactly_one(id, &hits[index])?;
        let addr = pcm_addr.checked_sub(WAVE_HEADER_BYTES).ok_or_else(|| {
            GenRomProfileError::StructMismatch {
                id: id.clone(),
                reason: "the PCM starts before a header could fit".to_owned(),
            }
        })?;
        let sample = &samples[index];
        check_wave_header(ctx, id, addr, sample)?;
        // The needle ends with the interpolation-guard byte wav2agb always emits.
        let buffer_len = u32::try_from(needles[index].len()).expect("a sample fits in u32");
        let data_len = sample.sample_count();
        let symbol = format!(
            "{DIRECT_SOUND_SYMBOL}{}",
            id.trim_start_matches(DIRECT_SOUND_PREFIX)
        );
        report.push(
            ReportLine::unique(id, addr, WAVE_HEADER_BYTES + buffer_len)
                .symbol(symbol)
                .note("PCM matched; WaveData header checked"),
        );
        plans.push(SamplePlan {
            id: id.clone(),
            addr,
            header_len: WAVE_HEADER_BYTES,
            data_len,
        });
    }
    Ok(plans)
}

/// Decodes a `DirectSound` pack entry with `Sample::decode`, so only payloads
/// the runtime pack loader accepts reach the ROM search.
///
/// # Errors
///
/// [`GenRomProfileError::EntryShape`] naming `id` when the payload does not
/// decode as a `DirectSound` sample.
fn direct_sound_of(ctx: &Context<'_>, id: &str) -> Result<DirectSoundSample, GenRomProfileError> {
    let payload = raw_payload(ctx, id)?;
    match Sample::decode(payload) {
        Ok(Sample::DirectSound(sample)) => Ok(sample),
        Ok(Sample::ProgrammableWave(_)) => Err(GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: "decoded as a programmable-wave table under a DirectSound pack id".to_owned(),
        }),
        Err(err) => Err(GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: err.to_string(),
        }),
    }
}

/// The sample's PCM as the raw bytes the ROM stores.
fn pcm_needle(sample: &DirectSoundSample) -> Vec<u8> {
    sample
        .data()
        .iter()
        .map(|&value| value.to_le_bytes()[0])
        .collect()
}

/// Requires the `WaveData` header at `addr` to be type 0 with the sample's
/// loop flag, base frequency, loop start and sample count.
///
/// # Errors
///
/// [`GenRomProfileError::StructMismatch`] naming `id` when the header is
/// truncated or any field differs.
fn check_wave_header(
    ctx: &Context<'_>,
    id: &str,
    addr: u32,
    sample: &DirectSoundSample,
) -> Result<(), GenRomProfileError> {
    let expected_frequency = sample.base_frequency;
    let expected_looping = sample.loop_start().is_some();
    let expected_loop = sample.loop_start().unwrap_or_default();
    let expected_size = sample.sample_count();

    let mismatch = |reason: String| GenRomProfileError::StructMismatch {
        id: id.to_owned(),
        reason,
    };
    let wave_type = u16_at_addr(ctx.rom, addr).ok_or_else(|| mismatch("truncated".into()))?;
    let status = u16_at_addr(ctx.rom, addr + 2).ok_or_else(|| mismatch("truncated".into()))?;
    let looping = status & WAVE_STATUS_LOOP != 0;
    let frequency = u32_at_addr(ctx.rom, addr + 4).ok_or_else(|| mismatch("truncated".into()))?;
    let loop_start = u32_at_addr(ctx.rom, addr + 8).ok_or_else(|| mismatch("truncated".into()))?;
    let size = u32_at_addr(ctx.rom, addr + 12).ok_or_else(|| mismatch("truncated".into()))?;
    if wave_type != 0
        || looping != expected_looping
        || frequency != expected_frequency
        || loop_start != expected_loop
        || size != expected_size
    {
        return Err(mismatch(format!(
            "WaveData at {addr:08X} reads type {wave_type}, looping {looping}, freq \
             {frequency}, loop {loop_start}, size {size}; the pack says type 0, looping \
             {expected_looping}, {expected_frequency}, {expected_loop}, {expected_size}"
        )));
    }
    Ok(())
}

/// The programmable-wave table length as a `u32`.
fn wave_table_len() -> u32 {
    u32::try_from(WAVE_TABLE_BYTES).expect("16 fits in u32")
}

/// Decodes a programmable-wave pack entry with `Sample::decode`.
///
/// # Errors
///
/// [`GenRomProfileError::EntryShape`] naming `id` when the payload does not
/// decode as a programmable-wave table.
fn programmable_wave_of(
    ctx: &Context<'_>,
    id: &str,
) -> Result<ProgrammableWave, GenRomProfileError> {
    let payload = raw_payload(ctx, id)?;
    match Sample::decode(payload) {
        Ok(Sample::ProgrammableWave(wave)) => Ok(wave),
        Ok(Sample::DirectSound(_)) => Err(GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: "decoded as a DirectSound sample under a programmable-wave id".to_owned(),
        }),
        Err(err) => Err(GenRomProfileError::EntryShape {
            id: id.to_owned(),
            reason: err.to_string(),
        }),
    }
}

/// Locates every programmable-wave table by its bytes.
pub(super) fn locate_programmable_wave(
    ctx: &Context<'_>,
    report: &mut Vec<ReportLine>,
) -> Result<Vec<SamplePlan>, GenRomProfileError> {
    let ids = ctx.pack.ids_with_prefix(PROGRAMMABLE_WAVE_PREFIX);
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        needles.push(programmable_wave_of(ctx, id)?.table.to_vec());
    }
    let hits = ctx.raw.find_all(&needles);

    let mut plans = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let addr = exactly_one(id, &hits[index])?;
        // Pack ids pad the index to two digits; upstream symbol names do not.
        let number = id
            .trim_start_matches(PROGRAMMABLE_WAVE_PREFIX)
            .trim_start_matches('0');
        report.push(
            ReportLine::unique(id, addr, wave_table_len())
                .symbol(format!("{PROGRAMMABLE_WAVE_SYMBOL}{number}")),
        );
        plans.push(SamplePlan {
            id: id.clone(),
            addr,
            header_len: 0,
            data_len: wave_table_len(),
        });
    }
    Ok(plans)
}

#[cfg(test)]
mod tests {
    use super::{locate_direct_sound, locate_programmable_wave, WAVE_STATUS_LOOP};
    use crate::gen_rom_profile::plan::{ReportLine, SamplePlan};
    use crate::gen_rom_profile::tests::with_context;
    use assets::audio::{ProgrammableWave, Sample};
    use rom_import::fixture::RomFixture;

    /// PCM distinctive enough to occur exactly once in a fixture image.
    fn pcm() -> Vec<u8> {
        (0..64u32)
            .map(|index| u8::try_from((index * 37 + 11) % 251).expect("modulo 251 fits in u8"))
            .collect()
    }

    /// The interpolation-guard byte `wav.rs` always appends.
    const GUARD_SAMPLE: u8 = 0xA5;

    /// `data` followed by [`GUARD_SAMPLE`].
    fn with_guard(data: &[u8]) -> Vec<u8> {
        let mut out = data.to_vec();
        out.push(GUARD_SAMPLE);
        out
    }

    /// A `DirectSound` pack payload with a guard byte after `data`.
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

    /// A ROM `WaveData` header.
    fn wave_header(wave_type: u16, status: u16, freq: u32, loop_start: u32, size: u32) -> Vec<u8> {
        let mut header = Vec::with_capacity(16);
        header.extend_from_slice(&wave_type.to_le_bytes());
        header.extend_from_slice(&status.to_le_bytes());
        header.extend_from_slice(&freq.to_le_bytes());
        header.extend_from_slice(&loop_start.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        header
    }

    /// Plants `header` and `data` plus its guard byte in a ROM and locates the
    /// sample against a one-entry pack holding `payload`.
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
            let mut report = Vec::new();
            locate_direct_sound(ctx, &mut report).map(|_| ())
        })
    }

    #[test]
    fn a_wave_header_whose_loop_status_disagrees_with_the_pack_is_rejected() {
        let data = pcm();
        let freq = 1 << 20;
        let size = u32::try_from(data.len()).expect("a sample fits in u32");
        // Only the ROM status flag says the sample loops; both loop starts are zero.
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

    #[test]
    fn a_pack_sample_whose_declared_count_disagrees_with_its_pcm_is_rejected() {
        let data = pcm();
        let freq = 1 << 20;
        let size = u32::try_from(data.len()).expect("a sample fits in u32");
        let header = wave_header(0, 0, freq, 0, size);
        let mut payload = direct_sound_payload(freq, None, &data);
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
        0x13, 0x57, 0x9B, 0xDF, 0x24, 0x68, 0xAC, 0xE0, 0x31, 0x75, 0xB9, 0xFD, 0x42, 0x86, 0xCA,
        0x0E,
    ];

    fn programmable_wave_payload() -> Vec<u8> {
        Sample::ProgrammableWave(ProgrammableWave { table: WAVE_TABLE }).encode()
    }

    fn locate_one_wave(
        name: &str,
        payload: Vec<u8>,
        planted: &[u8],
    ) -> Result<(Vec<SamplePlan>, Vec<ReportLine>), crate::gen_rom_profile::GenRomProfileError>
    {
        let rom = RomFixture::new()
            .emerald_header()
            .write(0x10_0000, planted)
            .finish();
        let entry = pack_format::raw_entry(WAVE_ID.to_owned(), payload);
        with_context(name, &rom, vec![entry], |ctx| {
            let mut report = Vec::new();
            locate_programmable_wave(ctx, &mut report).map(|plans| (plans, report))
        })
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
        let (plans, report) =
            locate_one_wave("wave-valid", programmable_wave_payload(), &WAVE_TABLE)
                .expect("a valid table locates");
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].id, WAVE_ID);
        assert_eq!(plans[0].addr, 0x0810_0000);
        assert_eq!(plans[0].header_len, 0);
        assert_eq!(plans[0].data_len, 16);
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].addr, 0x0810_0000);
        assert_eq!(report[0].len, 16);
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
        let mut payload = programmable_wave_payload();
        payload.pop();
        assert_wave_entry_shape("wave-truncated", payload, &WAVE_TABLE);
        assert_wave_entry_shape("wave-empty", Vec::new(), &WAVE_TABLE);
    }
}
