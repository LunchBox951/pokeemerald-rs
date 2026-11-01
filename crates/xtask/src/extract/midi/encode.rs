//! Encodes [`CompiledSong`] values into the asset pack's song payload. The
//! pack container around this payload is shared (`pack_format` owns it).
//! The payload schema is owned by `assets`: each [`CompiledSong`] is mapped
//! onto [`assets::audio::Song`] and written by [`assets::audio::Song::encode`],
//! so the tags, field order, string framing, and wait canonicalization exist
//! in exactly one place and the runtime decoder validates what the extractor
//! writes.
//!
//! The event mapping is the only schema knowledge kept here. Building the
//! typed song re-runs `assets`' own validation, so a value outside the
//! schema's domain surfaces as an [`assets::AudioError`] rather than as a pack
//! the runtime would refuse to load.

use assets::audio::{MusicPlayerId, Song, SongEvent as AssetEvent, VoiceGroupId};
use assets::AudioError;

use super::compile::CompiledSong;
use super::event::SongEvent;

/// Prefix prepended to a `midi.cfg` voicegroup label to form its pack id
/// (`crate::extract`'s "Asset id scheme" docs).
const VOICEGROUP_PACK_ID_PREFIX: &str = "audio/voicegroup/";

/// Maps one compiled event onto the schema's event. Compiled ties always name
/// their key, so `EndOfTie` never uses the schema's "currently sounding key"
/// form.
pub(super) const fn asset_event(event: &SongEvent) -> AssetEvent {
    match *event {
        SongEvent::Wait(ticks) => AssetEvent::Wait(ticks),
        SongEvent::Note {
            key,
            velocity,
            gate,
        } => AssetEvent::Note {
            key,
            velocity,
            gate,
        },
        SongEvent::EndOfTie { key } => AssetEvent::EndOfTie { key: Some(key) },
        SongEvent::Voice(index) => AssetEvent::Voice(index),
        SongEvent::Volume(volume) => AssetEvent::Volume(volume),
        SongEvent::Pan(pan) => AssetEvent::Pan(pan),
        SongEvent::Bend(bend) => AssetEvent::Bend(bend),
        SongEvent::BendRange(range) => AssetEvent::BendRange(range),
        SongEvent::Tune(tune) => AssetEvent::Tune(tune),
        SongEvent::KeyShift(shift) => AssetEvent::KeyShift(shift),
        SongEvent::Tempo(bpm) => AssetEvent::Tempo(bpm),
        SongEvent::Priority(priority) => AssetEvent::Priority(priority),
        SongEvent::LfoSpeed(speed) => AssetEvent::LfoSpeed(speed),
        SongEvent::LfoDelay(delay) => AssetEvent::LfoDelay(delay),
        SongEvent::Modulation(depth) => AssetEvent::Modulation(depth),
        SongEvent::ModType(kind) => AssetEvent::ModType(kind),
        SongEvent::PseudoEchoVolume(volume) => AssetEvent::PseudoEchoVolume(volume),
        SongEvent::PseudoEchoLength(length) => AssetEvent::PseudoEchoLength(length),
        SongEvent::Goto(target) => AssetEvent::Goto(target),
        SongEvent::Fine => AssetEvent::Fine,
    }
}

/// Encodes a compiled song through `assets`' canonical song encoder, which
/// also canonicalizes each track's waits. `player` is the song's `gSongTable`
/// `ms` value.
///
/// # Errors
///
/// Returns the [`AudioError`] `assets` raises when the song is outside its
/// schema's domain: a voicegroup pack id too long for the `u16` string
/// length, more tracks than the `u8` track count holds, a track whose event
/// count exceeds a `u32`, or a jump that does not address its own track.
pub(super) fn encode_song(song: &CompiledSong, player: u16) -> Result<Vec<u8>, AudioError> {
    let tracks = song
        .tracks
        .iter()
        .map(|track| track.iter().map(asset_event).collect())
        .collect();
    Ok(Song::new(
        VoiceGroupId(format!(
            "{VOICEGROUP_PACK_ID_PREFIX}{}",
            song.voicegroup_label
        )),
        MusicPlayerId(player),
        song.priority,
        song.reverb,
        tracks,
    )?
    .encode())
}

#[cfg(test)]
mod tests;
