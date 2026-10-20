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
//! - [`CryTable`] stores the normalized species-cry table of [`SampleId`] references, with the
//!   `SpeciesToCryId` lookup ([`cry_id`]) and the `PlayCryInternal` mode settings ([`CryMode`]).
//!
//! [`SampleId`] and [`VoiceGroupId`] name related pack entries. Upstream plays Pokémon cries from a
//! species-indexed `ToneData` table rather than through a song and voicegroup, so cries have their
//! own schema; the port has no cry playback yet.
//!
//! # Deferred commands
//!
//! [`SongEvent`] has no variant for `PATT`/`PEND`, `REPT`, `PORT`, or the `XCMD` sub-commands other
//! than `xIECV`/`xIECL`. A pattern call is expanded inline because it is a size representation, not
//! musical content; the rest are never emitted by `tools/mid2agb`. Adding a variant extends the tag
//! space, gated like every other content change by a [`crate::pack::FORMAT_VERSION`] bump.
//!
//! # Versioning
//!
//! Payloads do not carry separate version fields. [`crate::pack::FORMAT_VERSION`] is the
//! compatibility gate for the container and all entry contents, so an incompatible encoding
//! change here requires a pack-format version bump.

mod cry;
mod cursor;
mod error;
mod sample;
mod song;
mod voicegroup;

pub use cry::{
    cry_id, CryId, CryMode, CrySettings, CryTable, ResolvedCry, CRY_PRIORITY_AMBIENT,
    CRY_PRIORITY_NORMAL, CRY_TABLE_LEN, CRY_TABLE_PACK_ID, CRY_VOLUME,
};
pub use error::AudioError;
pub use sample::{DirectSoundSample, ProgrammableWave, Sample, SampleId};
pub use song::{MemAccCondition, MemAccOp, MusicPlayerId, Song, SongEvent, MAX_TRACKS};
pub use voicegroup::{
    DirectSoundMode, DirectSoundVoice, Envelope, KeySplitVoice, NoiseVoice, ProgrammableWaveVoice,
    RhythmVoice, Square1Voice, Square2Voice, VoiceEntry, VoiceGroup, VoiceGroupId,
    VOICE_SLOT_COUNT,
};
