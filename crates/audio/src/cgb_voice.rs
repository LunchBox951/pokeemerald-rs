//! Live CGB PSG voice playback, envelopes, and stereo routing.

use crate::cgb_envelope::{CgbAdsr, CgbEnvelope, HardwareEnvelopeVolume};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};
use crate::voice::StereoAcc;

mod oscillator;
mod routing;

use self::{oscillator::Oscillator, routing::StereoRouting};

const BIPOLAR_SAMPLE_SCALE: i32 = 127;
const WAVE_SAMPLE_SCALE: i32 = 16;
const LINEAR_ENVELOPE_SCALE: u32 = 16;
const SAMPLE_GAIN_BITS: u32 = 8;
const MIDI_KEY_COUNT: i32 = 256;
const CGB_FREQUENCY_REGISTER_BITS: u32 = 11;
const EVEN_FREQUENCY_REGISTER_MASK: u16 = (1 << CGB_FREQUENCY_REGISTER_BITS) - 2;
const NOISE_WIDTH_BIT: u8 = 1 << 3;
const FULL_GAIN_256: u32 = 256;
const THREE_QUARTER_GAIN_256: u32 = FULL_GAIN_256 * 3 / 4;
const HALF_GAIN_256: u32 = FULL_GAIN_256 / 2;
const QUARTER_GAIN_256: u32 = FULL_GAIN_256 / 4;
const SILENT_GAIN_256: u32 = 0;

// `gCgb3Vol` maps M4A envelope levels to the GBA's five NR32 gains
// (`m4a_tables.c:168`, `m4a.c:1211`).
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
            // Emerald rounds fixed-rate square and wave registers before
            // initializing the oscillator and sweep shadow (`m4a.c:1184..1202`).
            Self::FixedRate8Bit => (frequency_register + 1) & EVEN_FREQUENCY_REGISTER_MASK,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Gate {
    Tied,
    TicksRemaining(u16),
    Expired,
}

impl Gate {
    fn new(gate_time: u16) -> Self {
        if gate_time == 0 {
            Self::Tied
        } else {
            Self::TicksRemaining(gate_time)
        }
    }

    fn tick(&mut self) -> bool {
        match *self {
            Self::TicksRemaining(1) => {
                *self = Self::Expired;
                true
            }
            Self::TicksRemaining(remaining) => {
                *self = Self::TicksRemaining(remaining - 1);
                false
            }
            Self::Tied | Self::Expired => false,
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
    /// A retrigger owed to the oscillator, applied at the next
    /// `begin_frame` after this tick's pitch writes (`m4a.c:1185-1226`).
    pending_retrigger: bool,
    /// Set when a sweep overflow silences the hardware channel, whether at a
    /// trigger or on a later 128 Hz tick; the envelope stays alive so a safe
    /// trigger can revive it (`mgba/src/gb/audio.c:180-186`, `:667-672`).
    hardware_muted: bool,
    /// [`HardwareEnvelopeVolume`]'s doc; unused by a Wave voice.
    hardware_envelope_volume: HardwareEnvelopeVolume,
}

impl CgbVoice {
    /// Start a square-channel voice without fixed-rate DAC correction.
    /// `channel` must be [`CgbChannelNumber::Square1`] or `Square2`;
    /// `sweep_byte` is silently dropped unless `channel` is `Square1`.
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
        // Only channel 1 has NR10 (`mgba/src/gb/audio.c:170-186`), so a
        // channel-2 sweep byte is dropped rather than trusted.
        let sweep = sweep_byte
            .filter(|_| channel == CgbChannelNumber::Square1)
            .map(|b| crate::psg::Sweep::from_byte(b, freq_reg));
        let oscillator = Oscillator::Square(SquareChannel::new(duty, freq_reg, sweep));
        let muted_at_trigger = oscillator.disabled_at_trigger();
        let mut voice = Self::new(
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
        );
        voice.hardware_muted = muted_at_trigger;
        voice
    }

    /// Start a programmable-wave voice from 32 decoded wave-RAM samples.
    /// `fixed_rate` applies the 8-bit DAC correction at note-on and retunes.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "a CGB voice starts from one decoded note and its instrument state"
    )]
    pub fn wave(
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
        let oscillator = Oscillator::Wave(WaveChannel::new(samples, freq_reg));
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

    /// Start a noise-channel voice. The selector's low bit chooses the narrow
    /// LFSR; noise ignores fine pitch and fixed-rate DAC correction.
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
        // Rhythm voices pitch from the child key but end ties by the played
        // track key (`ply_note`, `ply_endtie`, `m4a_1.s:1594,1819`).
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

    /// Carries the oscillator's duty phase forward from the voice this one
    /// replaces on a shared hardware slot.
    pub(crate) fn carry_duty_phase_from(&mut self, other: &Self) {
        self.oscillator.carry_duty_phase_from(&other.oscillator);
    }

    /// Applies the off-write a voice's slot receives the instant it idles
    /// ([`SquareChannel::apply_hardware_off_write`]'s doc); its trigger
    /// revives or re-mutes the hardware channel like any other.
    pub(crate) fn apply_hardware_off_write(&mut self) {
        self.hardware_muted = !self.oscillator.apply_hardware_off_write();
    }

    /// Accounts an idle square slot's `samples` of silence, whose duty
    /// position the free-running register catches up over
    /// (`mgba/src/gb/audio.c:493-510`); a no-op for Wave/Noise.
    /// The idle channel's sweep keeps ticking at the frame's `sweep_ticks`,
    /// retuning it as [`Self::render`] does, and a sweep overflow mutes the
    /// channel, freezing its frequency. The catch-up itself is deferred to
    /// the next note-on ([`SquareChannel::defer_idle_samples`]'s doc), so the
    /// silence is rated at the frequency the sweep finally reaches.
    pub(crate) fn advance_idle_duty(&mut self, samples: usize, sweep_ticks: &[usize]) {
        if !self.hardware_muted {
            for _ in sweep_ticks {
                if !self.oscillator.step_sweep_tick() {
                    self.hardware_muted = true;
                    break;
                }
            }
        }
        self.oscillator.defer_idle_samples(samples);
    }

    /// Return whether `ply_endtie` may select this voice
    /// ([`CgbEnvelope::is_end_tie_eligible`]'s doc).
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

    /// Request note-off immediately; may owe the oscillator a retrigger
    /// ([`CgbEnvelope::note_off`]'s doc).
    pub fn note_off(&mut self) {
        if self.envelope.note_off() {
            self.pending_retrigger = true;
        }
    }

    /// Applies [`Oscillator::retrigger`], muting the channel instead of
    /// retiring the voice when the trigger disables it (`m4a.c:1053-1056`).
    fn apply_retrigger(&mut self) {
        self.hardware_muted = !self.oscillator.retrigger();
    }

    /// Update the live base volume; itself a retrigger, matching upstream's
    /// live volume/pan write (`m4a_1.s:1391-1400`). The envelope goal folds
    /// in only at the next real boundary ([`CgbEnvelope::step_frame`]'s doc),
    /// not here; the stereo route similarly stays latched until
    /// [`Self::begin_frame`] commits it.
    pub fn set_track_volume(&mut self, vol_mr: u8, vol_ml: u8) {
        self.routing.update_volumes(vol_mr, vol_ml);
        self.pending_retrigger = true;
    }

    /// Retune from the owning track while preserving note identity and noise width.
    pub fn set_track_pitch(&mut self, key_m: i32, pit_m: u8) {
        let translated_key = (i32::from(self.identity.pitch_key) + key_m).max(0) % MIDI_KEY_COUNT;
        let note_key = u8::try_from(translated_key).unwrap_or(0);
        self.oscillator.retune(note_key, pit_m, self.dac_correction);
    }

    /// Advance the software envelope and prepare gain for one render
    /// frame; applies any owed retrigger first ([`Oscillator::retrigger`]'s doc).
    ///
    /// Never scales by the DirectSound master volume: `MPlayExtender` pins
    /// CGB output at full scale (`pokeemerald/src/m4a.c:267-275,365-373`).
    pub fn begin_frame(&mut self, extra_envelope_iteration: bool) {
        let retriggered_by_note_off = std::mem::take(&mut self.pending_retrigger);
        // The goal `CgbModVol` would compute right now from the current side
        // volumes/pan; folded in only at a real boundary (`step_frame`'s doc).
        let live_goal = self.routing.envelope_goal();
        let (retriggered_by_transition, envelope_boundary) = self
            .envelope
            .step_frame(extra_envelope_iteration, live_goal);
        // `CgbModVol` recomputes `chan->pan` at every boundary, transition or
        // not (`m4a.c:1077-1085`); by itself this writes nothing audible.
        if envelope_boundary {
            self.routing.recompute_pan();
        }
        // NR51 moves only under `CGB_CHANNEL_MO_VOL`: a transition, a live
        // write (using whatever pan was last recomputed), or, Wave only,
        // any bare boundary (`m4a.c:1081-1082`, `:1205-1208`).
        let is_wave = matches!(self.channel, CgbChannelNumber::Wave);
        if retriggered_by_transition || retriggered_by_note_off || (is_wave && envelope_boundary) {
            self.routing.commit_pan();
        }
        let hardware_write = retriggered_by_note_off || retriggered_by_transition;
        if hardware_write {
            self.apply_retrigger();
        }
        let software_volume = self.envelope.volume();
        self.hardware_envelope_volume.end_frame(
            1 + u8::from(extra_envelope_iteration),
            hardware_write,
            software_volume,
            self.envelope.hardware_envelope_pacing(),
        );
        let envelope_gain = self
            .oscillator
            .envelope_gain_256(software_volume, self.hardware_envelope_volume.volume());
        self.frame_gain = i32::try_from(envelope_gain).unwrap_or(i32::MAX);
    }

    /// Accumulate this voice into one frame after [`Self::begin_frame`].
    /// `sweep_ticks` must contain ascending sample offsets from the shared
    /// 128 Hz CGB frame sequencer.
    pub fn render(&mut self, acc: &mut [StereoAcc], sweep_ticks: &[usize]) {
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
            let raw_sample = self.oscillator.normalized_sample();
            let contribution = (self.frame_gain * raw_sample) >> SAMPLE_GAIN_BITS;
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
#[path = "cgb_voice_pitch.rs"]
mod pitch_tests;

#[cfg(test)]
#[path = "cgb_voice_routing.rs"]
mod routing_tests;

#[cfg(test)]
#[path = "cgb_voice_lifecycle.rs"]
mod lifecycle_tests;
