//! [`Sample`]: one waveform, as either backend normalizes it into the pack.
//!
//! Two shapes, matching the two waveform kinds upstream's sound engine
//! plays: a `DirectSound` instrument sample (upstream `struct WaveData`,
//! `pokeemerald/include/gba/m4a_internal.h`) — signed 8-bit PCM, optionally
//! looping — or a CGB programmable-wave table (the 16-byte packed-nibble
//! waveform hardware channel 3 plays, upstream `voice_programmable_wave`'s
//! `wave_samples_pointer` target). A [`super::voicegroup::VoiceGroup`] slot
//! references one of these by its stable pack id ([`SampleId`]) rather than
//! embedding it — many voicegroup slots across many songs share the same
//! underlying sample, and keeping samples as their own pack entries lets the
//! pack (a later `#115` child's concern, not this one's) store each unique
//! waveform once.
//!
//! WAV normalization itself (upstream `.wav`/`.bin` sample sources through
//! resampling/loop-point extraction into this shape) is `#115` child 4, out
//! of scope here — this module only defines the shape and its `encode`/
//! `decode`.

use super::cursor::{Reader, Writer};
use super::error::AudioError;

/// A sample's stable pack id — the normalized asset id a
/// [`super::voicegroup::VoiceGroup`] slot references (e.g.
/// `"audio/sample/sc88pro_trumpet_60"`), not the sample's payload itself.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleId(pub String);

const KIND_DIRECT_SOUND: u8 = 0;
const KIND_PROGRAMMABLE_WAVE: u8 = 1;

/// Cap on how many samples [`Sample::decode`] pre-reserves from the
/// untrusted `sample_count` field, mirroring `crate::pack::format`'s
/// `MAX_PREALLOC_ENTRIES` (same rationale: a corrupt count near `u32::MAX`
/// must not speculatively allocate gigabytes before the first short read
/// fails the decode). The `Vec` still grows to whatever the input actually
/// holds.
const MAX_PREALLOC_SAMPLES: usize = 1 << 20;

/// One waveform: `DirectSound` PCM or a CGB programmable-wave table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sample {
    DirectSound(DirectSoundSample),
    ProgrammableWave(ProgrammableWave),
}

/// A `DirectSound` instrument sample: signed 8-bit PCM, optionally looping.
///
/// Mirrors upstream `struct WaveData`'s fields (`freq`, `loopStart`,
/// `size`/`data`) minus `status` — engine runtime state, not sample content
/// — and minus `type`.
///
/// `type` is not runtime state, and dropping it is a deliberate content
/// decision rather than a bookkeeping one: a non-zero `WaveData::type` means
/// the payload is DPCM-compressed (`tools/wav2agb -c`; every Pokémon cry is
/// stored this way), and `SoundMainRAM_Unk1` branches on it
/// (`pokeemerald/src/m4a_1.s`) to expand the nibble-delta stream while
/// mixing. Samples in this schema are always stored *decompressed*: a
/// backend reading DPCM-compressed `WaveData` (`type == 1`, e.g. all cries)
/// must expand it during extraction, and [`data`](Self::data) is the
/// expanded signed 8-bit PCM. There is deliberately no way to represent a
/// still-compressed payload — the pack is a local build artifact, so a
/// smaller on-disk form is worth less than a mixer that never has to
/// decompress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectSoundSample {
    /// The wave's pre-scaled base pitch constant (upstream `WaveData::freq`)
    /// — not a plain sample rate; it already bakes in the fixed-point
    /// scaling the mixer's pitch calculation expects.
    pub base_frequency: u32,
    /// The first sample index playback wraps to after the last sample, or
    /// `None` for a one-shot (non-looping) sample (upstream's
    /// `WAVE_DATA_FLAG_LOOP` status bit gates whether `loopStart` is used at
    /// all, so a non-looping sample's `loopStart` is not meaningful data —
    /// modelled as absent here rather than as a don't-care `0`).
    pub loop_start: Option<u32>,
    /// The signed 8-bit PCM samples, in playback order.
    pub data: Vec<i8>,
}

/// A CGB programmable-wave table: 16 bytes, two 4-bit samples each (hardware
/// channel 3's wave RAM shape). Referenced by
/// [`super::voicegroup::ProgrammableWaveVoice::wave`] the same way a
/// `DirectSound` voice references a [`DirectSoundSample`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgrammableWave {
    pub table: [u8; 16],
}

impl Sample {
    /// Encode to this schema's binary form. Staleness is gated by
    /// [`crate::pack::FORMAT_VERSION`] — see `super`'s module docs,
    /// "Versioning".
    ///
    /// # Panics
    ///
    /// Never in practice: only if a [`DirectSoundSample::data`] holds more
    /// than `u32::MAX` samples, which no real GBA sample does (the whole
    /// GBA cartridge address space is 32 MiB).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            Self::DirectSound(sample) => {
                w.u8(KIND_DIRECT_SOUND);
                w.u32(sample.base_frequency);
                w.bool(sample.loop_start.is_some());
                w.u32(sample.loop_start.unwrap_or(0));
                let len =
                    u32::try_from(sample.data.len()).expect("sample longer than 4 GiB samples");
                w.u32(len);
                for &s in &sample.data {
                    w.i8(s);
                }
            }
            Self::ProgrammableWave(wave) => {
                w.u8(KIND_PROGRAMMABLE_WAVE);
                w.bytes(&wave.table);
            }
        }
        w.into_bytes()
    }

    /// Decode from [`encode`](Self::encode)'s binary form.
    ///
    /// Structural decode only: a decoded [`DirectSoundSample::loop_start`]
    /// is *not* validated against [`DirectSoundSample::data`]'s length, so a
    /// loop point past the end of the wave decodes cleanly. Validating that
    /// (and the [`SampleId`]/[`super::voicegroup::VoiceGroupId`] cross
    /// references, which this module equally does not resolve) belongs to
    /// the later `#115` child that loads a whole pack's audio entries
    /// together and can see all of them at once.
    ///
    /// # Errors
    ///
    /// [`AudioError::Truncated`] if `bytes` is shorter than the format
    /// requires; [`AudioError::UnknownSampleKind`] for an unrecognized kind
    /// tag.
    pub fn decode(bytes: &[u8]) -> Result<Self, AudioError> {
        let mut r = Reader::new(bytes);
        match r.u8()? {
            KIND_DIRECT_SOUND => {
                let base_frequency = r.u32()?;
                let looping = r.bool()?;
                let loop_start_field = r.u32()?;
                let loop_start = looping.then_some(loop_start_field);
                let len = usize::try_from(r.u32()?).map_err(|_| AudioError::Truncated)?;
                let mut data = Vec::with_capacity(len.min(MAX_PREALLOC_SAMPLES));
                for _ in 0..len {
                    data.push(r.i8()?);
                }
                Ok(Self::DirectSound(DirectSoundSample {
                    base_frequency,
                    loop_start,
                    data,
                }))
            }
            KIND_PROGRAMMABLE_WAVE => {
                let mut table = [0u8; 16];
                for b in &mut table {
                    *b = r.u8()?;
                }
                Ok(Self::ProgrammableWave(ProgrammableWave { table }))
            }
            other => Err(AudioError::UnknownSampleKind(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_sound_one_shot_round_trips() {
        let sample = Sample::DirectSound(DirectSoundSample {
            base_frequency: 1 << 20,
            loop_start: None,
            data: vec![-128, -1, 0, 1, 127],
        });
        let bytes = sample.encode();
        assert_eq!(Sample::decode(&bytes).unwrap(), sample);
    }

    #[test]
    fn direct_sound_looping_round_trips() {
        let data: Vec<i8> = (0..100i32)
            .map(|i| i8::try_from(i % 7 - 3).expect("in -3..=3"))
            .collect();
        let sample = Sample::DirectSound(DirectSoundSample {
            base_frequency: 0x0012_3456,
            loop_start: Some(42),
            data,
        });
        let bytes = sample.encode();
        assert_eq!(Sample::decode(&bytes).unwrap(), sample);
    }

    #[test]
    fn direct_sound_empty_data_round_trips() {
        let sample = Sample::DirectSound(DirectSoundSample {
            base_frequency: 0,
            loop_start: None,
            data: vec![],
        });
        let bytes = sample.encode();
        assert_eq!(Sample::decode(&bytes).unwrap(), sample);
    }

    #[test]
    fn programmable_wave_round_trips() {
        let mut table = [0u8; 16];
        for (i, b) in table.iter_mut().enumerate() {
            *b = u8::try_from(i * 17).expect("i in 0..16, i * 17 <= 255");
        }
        let sample = Sample::ProgrammableWave(ProgrammableWave { table });
        let bytes = sample.encode();
        assert_eq!(Sample::decode(&bytes).unwrap(), sample);
    }

    #[test]
    fn unknown_kind_byte_is_rejected() {
        let mut bytes = Sample::ProgrammableWave(ProgrammableWave { table: [0; 16] }).encode();
        bytes[0] = 0xFF; // the leading kind tag
        assert_eq!(
            Sample::decode(&bytes),
            Err(AudioError::UnknownSampleKind(0xFF))
        );
    }

    #[test]
    fn a_loop_start_past_the_end_of_the_data_still_decodes() {
        // Documented non-validation: `decode` is structural, and cross-field
        // validation is a later `#115` child's job. Pin the current
        // behaviour so a future change to it is a deliberate one.
        let sample = Sample::DirectSound(DirectSoundSample {
            base_frequency: 1,
            loop_start: Some(9_999),
            data: vec![0, 1, 2],
        });
        assert_eq!(Sample::decode(&sample.encode()).unwrap(), sample);
    }

    #[test]
    fn truncated_input_is_rejected() {
        let bytes = Sample::DirectSound(DirectSoundSample {
            base_frequency: 1,
            loop_start: Some(0),
            data: vec![1, 2, 3],
        })
        .encode();
        for cut in 0..bytes.len() {
            assert!(Sample::decode(&bytes[..cut]).is_err());
        }
    }
}
