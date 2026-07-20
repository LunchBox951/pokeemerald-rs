//! A loaded song: its instrument voicegroup plus the decoded tracks the
//! sequencer plays.
//!
//! Behavioural model of `struct SongHeader` + its `ToneData` voicegroup
//! (`m4a_internal.h:57`, `:223`). Only the DirectSound (`type 0`) instrument
//! fields this slice uses — the wave and its ADSR — are modelled; key-split,
//! rhythm, CGB and fixed-frequency instrument kinds are out of scope.

use std::sync::Arc;

use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::sequence::Event;

/// One DirectSound instrument from a voicegroup.
#[derive(Clone, Debug)]
pub struct ToneData {
    /// The wave this instrument plays.
    pub wave: Arc<WaveData>,
    /// The instrument's attack/decay/sustain/release.
    pub adsr: Adsr,
}

impl ToneData {
    /// A DirectSound instrument wrapping `wave` with the given envelope.
    #[must_use]
    pub fn new(wave: Arc<WaveData>, adsr: Adsr) -> Self {
        Self { wave, adsr }
    }
}

/// A decoded, ready-to-play song.
#[derive(Clone, Debug)]
pub struct Song {
    /// Instruments, indexed by `VOICE` command.
    voices: Vec<ToneData>,
    /// Decoded event streams, one per track.
    tracks: Vec<Vec<Event>>,
    /// Initial tempo in BPM.
    initial_tempo: u16,
}

impl Song {
    /// Assemble a song from a voicegroup, decoded tracks, and an initial
    /// tempo (BPM).
    #[must_use]
    pub fn new(voices: Vec<ToneData>, tracks: Vec<Vec<Event>>, initial_tempo: u16) -> Self {
        Self {
            voices,
            tracks,
            initial_tempo,
        }
    }

    /// The instrument at `index`, if the voicegroup has one.
    #[must_use]
    pub fn voice(&self, index: usize) -> Option<&ToneData> {
        self.voices.get(index)
    }

    /// The decoded tracks.
    #[must_use]
    pub fn tracks(&self) -> &[Vec<Event>] {
        &self.tracks
    }

    /// Number of tracks.
    #[must_use]
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// The song's starting tempo in BPM.
    #[must_use]
    pub fn initial_tempo(&self) -> u16 {
        self.initial_tempo
    }
}
