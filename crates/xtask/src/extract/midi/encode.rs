//! Encodes [`CompiledSong`] values into the asset pack's song schema.
//!
//! `xtask` and `assets` intentionally remain decoupled, so schema changes must
//! be mirrored between their encoders. Strings are UTF-8 prefixed by their
//! `u16` little-endian byte length. Each track is prefixed by its `u32`
//! little-endian event count.

use super::compile::CompiledSong;
use super::error::MidiError;
use super::event::SongEvent;

#[derive(Clone, Copy)]
#[repr(u8)]
enum EventTag {
    Wait = 0,
    Note = 1,
    EndOfTie = 2,
    Voice = 3,
    Volume = 4,
    Pan = 5,
    Bend = 6,
    BendRange = 7,
    Tune = 8,
    KeyShift = 9,
    Tempo = 10,
    Priority = 11,
    LfoSpeed = 12,
    LfoDelay = 13,
    Modulation = 14,
    ModType = 15,
    Goto = 16,
    Fine = 17,
    PseudoEchoVolume = 18,
    PseudoEchoLength = 19,
}

impl EventTag {
    const fn byte(self) -> u8 {
        self as u8
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    let byte_len = u16::try_from(value.len())
        .expect("every pack id this pipeline generates fits in a u16 length");
    out.extend_from_slice(&byte_len.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn write_tagged_u8(out: &mut Vec<u8>, tag: EventTag, value: u8) {
    out.extend_from_slice(&[tag.byte(), value]);
}

fn write_tagged_i8(out: &mut Vec<u8>, tag: EventTag, value: i8) {
    out.extend_from_slice(&[tag.byte(), value.to_le_bytes()[0]]);
}

fn write_event(out: &mut Vec<u8>, event: &SongEvent) {
    match *event {
        SongEvent::Wait(ticks) => write_tagged_u8(out, EventTag::Wait, ticks),
        SongEvent::Note {
            key,
            velocity,
            gate,
        } => out.extend_from_slice(&[EventTag::Note.byte(), key, velocity, gate]),
        SongEvent::EndOfTie { key } => {
            out.extend_from_slice(&[EventTag::EndOfTie.byte(), u8::from(true), key]);
        }
        SongEvent::Voice(index) => write_tagged_u8(out, EventTag::Voice, index),
        SongEvent::Volume(volume) => write_tagged_u8(out, EventTag::Volume, volume),
        SongEvent::Pan(pan) => write_tagged_i8(out, EventTag::Pan, pan),
        SongEvent::Bend(bend) => write_tagged_i8(out, EventTag::Bend, bend),
        SongEvent::BendRange(range) => write_tagged_u8(out, EventTag::BendRange, range),
        SongEvent::Tune(tune) => write_tagged_i8(out, EventTag::Tune, tune),
        SongEvent::KeyShift(shift) => write_tagged_i8(out, EventTag::KeyShift, shift),
        SongEvent::Tempo(bpm) => {
            out.push(EventTag::Tempo.byte());
            out.extend_from_slice(&bpm.to_le_bytes());
        }
        SongEvent::Priority(priority) => write_tagged_u8(out, EventTag::Priority, priority),
        SongEvent::LfoSpeed(speed) => write_tagged_u8(out, EventTag::LfoSpeed, speed),
        SongEvent::LfoDelay(delay) => write_tagged_u8(out, EventTag::LfoDelay, delay),
        SongEvent::Modulation(depth) => write_tagged_u8(out, EventTag::Modulation, depth),
        SongEvent::ModType(kind) => write_tagged_u8(out, EventTag::ModType, kind),
        SongEvent::PseudoEchoVolume(volume) => {
            write_tagged_u8(out, EventTag::PseudoEchoVolume, volume);
        }
        SongEvent::PseudoEchoLength(length) => {
            write_tagged_u8(out, EventTag::PseudoEchoLength, length);
        }
        SongEvent::Goto(target) => {
            out.push(EventTag::Goto.byte());
            out.extend_from_slice(&target.to_le_bytes());
        }
        SongEvent::Fine => out.push(EventTag::Fine.byte()),
    }
}

fn write_track(out: &mut Vec<u8>, track: &[SongEvent]) {
    let event_count = u32::try_from(track.len())
        .expect("every compiled track this pipeline can hold fits in a u32 event count");
    out.extend_from_slice(&event_count.to_le_bytes());
    for event in track {
        write_event(out, event);
    }
}

/// Encodes a compiled song in the asset pack's song schema.
///
/// # Errors
///
/// Returns [`MidiError::TooManyTracks`] when the track count does not fit the
/// schema's `u8` field.
pub(super) fn encode_song(song: &CompiledSong) -> Result<Vec<u8>, MidiError> {
    let track_count =
        u8::try_from(song.tracks.len()).map_err(|_| MidiError::TooManyTracks(song.tracks.len()))?;
    let mut out = Vec::new();
    write_string(
        &mut out,
        &format!("audio/voicegroup/{}", song.voicegroup_label),
    );
    out.extend_from_slice(&[
        song.priority,
        u8::from(song.reverb.is_some()),
        song.reverb.unwrap_or(0),
        track_count,
    ]);
    for track in &song.tracks {
        write_track(&mut out, track);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
