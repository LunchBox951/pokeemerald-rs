//! Typed instruments and decoded tracks ready for sequencing.

use std::sync::Arc;

use crate::cgb_envelope::CgbAdsr;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::sequence::{clamp_tempo, Event};

/// Number of MIDI keys addressable by key-split and rhythm instruments.
pub const KEY_SLOTS: usize = 128;

const RHYTHM_PAN_OVERRIDE_BIT: u8 = 0x80;
const RHYTHM_PAN_CENTER: u8 = 0xC0;
const MAX_REVERB_LEVEL: u8 = 127;

/// One DirectSound instrument from a voicegroup.
#[derive(Clone, Debug)]
pub struct ToneData {
    pub wave: Arc<WaveData>,
    pub adsr: Adsr,
    fixed_rate: bool,
}

impl ToneData {
    #[must_use]
    pub fn new(wave: Arc<WaveData>, adsr: Adsr) -> Self {
        Self {
            wave,
            adsr,
            fixed_rate: false,
        }
    }

    /// Plays one source sample per output sample regardless of key, bend and
    /// key shift (`m4a_1.s:498..521`).
    #[must_use]
    pub fn fixed(mut self) -> Self {
        self.fixed_rate = true;
        self
    }

    #[must_use]
    pub fn is_fixed_rate(&self) -> bool {
        self.fixed_rate
    }
}

/// A CGB square instrument (channel 1 or 2).
#[derive(Clone, Copy, Debug)]
pub struct SquareTone {
    /// Duty selector: 12.5%, 25%, 50% or 75%.
    pub duty: u8,
    /// Raw `NR10` sweep byte, ignored by square channel 2.
    pub sweep: u8,
    pub adsr: CgbAdsr,
    /// Uses Emerald's fixed-rate CGB tuning.
    pub fixed_rate: bool,
}

/// A CGB programmable-wave instrument (hardware channel 3).
#[derive(Clone, Debug)]
pub struct WaveTone {
    /// Equal IDs name the same immutable wave, so wave RAM reloads only when
    /// it changes.
    pub wave_id: Arc<str>,
    /// Two 4-bit samples per byte.
    pub table: [u8; 16],
    pub adsr: CgbAdsr,
    /// Uses Emerald's fixed-rate CGB tuning.
    pub fixed_rate: bool,
}

/// A CGB noise instrument (hardware channel 4).
#[derive(Clone, Copy, Debug)]
pub struct NoiseTone {
    /// A set low bit selects the 7-bit LFSR; otherwise the 15-bit LFSR.
    pub lfsr_width_selector: u8,
    pub adsr: CgbAdsr,
}

/// A playable voice or a key-based indirection to a playable voice.
#[derive(Clone, Debug)]
pub enum Instrument {
    DirectSound(ToneData),
    CgbSquare1(SquareTone),
    CgbSquare2(SquareTone),
    CgbWave(WaveTone),
    CgbNoise(NoiseTone),
    KeySplit(KeySplit),
    Rhythm(Rhythm),
}

/// Maps each played key to a child instrument; pitch and pan keep the played
/// key. Nested indirections produce no note (`m4a_1.s:1582..1609`).
#[derive(Clone, Debug)]
pub struct KeySplit {
    pub table: [u8; KEY_SLOTS],
    /// Indexed by [`Self::table`]; `None` slots produce no note.
    pub children: Vec<Option<Instrument>>,
}

/// Maps each played key directly to a rhythm child.
#[derive(Clone, Debug)]
pub struct Rhythm {
    /// Missing and out-of-range slots are silent.
    pub children: Vec<Option<RhythmChild>>,
}

/// A rhythm voice with its own pitch key and optional pan.
#[derive(Clone, Debug)]
pub struct RhythmChild {
    pub instrument: Instrument,
    /// Pitches the child instead of the played key.
    pub base_key: u8,
    /// A further channel-volume factor, not a replacement for track pan.
    pub pan: Option<i8>,
}

/// Decodes a voice's `pan_sweep` byte: the high bit enables
/// `(pan_sweep - 0xC0) * 2`, otherwise track pan applies (`m4a_1.s:1612..1618`).
#[must_use]
pub fn rhythm_pan_from_pan_sweep(pan_sweep: u8) -> Option<i8> {
    if pan_sweep & RHYTHM_PAN_OVERRIDE_BIT == 0 {
        return None;
    }
    let doubled = (i32::from(pan_sweep) - i32::from(RHYTHM_PAN_CENTER)) * 2;
    i8::try_from(doubled).ok()
}

/// A decoded, ready-to-play song.
#[derive(Clone, Debug)]
pub struct Song {
    voices: Vec<Instrument>,
    tracks: Vec<Vec<Event>>,
    initial_tempo_bpm: u16,
    priority: u8,
    reverb_override: Option<u8>,
}

impl Song {
    /// Creates a song with no reverb override; `initial_tempo` is clamped to
    /// [`crate::sequence::MAX_TEMPO_BPM`].
    #[must_use]
    pub fn new(voices: Vec<Instrument>, tracks: Vec<Vec<Event>>, initial_tempo: u16) -> Self {
        Self {
            voices,
            tracks,
            initial_tempo_bpm: clamp_tempo(initial_tempo),
            priority: 0,
            reverb_override: None,
        }
    }

    /// Sets the priority added to each track's note priority.
    #[must_use]
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    #[must_use]
    pub fn priority(&self) -> u8 {
        self.priority
    }

    /// Sets the master reverb level, clamped to `0..=127`.
    ///
    /// Zero disables reverb. An unset override renders as zero through
    /// [`crate::Sequencer::with_config`]; inheriting the session level needs
    /// [`crate::Sequencer::with_resolved_reverb`] (`m4a.c:658..662`).
    #[must_use]
    pub fn with_reverb(mut self, level: u8) -> Self {
        self.reverb_override = Some(level.min(MAX_REVERB_LEVEL));
        self
    }

    /// The override level, or zero when unset.
    #[must_use]
    pub fn reverb(&self) -> u8 {
        self.reverb_override.unwrap_or(0)
    }

    /// `None` leaves the level to the caller.
    #[must_use]
    pub fn reverb_override(&self) -> Option<u8> {
        self.reverb_override
    }

    #[must_use]
    pub fn voice(&self, index: usize) -> Option<&Instrument> {
        self.voices.get(index)
    }

    #[must_use]
    pub fn tracks(&self) -> &[Vec<Event>] {
        &self.tracks
    }

    #[must_use]
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    #[must_use]
    pub fn initial_tempo(&self) -> u16 {
        self.initial_tempo_bpm
    }
}

#[cfg(test)]
mod tests {
    use super::{
        rhythm_pan_from_pan_sweep, Song, MAX_REVERB_LEVEL, RHYTHM_PAN_CENTER,
        RHYTHM_PAN_OVERRIDE_BIT,
    };
    use crate::sequence::MAX_TEMPO_BPM;

    #[test]
    fn reverb_level_is_clamped_to_supported_range() {
        let song = || Song::new(Vec::new(), Vec::new(), 150);

        assert_eq!(song().with_reverb(MAX_REVERB_LEVEL).reverb(), 127);
        assert_eq!(song().with_reverb(MAX_REVERB_LEVEL + 1).reverb(), 127);
    }

    #[test]
    fn initial_tempo_is_clamped_to_the_tempo_command_domain() {
        assert_eq!(
            Song::new(Vec::new(), Vec::new(), MAX_TEMPO_BPM).initial_tempo(),
            MAX_TEMPO_BPM
        );
        assert_eq!(
            Song::new(Vec::new(), Vec::new(), MAX_TEMPO_BPM + 1).initial_tempo(),
            MAX_TEMPO_BPM
        );
        assert_eq!(
            Song::new(Vec::new(), Vec::new(), u16::MAX).initial_tempo(),
            MAX_TEMPO_BPM
        );
    }

    #[test]
    fn no_override_reverb_is_distinct_from_an_explicit_zero() {
        let inherited = Song::new(Vec::new(), Vec::new(), 150);
        assert_eq!(inherited.reverb_override(), None);
        assert_eq!(inherited.reverb(), 0);

        let disabled = Song::new(Vec::new(), Vec::new(), 150).with_reverb(0);
        assert_eq!(disabled.reverb_override(), Some(0));
        assert_eq!(disabled.reverb(), 0);

        let explicit = Song::new(Vec::new(), Vec::new(), 150).with_reverb(50);
        assert_eq!(explicit.reverb_override(), Some(50));
        assert_eq!(explicit.reverb(), 50);
    }

    #[test]
    fn rhythm_pan_maps_the_enabled_byte_domain() {
        assert_eq!(
            rhythm_pan_from_pan_sweep(RHYTHM_PAN_OVERRIDE_BIT),
            Some(-128)
        );
        assert_eq!(rhythm_pan_from_pan_sweep(RHYTHM_PAN_CENTER), Some(0));
        assert_eq!(rhythm_pan_from_pan_sweep(u8::MAX), Some(126));
        assert_eq!(rhythm_pan_from_pan_sweep(RHYTHM_PAN_OVERRIDE_BIT / 2), None);
    }
}
