//! [`AudioError`]: the audio-pack schemas' error type.
//!
//! A dedicated enum, kept separate from [`crate::error::AssetError`] for the
//! same reason [`crate::pack::PackError`] is: it needs owned `String`
//! payloads (an unrecognized voicegroup-entry kind's context, an invalid
//! UTF-8 id) that would break the `const fn`-initializer tables elsewhere in
//! this crate if folded into `AssetError` — see `crate::error`'s module docs
//! for the full explanation, and `crate::pack::PackError`'s docs for the
//! prior instance of this exact split.

use std::fmt;

/// An error produced while decoding a [`super::song::Song`],
/// [`super::voicegroup::VoiceGroup`], or [`super::sample::Sample`] from
/// encoded bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// The buffer ended before the format's shape said it should — either
    /// truncated input or a length field that overran the actual data.
    Truncated,
    /// A length-prefixed string field's bytes were not valid UTF-8.
    InvalidString,
    /// The leading [`super::AUDIO_SCHEMA_VERSION`] field did not match the
    /// version this build's decoder understands. Carries the version found
    /// and which schema was being decoded.
    UnsupportedVersion { schema: &'static str, found: u32 },
    /// A sample's `kind` tag byte was not one of the two this format
    /// defines (0 = `DirectSound`, 1 = programmable wave). Carries the
    /// offending byte.
    UnknownSampleKind(u8),
    /// A voicegroup slot's `kind` tag byte was not one of the seven this
    /// format defines. Carries the offending byte.
    UnknownVoiceKind(u8),
    /// A `DirectSound` voice's `mode` tag byte was not one of the three this
    /// format defines (0 = resampled, 1 = fixed-rate, 2 = the rare `_alt`
    /// on-disk form). Carries the offending byte.
    UnknownDirectSoundMode(u8),
    /// A song event's tag byte was not one of the ones this format defines.
    /// Carries the offending byte.
    UnknownSongEvent(u8),
    /// A [`super::voicegroup::VoiceGroup`] was built ([`super::voicegroup::VoiceGroup::new`])
    /// or decoded with more than [`super::voicegroup::VOICE_SLOT_COUNT`]
    /// slots. Carries the slot count found.
    TooManyVoiceSlots(usize),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "audio-pack entry: truncated or corrupt"),
            Self::InvalidString => write!(f, "audio-pack entry: invalid UTF-8 in a string field"),
            Self::UnsupportedVersion { schema, found } => write!(
                f,
                "audio-pack {schema}: unsupported schema version `{found}` -- \
                 this pack entry predates this build's format; regenerate the \
                 pack with the matching extraction backend"
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
        }
    }
}

impl std::error::Error for AudioError {}
