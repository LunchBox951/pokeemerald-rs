//! A loaded song: its instrument voicegroup plus the decoded tracks the
//! sequencer plays.
//!
//! Behavioural model of `struct SongHeader` + its `ToneData` voicegroup
//! (`m4a_internal.h:57`, `:223`). [`Instrument`] mirrors `ToneData`'s `type`
//! field: a DirectSound sample, one of the four CGB PSG channel kinds
//! (`m4a.c:946`'s `ch` loop), or one of the two indirection kinds —
//! [`KeySplit`] (`TONEDATA_TYPE_SPL`) and [`Rhythm`] (`TONEDATA_TYPE_RHY`) —
//! that resolve to a concrete leaf instrument at note-on (`ply_note`,
//! `m4a_1.s:1580`..`:1609`).

use std::sync::Arc;

use crate::cgb_envelope::CgbAdsr;
use crate::envelope::Adsr;
use crate::sample::WaveData;
use crate::sequence::Event;

/// Every addressable key-split/rhythm slot spans exactly `0..=127`: both are
/// indexed directly by a raw command key byte, which the decoder guarantees
/// is always `< 0x80` ([`crate::sequence::decode_track`]'s argument-byte
/// convention).
pub const KEY_SLOTS: usize = 128;

/// The `pan_sweep` bit marking a rhythm child's own pan override
/// (`TONEDATA_P_S_PAN`, `m4a_internal.h:60`).
const PAN_SWEEP_OVERRIDE_BIT: u8 = 0x80;
/// The `pan_sweep` value subtracted before doubling to derive the rhythm pan
/// (`TONEDATA_P_S_PAN`, `m4a_internal.h:60`).
const PAN_SWEEP_BASE: i32 = 0xC0;

/// One DirectSound instrument from a voicegroup.
#[derive(Clone, Debug)]
pub struct ToneData {
    /// The wave this instrument plays.
    pub wave: Arc<WaveData>,
    /// The instrument's attack/decay/sustain/release.
    pub adsr: Adsr,
    /// `TONEDATA_TYPE_FIX`: play `wave` at its recorded rate, ignoring the
    /// played note's pitch entirely (see [`Self::fixed`]).
    fixed_rate: bool,
}

impl ToneData {
    /// A DirectSound instrument wrapping `wave` with the given envelope.
    #[must_use]
    pub fn new(wave: Arc<WaveData>, adsr: Adsr) -> Self {
        Self {
            wave,
            adsr,
            fixed_rate: false,
        }
    }

    /// Mark this instrument fixed-rate (`TONEDATA_TYPE_FIX`): the mixer plays
    /// `wave` at exactly one source sample per output sample, bypassing
    /// `MidiKeyToFreq`'s pitch scaling regardless of note key, `BEND`, or
    /// `KEYSH` (`m4a_1.s`'s `type & TONEDATA_TYPE_FIX` branch, `_081DD07C`..
    /// `_081DD134`; see [`crate::pitch::FRAC_ONE`]).
    #[must_use]
    pub fn fixed(mut self) -> Self {
        self.fixed_rate = true;
        self
    }

    /// Whether this instrument plays at a fixed, unscaled rate.
    #[must_use]
    pub fn is_fixed_rate(&self) -> bool {
        self.fixed_rate
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
    ///
    /// A programmable-wave instrument has no output-level field: upstream's
    /// `voice_programmable_wave` carries only key/pan/wave-pointer and the ADSR
    /// (`music_voice.inc`; `ToneData`, `m4a_internal.h:57`). Channel-3
    /// amplitude comes solely from the envelope (`m4a.c:1211`).
    pub table: [u8; 16],
    pub adsr: CgbAdsr,
}

/// A CGB noise instrument (hardware channel 4).
#[derive(Clone, Copy, Debug)]
pub struct NoiseTone {
    /// Width selector (`voice_noise`'s `period & 1`, `music_voice.inc:105`).
    /// Its low bit becomes `NR43` bit 3 (`0x08`), selecting the LFSR's narrow
    /// (7-bit periodic) mode; `0` leaves it in wide 15-bit mode. The
    /// `gNoiseTable` control bytes never set this bit themselves, so it comes
    /// only from the instrument (`m4a.c:1022`).
    pub period: u8,
    pub adsr: CgbAdsr,
}

/// One instrument from a voicegroup: a DirectSound sample, one of the four
/// CGB PSG channel kinds, or an indirection that resolves to one of those
/// leaf kinds at note-on. Selected uniformly by `VOICE` regardless of which
/// underlying kind it is (`ToneData::type`, `m4a_internal.h:59`).
#[derive(Clone, Debug)]
pub enum Instrument {
    DirectSound(ToneData),
    CgbSquare1(SquareTone),
    CgbSquare2(SquareTone),
    CgbWave(WaveTone),
    CgbNoise(NoiseTone),
    /// `TONEDATA_TYPE_SPL`: resolve a child leaf instrument via
    /// [`KeySplit::table`], keyed by the *played* note key. Pitch keeps using
    /// the played key — only the child sample/instrument changes at split
    /// boundaries (`ply_note`, `m4a_1.s:1586`..`:1589`, `:1598`).
    KeySplit(KeySplit),
    /// `TONEDATA_TYPE_RHY`: resolve a child directly by the played note key,
    /// substituting the child's own base key (and, when set, its own pan)
    /// for the played note's (`ply_note`, `m4a_1.s:1580`..`:1609`).
    Rhythm(Rhythm),
}

/// A key-split (`TONEDATA_TYPE_SPL`) indirection: `table[key]` selects which
/// of `children` plays, but pitch/pan resolution continues to use the played
/// note untouched (`ply_note`, `m4a_1.s:1589`, `:1598`).
///
/// A child that is itself a [`Instrument::KeySplit`] or [`Instrument::Rhythm`]
/// is unsupported (nested indirection): upstream aborts the note rather than
/// recursing (`_081DDB80`..`b _081DDCEA`, `m4a_1.s:1604`..`:1609`).
#[derive(Clone, Debug)]
pub struct KeySplit {
    /// `keySplitTable[key]` → index into [`Self::children`]
    /// (`o_MusicPlayerTrack_ToneData_keySplitTable`, `m4a_constants.inc:190`).
    /// Fixed at [`KEY_SLOTS`] entries: every raw command key (`0..=127`) is
    /// addressable, so there is never an unchecked read into an adjacent
    /// table.
    pub table: [u8; KEY_SLOTS],
    /// The concrete leaf instruments `table`'s entries index into.
    pub children: Vec<Instrument>,
}

/// A rhythm (`TONEDATA_TYPE_RHY`) indirection: the played note key indexes
/// directly into [`Self::children`] (no split table) — a drum-kit-style
/// mapping where each key triggers a distinct, independently-pitched hit
/// (`ply_note`, `m4a_1.s:1580`..`:1609`).
#[derive(Clone, Debug)]
pub struct Rhythm {
    /// One slot per raw command key (`0..=127`); `None` plays no note.
    /// Intended to hold exactly [`KEY_SLOTS`] entries so every addressable
    /// key — including the highest, `127` — is a deliberate, explicit choice
    /// by whoever builds the voicegroup rather than an unchecked read past
    /// the table (a shorter `Vec` simply resolves any higher key to `None`,
    /// same as an empty slot). A `Vec` rather than a fixed array so
    /// [`Instrument`] can embed [`Rhythm`] without an unconditional `Box`
    /// indirection on every leaf instrument.
    pub children: Vec<Option<RhythmChild>>,
}

/// One rhythm slot: a concrete leaf instrument plus the base key (and
/// optional pan override) that stand in for the played note's own
/// (`ply_note`, `m4a_1.s:1594`..`:1602`).
#[derive(Clone, Debug)]
pub struct RhythmChild {
    /// The concrete leaf instrument this key triggers.
    pub instrument: Instrument,
    /// The child's own base key (`ToneData::key`), substituted for the
    /// played key when resolving pitch (`m4a_1.s:1594`, `o_SoundChannel_type`
    /// aliasing `o_ToneData_key`'s offset).
    pub base_key: u8,
    /// Rhythm-pan override, already resolved from `pan_sweep`'s `0x80` bit —
    /// see [`rhythm_pan_from_pan_sweep`] — or `None` when the bit is unset
    /// (`m4a_1.s:1587`..`:1593`).
    pub pan: Option<i8>,
}

/// Resolve a rhythm child's pan override from its raw `ToneData::pan_sweep`
/// byte: `Some((pan_sweep - 0xC0) * 2)` when the `0x80` override bit is set,
/// `None` otherwise (`ply_note`, `m4a_1.s:1587`..`:1593`).
///
/// `pan_sweep` is `0x80..=0xFF` whenever the bit is set, so `pan_sweep -
/// 0xC0` always lands in `-64..=63`; doubled, `-128..=126` — always
/// representable in `i8` (upstream reaches the same values via a wrapping
/// byte subtract + shift; plain `i32` arithmetic reproduces the result
/// without that indirection `(no-verbatim)`).
#[must_use]
pub fn rhythm_pan_from_pan_sweep(pan_sweep: u8) -> Option<i8> {
    if pan_sweep & PAN_SWEEP_OVERRIDE_BIT == 0 {
        return None;
    }
    let doubled = (i32::from(pan_sweep) - PAN_SWEEP_BASE) * 2;
    Some(i8::try_from(doubled).unwrap_or(0))
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
    /// `SongHeader::reverb` (`0..=127`; `0` means no reverb) — see
    /// [`crate::reverb::Reverb`]. Set via [`Self::with_reverb`].
    reverb: u8,
}

impl Song {
    /// Assemble a song from a voicegroup, decoded tracks, and an initial
    /// tempo (BPM). Reverb defaults to `0` (off) — see [`Self::with_reverb`].
    #[must_use]
    pub fn new(voices: Vec<Instrument>, tracks: Vec<Vec<Event>>, initial_tempo: u16) -> Self {
        Self {
            voices,
            tracks,
            initial_tempo,
            reverb: 0,
        }
    }

    /// Set this song's master-mix reverb level (`SongHeader::reverb`,
    /// `0..=127`; `0` disables it — see [`crate::reverb::Reverb`]).
    /// Chainable onto [`Self::new`], mirroring [`ToneData::fixed`]'s builder
    /// shape.
    #[must_use]
    pub fn with_reverb(mut self, level: u8) -> Self {
        self.reverb = level.min(127);
        self
    }

    /// This song's master-mix reverb level (`0` means off).
    #[must_use]
    pub fn reverb(&self) -> u8 {
        self.reverb
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

#[cfg(test)]
mod tests {
    use super::rhythm_pan_from_pan_sweep;
    use super::Song;

    #[test]
    fn reverb_level_is_clamped_to_supported_range() {
        let song = || Song::new(Vec::new(), Vec::new(), 150);

        assert_eq!(song().with_reverb(127).reverb(), 127);
        assert_eq!(song().with_reverb(128).reverb(), 127);
    }

    #[test]
    fn rhythm_pan_pins_the_upstream_pan_sweep_mapping() {
        // ply_note (pokeemerald/src/m4a_1.s): only when pan_sweep's bit 7 is
        // set, rhythm pan = (pan_sweep - 0xC0) << 1. Pin the endpoints and
        // the unset-bit case so the formula can't drift.
        assert_eq!(rhythm_pan_from_pan_sweep(0x80), Some(-128));
        assert_eq!(rhythm_pan_from_pan_sweep(0xC0), Some(0));
        assert_eq!(rhythm_pan_from_pan_sweep(0xFF), Some(126));
        assert_eq!(rhythm_pan_from_pan_sweep(0x40), None);
    }
}
