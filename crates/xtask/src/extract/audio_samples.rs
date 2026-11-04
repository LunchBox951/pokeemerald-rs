//! Extracts normalized `DirectSound` and CGB programmable-wave samples into
//! raw pack entries.
//!
//! The samples are exactly the ids the emitted voicegroups reference
//! (`super::voicegroups::extract_voicegroups` returns them), so a sample
//! source that is missing or invalid fails extraction instead of leaving a
//! dangling reference in the pack. Real-pack integration tests verify that
//! every sample id reached through the resolved voicegroups exists in the pack.
//!
//! The payload schema is owned by `crates/assets`: each decoded WAV or
//! programmable-wave table is mapped onto [`assets::Sample`] and written by
//! [`assets::Sample::encode`], so the kind tags, field order, and guard-sample
//! framing exist in exactly one place and the runtime decoder validates what
//! the extractor writes. Building a [`assets::DirectSoundSample`] re-runs
//! `assets`' own validation, so a value outside the schema's domain surfaces
//! as [`ExtractError::InvalidSampleSchema`] rather than as a pack the runtime
//! would refuse to load. The WAV decoder already rejects those values with
//! source-specific errors first, so the extractor never reports that variant
//! for upstream's real samples.

use std::collections::BTreeSet;
use std::path::Path;

mod definitions;

use definitions::{Definitions, DIRECT_SOUND_DEFINITIONS_PATH, PROGRAMMABLE_WAVE_DEFINITIONS_PATH};

use super::voicegroups::{
    DIRECT_SOUND_ID_PREFIX, DIRECT_SOUND_SYMBOL_PREFIX, PROGRAMMABLE_WAVE_ID_PREFIX,
    PROGRAMMABLE_WAVE_SYMBOL_PREFIX,
};
use super::wav::{self, WavSample};
use super::{read_file, ExtractError};
use assets::{DirectSoundSample, ProgrammableWave, Sample};
use pack_format::{PackEntry, PackWriter};

const DIRECT_SOUND_DIRECTORY: &str = "sound/direct_sound_samples";
const PROGRAMMABLE_WAVE_DIRECTORY: &str = "sound/programmable_wave_samples";

const PROGRAMMABLE_WAVE_SIZE: usize = 16;

/// Maps one decoded WAV onto [`assets::Sample`] and encodes it through
/// `assets`' canonical encoder.
///
/// `sample.data` always holds exactly one more value than `sample_count`: a
/// retained interpolation-guard sample past the logical end (see
/// `wav.rs`'s module docs and `assets::DirectSoundSample`).
///
/// # Errors
///
/// Returns the [`assets::AudioError`] `assets` raises when the decoded fields
/// are outside the sample schema's domain.
fn encode_direct_sound(sample: WavSample) -> Result<Vec<u8>, assets::AudioError> {
    let direct_sound = DirectSoundSample::new(
        sample.base_frequency,
        sample.loop_start,
        sample.sample_count,
        sample.data,
    )?;
    Ok(Sample::DirectSound(direct_sound).encode())
}

fn encode_programmable_wave(table: [u8; PROGRAMMABLE_WAVE_SIZE]) -> Vec<u8> {
    Sample::ProgrammableWave(ProgrammableWave { table }).encode()
}

/// Decodes one `DirectSound` WAV and wraps its payload as the raw pack entry `id`.
///
/// The one codec every `DirectSound` family shares, so instrument and cry samples are
/// byte-identical for the same source.
pub(super) fn direct_sound_entry_from_path(
    source_path: &Path,
    id: String,
) -> Result<PackEntry, ExtractError> {
    let source = read_file(source_path)?;
    let sample = wav::decode(&source)
        .map_err(|error| ExtractError::Wav(source_path.to_path_buf(), error))?;

    let payload =
        encode_direct_sound(sample).map_err(|source| ExtractError::InvalidSampleSchema {
            path: source_path.to_path_buf(),
            source,
        })?;

    Ok(pack_format::raw_entry(id, payload))
}

fn programmable_wave_entry(
    upstream: &Path,
    id: &str,
    stem: &str,
) -> Result<PackEntry, ExtractError> {
    let source_path = upstream
        .join(PROGRAMMABLE_WAVE_DIRECTORY)
        .join(format!("{stem}.pcm"));
    let source = read_file(&source_path)?;
    let table = <[u8; PROGRAMMABLE_WAVE_SIZE]>::try_from(source.as_slice()).map_err(|_| {
        ExtractError::ProgrammableWaveWrongSize {
            path: source_path,
            actual: source.len(),
        }
    })?;

    Ok(pack_format::raw_entry(
        id.to_owned(),
        encode_programmable_wave(table),
    ))
}

/// A sample id's source file: a `DirectSound` WAV name or a programmable-wave number.
enum SampleSource<'a> {
    DirectSound(&'a str),
    ProgrammableWave(u32),
}

impl<'a> SampleSource<'a> {
    /// The source `id` names, or `None` unless it is a canonical instrument sample id: a
    /// lowercase-alphanumeric-and-underscore name, or a two-digit wave number.
    fn from_id(id: &'a str) -> Option<Self> {
        if let Some(name) = id.strip_prefix(DIRECT_SOUND_ID_PREFIX) {
            let is_name = !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
            return is_name.then_some(Self::DirectSound(name));
        }
        let digits = id.strip_prefix(PROGRAMMABLE_WAVE_ID_PREFIX)?;
        let is_number = digits.len() >= 2 && digits.bytes().all(|byte| byte.is_ascii_digit());
        let number: u32 = digits.parse().ok().filter(|_| is_number)?;
        (format!("{number:02}") == digits).then_some(Self::ProgrammableWave(number))
    }
}

/// Extracts every sample in `sample_ids` as an `audio/sample/*`
/// [`pack_format::EntryKind::Raw`] entry, in id order.
///
/// A sample's source is the file its symbol's `.incbin` definition names
/// ([`definitions`]), not a name inferred from the symbol.
///
/// # Errors
///
/// Returns [`ExtractError::MalformedSampleId`] for an id that names no canonical sample
/// symbol, [`ExtractError::UndefinedSample`] or [`ExtractError::SampleDefinitions`] for a symbol
/// with no usable definition, [`ExtractError::ReadFailed`] for an unreadable source,
/// [`ExtractError::Wav`] for an invalid WAV, [`ExtractError::ProgrammableWaveWrongSize`] for a
/// table that is not 16 bytes, or [`ExtractError::InvalidSampleSchema`] when `assets` rejects a
/// decoded sample.
pub(super) fn extract_audio_samples(
    upstream: &Path,
    sample_ids: &BTreeSet<String>,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let direct_sound = Definitions::read(upstream, DIRECT_SOUND_DEFINITIONS_PATH)?;
    let programmable_wave = Definitions::read(upstream, PROGRAMMABLE_WAVE_DEFINITIONS_PATH)?;
    for id in sample_ids {
        let entry = match SampleSource::from_id(id) {
            Some(SampleSource::DirectSound(name)) => {
                let label = format!("{DIRECT_SOUND_SYMBOL_PREFIX}{name}");
                let stem =
                    direct_sound.stem_of(&label, &format!("{DIRECT_SOUND_DIRECTORY}/"), ".bin")?;
                let source_path = upstream
                    .join(DIRECT_SOUND_DIRECTORY)
                    .join(format!("{stem}.wav"));
                direct_sound_entry_from_path(&source_path, id.clone())?
            }
            Some(SampleSource::ProgrammableWave(number)) => {
                let label = format!("{PROGRAMMABLE_WAVE_SYMBOL_PREFIX}{number}");
                let stem = programmable_wave.stem_of(
                    &label,
                    &format!("{PROGRAMMABLE_WAVE_DIRECTORY}/"),
                    ".pcm",
                )?;
                programmable_wave_entry(upstream, id, stem)?
            }
            None => return Err(ExtractError::MalformedSampleId(id.clone())),
        };
        writer.push(entry);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
