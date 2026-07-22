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
        let oscillator = Oscillator::Square(SquareChannel::new(duty, freq_reg, sweep));
        Self::new(
            channel, oscillator, adsr, vol_mr, vol_ml, velocity, gate_time, midi_key, track,
        )
    }

    /// Start a programmable-wave (channel 3) voice from already-decoded
    /// samples (see [`crate::psg::WaveChannel::decode_wave_ram`]).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn wave(
        samples: [i8; 32],
        volume_shift: u8,
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
        let oscillator = Oscillator::Wave(WaveChannel::new(samples, volume_shift, freq_reg));
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
    /// `MidiKeyToCgbFreq`'s noise branch (`m4a.c:812`).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn noise(
        adsr: CgbAdsr,
        note_key: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
    ) -> Self {
        let control = midi_key_to_noise_control(note_key);
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
        }
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
        // Scale the envelope's coarse `0..=31` level up to the same rough
        // `0..=255` range `Voice::begin_frame` mixes at, so a CGB channel's
        // loudness is comparable to a DirectSound one at the same nominal
        // volume.
        let volume_255 = u32::from(self.envelope.volume()) * 8;
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
