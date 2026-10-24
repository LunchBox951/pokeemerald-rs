//! Live CGB PSG voice playback, envelopes, and stereo routing.

use crate::cgb_envelope::{CgbAdsr, CgbEnvelope, HardwareEnvelopeVolume};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::gate::Gate;
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};
use crate::voice::StereoAcc;
use std::sync::Arc;

mod oscillator;
mod routing;

use self::{oscillator::Oscillator, routing::StereoRouting};

// PSG output in DirectSound accumulator units (one s8 FIFO unit each).
//
// Emerald mixes the PSG at 100% (`SOUND_ALL_MIX_FULL`) with NR50 = 0x77
// (`pokeemerald/src/m4a.c:275,370-372`; `include/gba/io_reg.h:618-623`). mGBA sums
// the unipolar channel outputs, shifts them left 3, and multiplies by
// `1 + 7` (`mgba/src/gb/audio.c:778-793`), then shifts right by `4 - 2` for the
// full PSG mix (`mgba/src/gba/audio.c:362-366`): one PSG volume or nibble unit
// is `8 * 8 >> 2 = 16` hardware units. A full-volume FIFO s8 sample is
// `<< 2` with no halving (`mgba/src/gba/audio.c:368-384`): 4 hardware units. The mixer
// accumulates in FIFO units, so one PSG unit is `16 / 4 = 4`.
const PSG_HARDWARE_UNITS_PER_LEVEL: i32 = ((1 << 3) * (1 + 7)) >> (4 - 2);
const FIFO_HARDWARE_UNITS_PER_SAMPLE: i32 = 1 << 2;
const PSG_LEVEL_FIFO_UNITS: i32 = PSG_HARDWARE_UNITS_PER_LEVEL / FIFO_HARDWARE_UNITS_PER_SAMPLE;
const WAVE_SAMPLE_SCALE: i32 = PSG_LEVEL_FIFO_UNITS;
const LINEAR_ENVELOPE_SCALE: u32 = 16;
const SAMPLE_GAIN_BITS: u32 = 8;
// Square and noise are unipolar on hardware (`0..=volume`, `mgba/src/gb/audio.c:641,917`)
// but rendered about their midpoint as `+-volume`: one volume unit then spans
// half of `PSG_LEVEL_FIFO_UNITS` per side. `frame_gain` already carries
// `LINEAR_ENVELOPE_SCALE` per volume unit in Q8, so the scale below makes a
// full `+-1` sample contribute `+-volume * PSG_LEVEL_FIFO_UNITS / 2`.
const BIPOLAR_SAMPLE_SCALE: i32 =
    (PSG_LEVEL_FIFO_UNITS << SAMPLE_GAIN_BITS) / (2 * LINEAR_ENVELOPE_SCALE.cast_signed());
const MIDI_KEY_COUNT: i32 = 256;
const CGB_FREQUENCY_REGISTER_BITS: u32 = 11;
const EVEN_FREQUENCY_REGISTER_MASK: u16 = (1 << CGB_FREQUENCY_REGISTER_BITS) - 2;
const NOISE_WIDTH_BIT: u8 = 1 << 3;
const FULL_GAIN_256: u32 = 256;
const THREE_QUARTER_GAIN_256: u32 = FULL_GAIN_256 * 3 / 4;
const HALF_GAIN_256: u32 = FULL_GAIN_256 / 2;
const QUARTER_GAIN_256: u32 = FULL_GAIN_256 / 4;
const SILENT_GAIN_256: u32 = 0;

// `gCgb3Vol`: M4A envelope levels onto the five NR32 gains (`m4a_tables.c:168`).
#[rustfmt::skip]
const LEVEL_256: [u32; 16] = [
    SILENT_GAIN_256, SILENT_GAIN_256,
    QUARTER_GAIN_256, QUARTER_GAIN_256, QUARTER_GAIN_256, QUARTER_GAIN_256,
    HALF_GAIN_256, HALF_GAIN_256, HALF_GAIN_256, HALF_GAIN_256,
    THREE_QUARTER_GAIN_256, THREE_QUARTER_GAIN_256, THREE_QUARTER_GAIN_256, THREE_QUARTER_GAIN_256,
    FULL_GAIN_256, FULL_GAIN_256,
];

fn cgb3_wave_gain_256(envelope_volume: u8) -> u32 {
    LEVEL_256[usize::from(envelope_volume).min(LEVEL_256.len() - 1)]
}

/// A fixed CGB hardware channel owned by at most one live voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CgbChannelNumber {
    Square1,
    Square2,
    Wave,
    Noise,
}

impl CgbChannelNumber {
    /// Return this channel's fixed mixer slot.
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

fn noise_control_byte(note_key: u8, lfsr_width_selector: u8) -> u8 {
    let width_bit = (lfsr_width_selector & 1) * NOISE_WIDTH_BIT;
    midi_key_to_noise_control(note_key) | width_bit
}

/// Whether a voice's note-on trigger has reached the (modelled) hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitialTrigger {
    Pending,
    Committed,
    /// Note-off arrived first, so no trigger was ever written.
    Cancelled,
}

#[derive(Clone, Copy, Debug)]
enum DacCorrection {
    None,
    FixedRate8Bit,
}

impl DacCorrection {
    fn from_fixed_rate(fixed_rate: bool) -> Self {
        if fixed_rate {
            Self::FixedRate8Bit
        } else {
            Self::None
        }
    }

    fn apply(self, frequency_register: u16) -> u16 {
        match self {
            Self::None => frequency_register,
            Self::FixedRate8Bit => (frequency_register + 1) & EVEN_FREQUENCY_REGISTER_MASK,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct VoiceIdentity {
    track: usize,
    played_key: u8,
    pitch_key: u8,
    note_on_ordinal: u64,
    priority: u8,
}

impl VoiceIdentity {
    fn new(track: usize, played_key: u8) -> Self {
        Self {
            track,
            played_key,
            pitch_key: played_key,
            note_on_ordinal: 0,
            priority: 0,
        }
    }
}

/// A live CGB PSG voice.
#[derive(Clone, Debug)]
pub struct CgbVoice {
    channel: CgbChannelNumber,
    oscillator: Oscillator,
    envelope: CgbEnvelope,
    routing: StereoRouting,
    frame_gain: i32,
    gate: Gate,
    identity: VoiceIdentity,
    dac_correction: DacCorrection,
    pending_retrigger: bool,
    initial_trigger: InitialTrigger,
    /// The track's latest `(note_key, fine_pitch)`; a retiring frame discards it
    /// (`m4a.c:1053-1056,1194-1203`).
    pending_pitch: Option<(u8, u8)>,
    /// Sweep overflow silenced the channel; a later safe trigger revives it
    /// (`mgba/src/gb/audio.c:180-186,667-672`).
    hardware_muted: bool,
    hardware_envelope_volume: HardwareEnvelopeVolume,
}

impl CgbVoice {
    /// Start a square-channel voice without fixed-rate DAC correction.
    ///
    /// `sweep_byte` is dropped unless `channel` is `Square1`, the only channel
    /// with NR10 (`mgba/src/gb/audio.c:170-186`).
    ///
    /// # Panics
    ///
    /// Panics if `channel` is [`CgbChannelNumber::Wave`] or `Noise`.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "a CGB voice starts from one decoded note and its instrument state"
    )]
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
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        Self::square_with_fixed_rate(
            channel,
            duty,
            sweep_byte,
            adsr,
            false,
            note_key,
            pit_m,
            vol_mr,
            vol_ml,
            velocity,
            gate_time,
            midi_key,
            track,
            rhythm_pan,
            echo_volume,
            echo_length,
        )
    }

    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "a CGB voice starts from one decoded note and its instrument state"
    )]
    pub(crate) fn square_with_fixed_rate(
        channel: CgbChannelNumber,
        duty: u8,
        sweep_byte: Option<u8>,
        adsr: CgbAdsr,
        fixed_rate: bool,
        note_key: u8,
        pit_m: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        assert!(
            matches!(
                channel,
                CgbChannelNumber::Square1 | CgbChannelNumber::Square2
            ),
            "square voice requires Square1 or Square2, got {channel:?}"
        );
        let dac_correction = DacCorrection::from_fixed_rate(fixed_rate);
        let freq_reg = dac_correction.apply(midi_key_to_cgb_freq_reg(note_key, pit_m));
        let sweep = sweep_byte
            .filter(|_| channel == CgbChannelNumber::Square1)
            .map(|b| crate::psg::Sweep::from_byte(b, freq_reg));
        let oscillator = Oscillator::Square(SquareChannel::new(duty, freq_reg, sweep));
        Self::new(
            channel,
            oscillator,
            adsr,
            dac_correction,
            vol_mr,
            vol_ml,
            velocity,
            gate_time,
            midi_key,
            track,
            rhythm_pan,
            echo_volume,
            echo_length,
        )
    }

    /// Start a programmable-wave voice from 32 decoded wave-RAM samples.
    /// `fixed_rate` applies the 8-bit DAC correction at note-on and retunes.
    ///
    /// Equal `wave_id`s must name the same table; a voice replacing one with an
    /// equal ID keeps its wave-RAM rotation (`m4a.c:1003-1012`).
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "a CGB voice starts from one decoded note and its instrument state"
    )]
    pub fn wave(
        wave_id: Arc<str>,
        samples: [i8; 32],
        adsr: CgbAdsr,
        fixed_rate: bool,
        note_key: u8,
        pit_m: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        let dac_correction = DacCorrection::from_fixed_rate(fixed_rate);
        let freq_reg = dac_correction.apply(midi_key_to_cgb_freq_reg(note_key, pit_m));
        let oscillator = Oscillator::Wave {
            wave_id,
            channel: WaveChannel::new(samples, freq_reg),
        };
        Self::new(
            CgbChannelNumber::Wave,
            oscillator,
            adsr,
            dac_correction,
            vol_mr,
            vol_ml,
            velocity,
            gate_time,
            midi_key,
            track,
            rhythm_pan,
            echo_volume,
            echo_length,
        )
    }

    /// Start a noise-channel voice; the selector's low bit chooses the narrow
    /// LFSR.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "a CGB voice starts from one decoded note and its instrument state"
    )]
    pub fn noise(
        adsr: CgbAdsr,
        note_key: u8,
        lfsr_width_selector: u8,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        let control = noise_control_byte(note_key, lfsr_width_selector);
        let oscillator = Oscillator::Noise(NoiseChannel::from_control_byte(control));
        Self::new(
            CgbChannelNumber::Noise,
            oscillator,
            adsr,
            DacCorrection::None,
            vol_mr,
            vol_ml,
            velocity,
            gate_time,
            midi_key,
            track,
            rhythm_pan,
            echo_volume,
            echo_length,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "a CGB voice starts from one decoded note and its instrument state"
    )]
    fn new(
        channel: CgbChannelNumber,
        oscillator: Oscillator,
        adsr: CgbAdsr,
        dac_correction: DacCorrection,
        vol_mr: u8,
        vol_ml: u8,
        velocity: u8,
        gate_time: u16,
        midi_key: u8,
        track: usize,
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    ) -> Self {
        let routing = StereoRouting::new(vol_mr, vol_ml, velocity, rhythm_pan);
        let envelope = CgbEnvelope::new(adsr, routing.envelope_goal(), echo_volume, echo_length);
        let hardware_envelope_volume = HardwareEnvelopeVolume::at_note_on();
        Self {
            channel,
            oscillator,
            envelope,
            routing,
            frame_gain: 0,
            gate: Gate::new(gate_time),
            identity: VoiceIdentity::new(track, midi_key),
            dac_correction,
            pending_retrigger: false,
            initial_trigger: InitialTrigger::Pending,
            pending_pitch: None,
            hardware_muted: false,
            hardware_envelope_volume,
        }
    }

    #[must_use]
    pub(crate) fn with_priority(mut self, priority: u8) -> Self {
        self.identity.priority = priority;
        self
    }

    #[must_use]
    pub(crate) fn priority(&self) -> u8 {
        self.identity.priority
    }

    #[must_use]
    pub(crate) fn with_pitch_key(mut self, pitch_key: u8) -> Self {
        self.identity.pitch_key = pitch_key;
        self
    }

    pub(crate) fn set_seq(&mut self, seq: u64) {
        self.identity.note_on_ordinal = seq;
    }

    #[must_use]
    pub(crate) fn seq(&self) -> u64 {
        self.identity.note_on_ordinal
    }

    /// Return the fixed hardware channel this voice owns.
    #[must_use]
    pub fn channel(&self) -> CgbChannelNumber {
        self.channel
    }

    /// Return the owning track index.
    #[must_use]
    pub fn track(&self) -> usize {
        self.identity.track
    }

    /// Return the played MIDI key used for tie matching.
    #[must_use]
    pub fn midi_key(&self) -> u8 {
        self.identity.played_key
    }

    /// Return whether the voice can still produce sound.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.envelope.is_active()
    }

    /// Return whether note-off has been requested.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.envelope.is_stopping()
    }

    /// Inherit the oscillator's duty phase and noise latch from the voice this
    /// one replaces on the shared slot.
    pub(crate) fn carry_hardware_state_from(&mut self, other: &Self) {
        self.oscillator.carry_hardware_state_from(&other.oscillator);
        // Hold the predecessor's clocks for the note-on writes (`commit_initial_writes`).
        if let (Oscillator::Noise(noise), Oscillator::Noise(previous)) =
            (&mut self.oscillator, &other.oscillator)
        {
            noise.inherit_hardware_from(previous, other.hardware_envelope_volume.volume());
        }
    }

    /// Apply the off-write the slot receives the instant its voice idles.
    pub(crate) fn apply_hardware_off_write(&mut self) {
        self.settle_owed_noise_clocks();
        self.hardware_muted = !self.oscillator.apply_hardware_off_write();
        if self.oscillator.noise_mut().is_some() {
            self.hardware_envelope_volume.write_off();
        }
    }

    /// Whether the idle slot is a noise channel whose latch is yet to settle.
    pub(crate) fn has_unsettled_noise_output(&self) -> bool {
        match &self.oscillator {
            Oscillator::Noise(noise) => noise.has_unsettled_output(),
            _ => false,
        }
    }

    /// Account one idle frame of a vacated slot: a noise channel keeps clocking
    /// and emits its latch (`mgba/src/gb/audio.c:606-641,782`), a square keeps
    /// its duty running, and wave is unchanged.
    pub(crate) fn render_idle_hardware(&mut self, acc: &mut [StereoAcc], sweep_ticks: &[usize]) {
        if self.oscillator.noise_mut().is_some() {
            let volume = self.hardware_envelope_volume.volume();
            for output in acc.iter_mut() {
                let contribution = self.oscillator.frame_contribution(0, volume, 0);
                self.routing.accumulate(contribution, output);
            }
        } else {
            self.advance_idle_duty(acc.len(), sweep_ticks);
        }
    }

    /// Account an idle square slot's `samples` of silence while its sweep keeps
    /// ticking (`mgba/src/gb/audio.c:493-510`); a no-op for wave and noise.
    pub(crate) fn advance_idle_duty(&mut self, samples: usize, sweep_ticks: &[usize]) {
        self.oscillator.defer_idle_samples(samples);
        if !self.hardware_muted {
            for _ in sweep_ticks {
                if !self.oscillator.step_sweep_tick() {
                    self.hardware_muted = true;
                    break;
                }
            }
        }
    }

    /// Return whether `ply_endtie` may select this voice.
    #[must_use]
    pub(crate) fn is_end_tie_eligible(&self) -> bool {
        self.envelope.is_end_tie_eligible()
    }

    /// Advance the note-off gate by one sequencer tick.
    pub fn tick_gate(&mut self) {
        if self.gate.tick() && self.envelope.note_off() {
            self.pending_retrigger = true;
        }
    }

    /// Request note-off immediately.
    pub fn note_off(&mut self) {
        if self.envelope.note_off() {
            self.pending_retrigger = true;
        }
    }

    fn apply_retrigger(&mut self) {
        self.settle_owed_noise_clocks();
        self.hardware_muted = !self.oscillator.retrigger();
    }

    fn settle_owed_noise_clocks(&mut self) {
        let volume = self.hardware_envelope_volume.volume();
        if let Some(noise) = self.oscillator.noise_mut() {
            noise.settle_owed_clocks(volume);
        }
    }

    /// Update the live base volume and owe a retrigger, as upstream's live
    /// volume/pan write does (`m4a_1.s:1391-1400`).
    ///
    /// The new volumes reach the envelope goal and the stereo route only from
    /// the envelope's next boundary on (`m4a.c:903-923,1077-1085`).
    pub fn set_track_volume(&mut self, vol_mr: u8, vol_ml: u8) {
        self.routing.update_volumes(vol_mr, vol_ml);
        self.pending_retrigger = true;
    }

    /// Request a retune, held until [`Self::begin_frame`] commits it; a frame
    /// that retires the voice discards it (`m4a.c:1053-1056`). Noise ignores
    /// `pit_m` (`m4a.c:810-826`).
    pub fn set_track_pitch(&mut self, key_m: i32, pit_m: u8) {
        let translated_key = (i32::from(self.identity.pitch_key) + key_m).max(0) % MIDI_KEY_COUNT;
        let note_key = u8::try_from(translated_key).unwrap_or(0);
        self.pending_pitch = Some((note_key, pit_m));
    }

    pub(crate) fn initial_trigger_committed(&self) -> bool {
        self.initial_trigger == InitialTrigger::Committed
    }

    /// Advance the envelope and prepare gain for one render frame.
    ///
    /// Ignores the DirectSound master volume: `MPlayExtender` pins CGB output at
    /// full scale (`pokeemerald/src/m4a.c:267-275,365-373`).
    pub fn begin_frame(&mut self, extra_envelope_iteration: bool) {
        let retriggered_by_note_off = std::mem::take(&mut self.pending_retrigger);
        // A note stopped before its first pass never reaches the note-on
        // hardware writes (`m4a.c:1043-1056`).
        let initial_trigger =
            self.initial_trigger == InitialTrigger::Pending && !self.envelope.is_stopping();
        if self.initial_trigger == InitialTrigger::Pending {
            self.initial_trigger = if initial_trigger {
                InitialTrigger::Committed
            } else {
                InitialTrigger::Cancelled
            };
        }
        let live_goal = self.routing.envelope_goal();
        let (retriggered_by_transition, envelope_boundary) = self
            .envelope
            .step_frame(extra_envelope_iteration, live_goal);
        // A retiring frame skips the pitch write (`m4a.c:1053-1056,1194-1203`).
        if let Some((note_key, fine_pitch)) = self.pending_pitch.take() {
            if self.envelope.is_active() {
                self.oscillator
                    .retune(note_key, fine_pitch, self.dac_correction);
            }
        }
        // Pan is recomputed at every boundary, transition or not
        // (`m4a.c:1077-1085`).
        if envelope_boundary {
            self.routing.recompute_pan();
        }
        // NR51 moves only under `CGB_CHANNEL_MO_VOL` (`m4a.c:1081-1082,1205-1208`).
        let is_wave = matches!(self.channel, CgbChannelNumber::Wave);
        if retriggered_by_transition || retriggered_by_note_off || (is_wave && envelope_boundary) {
            self.routing.commit_pan();
        }
        let hardware_write = retriggered_by_note_off || retriggered_by_transition;
        if initial_trigger {
            self.oscillator.commit_initial_writes();
        }
        if hardware_write || initial_trigger {
            self.apply_retrigger();
        }
        let software_volume = self.envelope.volume();
        if hardware_write || initial_trigger {
            // A zero volume nibble with a clear direction bit disables the
            // channel (`mgba/src/gb/audio.c:856-860`).
            let runs = self.envelope.hardware_dac_enabled();
            if let Some(noise) = self.oscillator.noise_mut() {
                noise.set_clocking(runs);
            }
        }
        let stepped = self.hardware_envelope_volume.end_frame(
            1 + u8::from(extra_envelope_iteration),
            hardware_write,
            software_volume,
            self.envelope.hardware_envelope_pacing(),
        );
        if stepped {
            let volume = self.hardware_envelope_volume.volume();
            if let Some(noise) = self.oscillator.noise_mut() {
                noise.apply_envelope_step(volume);
            }
        }
        let envelope_gain = self
            .oscillator
            .envelope_gain_256(software_volume, self.hardware_envelope_volume.volume());
        self.frame_gain = i32::try_from(envelope_gain).unwrap_or(i32::MAX);
    }

    /// A square held at hardware level 0 skips its duty catch-up
    /// (`mgba/src/gb/audio.c:493-503,948-954`).
    fn is_dead_at_zero(&self) -> bool {
        matches!(self.oscillator, Oscillator::Square(_))
            && self.envelope.is_active()
            && self.frame_gain == 0
            && self.hardware_envelope_volume.volume() == 0
            && !self
                .envelope
                .hardware_envelope_pacing()
                .is_some_and(|pacing| pacing.increasing)
    }

    /// Accumulate this voice into one frame after [`Self::begin_frame`].
    /// `sweep_ticks` holds ascending sample offsets of the 128 Hz sequencer.
    pub fn render(&mut self, acc: &mut [StereoAcc], sweep_ticks: &[usize]) {
        if self.is_dead_at_zero() {
            self.advance_idle_duty(acc.len(), sweep_ticks);
            return;
        }
        if self.hardware_muted {
            self.oscillator.advance_silently(acc.len());
            return;
        }
        let frame_len = acc.len();
        let mut ticks = sweep_ticks.iter().copied().peekable();
        for (sample_offset, output) in acc.iter_mut().enumerate() {
            if !self.envelope.is_active() {
                break;
            }
            if ticks.peek() == Some(&sample_offset) {
                ticks.next();
                if !self.oscillator.step_sweep_tick() {
                    self.hardware_muted = true;
                    self.oscillator.advance_silently(frame_len - sample_offset);
                    break;
                }
            }
            let contribution = self.oscillator.frame_contribution(
                self.frame_gain,
                self.hardware_envelope_volume.volume(),
                self.envelope.volume(),
            );
            self.routing.accumulate(contribution, output);
        }
    }
}

#[cfg(test)]
impl CgbVoice {
    fn noise_is_narrow(&self) -> Option<bool> {
        match &self.oscillator {
            Oscillator::Noise(n) => Some(n.is_narrow()),
            _ => None,
        }
    }

    pub(crate) fn sweep_frequency(&self) -> Option<u16> {
        match &self.oscillator {
            Oscillator::Square(s) => s.sweep_frequency(),
            _ => None,
        }
    }

    fn noise_output_latch(&self) -> Option<u8> {
        match &self.oscillator {
            Oscillator::Noise(n) => Some(n.level()),
            _ => None,
        }
    }

    fn hardware_volume(&self) -> u8 {
        self.hardware_envelope_volume.volume()
    }

    fn noise_lfsr(&self) -> Option<u16> {
        match &self.oscillator {
            Oscillator::Noise(n) => Some(n.lfsr()),
            _ => None,
        }
    }

    pub(crate) fn envelope_volume(&self) -> u8 {
        self.envelope.volume()
    }

    fn square_oscillator(&self) -> Option<&SquareChannel> {
        match &self.oscillator {
            Oscillator::Square(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "cgb_voice_fixture.rs"]
mod fixture;

#[cfg(test)]
#[path = "cgb_voice_noise.rs"]
mod noise_tests;

#[cfg(test)]
#[path = "cgb_voice_wave.rs"]
mod wave_tests;

#[cfg(test)]
#[path = "cgb_voice_envelope.rs"]
mod envelope_tests;

#[cfg(test)]
#[path = "cgb_voice_sweep.rs"]
mod sweep_tests;

#[cfg(test)]
#[path = "cgb_voice_sweep_start.rs"]
mod sweep_start_tests;

#[cfg(test)]
#[path = "cgb_voice_pitch.rs"]
mod pitch_tests;

#[cfg(test)]
#[path = "cgb_voice_routing.rs"]
mod routing_tests;

#[cfg(test)]
#[path = "cgb_voice_lifecycle.rs"]
mod lifecycle_tests;
