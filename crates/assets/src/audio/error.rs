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
    /// A pack id (a [`super::sample::SampleId`] or a
    /// [`super::voicegroup::VoiceGroupId`]) was longer than the `u16` length
    /// prefix written in front of it can describe. Carries the offending
    /// length in bytes.
    IdTooLong(usize),
    /// A sample's `kind` tag byte was not one of the two this format
    /// defines (0 = `DirectSound`, 1 = programmable wave). Carries the
    /// offending byte.
    UnknownSampleKind(u8),
    /// A voicegroup slot's `kind` tag byte was not one of the seven this
    /// format defines. Carries the offending byte.
    UnknownVoiceKind(u8),
    /// A `DirectSound` voice's `mode` tag byte was not one of the three this
    /// format defines (0 = resampled, 1 = fixed-rate, 2 = reverse). Carries
    /// the offending byte.
    UnknownDirectSoundMode(u8),
    /// A song event's tag byte was not one of the ones this format defines.
    /// Carries the offending byte.
    UnknownSongEvent(u8),
    /// A [`super::voicegroup::VoiceGroup`] was built ([`super::voicegroup::VoiceGroup::new`])
    /// or decoded with more than [`super::voicegroup::VOICE_SLOT_COUNT`]
    /// slots. Carries the slot count found.
    TooManyVoiceSlots(usize),
    /// A [`super::voicegroup::KeySplitVoice`] was built
    /// ([`super::voicegroup::KeySplitVoice::new`]) or decoded with a table
    /// longer than [`super::voicegroup::VOICE_SLOT_COUNT`] entries. Carries
    /// the table length found.
    KeySplitTableTooLong(usize),
    /// A [`super::song::Song`] was built ([`super::song::Song::new`]) with
    /// more tracks than the encoding's `u8` track count can describe.
    /// Carries the track count found.
    TooManyTracks(usize),
    /// A [`super::song::Song`] track was built with more events than the
    /// encoding's `u32` per-track event count can describe. Carries the
    /// event count found.
    TooManyEvents(usize),
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
        }
    }
}

impl std::error::Error for AudioError {}
