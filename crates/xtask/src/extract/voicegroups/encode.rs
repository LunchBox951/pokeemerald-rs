//! Encodes resolved voicegroups into the asset pack's voicegroup payload.
//! The pack container around this payload is shared (`pack_format` owns it).
//! The payload schema is owned by `assets`: each [`ResolvedVoiceGroup`] is
//! mapped onto [`assets::VoiceGroup`] and written by [`assets::VoiceGroup::encode`],
//! so the tags, field order, id framing, and pan sentinel exist in exactly one
//! place and the runtime decoder validates what the extractor writes.
//!
//! The mapping is the only schema knowledge kept here. Building the typed
//! group re-runs `assets`' own validation, so a value outside the schema's
//! domain surfaces as [`VoiceGroupError::InvalidAssetSchema`] rather than as
//! a pack the runtime would refuse to load. The parser and resolver already
//! reject those values with source-specific errors first, so the extractor
//! never reports this variant for upstream's real voicegroups.

use assets::{
    DirectSoundMode as AssetDirectSoundMode, DirectSoundVoice, Envelope as AssetEnvelope,
    KeySplitVoice, NoiseVoice, ProgrammableWaveVoice, RhythmVoice, SampleId, Square1Voice,
    Square2Voice, VoiceEntry, VoiceGroup, VoiceGroupId,
};

use super::parser::{DirectSoundMode, Envelope, VoiceGroupError};
use super::resolve::{ResolvedVoiceGroup, VoiceSlot};

const fn asset_mode(mode: DirectSoundMode) -> AssetDirectSoundMode {
    match mode {
        DirectSoundMode::Resampled => AssetDirectSoundMode::Resampled,
        DirectSoundMode::Fixed => AssetDirectSoundMode::Fixed,
        DirectSoundMode::Reverse => AssetDirectSoundMode::Reverse,
    }
}

const fn asset_envelope(envelope: Envelope) -> AssetEnvelope {
    AssetEnvelope {
        attack: envelope.attack,
        decay: envelope.decay,
        sustain: envelope.sustain,
        release: envelope.release,
    }
}

/// Encodes `group` through `assets`' canonical voicegroup encoder.
///
/// # Errors
///
/// Returns [`VoiceGroupError::InvalidAssetSchema`] when `assets` rejects a
/// slot as outside its schema's domain.
pub(super) fn encode_voice_group(group: &ResolvedVoiceGroup) -> Result<Vec<u8>, VoiceGroupError> {
    let invalid = |source| VoiceGroupError::InvalidAssetSchema {
        group: group.label.clone(),
        source,
    };
    let slots = group
        .slots
        .iter()
        .map(|slot| asset_entry(slot).map_err(invalid))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(VoiceGroup::new(slots).map_err(invalid)?.encode())
}

fn asset_entry(slot: &VoiceSlot) -> Result<VoiceEntry, assets::AudioError> {
    Ok(match slot {
        VoiceSlot::DirectSound {
            base_key,
            pan,
            sample_id,
            envelope,
            mode,
        } => VoiceEntry::DirectSound(DirectSoundVoice {
            base_key: *base_key,
            pan: *pan,
            sample: SampleId(sample_id.clone()),
            envelope: asset_envelope(*envelope),
            mode: asset_mode(*mode),
        }),
        VoiceSlot::Square1 {
            base_key,
            length,
            sweep,
            duty,
            envelope,
            fixed_rate,
        } => VoiceEntry::Square1(Square1Voice {
            base_key: *base_key,
            length: *length,
            sweep: *sweep,
            duty: *duty,
            envelope: asset_envelope(*envelope),
            fixed_rate: *fixed_rate,
        }),
        VoiceSlot::Square2 {
            base_key,
            length,
            duty,
            envelope,
            fixed_rate,
        } => VoiceEntry::Square2(Square2Voice {
            base_key: *base_key,
            length: *length,
            duty: *duty,
            envelope: asset_envelope(*envelope),
            fixed_rate: *fixed_rate,
        }),
        VoiceSlot::ProgrammableWave {
            base_key,
            length,
            wave_id,
            envelope,
            fixed_rate,
        } => VoiceEntry::ProgrammableWave(ProgrammableWaveVoice {
            base_key: *base_key,
            length: *length,
            wave: SampleId(wave_id.clone()),
            envelope: asset_envelope(*envelope),
            fixed_rate: *fixed_rate,
        }),
        VoiceSlot::Noise {
            base_key,
            length,
            period,
            envelope,
            fixed_rate,
        } => VoiceEntry::Noise(NoiseVoice {
            base_key: *base_key,
            length: *length,
            period: *period,
            envelope: asset_envelope(*envelope),
            fixed_rate: *fixed_rate,
        }),
        VoiceSlot::KeySplit {
            starting_note,
            table,
            children_id,
        } => VoiceEntry::KeySplit(KeySplitVoice::new(
            *starting_note,
            table.clone(),
            VoiceGroupId(children_id.clone()),
        )?),
        VoiceSlot::Rhythm { children_id } => VoiceEntry::Rhythm(RhythmVoice {
            children: VoiceGroupId(children_id.clone()),
        }),
        VoiceSlot::Empty => VoiceEntry::Empty,
    })
}

#[cfg(test)]
mod tests;
