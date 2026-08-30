//! Failures from constructing or decoding audio-pack schema values.

use std::fmt;

/// An audio-pack schema construction or decoding failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// The payload is incomplete or structurally malformed.
    Truncated,
    /// A length-prefixed string is not valid UTF-8.
    InvalidString,
    /// An id's byte length exceeds its `u16` wire prefix. The value is that length.
    IdTooLong(usize),
    /// A sample kind tag does not identify a [`super::sample::Sample`] variant.
    /// The value is the unrecognized tag.
    UnknownSampleKind(u8),
    /// A voice kind tag does not identify a [`super::voicegroup::VoiceEntry`] variant.
    /// The value is the unrecognized tag.
    UnknownVoiceKind(u8),
    /// A `DirectSound` mode tag does not identify a [`super::voicegroup::DirectSoundMode`].
    /// The value is the unrecognized tag.
    UnknownDirectSoundMode(u8),
    /// A song event tag does not identify a [`super::song::SongEvent`] variant.
    /// The value is the unrecognized tag.
    UnknownSongEvent(u8),
    /// A voicegroup exceeds [`super::voicegroup::VOICE_SLOT_COUNT`] slots. The value is
    /// the slot count.
    TooManyVoiceSlots(usize),
    /// A key-split table exceeds [`super::voicegroup::VOICE_SLOT_COUNT`] entries. The
    /// value is the table length.
    KeySplitTableTooLong(usize),
    /// A song has more tracks than its `u8` wire count can encode. The value is the
    /// track count.
    TooManyTracks(usize),
    /// A song track has more events than its `u32` wire count can encode. The value is
    /// the event count.
    TooManyEvents(usize),
    /// A `DirectSound` sample has more values than its `u32` wire length can encode. The
    /// value is the sample count.
    SampleTooLong(usize),
    /// A loop start is not before the end of its PCM payload.
    ///
    /// Upstream computes the loop length as `size - loopStart`, so a valid
    /// loop must contain at least one sample (`pokeemerald/src/m4a_1.s`).
    LoopStartOutOfRange { loop_start: u32, sample_count: u32 },
    /// A zero pan override cannot round-trip because zero encodes no override.
    PanOverrideZero,
    /// A MEMACC tag identifies neither [`super::song::MemAccOp`] nor
    /// [`super::song::MemAccCondition`]. The value is the unrecognized tag.
    UnknownMemAccOp(u8),
    /// A decoded value did not consume the complete payload. The value is the number
    /// of trailing bytes.
    TrailingBytes(usize),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "audio-pack entry: truncated or corrupt"),
            Self::InvalidString => write!(f, "audio-pack entry: invalid UTF-8 in a string field"),
            Self::IdTooLong(len) => write!(
                f,
                "audio-pack entry: pack id of {len} bytes exceeds the maximum \
                 of {}",
                usize::from(u16::MAX)
            ),
            Self::UnknownSampleKind(byte) => {
                write!(f, "audio-pack sample: invalid kind byte `{byte}`")
            }
            Self::UnknownVoiceKind(byte) => {
                write!(f, "audio-pack voicegroup: invalid slot kind byte `{byte}`")
            }
            Self::UnknownDirectSoundMode(byte) => write!(
                f,
                "audio-pack voicegroup: invalid DirectSound mode byte `{byte}`"
            ),
            Self::UnknownSongEvent(byte) => {
                write!(f, "audio-pack song: invalid event tag byte `{byte}`")
            }
            Self::TooManyVoiceSlots(count) => write!(
                f,
                "audio-pack voicegroup: {count} slots exceeds the maximum of \
                 {}",
                super::voicegroup::VOICE_SLOT_COUNT
            ),
            Self::KeySplitTableTooLong(len) => write!(
                f,
                "audio-pack voicegroup: key-split table of {len} entries \
                 exceeds the maximum of {}",
                super::voicegroup::VOICE_SLOT_COUNT
            ),
            Self::TooManyTracks(count) => write!(
                f,
                "audio-pack song: {count} tracks exceeds the maximum of {}",
                super::song::MAX_TRACKS
            ),
            Self::TooManyEvents(count) => write!(
                f,
                "audio-pack song: a track of {count} events exceeds the \
                 maximum of {}",
                u32::MAX
            ),
            Self::SampleTooLong(len) => write!(
                f,
                "audio-pack sample: {len} samples exceeds the maximum of {}",
                u32::MAX
            ),
            Self::LoopStartOutOfRange {
                loop_start,
                sample_count,
            } => write!(
                f,
                "audio-pack sample: loop start {loop_start} is not less than \
                 the sample count {sample_count}"
            ),
            Self::PanOverrideZero => write!(
                f,
                "audio-pack voicegroup: a DirectSound pan override of Some(0) is \
                 indistinguishable from no override on the wire; use None"
            ),
            Self::UnknownMemAccOp(byte) => {
                write!(f, "audio-pack song: invalid MEMACC op byte `{byte}`")
            }
            Self::TrailingBytes(count) => write!(
                f,
                "audio-pack entry: {count} trailing byte(s) after the decoded payload"
            ),
        }
    }
}

impl std::error::Error for AudioError {}
