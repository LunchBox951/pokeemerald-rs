//! Backend-neutral audio-pack schemas (S-4, issue #180, `#115` child 1):
//! the serialized song / voicegroup / sample record types that both
//! audio-extraction backends emit into the local asset pack —
//! the Policy-A dev backend (`cargo xtask extract`, normalizing straight
//! from the upstream `pokeemerald/` checkout's `.mid`/`.inc`/`.bin` sound
//! sources) and a future ROM backend (decompiling an already-built ROM's
//! compiled MP2K sound engine data). Runtime consumers read these types back
//! out of the pack (`crate::pack`) without ever knowing which backend
//! produced them — the same "policy A" separation Discussion #71 established
//! for graphics/map assets, applied here to audio.
//!
//! # Scope: schema only
//!
//! This module defines the *shapes* and their encode/decode — it does not
//! extract anything from `pokeemerald/`, does not compile MIDI, does not
//! resolve a voicegroup's key splits/rhythm indirection into concrete
//! instruments, and does not play sound. Those are the four remaining
//! `#115` children (MIDI compiler, voicegroup resolver, WAV normalization,
//! pack accessors) and build on top of the types here. Nothing in this
//! module reads a file, references a ROM offset, or names an upstream
//! checkout path — see [`song`], [`voicegroup`], and [`sample`]'s own docs
//! for the specific upstream structures each type's *shape* is modelled on
//! (cited for provenance only, exactly as [`crate::map_layouts`] cites
//! `struct MapLayout`).
//!
//! # The three schemas
//!
//! - [`sample::Sample`]: one waveform — either a `DirectSound` instrument
//!   sample (8-bit signed PCM, upstream `struct WaveData`,
//!   `pokeemerald/include/gba/m4a_internal.h`) or a CGB programmable-wave
//!   table (the 16-byte packed nibble waveform hardware channel 3 plays).
//! - [`voicegroup::VoiceGroup`]: up to 128 instrument slots (upstream
//!   `struct ToneData`), selected by a song's `VOICE` command. A slot is
//!   either a concrete leaf instrument (`DirectSound`, one of the two square
//!   channels, programmable wave, or noise) or an indirection — key-split or
//!   rhythm — that resolves through *another* [`voicegroup::VoiceGroup`]
//!   pack entry, referenced by its stable [`voicegroup::VoiceGroupId`]
//!   rather than embedded inline (matching upstream's own pointer
//!   indirection, and letting instrument groups like `trumpet_keysplit` be
//!   shared across many songs' voicegroups without duplication).
//! - [`song::Song`]: one or more normalized, MIDI-semantic event streams —
//!   note/wait/controller commands, not raw standard-MIDI bytes and not
//!   upstream's own compiled MP2K byte-code — plus the song-level metadata
//!   (which [`voicegroup::VoiceGroup`] it plays through, priority, reverb,
//!   initial tempo) upstream's `struct SongHeader` carries alongside the
//!   per-track data.
//!
//! # All 128 voice slots
//!
//! A `VOICE` command operand is a plain byte (`0..=255` on the wire; `0..127`
//! by General-MIDI-program convention, matching upstream's own usage) and a
//! [`voicegroup::VoiceGroup`] may define anywhere from 1 to
//! [`voicegroup::VOICE_SLOT_COUNT`] (128) slots — several real upstream
//! voicegroups (e.g. `sound/voicegroups/vs_rayquaza.inc`) use the full 128.
//! `MUS_TITLE`'s own MIDI source selects instrument 127 on one of its
//! channels, so slot 127 is not a theoretical edge: [`song::SongEvent::Voice`]
//! and [`voicegroup::VoiceGroup`] both round-trip it (see each module's
//! tests).
//!
//! # Versioning
//!
//! Every encoded payload — [`song::Song::encode`],
//! [`voicegroup::VoiceGroup::encode`], [`sample::Sample::encode`] — begins
//! with a 4-byte little-endian [`AUDIO_SCHEMA_VERSION`] field, decoded and
//! checked first by the matching `decode`. This is a second, independent
//! version from [`crate::pack::FORMAT_VERSION`] (which only versions the
//! *outer* pack directory's shape): these three payload shapes are new and
//! still settling, so they get their own version to bump without forcing an
//! unrelated outer-format bump, the same layering
//! [`crate::pack::EntryKind::Raw`] already gives every other typed decode
//! layer in this crate (`map_layouts`, `metatile_attributes`, `fonts`) —
//! those simply haven't needed a self-describing version yet because their
//! shapes have been stable since introduction.
//!
//! # Deliberately deferred
//!
//! [`song::SongEvent`] covers the note/timing/controller commands every
//! song audibly needs (`(behavioral-fidelity)`), but not upstream's
//! loop-compression forms (`PATT`/`PEND`/`REPT`), the `MEMACC`
//! conditional-jump mini-language, `XCMD`, or `PORT` — all rare, and none
//! needed to represent a song's *musical* content, only its on-disk
//! compression. A later slice can extend the enum under a new
//! [`AUDIO_SCHEMA_VERSION`] if a concrete song turns out to need one
//! `(test-ratchet` note: that would be an additive change, not a breaking
//! one, since decoders already reject an unknown version rather than
//! silently misreading newer data`)`.

mod cursor;
mod error;
mod sample;
mod song;
mod voicegroup;

pub use error::AudioError;
pub use sample::{DirectSoundSample, ProgrammableWave, Sample, SampleId};
pub use song::{Song, SongEvent};
pub use voicegroup::{
    DirectSoundMode, DirectSoundVoice, Envelope, KeySplitVoice, NoiseVoice, ProgrammableWaveVoice,
    RhythmVoice, Square1Voice, Square2Voice, VoiceEntry, VoiceGroup, VoiceGroupId,
    VOICE_SLOT_COUNT,
};

/// The shared payload-format version every [`song::Song`], [`voicegroup::VoiceGroup`],
/// and [`sample::Sample`] encoding is prefixed with. See the module docs'
/// "Versioning" section.
pub const AUDIO_SCHEMA_VERSION: u32 = 1;
