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
//!   (which [`voicegroup::VoiceGroup`] it plays through, priority, reverb)
//!   upstream's `struct SongHeader` carries alongside the per-track data.
//!   Tempo is *not* header metadata upstream and is not modelled as such
//!   here — it is a [`song::SongEvent::Tempo`] inside a track's own stream.
//!
//! # Cries are out of scope
//!
//! Pokémon cries are deliberately not modelled by these three schemas.
//! Upstream plays them from `gCryTable`/`gCryTable_Reverse`
//! (`pokeemerald/sound/cry_tables.inc`) — flat arrays of bare `ToneData`
//! entries indexed by species, driven by `PlayCry`'s own
//! `struct PokemonCrySong` synthetic song rather than by a `SongHeader` and
//! a voicegroup. Modelling that path is its own concern (species-indexed
//! table + a synthesized playback song), not a shape any of the three types
//! here should be stretched to cover; it belongs to a later `#115` child.
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
//! # Versioning: [`crate::pack::FORMAT_VERSION`], and nothing else
//!
//! These payloads carry no version field of their own.
//! [`crate::pack::FORMAT_VERSION`] is this project's single staleness gate,
//! and it already versions entry *content*, not just the outer directory's
//! shape: it went to `2` when NPC sprite-sheet and palette entries were
//! added (issue #161), precisely so a pack missing new entry content is
//! rejected at load rather than failing later with a confusing per-asset
//! error. Audio entries do not exist in the pack yet; the `#115` child that
//! first writes them bumps `FORMAT_VERSION` to `3`, and any later change to
//! the shapes here bumps it again.
//!
//! A second, per-payload version would be a redundant axis: two gates to
//! keep in sync, and a failure mode (outer version accepted, inner version
//! rejected) that reports staleness at asset-load time instead of at pack
//! load, which is the opposite of what the `FORMAT_VERSION` gate exists to
//! do. It would also make these three the only typed decode layers in the
//! crate that are self-describing — `map_layouts`, `metatile_attributes`,
//! and `fonts` all read [`crate::pack::EntryKind::Raw`] payloads with no
//! embedded version. The pack is a local build artifact, never shipped and
//! always regenerable with `cargo xtask extract`, so "regenerate the pack"
//! is the only remedy any version mismatch has ever had.
//!
//! # Deliberately deferred
//!
//! [`song::SongEvent`] covers the note/timing/controller commands songs
//! actually carry (`(behavioral-fidelity)`), including the pseudo-echo
//! pair — see [`song::SongEvent::PseudoEchoVolume`]. What it does not cover,
//! and why:
//!
//! - `PATT`/`PEND`: `tools/mid2agb`'s loop compression — a repeated block
//!   emitted once and "called" from each occurrence. Expanding the pattern
//!   inline into the event stream reproduces every audible note, so this is
//!   an on-disk size representation, not musical content.
//! - `REPT` and `PORT`: defined by the sound engine (`sound/MPlayDef.s`,
//!   `PORT` = portamento) but never emitted by `tools/mid2agb` — no
//!   `.mid`-sourced song can contain one. A ROM backend decompiling
//!   hand-written sequence data is the only way either could appear.
//! - `MEMACC` is *not* deferred (review finding on #193): exactly one
//!   canonical song carries it (`mus_vs_trainer`'s conditional jump), but a
//!   shared pack contract that cannot represent a canonical song would
//!   force that backend to drop the branch or lower it to an unconditional
//!   [`song::SongEvent::Goto`] — either audibly changes the trainer theme.
//!   [`song::SongEvent::MemAcc`]/[`song::SongEvent::MemAccBranch`] model
//!   the full `mem_*` op table (`sound/MPlayDef.s:410-427`).
//! - `XCMD` sub-commands other than `xIECV`/`xIECL` (ext cmds `8`/`9`):
//!   `mid2agb`'s `PrintExtendedOp` handles only those two and drops the
//!   rest (`// TODO: support for other extended commands`), so no other
//!   `XCMD` reaches a compiled song.
//!
//! A later slice can add variants for any of these; that is an additive
//! change to [`song::SongEvent`]'s tag space (the tag byte is the
//! discriminator, and unknown tags are already rejected with
//! [`AudioError::UnknownSongEvent`] rather than silently misread), gated
//! like every other content change by a [`crate::pack::FORMAT_VERSION`]
//! bump.

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
