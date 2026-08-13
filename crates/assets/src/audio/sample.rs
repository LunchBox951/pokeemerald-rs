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
/// `"audio/sample/direct-sound/sc88pro_flute"`), not the sample's payload
/// itself — the extraction pipeline's id scheme, see
/// `xtask::extract::audio_samples`'s module docs.
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
///
/// Build one with [`DirectSoundSample::new`]: the sample count is written as
/// a `u32`, and that bound is what makes [`Sample::encode`] total, so
/// [`data`](Self::data) is private and the bound is enforced at construction
/// — returning [`AudioError::SampleTooLong`] where a caller can see it —
/// rather than left to a panic at encode time. The constructor also enforces
/// that a `Some` [`loop_start`](Self::loop_start) is strictly less than
/// `data`'s length, returning [`AudioError::LoopStartOutOfRange`] otherwise:
/// upstream's mixer computes the loop region's length as `size - loopStart`
/// (`pokeemerald/src/m4a_1.s`), so a loop region at or past the end of the
/// data would be empty or reach past it. Same discipline as
/// [`super::song::Song::new`] and [`super::voicegroup::VoiceGroup::new`].
/// (Its sibling [`ProgrammableWave`] needs no such constructor: its table is
/// a fixed-size `[u8; 16]`, so there is no length to bound.)
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
    data: Vec<i8>,
}

/// The one length bound this schema's sample payload has: the encoding
/// writes the sample count as a `u32`. Shared by
/// [`DirectSoundSample::new`] (which surfaces the failure as
/// [`AudioError::SampleTooLong`]) and [`Sample::encode`] (which, given the
/// constructor already rejected it, can only ever see the `Ok` arm) so the
/// two can never disagree about where the bound is.
fn checked_sample_len(len: usize) -> Result<u32, AudioError> {
    u32::try_from(len).map_err(|_| AudioError::SampleTooLong(len))
}

impl DirectSoundSample {
    /// Build a `DirectSound` sample from its pitch constant, optional loop
    /// point, and PCM payload.
    ///
    /// # Errors
    ///
    /// [`AudioError::SampleTooLong`] if `data.len()` overflows the `u32`
    /// sample count [`Sample::encode`] writes. Unreachable for any real GBA
    /// sample — the whole cartridge address space is 32 MiB — but it is the
    /// one bound that would otherwise be a panic reachable from safe code.
    ///
    /// [`AudioError::LoopStartOutOfRange`] if `loop_start` is `Some` value
    /// that is not strictly less than `data.len()` — including `Some(0)`
    /// against empty data. Upstream's mixer computes the loop region's
    /// length as `size - loopStart` (`pokeemerald/src/m4a_1.s`), so a loop
    /// region must be nonempty.
    pub fn new(
        base_frequency: u32,
        loop_start: Option<u32>,
        data: Vec<i8>,
    ) -> Result<Self, AudioError> {
        let sample_count = checked_sample_len(data.len())?;
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

    /// The signed 8-bit PCM samples, in playback order.
    #[must_use]
    pub fn data(&self) -> &[i8] {
        &self.data
    }
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
    /// Unreachable: [`DirectSoundSample::new`] is the only way to build a
    /// `DirectSound` sample, and it rejects a payload longer than the `u32`
    /// sample count written here — returning [`AudioError::SampleTooLong`]
    /// where a caller can see it. The payload field is private, so it cannot
    /// grow after construction.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            Self::DirectSound(sample) => {
                w.u8(KIND_DIRECT_SOUND);
                w.u32(sample.base_frequency);
                w.bool(sample.loop_start.is_some());
                w.u32(sample.loop_start.unwrap_or(0));
                let len = checked_sample_len(sample.data.len())
                    .expect("DirectSoundSample::new enforces data.len() fits a u32");
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
    /// A decoded [`DirectSoundSample`] is built through
    /// [`DirectSoundSample::new`], so it is validated exactly as a
    /// programmatically-constructed one is — in particular, a `Some`
    /// [`loop_start`](DirectSoundSample::loop_start) at or past the decoded
    /// payload's length is rejected rather than decoding cleanly. Resolving
    /// the [`SampleId`]/[`super::voicegroup::VoiceGroupId`] cross references
    /// is not this module's job — that belongs to the later `#115` child
    /// that loads a whole pack's audio entries together and can see all of
    /// them at once.
    ///
    /// # Errors
    ///
    /// [`AudioError::Truncated`] if `bytes` is shorter than the format
    /// requires; [`AudioError::UnknownSampleKind`] for an unrecognized kind
    /// tag; [`AudioError::LoopStartOutOfRange`] if a looping sample's loop
    /// start is not strictly less than its decoded sample count.
    /// [`AudioError::TrailingBytes`] if unread bytes remain after the
    /// payload.
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
                r.expect_eof()?;
                Ok(Self::DirectSound(DirectSoundSample::new(
                    base_frequency,
                    loop_start,
                    data,
                )?))
            }
            KIND_PROGRAMMABLE_WAVE => {
                let mut table = [0u8; 16];
                for b in &mut table {
                    *b = r.u8()?;
                }
                r.expect_eof()?;
                Ok(Self::ProgrammableWave(ProgrammableWave { table }))
            }
            other => Err(AudioError::UnknownSampleKind(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review finding on #193: a decoder that ignores trailing bytes
    /// validates a corrupt (or newer-producer) payload as its own prefix.
    #[test]
    fn decode_rejects_trailing_bytes_after_either_kind() {
        let ds = Sample::DirectSound(DirectSoundSample::new(13379, None, vec![0, 1, -1]).unwrap());
        let wave = Sample::ProgrammableWave(ProgrammableWave { table: [7; 16] });
        for sample in [ds, wave] {
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
        let sample = Sample::DirectSound(
            DirectSoundSample::new(1 << 20, None, vec![-128, -1, 0, 1, 127]).unwrap(),
        );
        let bytes = sample.encode();
        assert_eq!(Sample::decode(&bytes).unwrap(), sample);
    }

    #[test]
    fn direct_sound_looping_round_trips() {
        let data: Vec<i8> = (0..100i32)
            .map(|i| i8::try_from(i % 7 - 3).expect("in -3..=3"))
            .collect();
        let sample =
            Sample::DirectSound(DirectSoundSample::new(0x0012_3456, Some(42), data).unwrap());
        let bytes = sample.encode();
        assert_eq!(Sample::decode(&bytes).unwrap(), sample);
    }

    #[test]
    fn direct_sound_empty_data_round_trips() {
        let sample = Sample::DirectSound(DirectSoundSample::new(0, None, vec![]).unwrap());
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

    /// The same boundary the constructor enforces, but reached through
    /// [`Sample::decode`]'s route via [`DirectSoundSample::new`]: mutate the
    /// little-endian loop-start field of an otherwise-valid encoded
    /// `DirectSound` payload so the wire bytes are self-consistent (the
    /// `looping` flag and `len` field are untouched) but the loop start no
    /// longer points inside the decoded data.
    #[test]
    fn decode_rejects_a_loop_start_at_or_past_the_decoded_data_length() {
        let sample =
            Sample::DirectSound(DirectSoundSample::new(1, Some(2), vec![0i8, 1, 2]).unwrap());
        let mut bytes = sample.encode();
        // Layout: u8 kind, u32 base_frequency, bool looping, u32 loop_start,
        // u32 len, data...
        let loop_start_field = 1 + 4 + 1;
        bytes[loop_start_field..loop_start_field + 4].copy_from_slice(&9_999u32.to_le_bytes());
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
        // The bound `DirectSoundSample::new` enforces, exercised through the
        // same internal check `encode` relies on. Building the rejected case
        // for real would mean allocating a >4 GiB `Vec<i8>`, which is not
        // something a unit test should do -- so the check itself is the unit
        // under test, and the constructor is verified to route through it by
        // the accepted cases above plus the `Ok` boundary below.
        let too_long = u64::from(u32::MAX) + 1;
        if let Ok(len) = usize::try_from(too_long) {
            assert_eq!(checked_sample_len(len), Err(AudioError::SampleTooLong(len)));
        } // else: a 32-bit `usize` can never exceed `u32::MAX` at all.
        assert_eq!(checked_sample_len(0), Ok(0));
        assert_eq!(
            checked_sample_len(usize::try_from(u32::MAX).expect("u32 fits usize")),
            Ok(u32::MAX)
        );
    }
}
