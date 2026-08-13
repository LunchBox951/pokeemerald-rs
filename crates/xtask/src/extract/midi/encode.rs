//! Serializes a [`compile::CompiledSong`] to the exact binary shape
//! `crates/assets/src/audio/song.rs`'s `Song::encode`/`SongEvent::write`
//! produce — duplicated rather than shared, since this crate never depends
//! on `crates/assets` (`crate::extract::pack`'s module docs: the two crates
//! stay decoupled the same way the pack container format itself is
//! duplicated on both sides). Any change to that schema's wire shape must be
//! mirrored here by hand — mirrors `xtask::extract::voicegroups::encode`'s
//! own doc note verbatim.
//!
//! Wire shape (one song): `voicegroup_id: string`, `priority: u8`, a
//! reverb-presence `bool` plus `reverb: u8` (present but meaningless when
//! the flag is clear), `track_count: u8`, then per track: `event_count: u32`
//! followed by that many tagged events.
//!
//! `string` is a `u16` little-endian length prefix followed by that many
//! UTF-8 bytes; `bool` is a single `0`/`1` byte — both matching
//! `crates/assets/src/audio/cursor.rs`'s `Writer`.
//!
//! Event tag bytes mirror `song.rs`'s `TAG_*` constants exactly (`TAG_MEM_ACC`
//! = `20` and `TAG_MEM_ACC_BRANCH` = `21` are reserved but never written —
//! see `super::event`'s module docs):
//!
//! | tag | event | payload |
//! |---|---|---|
//! | 0 | `Wait` | `ticks: u8` |
//! | 1 | `Note` | `key: u8`, `velocity: u8`, `gate: u8` |
//! | 2 | `EndOfTie` | `has_key: bool` (always `true` here), `key: u8` |
//! | 3 | `Voice` | `index: u8` |
//! | 4 | `Volume` | `volume: u8` |
//! | 5 | `Pan` | `pan: i8` |
//! | 6 | `Bend` | `bend: i8` |
//! | 7 | `BendRange` | `range: u8` |
//! | 8 | `Tune` | `tune: i8` |
//! | 9 | `KeyShift` | `shift: i8` |
//! | 10 | `Tempo` | `bpm: u16` |
//! | 11 | `Priority` | `priority: u8` |
//! | 12 | `LfoSpeed` | `speed: u8` |
//! | 13 | `LfoDelay` | `delay: u8` |
//! | 14 | `Modulation` | `depth: u8` |
//! | 15 | `ModType` | `kind: u8` |
//! | 16 | `Goto` | `target: u32` |
//! | 17 | `Fine` | (none) |
//! | 18 | `PseudoEchoVolume` | `volume: u8` |
//! | 19 | `PseudoEchoLength` | `length: u8` |

use super::compile::CompiledSong;
use super::error::MidiError;
use super::event::SongEvent;

const TAG_WAIT: u8 = 0;
const TAG_NOTE: u8 = 1;
const TAG_END_OF_TIE: u8 = 2;
const TAG_VOICE: u8 = 3;
const TAG_VOLUME: u8 = 4;
const TAG_PAN: u8 = 5;
const TAG_BEND: u8 = 6;
const TAG_BEND_RANGE: u8 = 7;
const TAG_TUNE: u8 = 8;
const TAG_KEY_SHIFT: u8 = 9;
const TAG_TEMPO: u8 = 10;
const TAG_PRIORITY: u8 = 11;
const TAG_LFO_SPEED: u8 = 12;
const TAG_LFO_DELAY: u8 = 13;
const TAG_MODULATION: u8 = 14;
const TAG_MOD_TYPE: u8 = 15;
const TAG_GOTO: u8 = 16;
const TAG_FINE: u8 = 17;
const TAG_PSEUDO_ECHO_VOLUME: u8 = 18;
const TAG_PSEUDO_ECHO_LENGTH: u8 = 19;

fn write_string(out: &mut Vec<u8>, value: &str) {
    let len = u16::try_from(value.len())
        .expect("every pack id this pipeline generates is far shorter than u16::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn write_event(out: &mut Vec<u8>, event: &SongEvent) {
    match *event {
        SongEvent::Wait(ticks) => {
            out.push(TAG_WAIT);
            out.push(ticks);
        }
        SongEvent::Note {
            key,
            velocity,
            gate,
        } => {
            out.push(TAG_NOTE);
            out.push(key);
            out.push(velocity);
            out.push(gate);
        }
        SongEvent::EndOfTie { key } => {
            out.push(TAG_END_OF_TIE);
            out.push(1); // has_key -- always true, see the module docs
            out.push(key);
        }
        SongEvent::Voice(index) => {
            out.push(TAG_VOICE);
            out.push(index);
        }
        SongEvent::Volume(volume) => {
            out.push(TAG_VOLUME);
            out.push(volume);
        }
        SongEvent::Pan(pan) => {
            out.push(TAG_PAN);
            out.push(pan.to_ne_bytes()[0]);
        }
        SongEvent::Bend(bend) => {
            out.push(TAG_BEND);
            out.push(bend.to_ne_bytes()[0]);
        }
        SongEvent::BendRange(range) => {
            out.push(TAG_BEND_RANGE);
            out.push(range);
        }
        SongEvent::Tune(tune) => {
            out.push(TAG_TUNE);
            out.push(tune.to_ne_bytes()[0]);
        }
        SongEvent::KeyShift(shift) => {
            out.push(TAG_KEY_SHIFT);
            out.push(shift.to_ne_bytes()[0]);
        }
        SongEvent::Tempo(bpm) => {
            out.push(TAG_TEMPO);
            out.extend_from_slice(&bpm.to_le_bytes());
        }
        SongEvent::Priority(priority) => {
            out.push(TAG_PRIORITY);
            out.push(priority);
        }
        SongEvent::LfoSpeed(speed) => {
            out.push(TAG_LFO_SPEED);
            out.push(speed);
        }
        SongEvent::LfoDelay(delay) => {
            out.push(TAG_LFO_DELAY);
            out.push(delay);
        }
        SongEvent::Modulation(depth) => {
            out.push(TAG_MODULATION);
            out.push(depth);
        }
        SongEvent::ModType(kind) => {
            out.push(TAG_MOD_TYPE);
            out.push(kind);
        }
        SongEvent::PseudoEchoVolume(volume) => {
            out.push(TAG_PSEUDO_ECHO_VOLUME);
            out.push(volume);
        }
        SongEvent::PseudoEchoLength(length) => {
            out.push(TAG_PSEUDO_ECHO_LENGTH);
            out.push(length);
        }
        SongEvent::Goto(target) => {
            out.push(TAG_GOTO);
            out.extend_from_slice(&target.to_le_bytes());
        }
        SongEvent::Fine => out.push(TAG_FINE),
    }
}

/// Encode a [`CompiledSong`] to the wire shape documented above.
///
/// # Errors
///
/// [`MidiError::TooManyTracks`] if the song holds more tracks than the
/// `u8` track-count field can describe. Sixteen channels per `MTrk` chunk
/// keeps a single-chunk file well inside that, but a format-1 file with
/// sixteen or more note-carrying `MTrk` chunks can produce 256 playable
/// tracks once every channel in each chunk is playable — a bound on the
/// *wire format*, not on anything upstream rejects, so this returns an
/// error the caller can attach a path to rather than aborting the whole
/// extraction. Mirrors `crates/assets::audio::AudioError::TooManyTracks`,
/// which `Song::new` raises for the same reason on the read side.
///
/// # Panics
///
/// If a single track holds more than `u32::MAX` events. Unreachable by
/// construction: every event costs at least one byte of `.mid` input, so a
/// file that could do it would not fit in the address space this pipeline
/// reads it into.
pub(super) fn encode_song(song: &CompiledSong) -> Result<Vec<u8>, MidiError> {
    let track_count =
        u8::try_from(song.tracks.len()).map_err(|_| MidiError::TooManyTracks(song.tracks.len()))?;
    let mut out = Vec::new();
    write_string(
        &mut out,
        &format!("audio/voicegroup/{}", song.voicegroup_label),
    );
    out.push(song.priority);
    out.push(u8::from(song.reverb.is_some()));
    out.push(song.reverb.unwrap_or(0));
    out.push(track_count);
    for track in &song.tracks {
        let event_count = u32::try_from(track.len())
            .expect("a real compiled track has far fewer than u32::MAX events");
        out.extend_from_slice(&event_count.to_le_bytes());
        for event in track {
            write_event(&mut out, event);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
