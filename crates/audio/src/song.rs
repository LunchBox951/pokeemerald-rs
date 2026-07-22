//! A loaded song: its instrument voicegroup plus the decoded tracks the
//! sequencer plays.
//!
//! Behavioural model of `struct SongHeader` + its `ToneData` voicegroup
//! (`m4a_internal.h:57`, `:223`). [`Instrument`] mirrors `ToneData`'s `type`
//! field: a DirectSound sample, or one of the four CGB PSG channel kinds
//! (`m4a.c:946`'s `ch` loop). Key-split and rhythm instrument kinds remain
//! out of scope.

use std::sync::Arc;

use crate::cgb_envelope::CgbAdsr;
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

/// A CGB square-channel instrument (hardware channel 1 or 2).
#[derive(Clone, Copy, Debug)]
pub struct SquareTone {
    /// Duty cycle selector, `0..=3` (12.5%/25%/50%/75%).
    pub duty: u8,
    /// Raw `NR10`-style sweep byte. Only meaningful on channel 1 — channel 2
    /// has no hardware sweep register, so a square-2 instrument's sweep is
    /// simply never read.
    pub sweep: u8,
    pub adsr: CgbAdsr,
}

/// A CGB programmable-wave instrument (hardware channel 3).
#[derive(Clone, Debug)]
pub struct WaveTone {
    /// Packed wave RAM: 16 bytes, two 4-bit samples each
    /// (see [`crate::psg::WaveChannel::decode_wave_ram`]).
    pub table: [u8; 16],
    /// `NR32`-style output level code, `0..=3` (mute/100%/50%/25%).
    pub volume_shift: u8,
    pub adsr: CgbAdsr,
}

/// A CGB noise instrument (hardware channel 4).
#[derive(Clone, Copy, Debug)]
pub struct NoiseTone {
    pub adsr: CgbAdsr,
}

/// One instrument from a voicegroup: either a DirectSound sample or one of
/// the four CGB PSG channel kinds, selected uniformly by `VOICE` regardless
/// of which underlying kind it is (`ToneData::type`, `m4a_internal.h:59`).
#[derive(Clone, Debug)]
pub enum Instrument {
    DirectSound(ToneData),
    CgbSquare1(SquareTone),
    CgbSquare2(SquareTone),
    CgbWave(WaveTone),
    CgbNoise(NoiseTone),
}

/// A decoded, ready-to-play song.
#[derive(Clone, Debug)]
pub struct Song {
    /// Instruments, indexed by `VOICE` command.
    voices: Vec<Instrument>,
    /// Decoded event streams, one per track.
    tracks: Vec<Vec<Event>>,
    /// Initial tempo in BPM.
    initial_tempo: u16,
}

impl Song {
    /// Assemble a song from a voicegroup, decoded tracks, and an initial
    /// tempo (BPM).
    #[must_use]
    pub fn new(voices: Vec<Instrument>, tracks: Vec<Vec<Event>>, initial_tempo: u16) -> Self {
        Self {
            voices,
            tracks,
            initial_tempo,
        }
    }

    /// The instrument at `index`, if the voicegroup has one.
    #[must_use]
    pub fn voice(&self, index: usize) -> Option<&Instrument> {
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
