//! Binary schema for normalized `DirectSound` and programmable-wave samples.

use super::cursor::{Reader, Writer};
use super::error::AudioError;

/// A sample's stable asset-pack identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum SampleKind {
    DirectSound = 0,
    ProgrammableWave = 1,
}

impl SampleKind {
    const fn tag(self) -> u8 {
        self as u8
    }

    fn from_tag(tag: u8) -> Result<Self, AudioError> {
        match tag {
            tag if tag == Self::DirectSound.tag() => Ok(Self::DirectSound),
            tag if tag == Self::ProgrammableWave.tag() => Ok(Self::ProgrammableWave),
            tag => Err(AudioError::UnknownSampleKind(tag)),
        }
    }
}

const PROGRAMMABLE_WAVE_BYTE_COUNT: usize = 16;
// Bounds speculative allocation from an untrusted sample count; complete
// inputs can grow past it.
const MAX_PREALLOCATED_DIRECT_SOUND_SAMPLES: usize = 1 << 20;

/// One normalized waveform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sample {
    DirectSound(DirectSoundSample),
    ProgrammableWave(ProgrammableWave),
}

/// A normalized `DirectSound` waveform with signed 8-bit PCM data.
///
/// Upstream `WaveData` with a nonzero `type` stores DPCM that
/// `SoundMainRAM_Unk1` expands while mixing (`pokeemerald/src/m4a_1.s`).
/// Extraction must expand such data before constructing this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectSoundSample {
    /// The pre-scaled pitch constant expected by the mixer.
    pub base_frequency: u32,
    loop_start: Option<u32>,
    data: Vec<i8>,
}

fn checked_sample_count(len: usize) -> Result<u32, AudioError> {
    u32::try_from(len).map_err(|_| AudioError::SampleTooLong(len))
}

impl DirectSoundSample {
    /// Constructs a sample with wire-format and playback bounds validated.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::SampleTooLong`] when the PCM length does not fit
    /// the encoded `u32` sample count.
    ///
    /// Returns [`AudioError::LoopStartOutOfRange`] when a loop starts at or
    /// after the end of the PCM data. Upstream derives the loop length as
    /// `size - loopStart`, so every loop must contain a sample
    /// (`pokeemerald/src/m4a_1.s`).
    pub fn new(
        base_frequency: u32,
        loop_start: Option<u32>,
        data: Vec<i8>,
    ) -> Result<Self, AudioError> {
        let sample_count = checked_sample_count(data.len())?;
        if let Some(start) = loop_start {
            if start >= sample_count {
                return Err(AudioError::LoopStartOutOfRange {
                    loop_start: start,
                    sample_count,
                });
            }
        }
        Ok(Self {
            base_frequency,
            loop_start,
            data,
        })
    }

    /// Returns the signed 8-bit PCM samples in playback order.
    #[must_use]
    pub fn data(&self) -> &[i8] {
        &self.data
    }

    /// Returns the first sample replayed after the end, or `None` for one-shot playback.
    #[must_use]
    pub fn loop_start(&self) -> Option<u32> {
        self.loop_start
    }
}

/// A packed CGB wave-RAM table containing two 4-bit samples per byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgrammableWave {
    pub table: [u8; PROGRAMMABLE_WAVE_BYTE_COUNT],
}

impl Sample {
    /// Encodes the sample into its asset-pack representation.
    ///
    /// # Panics
    ///
    /// Panics if this module's private `DirectSound` sample-count invariant is broken.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        match self {
            Self::DirectSound(sample) => {
                writer.u8(SampleKind::DirectSound.tag());
                writer.u32(sample.base_frequency);
                writer.bool(sample.loop_start.is_some());
                writer.u32(sample.loop_start.unwrap_or_default());
                let sample_count = checked_sample_count(sample.data.len())
                    .expect("DirectSoundSample preserves its validated sample count");
                writer.u32(sample_count);
                for &value in &sample.data {
                    writer.i8(value);
                }
            }
            Self::ProgrammableWave(wave) => {
                writer.u8(SampleKind::ProgrammableWave.tag());
                writer.bytes(&wave.table);
            }
        }
        writer.into_bytes()
    }

    /// Decodes and validates one complete asset-pack sample payload.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::Truncated`] for incomplete or structurally
    /// malformed data, [`AudioError::UnknownSampleKind`] for an unknown kind
    /// tag, [`AudioError::LoopStartOutOfRange`] for an invalid loop, or
    /// [`AudioError::TrailingBytes`] when data follows the sample payload.
    pub fn decode(bytes: &[u8]) -> Result<Self, AudioError> {
        let mut reader = Reader::new(bytes);
        match SampleKind::from_tag(reader.u8()?)? {
            SampleKind::DirectSound => {
                let base_frequency = reader.u32()?;
                let looping = reader.bool()?;
                let loop_start_field = reader.u32()?;
                let loop_start = looping.then_some(loop_start_field);
                let sample_count =
                    usize::try_from(reader.u32()?).map_err(|_| AudioError::Truncated)?;
                let mut data =
                    Vec::with_capacity(sample_count.min(MAX_PREALLOCATED_DIRECT_SOUND_SAMPLES));
                for _ in 0..sample_count {
                    data.push(reader.i8()?);
                }
                reader.expect_eof()?;
                Ok(Self::DirectSound(DirectSoundSample::new(
                    base_frequency,
                    loop_start,
                    data,
                )?))
            }
            SampleKind::ProgrammableWave => {
                let mut table = [0u8; PROGRAMMABLE_WAVE_BYTE_COUNT];
                for value in &mut table {
                    *value = reader.u8()?;
                }
                reader.expect_eof()?;
                Ok(Self::ProgrammableWave(ProgrammableWave { table }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_round_trip(sample: &Sample) {
        let decoded = Sample::decode(&sample.encode()).unwrap();
        assert_eq!(&decoded, sample);
    }

    #[test]
    fn decode_rejects_trailing_bytes_after_either_kind() {
        let direct_sound =
            Sample::DirectSound(DirectSoundSample::new(13_379, None, vec![0, 1, -1]).unwrap());
        let wave = Sample::ProgrammableWave(ProgrammableWave {
            table: [7; PROGRAMMABLE_WAVE_BYTE_COUNT],
        });
        for sample in [direct_sound, wave] {
            let mut bytes = sample.encode();
            bytes.push(0);
            assert_eq!(
                Sample::decode(&bytes),
                Err(AudioError::TrailingBytes(1)),
                "{sample:?}"
            );
        }
    }

    #[test]
    fn direct_sound_one_shot_round_trips() {
        assert_round_trip(&Sample::DirectSound(
            DirectSoundSample::new(1 << 20, None, vec![-128, -1, 0, 1, 127]).unwrap(),
        ));
    }

    #[test]
    fn direct_sound_looping_round_trips() {
        let data = (0..100i32)
            .map(|index| i8::try_from(index % 7 - 3).expect("value is in -3..=3"))
            .collect();
        assert_round_trip(&Sample::DirectSound(
            DirectSoundSample::new(0x0012_3456, Some(42), data).unwrap(),
        ));
    }

    #[test]
    fn direct_sound_empty_data_round_trips() {
        assert_round_trip(&Sample::DirectSound(
            DirectSoundSample::new(0, None, vec![]).unwrap(),
        ));
    }

    #[test]
    fn programmable_wave_round_trips() {
        let mut table = [0u8; PROGRAMMABLE_WAVE_BYTE_COUNT];
        for (index, value) in table.iter_mut().enumerate() {
            *value = u8::try_from(index * 17).expect("value fits in a byte");
        }
        assert_round_trip(&Sample::ProgrammableWave(ProgrammableWave { table }));
    }

    #[test]
    fn unknown_kind_byte_is_rejected() {
        let mut bytes = Sample::ProgrammableWave(ProgrammableWave {
            table: [0; PROGRAMMABLE_WAVE_BYTE_COUNT],
        })
        .encode();
        *bytes.first_mut().unwrap() = u8::MAX;
        assert_eq!(
            Sample::decode(&bytes),
            Err(AudioError::UnknownSampleKind(u8::MAX))
        );
    }

    #[test]
    fn constructor_rejects_a_loop_start_at_or_past_the_data_length() {
        for (loop_start, data) in [(3u32, vec![0i8, 1, 2]), (9_999, vec![0, 1, 2]), (0, vec![])] {
            let sample_count = u32::try_from(data.len()).unwrap();
            assert_eq!(
                DirectSoundSample::new(1, Some(loop_start), data),
                Err(AudioError::LoopStartOutOfRange {
                    loop_start,
                    sample_count,
                })
            );
        }
    }

    #[test]
    fn decode_rejects_a_loop_start_at_or_past_the_decoded_data_length() {
        let sample =
            Sample::DirectSound(DirectSoundSample::new(1, Some(2), vec![0i8, 1, 2]).unwrap());
        let mut bytes = sample.encode();
        let loop_start_offset =
            std::mem::size_of::<u8>() + std::mem::size_of::<u32>() + std::mem::size_of::<u8>();
        let loop_start_bytes = bytes
            .get_mut(loop_start_offset..loop_start_offset + std::mem::size_of::<u32>())
            .unwrap();
        loop_start_bytes.copy_from_slice(&9_999u32.to_le_bytes());
        assert_eq!(
            Sample::decode(&bytes),
            Err(AudioError::LoopStartOutOfRange {
                loop_start: 9_999,
                sample_count: 3,
            })
        );
    }

    #[test]
    fn truncated_input_is_rejected() {
        let bytes = Sample::DirectSound(DirectSoundSample::new(1, Some(0), vec![1, 2, 3]).unwrap())
            .encode();
        for cut in 0..bytes.len() {
            assert!(Sample::decode(&bytes[..cut]).is_err());
        }
    }

    #[test]
    fn the_accessor_returns_the_payload_the_constructor_was_given() {
        let sample = DirectSoundSample::new(1, None, vec![-1, 0, 1]).unwrap();
        assert_eq!(sample.data(), &[-1, 0, 1]);
    }

    #[test]
    fn a_payload_too_long_for_the_u32_sample_count_is_rejected() {
        // Constructing the rejected sample would require an impractically large allocation.
        let too_long = u64::from(u32::MAX) + 1;
        if let Ok(len) = usize::try_from(too_long) {
            assert_eq!(
                checked_sample_count(len),
                Err(AudioError::SampleTooLong(len))
            );
        }
        assert_eq!(checked_sample_count(0), Ok(0));
        assert_eq!(
            checked_sample_count(usize::try_from(u32::MAX).expect("u32 fits usize")),
            Ok(u32::MAX)
        );
    }
}
