//! One playing CGB PSG voice: an oscillator ([`crate::psg`]) shaped by a
//! [`CgbEnvelope`] and routed to the stereo accumulator, mirroring
//! `CgbSound`'s per-hardware-channel loop (`m4a.c:925`) and its
//! `CgbPan`/`CgbModVol` panning helpers (`m4a.c:878`..`:923`).

use crate::cgb_envelope::{cgb_envelope_goal, cgb_pan, CgbAdsr, CgbEnvelope, Panning};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};
use crate::voice::{channel_volume, StereoAcc};

/// Which of the four fixed CGB hardware channels a voice occupies.
///
/// Unlike DirectSound's pooled voices, each of these exists exactly once —
/// starting a new note on the same channel number retriggers whatever was
/// already sounding there, mirroring `CgbSound`'s `for (ch = 1; ch <= 4;
/// ch++)` loop over fixed channel slots (`m4a.c:946`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CgbChannelNumber {
    Square1,
    Square2,
    Wave,
    Noise,
}

impl CgbChannelNumber {
    /// Index into a 4-slot array (`0..=3`).
    #[must_use]
    pub fn slot(self) -> usize {
        match self {
            Self::Square1 => 0,
            Self::Square2 => 1,
            Self::Wave => 2,
            Self::Noise => 3,
        }
    }
}

/// The waveform generator backing a live [`CgbVoice`].
#[derive(Clone, Debug)]
enum Oscillator {
    Square(SquareChannel),
    Wave(WaveChannel),
    Noise(NoiseChannel),
}

/// Common amplitude normalisation so every oscillator kind contributes a
/// comparable pre-envelope magnitude, in the same rough `s8` range
/// [`crate::voice::Voice`]'s interpolated samples occupy. Square/noise
/// oscillators are unit bipolar (`crate::psg`'s `-1`/`1`); the wave
/// channel's decoded nibble range (`-8..=7`) is scaled up to match.
fn oscillator_sample(osc: &mut Oscillator) -> i32 {
    match osc {
        Oscillator::Square(s) => i32::from(s.sample()) * 127,
        Oscillator::Wave(w) => i32::from(w.sample()) * 16,
        Oscillator::Noise(n) => i32::from(n.sample()) * 127,
    }
}

/// Channel-3 output level for a `0..=15` envelope volume, returned as a
/// `0..=256` gain numerator (100% -> 256, so the render path's `>> 8` yields
/// the fractional level).
///
/// Behavioural port of `gCgb3Vol[envelopeVolume]` (`m4a_tables.c:168`), the
/// table the driver writes to `NR32` for channel 3 (`m4a.c:1211`). Unlike the
/// square/noise channels — whose `NRx2` envelope volume scales the DAC roughly
/// linearly, modelled by the `* 8` in [`CgbVoice::begin_frame`] — channel 3
/// has only five coarse output levels, so its envelope is *quantised* here.
/// The `gCgb3Vol` codes decode to these `NR32` output-level percentages (bits:
/// `0x00` mute, `0x20` 100%, `0x40` 50%, `0x60` 25%, `0x80` the GBA's extra
/// 75%): envelope volume `0,1 -> 0%`, `2..=5 -> 25%`, `6..=9 -> 50%`,
/// `10..=13 -> 75%`, `14,15 -> 100%`. A centred note's envelope goal can reach
/// 31 (`cgb_envelope_goal`), past the 16-entry table; those saturate at 100%
/// here, since `gCgb3Vol` has no entry there.
fn cgb3_wave_level(envelope_volume: u8) -> u32 {
    const LEVEL_256: [u32; 16] = [
        0, 0, 64, 64, 64, 64, 128, 128, 128, 128, 192, 192, 192, 192, 256, 256,
    ];
    LEVEL_256[(envelope_volume as usize).min(15)]
}

/// The `NR43`-style noise control byte for `note_key`, with the instrument's
/// width selector folded into bit 3 (`0x08`).
///
/// The `gNoiseTable` byte carries clock-shift and divisor bits but never the
/// width bit (`m4a_tables.c:149`); `CgbSound` supplies it separately from the
/// instrument via `*nrx3ptr = wavePointer << 3` (`m4a.c:1022`), whose low bit
/// is `voice_noise`'s `period & 1` (`music_voice.inc:105`). Reproduced here by
/// ORing that bit into the table byte.
fn noise_control_byte(note_key: u8, period: u8) -> u8 {
    midi_key_to_noise_control(note_key) | ((period & 1) << 3)
}

/// A live CGB PSG voice.
#[derive(Clone, Debug)]
pub struct CgbVoice {
    channel: CgbChannelNumber,
    oscillator: Oscillator,
    envelope: CgbEnvelope,
    adsr: CgbAdsr,
    base_right: u8,
    base_left: u8,
    velocity: u8,
    left_enabled: bool,
    right_enabled: bool,
    env_gain: i32,
    gate_time: u16,
    midi_key: u8,
    track: usize,
    /// Monotonic note-on ordinal, shared with the DirectSound
    /// [`crate::voice::Voice`]s so an end-of-tie can pick the newest match
    /// across both kinds (see that type's `seq` field). Stamped by the mixer.
    seq: u64,
}

impl CgbVoice {
    /// Start a square-channel (1 or 2) voice. `sweep_byte` is `Some` only for
    /// channel 1 (channel 2 has no hardware sweep register to drive).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn square(
        channel: CgbChannelNumber,
        duty: u8,
        sweep_byte: Option<u8>,
        adsr: CgbAdsr,
        note_key: u8,
        pit_m: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
    ) -> Self {
        let freq_reg = midi_key_to_cgb_freq_reg(note_key, pit_m);
        let sweep = sweep_byte.map(|b| crate::psg::Sweep::from_byte(b, freq_reg));
        let square = SquareChannel::new(duty, freq_reg, sweep);
        // Hardware runs the sweep overflow calc immediately at trigger, so an
        // upward sweep whose first step overflows disables the channel at
        // note-on before any tick (`mgba audio.c:184`). Mirror the tick-time
        // sweep-disable path (see [`Self::begin_frame`]): retire the envelope so
        // the voice is inactive from frame 0.
        let born_dead = square.is_disabled();
        let oscillator = Oscillator::Square(square);
        let mut voice = Self::new(
            channel, oscillator, adsr, vol_mr, vol_ml, velocity, gate_time, midi_key, track,
        );
        if born_dead {
            voice.envelope.retire();
        }
        voice
    }

    /// Start a programmable-wave (channel 3) voice from already-decoded
    /// samples (see [`crate::psg::WaveChannel::decode_wave_ram`]).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn wave(
        samples: [i8; 32],
        adsr: CgbAdsr,
        note_key: u8,
        pit_m: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
    ) -> Self {
        let freq_reg = midi_key_to_cgb_freq_reg(note_key, pit_m);
        let oscillator = Oscillator::Wave(WaveChannel::new(samples, freq_reg));
        Self::new(
            CgbChannelNumber::Wave,
            oscillator,
            adsr,
            vol_mr,
            vol_ml,
            velocity,
            gate_time,
            midi_key,
            track,
        )
    }

    /// Start a noise (channel 4) voice. Noise ignores fine pitch, matching
    /// `MidiKeyToCgbFreq`'s noise branch (`m4a.c:812`). `period` is the
    /// instrument's width selector (`ToneData` byte from `voice_noise`'s
    /// `period & 1`, `music_voice.inc:105`): its low bit becomes `NR43` bit 3
    /// (`0x08`), which `CgbSound` sets via `*nrx3ptr = wavePointer << 3`
    /// (`m4a.c:1022`) and which the `gNoiseTable` control byte never carries
    /// itself — so it alone selects the LFSR's narrow (7-bit) mode.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn noise(
        adsr: CgbAdsr,
        note_key: u8,
        period: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
    ) -> Self {
        let control = noise_control_byte(note_key, period);
        let oscillator = Oscillator::Noise(NoiseChannel::from_control_byte(control));
        Self::new(
            CgbChannelNumber::Noise,
            oscillator,
            adsr,
            vol_mr,
            vol_ml,
            velocity,
            gate_time,
            midi_key,
            track,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        channel: CgbChannelNumber,
        oscillator: Oscillator,
        adsr: CgbAdsr,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
    ) -> Self {
        let base_right = channel_volume(vol_mr, 0x80, velocity);
        let base_left = channel_volume(vol_ml, 0x7F, velocity);
        let panning = cgb_pan(base_right, base_left);
        let goal = cgb_envelope_goal(base_right, base_left, panning);
        Self {
            channel,
            oscillator,
            envelope: CgbEnvelope::new(adsr, goal),
            adsr,
            base_right,
            base_left,
            velocity,
            left_enabled: matches!(panning, Panning::Left | Panning::Both),
            right_enabled: matches!(panning, Panning::Right | Panning::Both),
            env_gain: 0,
            gate_time,
            midi_key,
            track,
            seq: 0,
        }
    }

    /// Stamp this voice's shared note-on ordinal (see [`Self::seq`]). Called by
    /// the mixer as it accepts the voice.
    pub(crate) fn set_seq(&mut self, seq: u64) {
        self.seq = seq;
    }

    /// This voice's shared note-on ordinal (higher is newer).
    #[must_use]
    pub(crate) fn seq(&self) -> u64 {
        self.seq
    }

    /// Which hardware channel slot this voice occupies.
    #[must_use]
    pub fn channel(&self) -> CgbChannelNumber {
        self.channel
    }

    /// The owning track index.
    #[must_use]
    pub fn track(&self) -> usize {
        self.track
    }

    /// The MIDI key this voice was started on.
    #[must_use]
    pub fn midi_key(&self) -> u8 {
        self.midi_key
    }

    /// Whether the voice is still producing sound.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.envelope.is_active()
    }

    /// Whether the note has already been released.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.envelope.is_stopping()
    }

    /// Tick the note-off gate down by one sequencer tick.
    pub fn tick_gate(&mut self) {
        if self.gate_time > 0 {
            self.gate_time -= 1;
            if self.gate_time == 0 {
                self.envelope.note_off();
            }
        }
    }

    /// Force note-off (tie termination / explicit stop).
    pub fn note_off(&mut self) {
        self.envelope.note_off();
    }

    /// Re-run `ChnVolSetAsm` + `CgbModVol` against this live channel from
    /// updated track `volMR`/`volML`: rewrites the base volumes, panning,
    /// and envelope goal (`MPT_FLG_VOLCHG`, `m4a_1.s:1394`..`:1401`).
    pub fn set_track_volume(&mut self, vol_mr: u8, vol_ml: u8) {
        self.base_right = channel_volume(vol_mr, 0x80, self.velocity);
        self.base_left = channel_volume(vol_ml, 0x7F, self.velocity);
        let panning = cgb_pan(self.base_right, self.base_left);
        self.left_enabled = matches!(panning, Panning::Left | Panning::Both);
        self.right_enabled = matches!(panning, Panning::Right | Panning::Both);
        let goal = cgb_envelope_goal(self.base_right, self.base_left, panning);
        self.envelope.set_goal(self.adsr, goal);
    }

    /// Recompute this live channel's playback frequency from updated track
    /// `keyM`/`pitM`, mirroring `MidiKeyToCgbFreq` re-runs in `MPlayMain`'s
    /// per-tick pitch pass (`m4a_1.s:1416`..`:1425`). Noise channels ignore
    /// `pit_m`, matching `MidiKeyToCgbFreq`'s noise branch.
    ///
    /// The noise retune only carries the table's clock/divisor bits; the width
    /// selector set at note-on is preserved by [`NoiseChannel::retune`],
    /// mirroring `CgbSound`'s `*nrx3ptr = (*nrx3ptr & 0x08) | frequency`
    /// (`m4a.c:1200`).
    pub fn set_track_pitch(&mut self, key_m: i32, pit_m: u8) {
        let note_key = u8::try_from((i32::from(self.midi_key) + key_m).max(0) & 0xFF).unwrap_or(0);
        match &mut self.oscillator {
            Oscillator::Square(s) => s.set_frequency(midi_key_to_cgb_freq_reg(note_key, pit_m)),
            Oscillator::Wave(w) => w.set_frequency(midi_key_to_cgb_freq_reg(note_key, pit_m)),
            Oscillator::Noise(n) => n.retune(midi_key_to_noise_control(note_key)),
        }
    }

    /// Advance the envelope (and channel-1 sweep) one frame and recompute
    /// the frame's gain.
    pub fn begin_frame(&mut self, master_volume: u8) {
        self.envelope.step();
        if let Oscillator::Square(s) = &mut self.oscillator {
            if !s.step_sweep_frame() {
                self.envelope.retire();
            }
        }
        // Scale the envelope's coarse level up to the same rough `0..=255`
        // range `Voice::begin_frame` mixes at, so a CGB channel's loudness is
        // comparable to a DirectSound one at the same nominal volume. The wave
        // channel takes its level from `gCgb3Vol`'s stepped quantisation
        // (`m4a.c:1211`) instead of the square/noise linear `* 8`, since
        // channel-3 amplitude comes solely from that table (see
        // [`cgb3_wave_level`]).
        let volume_255 = match &self.oscillator {
            Oscillator::Wave(_) => cgb3_wave_level(self.envelope.volume()),
            _ => u32::from(self.envelope.volume()) * 8,
        };
        let effective = ((u32::from(master_volume) + 1) * volume_255) >> 4;
        self.env_gain = i32::try_from(effective).unwrap_or(i32::MAX);
    }

    /// Render this voice's contribution across a frame, accumulating into
    /// `acc`. [`Self::begin_frame`] must have run for this frame first.
    pub fn render(&mut self, acc: &mut [StereoAcc]) {
        for slot in acc.iter_mut() {
            if !self.envelope.is_active() {
                break;
            }
            let raw = oscillator_sample(&mut self.oscillator);
            let contribution = (self.env_gain * raw) >> 8;
            if self.right_enabled {
                slot.1 += contribution;
            }
            if self.left_enabled {
                slot.0 += contribution;
            }
        }
    }
}

#[cfg(test)]
impl CgbVoice {
    /// Whether this voice's noise oscillator is in narrow (7-bit) mode, or
    /// `None` if it is not a noise voice.
    fn noise_is_narrow(&self) -> Option<bool> {
        match &self.oscillator {
            Oscillator::Noise(n) => Some(n.is_narrow()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_control_byte_sets_width_bit_only_for_odd_period() {
        // The `gNoiseTable` entry itself never carries the width bit
        // (`m4a_tables.c:149`); the instrument's `period` supplies it.
        assert_eq!(midi_key_to_noise_control(60) & 0x08, 0);
        assert_eq!(noise_control_byte(60, 1) & 0x08, 0x08);
        assert_eq!(noise_control_byte(60, 0) & 0x08, 0);
    }

    #[test]
    fn noise_period_bit_drives_narrow_lfsr_mode() {
        // A `period == 1` instrument must produce a narrow (periodic) noise
        // channel; `period == 0` stays wide. Before the fix `NoiseTone`
        // carried no period, so narrow mode was unreachable and every
        // instrument played wide 15-bit noise.
        let narrow = CgbVoice::noise(CgbAdsr::flat(), 60, 1, 0xFF, 0xFF, 127, 0, 60, 0);
        assert_eq!(narrow.noise_is_narrow(), Some(true));
        let wide = CgbVoice::noise(CgbAdsr::flat(), 60, 0, 0xFF, 0xFF, 127, 0, 60, 0);
        assert_eq!(wide.noise_is_narrow(), Some(false));
    }

    #[test]
    fn noise_retune_preserves_the_width_bit() {
        // `MidiKeyToCgbFreq`'s noise retune supplies only clock/divisor bits;
        // the width selector set at note-on survives (`m4a.c:1200`).
        let mut narrow = CgbVoice::noise(CgbAdsr::flat(), 60, 1, 0xFF, 0xFF, 127, 0, 60, 0);
        narrow.set_track_pitch(12, 0);
        assert_eq!(narrow.noise_is_narrow(), Some(true));
    }

    /// A full-swing wave table (nibbles alternating 0x0 and 0xF -> decoded
    /// -8 / 7), so any non-zero output level produces audible samples.
    fn full_swing_wave() -> [i8; 32] {
        WaveChannel::decode_wave_ram(&[0x0F; 16])
    }

    #[test]
    fn wave_note_with_active_envelope_is_audible() {
        // Channel-3 amplitude comes solely from the envelope via `gCgb3Vol`
        // (`m4a.c:1211`); with a live (flat, full-volume) envelope the note must
        // produce sound. Before the fix a loader-defaulted `volume_shift` of 0
        // muted the generator outright, silencing every wave note — this test
        // would fail under that bug.
        let mut voice = CgbVoice::wave(
            full_swing_wave(),
            CgbAdsr::flat(),
            60,
            0,
            0xFF,
            0xFF,
            127,
            0,
            60,
            0,
        );
        let mut acc = vec![(0i32, 0i32); 8];
        voice.begin_frame(15);
        voice.render(&mut acc);
        assert!(
            acc.iter().any(|&(l, r)| l != 0 || r != 0),
            "a live-envelope wave note must be audible"
        );
    }

    #[test]
    fn cgb3_wave_level_pins_the_stepped_output_levels() {
        // `gCgb3Vol` quantises the envelope to five NR32 levels
        // (`m4a_tables.c:168`): 0,1 -> mute; 2..=5 -> 25%; 6..=9 -> 50%;
        // 10..=13 -> 75%; 14,15 -> 100%. Returned as a 0..=256 numerator.
        assert_eq!(cgb3_wave_level(0), 0); // mute
        assert_eq!(cgb3_wave_level(1), 0); // mute
        assert_eq!(cgb3_wave_level(2), 64); // 25%
        assert_eq!(cgb3_wave_level(6), 128); // 50%
        assert_eq!(cgb3_wave_level(10), 192); // 75%
        assert_eq!(cgb3_wave_level(15), 256); // 100%
                                              // A centred note's goal can exceed 15; those saturate at 100%.
        assert_eq!(cgb3_wave_level(31), 256);
    }

    #[test]
    fn square1_upward_sweep_overflow_is_born_dead() {
        // A very high note (key 120 -> frequency register near 0x7FF) with an
        // upward sweep (shift 1) overflows the trigger-time overflow calc, so
        // the channel is inactive and silent from frame 0 — including with a
        // sweep period of 0 (`mgba audio.c:184`). A normal note is unaffected.
        for sweep_byte in [0b0000_0001u8, 0b0011_0001u8] {
            // period 0 and period 3, both add + shift 1
            let mut dead = CgbVoice::square(
                CgbChannelNumber::Square1,
                2,
                Some(sweep_byte),
                CgbAdsr::flat(),
                120,
                0,
                0xFF,
                0xFF,
                127,
                0,
                120,
                0,
            );
            assert!(!dead.is_active(), "overflowing sweep note is born dead");
            let mut acc = vec![(0i32, 0i32); 8];
            dead.begin_frame(15);
            dead.render(&mut acc);
            assert!(
                acc.iter().all(|&(l, r)| l == 0 && r == 0),
                "born-dead channel must be silent from frame 0"
            );
        }

        let normal = CgbVoice::square(
            CgbChannelNumber::Square1,
            2,
            Some(0b0000_0001), // period 0, add, shift 1
            CgbAdsr::flat(),
            48, // low note: frequency register well within 0x7FF
            0,
            0xFF,
            0xFF,
            127,
            0,
            48,
            0,
        );
        assert!(
            normal.is_active(),
            "a normal-frequency sweep note keeps playing"
        );
    }
}
