//! Locating `DirectSound` and programmable-wave samples by their payloads, with their colocated tests.

use assets::audio::{DirectSoundSample, ProgrammableWave, Sample};

use crate::extract::cry::SAMPLE_ID_PREFIX as CRY_SAMPLE_PREFIX;
use crate::gen_rom_profile::error::GenRomProfileError;
use crate::gen_rom_profile::locate::{to_addr, u16_at_addr, u32_at_addr};
use crate::gen_rom_profile::plan::{ReportLine, Resolution, SamplePlan};
use crate::gen_rom_profile::Context;

use super::{
    raw_payload, DIRECT_SOUND_PREFIX, DIRECT_SOUND_SYMBOL, PROGRAMMABLE_WAVE_PREFIX,
    PROGRAMMABLE_WAVE_SYMBOL, WAVE_HEADER_BYTES, WAVE_STATUS_LOOP, WAVE_TABLE_BYTES,
};

/// A pack sample and every ROM address that holds its bytes.
///
/// A sample can be stored more than once; which copy is the instrument is
/// decided by the voicegroup slot that points at it (`closure`), never here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SampleCandidates {
    /// The pack id.
    pub id: String,
    /// The `WaveData` header length, `0` for a programmable wave.
    pub header_len: u32,
    /// The PCM payload length: the sample count, or 16 for a wave.
    pub data_len: u32,
    /// Every address a voicegroup slot may point at, ascending.
    pub addrs: Vec<u32>,
}

impl SampleCandidates {
    /// The plan for this sample once a voicegroup slot chose `addr`.
    pub(super) fn at(&self, addr: u32) -> SamplePlan {
        SamplePlan {
            id: self.id.clone(),
            addr,
            header_len: self.header_len,
            data_len: self.data_len,
        }
    }
}

/// Locate every instrument `DirectSound` sample by its PCM, then check its header.
///
/// Cry samples under [`CRY_SAMPLE_PREFIX`] are not instruments: the ROM stores
/// them DPCM-compressed (`wav2agb -c`, a nonzero `WaveData` type), so their
/// expanded PCM never occurs in the ROM and this uncompressed locator cannot
/// place them. They are left to a dedicated cry locator.
///
/// # Errors
///
/// [`GenRomProfileError::NotFound`] when no copy has a header that agrees
/// with the pack, otherwise the first copy's header disagreement.
pub(super) fn locate_direct_sound(
    ctx: &Context<'_>,
) -> Result<Vec<SampleCandidates>, GenRomProfileError> {
    let ids = instrument_ids(ctx);
    let mut samples = Vec::with_capacity(ids.len());
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        let sample = direct_sound_of(ctx, id)?;
        needles.push(pcm_needle(&sample));
        samples.push(sample);
    }
    let hits = ctx.raw.find_all(&needles);

    let mut found = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let sample = &samples[index];
        let mut addrs = Vec::new();
        let mut refusal = None;
        for &hit in &hits[index] {
            let candidate = to_addr(hit).checked_sub(WAVE_HEADER_BYTES).ok_or_else(|| {
                GenRomProfileError::StructMismatch {
                    id: id.clone(),
                    reason: "the PCM starts before a header could fit".to_owned(),
                }
            });
            match candidate.and_then(|addr| check_wave_header(ctx, id, addr, sample).map(|()| addr))
            {
                Ok(addr) => addrs.push(addr),
                Err(err) => {
                    refusal.get_or_insert(err);
                }
            }
        }
        if addrs.is_empty() {
            return Err(refusal.unwrap_or_else(|| GenRomProfileError::NotFound { id: id.clone() }));
        }
        found.push(SampleCandidates {
            id: id.clone(),
            header_len: WAVE_HEADER_BYTES,
            data_len: sample.sample_count(),
            addrs,
        });
    }
    Ok(found)
}

/// The report line for a located `DirectSound` sample.
///
/// The ROM holds `data_len` logical samples plus one interpolation-guard byte
/// wav2agb's payload writer always emits (`crates/xtask/src/extract/wav.rs`'s
/// module docs).
pub(super) fn direct_sound_line(plan: &SamplePlan, copies: usize) -> ReportLine {
    let symbol = format!(
        "{DIRECT_SOUND_SYMBOL}{}",
        plan.id.trim_start_matches(DIRECT_SOUND_PREFIX)
    );
    let line =
        ReportLine::unique(&plan.id, plan.addr, plan.header_len + plan.data_len + 1).symbol(symbol);
    if copies > 1 {
        line.with(Resolution::StructDerived).note(format!(
            "{copies} copies; the voicegroup slot pointer chose this one"
        ))
    } else {
        line.note("PCM matched; WaveData header checked")
    }
}

/// The pack's `DirectSound` ids, without the cry samples.
fn instrument_ids(ctx: &Context<'_>) -> Vec<String> {
    let mut ids = ctx.pack.ids_with_prefix(DIRECT_SOUND_PREFIX);
    ids.retain(|id| !id.starts_with(CRY_SAMPLE_PREFIX));
    ids
}

/// Decode and validate a `DirectSound` pack entry through the canonical
/// wire schema (`assets::audio::Sample::decode`): the same schema the
/// runtime pack loader enforces decides whether a payload is well-formed
/// enough to search for, so a declared `sample_count` that disagrees with
/// the retained PCM, or any other canonical violation (loop bounds,
/// trailing bytes, an unrecognized kind tag), is refused here rather than
/// reaching the ROM search.
///
/// # Errors
///
/// [`GenRomProfileError::EntryShape`] naming `id` when the payload does not
/// decode as a valid `DirectSound` sample.
pub(super) fn direct_sound_of(
    ctx: &Context<'_>,
    id: &str,
) -> Result<DirectSoundSample, GenRomProfileError> {
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

/// The ROM's raw PCM bytes for a validated `DirectSound` sample.
///
/// [`DirectSoundSample::data`] returns `i8` values in playback order; the
/// ROM (and the pack payload the schema decoded them from) stores the
/// identical bit pattern as plain bytes.
fn pcm_needle(sample: &DirectSoundSample) -> Vec<u8> {
    sample
        .data()
        .iter()
        .map(|&value| value.to_le_bytes()[0])
        .collect()
}

/// Check a `WaveData` header against the pack entry's own validated fields.
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

/// A programmable-wave table's length, as the `u32` the profile records.
fn wave_table_len() -> u32 {
    u32::try_from(WAVE_TABLE_BYTES).expect("16 fits in u32")
}

/// Decode a programmable-wave pack entry through the canonical sample schema.
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

/// Locate every programmable-wave table by its 16 bytes.
///
/// # Errors
///
/// [`GenRomProfileError::NotFound`] for a table that occurs nowhere.
pub(super) fn locate_programmable_wave(
    ctx: &Context<'_>,
) -> Result<Vec<SampleCandidates>, GenRomProfileError> {
    let ids = ctx.pack.ids_with_prefix(PROGRAMMABLE_WAVE_PREFIX);
    let mut needles = Vec::with_capacity(ids.len());
    for id in &ids {
        needles.push(programmable_wave_of(ctx, id)?.table.to_vec());
    }
    let hits = ctx.raw.find_all(&needles);

    ids.into_iter()
        .zip(hits)
        .map(|(id, hits)| {
            if hits.is_empty() {
                return Err(GenRomProfileError::NotFound { id });
            }
            Ok(SampleCandidates {
                id,
                header_len: 0,
                data_len: wave_table_len(),
                addrs: hits.into_iter().map(to_addr).collect(),
            })
        })
        .collect()
}

/// The report line for a located programmable-wave table.
pub(super) fn programmable_wave_line(plan: &SamplePlan, copies: usize) -> ReportLine {
    // The pack pads the index to two digits; upstream does not.
    let number = plan
        .id
        .trim_start_matches(PROGRAMMABLE_WAVE_PREFIX)
        .trim_start_matches('0');
    let line = ReportLine::unique(&plan.id, plan.addr, wave_table_len())
        .symbol(format!("{PROGRAMMABLE_WAVE_SYMBOL}{number}"));
    if copies > 1 {
        line.with(Resolution::StructDerived).note(format!(
            "{copies} copies; the voicegroup slot pointer chose this one"
        ))
    } else {
        line
    }
}

#[cfg(test)]
mod tests;
