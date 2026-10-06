//! Fixture shared by the player test seams.

use std::sync::Arc;

use audio::{Adsr, Event, Instrument, Song, ToneData, WaveData};

pub(super) fn short_song_without_its_own_reverb() -> Song {
    let wave = Arc::new(WaveData::one_shot(1 << 20, vec![100; 64]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let events = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 1,
        },
        Event::Wait(2),
        Event::Fine,
    ];
    Song::new(voices, vec![events], 150)
}
