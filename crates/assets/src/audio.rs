//! Backend-neutral schemas for audio entries in the local asset pack.
//!
//! The developer extractor and ROM importer produce payloads that follow these schemas;
//! [`crate::pack::AssetPack`] decodes them for runtime consumers. This module defines the shared
//! payload boundary. It performs no extraction or playback and does not resolve referenced
//! voicegroups.
//!
//! - [`Sample`] stores a normalized `DirectSound` waveform or programmable-wave table.
//! - [`VoiceGroup`] stores addressable instrument slots. Key-split and rhythm slots refer to
//!   another group by [`VoiceGroupId`] instead of embedding it.
//! - [`Song`] stores header metadata and normalized per-track [`SongEvent`] streams. Events use
//!   musical commands and event-index jumps, not source MIDI or compiled MP2K bytes.
//!
//! [`SampleId`] and [`VoiceGroupId`] name related pack entries. Pokémon cries follow a separate
//! species-indexed playback path and are not represented by these schemas.
//!
//! # Versioning
//!
//! Payloads do not carry separate version fields. [`crate::pack::FORMAT_VERSION`] is the
//! compatibility gate for the container and all entry contents, so an incompatible encoding
//! change here requires a pack-format version bump.

mod cursor;
mod error;
mod sample;
mod song;
mod voicegroup;

pub use error::AudioError;
pub use sample::{DirectSoundSample, ProgrammableWave, Sample, SampleId};
pub use song::{MemAccCondition, MemAccOp, Song, SongEvent, MAX_TRACKS};
pub use voicegroup::{
    DirectSoundMode, DirectSoundVoice, Envelope, KeySplitVoice, NoiseVoice, ProgrammableWaveVoice,
    RhythmVoice, Square1Voice, Square2Voice, VoiceEntry, VoiceGroup, VoiceGroupId,
    VOICE_SLOT_COUNT,
};
