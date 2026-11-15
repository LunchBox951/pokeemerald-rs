//! Live CGB PSG voice playback, envelopes, and stereo routing.

use crate::cgb_envelope::{CgbAdsr, CgbEnvelope, HardwareEnvelopeVolume};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::gate::Gate;
use crate::psg::{FrameSequencer, LengthCounter, NoiseChannel, SquareChannel, WaveChannel};
use crate::voice::StereoAcc;
use std::sync::Arc;

mod length;
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

const NR32_THREE_QUARTER_BIT: u8 = 0x80;
const NR32_VOLUME_SHIFT: u32 = 5;
const NR32_VOLUME_MASK: u8 = 0b11;

// The NR32 byte `CgbSound` writes for each wave envelope level (`m4a.c:1211`).
// Levels 0-15 are `gCgb3Vol` (`m4a_tables.c:168-174`). Upstream indexes it
// unchecked and centred goals reach 31 (`CgbModVol`), so levels 16-31 read the
// bytes after it in `m4a_tables.o` `.rodata`: `gClockTable` (`m4a_tables.c:177`),
// whose first sixteen bytes are `0x00..=0x0F`. The built ROM confirms the layout:
// `00 00 60 60 60 60 40 40 40 40 80 80 80 80 20 20 | 00 01 02 .. 0F 10 11`.
#[rustfmt::skip]
const NR32_BY_LEVEL: [u8; 32] = [
    0x00, 0x00,
    0x60, 0x60, 0x60, 0x60,
    0x40, 0x40, 0x40, 0x40,
    0x80, 0x80, 0x80, 0x80,
    0x20, 0x20,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
];

/// Wave gain an NR32 byte selects: bit 7 forces three-quarter volume, else
/// bits 5-6 pick mute, full, half, or quarter.
fn nr32_wave_gain_256(nr32: u8) -> u32 {
    if nr32 & NR32_THREE_QUARTER_BIT != 0 {
        return THREE_QUARTER_GAIN_256;
    }
    match (nr32 >> NR32_VOLUME_SHIFT) & NR32_VOLUME_MASK {
        0 => SILENT_GAIN_256,
        1 => FULL_GAIN_256,
        2 => HALF_GAIN_256,
        _ => QUARTER_GAIN_256,
    }
}

// Envelope levels top out at 31 (`cgb_envelope_goal`), the last table entry.
fn cgb3_wave_gain_256(envelope_volume: u8) -> u32 {
    let index = usize::from(envelope_volume).min(NR32_BY_LEVEL.len() - 1);
    nr32_wave_gain_256(NR32_BY_LEVEL[index])
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag mirrors an independent hardware channel bit: pending retrigger, \
              hardware mute, sweep hold, and length stop"
)]
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
    /// A failed sweep check stopped the channel; a later safe trigger revives it
    /// (`mgba/src/gb/audio.c:180-186,667-672`).
    hardware_muted: bool,
    /// A sweep tick overflowed this playing square: the channel stops running
    /// but keeps mixing the level it last rendered until the next trigger or
    /// register-write catch-up (`mgba/src/gb/audio.c:667-672,745-754`).
    sweep_held: bool,
    hardware_envelope_volume: HardwareEnvelopeVolume,
    /// The instrument's raw `ToneData::length`; nonzero sets `NRx4` bit 6.
    length_register: u8,
    /// The hardware sound-length counter and enable bit this channel holds.
    length: LengthCounter,
    /// The counter stopped the channel: it holds its last output, its LFSR and
    /// hardware envelope stand still, its duty position is caught up by the
    /// next register write, and the note stays allocated until an `NRx4`
    /// trigger plays it again (`mgba/src/gb/audio.c:493-510,678-707`).
    length_stopped: bool,
    /// The frame-sequencer step completed before this voice's next frame.
    sequencer_step: u8,
    /// The last sample contribution rendered, which a stopped channel holds.
    last_contribution: i32,
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

    /// The square channel `MPlayExtender` leaves triggered at frequency 0 with
    /// its DAC on before any note (`pokeemerald/src/m4a.c:268-273`). It is
    /// idle-slot hardware only: its duty index free-runs while the slot is
    /// empty, and the first note on the slot inherits it
    /// (`mgba/src/gb/audio.c:162-171,493-510`).
    ///
    /// # Panics
    ///
    /// Panics if `channel` is [`CgbChannelNumber::Wave`] or `Noise`.
    #[must_use]
    pub(crate) fn initialized_square_hardware(channel: CgbChannelNumber) -> Self {
        assert!(
            matches!(
                channel,
                CgbChannelNumber::Square1 | CgbChannelNumber::Square2
            ),
            "square hardware requires Square1 or Square2, got {channel:?}"
        );
        let mut hardware = Self::new(
            channel,
            Oscillator::Square(SquareChannel::new(0, 0, None)),
            CgbAdsr::flat(),
            DacCorrection::None,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        );
        hardware.initial_trigger = InitialTrigger::Committed;
        hardware
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
            sweep_held: false,
            hardware_envelope_volume,
            length_register: 0,
            length: match channel {
                CgbChannelNumber::Wave => LengthCounter::two_hundred_fifty_six_step(),
                _ => LengthCounter::sixty_four_step(),
            },
            length_stopped: false,
            sequencer_step: FrameSequencer::INITIAL_STEP,
            last_contribution: 0,
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
        // Channel 3 alone commits its pan only at a surviving boundary (`begin_frame`).
        if matches!(self.channel, CgbChannelNumber::Wave) {
            self.routing.carry_committed_route_from(other.routing);
        }
        self.carry_length_from(other);
        // Hold the predecessor's clocks for the note-on writes (`commit_initial_writes`).
        if let (Oscillator::Noise(noise), Oscillator::Noise(previous)) =
            (&mut self.oscillator, &other.oscillator)
        {
            noise.inherit_hardware_from(
                previous,
                other.hardware_envelope_volume.volume(),
                other.length_stopped,
            );
        }
    }

    /// Apply the off-write the slot receives the instant its voice idles.
    pub(crate) fn apply_hardware_off_write(&mut self) {
        self.settle_owed_noise_clocks();
        self.hardware_muted = !self.oscillator.apply_hardware_off_write();
        self.sweep_held = false;
        self.off_write_length();
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

    /// Whether the idle slot is a wave channel whose held sample is nonzero
    /// and still routed to an output side, so every idle frame keeps mixing it
    /// (`render_idle_hardware`'s doc). Unlike a noise latch, nothing the idle
    /// slot does ever settles it: only a retrigger or a route write ends it
    /// (`mgba/src/gb/audio.c:246-253,399-403,768-774`).
    pub(crate) fn holds_wave_output(&self) -> bool {
        self.routing.is_routed()
            && self
                .oscillator
                .held_wave_contribution()
                .is_some_and(|contribution| contribution != 0)
    }

    /// Account one idle frame of a vacated slot: a noise channel keeps clocking
    /// and emits its latch (`mgba/src/gb/audio.c:606-641,782`), a square keeps
    /// its duty running, and a wave stops clocking but keeps emitting its held
    /// sample on the last committed route, since `NR30 = 0` clears neither the
    /// sample nor `NR51` (`mgba/src/gb/audio.c:246-253,768-774`).
    ///
    /// The wave's length counter keeps counting while `NR30` holds it off
    /// (`mgba/src/gb/audio.c:690-696`), so `length_ticks` still clock it.
    pub(crate) fn render_idle_hardware(
        &mut self,
        acc: &mut [StereoAcc],
        sweep_ticks: &[usize],
        length_ticks: &[usize],
    ) {
        if let Some(contribution) = self.oscillator.held_wave_contribution() {
            self.clock_length_silently(length_ticks.iter().copied());
            for output in acc.iter_mut() {
                self.routing.accumulate(contribution, output);
            }
        } else if self.oscillator.noise_mut().is_some() {
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
        self.sweep_held = false;
        self.trigger_length();
    }

    fn settle_owed_noise_clocks(&mut self) {
        // A stopped channel is not run before a register write
        // (`mgba/src/gb/audio.c:602-644`).
        if self.length_stopped {
            return;
        }
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
        if initial_trigger {
            self.load_length();
        }
        let live_goal = self.routing.envelope_goal();
        let (retriggered_by_transition, envelope_boundary) = self
            .envelope
            .step_frame(extra_envelope_iteration, live_goal);
        // A retiring frame skips the pitch write (`m4a.c:1053-1056,1194-1203`).
        // The level a pitch write's `NR14` catch-up latches into a stopped
        // square (length-expired or sweep-overflowed), before the high-byte
        // store (`mgba/src/gb/audio.c:162-169,219-224`).
        let mut stopped_write_latch = None;
        if let Some((note_key, fine_pitch)) = self.pending_pitch.take() {
            if self.envelope.is_active() {
                let latched = self
                    .oscillator
                    .retune(note_key, fine_pitch, self.dac_correction);
                if self.is_stopped_square() {
                    stopped_write_latch = latched;
                }
            }
        }
        // The note's first pitch block stores `NRx4` with the enable bit.
        if initial_trigger && self.envelope.is_active() {
            self.write_first_pitch_length();
        }
        // Pan is recomputed at every boundary, transition or not
        // (`m4a.c:1077-1085`).
        if envelope_boundary {
            self.routing.recompute_pan();
        }
        // A retiring frame jumps through `oscillator_off` to `channel_complete`,
        // past the `CGB_CHANNEL_MO_VOL` NR51 and NRx2/NRx4 writes
        // (`m4a.c:1053-1056,1084-1101,1205-1226`); only a note's first pass keeps
        // its note-on writes.
        let commits_hardware = self.envelope.is_active() || initial_trigger;
        // NR51 moves only under `CGB_CHANNEL_MO_VOL` (`m4a.c:1081-1082,1205-1208`).
        // A wave's retiring first pass reports a boundary yet jumps past the store
        // (`m4a.c:1053-1056`), leaving the carried predecessor route.
        let is_wave = matches!(self.channel, CgbChannelNumber::Wave);
        if commits_hardware
            && (!is_wave || self.envelope.is_active())
            && (retriggered_by_transition
                || retriggered_by_note_off
                || (is_wave && envelope_boundary))
        {
            self.routing.commit_pan();
        }
        let hardware_write =
            commits_hardware && (retriggered_by_note_off || retriggered_by_transition);
        if initial_trigger {
            self.oscillator
                .commit_initial_writes(!self.envelope.is_active());
        }
        // A square's retiring first pass reaches no trigger of its own; only
        // `CgbOscOff`'s `NR14` write follows (`m4a.c:1053-1056,1205-1226`).
        let skips_trigger = initial_trigger
            && !self.envelope.is_active()
            && matches!(self.oscillator, Oscillator::Square(_));
        if (hardware_write || initial_trigger) && !skips_trigger {
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
        // A stopped channel's envelope timer is not clocked
        // (`mgba/src/gb/audio.c:711-732`), though a store still latches.
        let stepped = if self.is_stopped_square() && !(hardware_write || initial_trigger) {
            false
        } else {
            self.hardware_envelope_volume.end_frame(
                1 + u8::from(extra_envelope_iteration),
                hardware_write,
                software_volume,
                self.envelope.hardware_envelope_pacing(),
            )
        };
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
        // A write's catch-up re-latches a stopped square at the duty position
        // it ran to before the high-byte store; nothing runs the stopped
        // channel after it (`mgba/src/gb/audio.c:162-171,219-224,493-501,916-918`).
        if let Some(level) = stopped_write_latch.filter(|_| self.is_stopped_square()) {
            if let Some(held) = self
                .oscillator
                .relatched_square_contribution(level, self.frame_gain)
            {
                self.last_contribution = held;
            }
        }
    }

    /// Whether the length counter or a sweep overflow stopped the channel, so
    /// that only a register write's catch-up runs it again.
    fn is_stopped_square(&self) -> bool {
        self.length_stopped || self.sweep_held
    }

    /// A square held at hardware level 0 skips its duty catch-up
    /// (`mgba/src/gb/audio.c:493-503,948-954`).
    fn is_dead_at_zero(&self) -> bool {
        matches!(self.oscillator, Oscillator::Square(_))
            && !self.sweep_held
            && self.envelope.is_active()
            && self.frame_gain == 0
            && self.hardware_envelope_volume.volume() == 0
            && !self
                .envelope
                .hardware_envelope_pacing()
                .is_some_and(|pacing| pacing.increasing)
    }

    /// Accumulate this voice into one frame after [`Self::begin_frame`], with
    /// the sweep clocks alone: no length clock reaches it.
    /// `sweep_ticks` holds ascending sample offsets of the 128 Hz sequencer.
    pub fn render(&mut self, acc: &mut [StereoAcc], sweep_ticks: &[usize]) {
        self.render_clocked(acc, sweep_ticks, &[]);
    }

    /// Accumulate this voice into one frame after [`Self::begin_frame`];
    /// `length_ticks` holds ascending sample offsets of the 256 Hz length clock.
    pub fn render_clocked(
        &mut self,
        acc: &mut [StereoAcc],
        sweep_ticks: &[usize],
        length_ticks: &[usize],
    ) {
        if !self.length_stopped {
            if self.is_dead_at_zero() {
                self.advance_idle_duty(acc.len(), sweep_ticks);
                self.last_contribution = 0;
                self.clock_length_silently(length_ticks.iter().copied());
                return;
            }
            if self.hardware_muted {
                self.oscillator.advance_silently(acc.len());
                if self.sweep_held {
                    self.accumulate_held_level(acc);
                } else {
                    self.last_contribution = 0;
                }
                self.clock_length_silently(length_ticks.iter().copied());
                return;
            }
        }
        let frame_len = acc.len();
        let mut ticks = sweep_ticks.iter().copied().peekable();
        let mut lengths = length_ticks.iter().copied().peekable();
        let mut held_from = None;
        for (sample_offset, output) in acc.iter_mut().enumerate() {
            if !self.envelope.is_active() {
                break;
            }
            // The frame event runs a playing square to its timestamp before the
            // sweep and length clocks, so a length expiry latches the duty level
            // as it stood ahead of a coincident sweep retune
            // (`mgba/src/gb/audio.c:657-691`).
            let expiry_latch = self.expiry_square_latch(lengths.peek() == Some(&sample_offset));
            if ticks.peek() == Some(&sample_offset) {
                ticks.next();
                // mGBA runs the channel up to the sequencer step before the
                // sweep can clear `playingCh1`, so an overflow holds the duty
                // level at this tick, read before the sweep's write-back
                // retunes it (`mgba/src/gb/audio.c:657,667-672`).
                // A length-stopped square keeps its catch-up deferred for the next
                // register write, so only a running channel is read here.
                let level_at_tick = (!self.hardware_muted && !self.length_stopped)
                    .then(|| match &mut self.oscillator {
                        Oscillator::Square(square) => Some(square.held_level()),
                        Oscillator::Wave { .. } | Oscillator::Noise(_) => None,
                    })
                    .flatten();
                if !self.hardware_muted && !self.oscillator.step_sweep_tick() {
                    self.hardware_muted = true;
                    if !self.length_stopped {
                        self.sweep_held = true;
                        if let Some(held) = level_at_tick.and_then(|level| {
                            self.oscillator
                                .relatched_square_contribution(level, self.frame_gain)
                        }) {
                            self.last_contribution = held;
                        }
                        self.oscillator.advance_silently(frame_len - sample_offset);
                        self.clock_length_silently(lengths);
                        held_from = Some(sample_offset);
                        break;
                    }
                }
            }
            while lengths.peek() == Some(&sample_offset) {
                lengths.next();
                self.clock_length();
            }
            if self.length_stopped {
                // The duty index does not step, but the time passes: the
                // next register write catches it up (`mgba/src/gb/audio.c:162-171,493-510`).
                // A wave presents the latch its last clock left
                // (`mgba/src/gb/audio.c:768-774`), which its one-sample-late
                // output has not reached yet.
                self.oscillator.defer_idle_samples(1);
                if let Some(held) = self.oscillator.held_wave_contribution().or(expiry_latch) {
                    self.last_contribution = held;
                }
            } else {
                self.last_contribution = self.oscillator.frame_contribution(
                    self.frame_gain,
                    self.hardware_envelope_volume.volume(),
                    self.envelope.volume(),
                );
            }
            self.routing.accumulate(self.last_contribution, output);
        }
        if let Some(sample_offset) = held_from {
            self.accumulate_held_level(&mut acc[sample_offset..]);
        }
    }

    /// Mix the level the channel last rendered into each sample of `acc`.
    fn accumulate_held_level(&self, acc: &mut [StereoAcc]) {
        for output in acc {
            self.routing.accumulate(self.last_contribution, output);
        }
    }
}

#[cfg(test)]
impl CgbVoice {
    /// Writes `freq_reg` through `NR13`/`NR14` to a square voice's hardware.
    pub(crate) fn with_square_frequency_register(mut self, freq_reg: u16) -> Self {
        if let Oscillator::Square(square) = &mut self.oscillator {
            square.set_frequency(freq_reg);
        }
        self
    }

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

    pub(crate) fn square_frequency(&self) -> Option<u16> {
        self.square_oscillator().map(SquareChannel::frequency)
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

#[cfg(test)]
#[path = "cgb_voice_length.rs"]
mod length_tests;
