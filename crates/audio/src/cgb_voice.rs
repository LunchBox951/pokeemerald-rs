//! Live CGB PSG voice playback, envelopes, and stereo routing.

use crate::cgb_envelope::{
    cgb_envelope_goal, cgb_pan, CgbAdsr, CgbEnvelope, HardwareEnvelopePacing, Panning,
};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};
use crate::voice::{channel_volume, pan_terms, StereoAcc};

const BIPOLAR_SAMPLE_SCALE: i32 = 127;
const WAVE_SAMPLE_SCALE: i32 = 16;
const LINEAR_ENVELOPE_SCALE: u32 = 16;
const MASTER_VOLUME_BITS: u32 = 4;
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

#[derive(Clone, Debug)]
enum Oscillator {
    Square(SquareChannel),
    Wave(WaveChannel),
    Noise(NoiseChannel),
}

impl Oscillator {
    fn normalized_sample(&mut self) -> i32 {
        match self {
            Self::Square(square) => i32::from(square.sample()) * BIPOLAR_SAMPLE_SCALE,
            Self::Wave(wave) => i32::from(wave.sample()) * WAVE_SAMPLE_SCALE,
            Self::Noise(noise) => i32::from(noise.sample()) * BIPOLAR_SAMPLE_SCALE,
        }
    }

    /// `hardware_volume` is [`HardwareEnvelopeVolume`]'s resolved byte; the
    /// Wave arm ignores it and reads `envelope_volume` (0..=31) through its
    /// own `gCgb3Vol` lookup instead (`m4a.c:1211`), since NR32 has no such
    /// register.
    fn envelope_gain_256(&self, envelope_volume: u8, hardware_volume: u8) -> u32 {
        match self {
            Self::Wave(_) => cgb3_wave_gain_256(envelope_volume),
            Self::Square(_) | Self::Noise(_) => u32::from(hardware_volume) * LINEAR_ENVELOPE_SCALE,
        }
    }

    fn retune(&mut self, note_key: u8, fine_pitch: u8, correction: DacCorrection) {
        match self {
            Self::Square(square) => square
                .set_frequency(correction.apply(midi_key_to_cgb_freq_reg(note_key, fine_pitch))),
            Self::Wave(wave) => {
                wave.set_frequency(
                    correction.apply(midi_key_to_cgb_freq_reg(note_key, fine_pitch)),
                );
            }
            Self::Noise(noise) => noise.retune(midi_key_to_noise_control(note_key)),
        }
    }

    fn step_sweep_tick(&mut self) -> bool {
        match self {
            Self::Square(square) => square.step_sweep_tick(),
            Self::Wave(_) | Self::Noise(_) => true,
        }
    }

    fn disabled_at_trigger(&self) -> bool {
        matches!(self, Self::Square(square) if square.is_disabled())
    }

    /// Re-applies a `CGB_CHANNEL_MO_VOL` volume-write trigger (`m4a.c:1219-1226`)
    /// to the channel's own state and returns whether it still plays.
    fn retrigger(&mut self) -> bool {
        match self {
            Self::Square(square) => square.retrigger(),
            Self::Wave(_) => true,
            Self::Noise(noise) => {
                noise.retrigger();
                true
            }
        }
    }
}

/// The NRx2 volume nibble a live Square or Noise channel's hardware envelope
/// holds, and the hardware timer that paces it.
///
/// A `CGB_CHANNEL_MO_VOL` write truncates the software envelope into this
/// four-bit domain (`*nrx2ptr = (envelopeStepTimeAndDir & 0xF) +
/// (channels->envelopeVolume << 4)` through a `vu8 *`, `m4a.c:1219-1223`) and
/// then sets NRx4's trigger bit (`*nrx4ptr = channels->n4 | 0x80`,
/// `m4a.c:1222-1223`). That trigger reloads the hardware envelope timer from
/// the write's own step-time nibble as well as the level
/// (`_resetEnvelope`'s `currentVolume = initialVolume; nextStep = stepTime`,
/// `mgba/src/gb/audio.c:856-860`, reached from `GBAudioWriteNR14`'s restart
/// branch, `:180-181`), so the timer is the hardware's own and never the
/// software envelope's counter: a live VOL, pan, or tremolo write lands
/// partway through a phase without disturbing upstream's `envelopeCounter`
/// (`m4a_1.s:1394-1400`), and hardware must still wait a full step time
/// afterwards.
///
/// Between writes the timer steps the nibble by one in NRx2's direction and
/// reloads, going dead at 0 or 15 rather than wrapping past them
/// (`_updateEnvelope`, `mgba/src/gb/audio.c:931-945`). While
/// [`CgbEnvelope::hardware_envelope_pacing`] returns `None` hardware is dead
/// at the last write's level, regardless of how far the software envelope
/// drifts in the meantime (that method's doc).
#[derive(Clone, Copy, Debug)]
struct HardwareEnvelopeVolume {
    volume: u8,
    /// Envelope iterations until the next hardware step; zero is a dead
    /// envelope, which only a write revives (`envelope->dead`,
    /// `mgba/src/gb/audio.c:711-714`).
    iterations_until_step: u8,
}

impl HardwareEnvelopeVolume {
    const NIBBLE_MASK: u8 = 0x0F;

    /// Level upstream's attack starts a fresh voice from
    /// (`envelopeVolume = 0`, `m4a.c:1032`).
    const NOTE_ON_LEVEL: u8 = 0;

    /// Arm a fresh voice from its note-on write. That write is the attack's
    /// own NRx2 store ([`CgbEnvelope::attack_pacing`]'s doc), so it latches
    /// exactly as [`Self::write`] does — decoded step time, and dead on
    /// arrival when the note-on level is already saturated in `pacing`'s
    /// direction, as a downward write at level zero is.
    fn at_note_on(pacing: Option<HardwareEnvelopePacing>) -> Self {
        let mut hardware = Self {
            volume: Self::NOTE_ON_LEVEL,
            iterations_until_step: 0,
        };
        hardware.write(Self::NOTE_ON_LEVEL, pacing);
        if hardware.iterations_until_step != 0 {
            // Upstream stores NRx2 at the end of the note-on `CgbSound` pass
            // (`m4a.c:1206-1226`), after that pass's own envelope iteration,
            // which `begin_frame` has already run by the time it advances
            // this timer. The first step therefore owes one more iteration —
            // the same offset `transition_frame_delay` puts on the software
            // counter, so the two share one phase from the first frame.
            hardware.iterations_until_step += 1;
        }
        hardware
    }

    /// Latch a hardware write: NRx2 takes the software level's low nibble and
    /// NRx4's trigger restarts the timer from that write's step time (this
    /// type's doc). A level already saturated in `pacing`'s direction is dead
    /// on arrival (`_updateEnvelopeDead`, `mgba/src/gb/audio.c:948-954`).
    fn write(&mut self, software_volume: u8, pacing: Option<HardwareEnvelopePacing>) {
        self.volume = software_volume & Self::NIBBLE_MASK;
        self.iterations_until_step = match pacing {
            Some(pacing) if !self.is_saturated_toward(pacing) => pacing.step_time,
            _ => 0,
        };
    }

    /// Run the hardware timer for `iterations` envelope iterations since the
    /// last write, stepping the nibble whenever it expires (this type's doc).
    fn advance(&mut self, iterations: u8, pacing: Option<HardwareEnvelopePacing>) {
        let Some(pacing) = pacing else { return };
        for _ in 0..iterations {
            if self.iterations_until_step == 0 {
                return;
            }
            self.iterations_until_step -= 1;
            if self.iterations_until_step != 0 {
                continue;
            }
            if pacing.increasing {
                self.volume += 1;
            } else {
                self.volume -= 1;
            }
            if self.is_saturated_toward(pacing) {
                return;
            }
            self.iterations_until_step = pacing.step_time;
        }
    }

    /// Return the nibble the hardware volume register currently holds.
    fn volume(self) -> u8 {
        self.volume
    }

    fn is_saturated_toward(self, pacing: HardwareEnvelopePacing) -> bool {
        if pacing.increasing {
            self.volume >= Self::NIBBLE_MASK
        } else {
            self.volume == 0
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
struct StereoRouting {
    right: u8,
    left: u8,
    velocity: u8,
    rhythm_pan: i8,
    right_enabled: bool,
    left_enabled: bool,
}

impl StereoRouting {
    fn new(track_right: u8, track_left: u8, velocity: u8, rhythm_pan: i8) -> Self {
        let mut routing = Self {
            right: 0,
            left: 0,
            velocity,
            rhythm_pan,
            right_enabled: false,
            left_enabled: false,
        };
        routing.update_from_track(track_right, track_left);
        routing
    }

    fn update_from_track(&mut self, track_right: u8, track_left: u8) {
        let (pan_right, pan_left) = pan_terms(self.rhythm_pan);
        self.right = channel_volume(track_right, pan_right, self.velocity);
        self.left = channel_volume(track_left, pan_left, self.velocity);
        let panning = cgb_pan(self.right, self.left);
        self.right_enabled = matches!(panning, Panning::Right | Panning::Both);
        self.left_enabled = matches!(panning, Panning::Left | Panning::Both);
    }

    fn envelope_goal(self) -> u8 {
        cgb_envelope_goal(self.right, self.left, cgb_pan(self.right, self.left))
    }

    fn accumulate(self, contribution: i32, output: &mut StereoAcc) {
        if self.right_enabled {
            output.1 += contribution;
        }
        if self.left_enabled {
            output.0 += contribution;
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
    adsr: CgbAdsr,
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
    /// `sweep_byte` is valid only for channel 1.
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
        let dac_correction = DacCorrection::from_fixed_rate(fixed_rate);
        let freq_reg = dac_correction.apply(midi_key_to_cgb_freq_reg(note_key, pit_m));
        let sweep = sweep_byte.map(|b| crate::psg::Sweep::from_byte(b, freq_reg));
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
        let hardware_envelope_volume = HardwareEnvelopeVolume::at_note_on(envelope.attack_pacing());
        Self {
            channel,
            oscillator,
            envelope,
            adsr,
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

    /// Update base volume, panning, and envelope goal; itself a retrigger,
    /// matching upstream's live volume/pan write (`m4a_1.s:1391-1400`).
    pub fn set_track_volume(&mut self, vol_mr: u8, vol_ml: u8) {
        self.routing.update_from_track(vol_mr, vol_ml);
        self.envelope
            .set_goal(self.adsr, self.routing.envelope_goal());
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
    pub fn begin_frame(&mut self, master_volume: u8, extra_envelope_iteration: bool) {
        let retriggered_by_note_off = std::mem::take(&mut self.pending_retrigger);
        let retriggered_by_transition = self.envelope.step_frame(extra_envelope_iteration);
        let hardware_write = retriggered_by_note_off || retriggered_by_transition;
        if hardware_write {
            self.apply_retrigger();
        }
        let software_volume = self.envelope.volume();
        // Upstream applies at most one register write per `CgbSound` pass, at
        // its end (`m4a.c:1206-1226`), so a frame either relatches and
        // restarts the hardware timer or lets it run.
        let pacing = self.envelope.hardware_envelope_pacing();
        if hardware_write {
            self.hardware_envelope_volume.write(software_volume, pacing);
        } else {
            self.hardware_envelope_volume
                .advance(1 + u8::from(extra_envelope_iteration), pacing);
        }
        let envelope_gain = self
            .oscillator
            .envelope_gain_256(software_volume, self.hardware_envelope_volume.volume());
        let effective = ((u32::from(master_volume) + 1) * envelope_gain) >> MASTER_VOLUME_BITS;
        self.frame_gain = i32::try_from(effective).unwrap_or(i32::MAX);
    }

    /// Accumulate this voice into one frame after [`Self::begin_frame`].
    /// `sweep_ticks` must contain ascending sample offsets from the shared
    /// 128 Hz CGB frame sequencer.
    pub fn render(&mut self, acc: &mut [StereoAcc], sweep_ticks: &[usize]) {
        if self.hardware_muted {
            return;
        }
        let mut ticks = sweep_ticks.iter().copied().peekable();
        for (sample_offset, output) in acc.iter_mut().enumerate() {
            if !self.envelope.is_active() {
                break;
            }
            if ticks.peek() == Some(&sample_offset) {
                ticks.next();
                if !self.oscillator.step_sweep_tick() {
                    self.hardware_muted = true;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::psg::FrameSequencer128Hz;

    const TEST_KEY: u8 = 60;
    const FULL_TRACK_VOLUME: u8 = u8::MAX;
    const FULL_VELOCITY: u8 = 127;
    const MAX_MASTER_VOLUME: u8 = 15;
    const HALF_DUTY: u8 = 2;
    const NARROW_NOISE: u8 = 1;
    const WIDE_NOISE: u8 = 0;
    const SWEEP_PERIOD_SHIFT: u32 = 4;
    const FULL_SWING_WAVE_BYTE: u8 = 0x0F;

    #[derive(Clone, Copy)]
    struct TestNote {
        note_key: u8,
        fine_pitch: u8,
        track_right: u8,
        track_left: u8,
        velocity: u8,
        gate_time: u16,
        played_key: u8,
        track: usize,
        rhythm_pan: i8,
        echo_volume: u8,
        echo_length: u8,
    }

    impl TestNote {
        fn at_key(key: u8) -> Self {
            Self {
                note_key: key,
                played_key: key,
                ..Self::default()
            }
        }
    }

    impl Default for TestNote {
        fn default() -> Self {
            Self {
                note_key: TEST_KEY,
                fine_pitch: 0,
                track_right: FULL_TRACK_VOLUME,
                track_left: FULL_TRACK_VOLUME,
                velocity: FULL_VELOCITY,
                gate_time: 0,
                played_key: TEST_KEY,
                track: 0,
                rhythm_pan: 0,
                echo_volume: 0,
                echo_length: 0,
            }
        }
    }

    fn square_voice_with_adsr(
        channel: CgbChannelNumber,
        sweep: Option<u8>,
        adsr: CgbAdsr,
        note: TestNote,
    ) -> CgbVoice {
        CgbVoice::square(
            channel,
            HALF_DUTY,
            sweep,
            adsr,
            note.note_key,
            note.fine_pitch,
            note.track_right,
            note.track_left,
            note.velocity,
            note.gate_time,
            note.played_key,
            note.track,
            note.rhythm_pan,
            note.echo_volume,
            note.echo_length,
        )
    }

    fn square_voice(channel: CgbChannelNumber, sweep: Option<u8>, note: TestNote) -> CgbVoice {
        square_voice_with_adsr(channel, sweep, CgbAdsr::flat(), note)
    }

    fn fixed_square_voice(
        channel: CgbChannelNumber,
        sweep: Option<u8>,
        note: TestNote,
    ) -> CgbVoice {
        CgbVoice::square_with_fixed_rate(
            channel,
            HALF_DUTY,
            sweep,
            CgbAdsr::flat(),
            true,
            note.note_key,
            note.fine_pitch,
            note.track_right,
            note.track_left,
            note.velocity,
            note.gate_time,
            note.played_key,
            note.track,
            note.rhythm_pan,
            note.echo_volume,
            note.echo_length,
        )
    }

    fn wave_voice(fixed_rate: bool, note: TestNote) -> CgbVoice {
        CgbVoice::wave(
            full_swing_wave(),
            CgbAdsr::flat(),
            fixed_rate,
            note.note_key,
            note.fine_pitch,
            note.track_right,
            note.track_left,
            note.velocity,
            note.gate_time,
            note.played_key,
            note.track,
            note.rhythm_pan,
            note.echo_volume,
            note.echo_length,
        )
    }

    fn noise_voice(adsr: CgbAdsr, width_selector: u8, note: TestNote) -> CgbVoice {
        CgbVoice::noise(
            adsr,
            note.note_key,
            width_selector,
            note.track_right,
            note.track_left,
            note.velocity,
            note.gate_time,
            note.played_key,
            note.track,
            note.rhythm_pan,
            note.echo_volume,
            note.echo_length,
        )
    }

    fn upward_sweep(period_ticks: u8, shift: u8) -> u8 {
        (period_ticks << SWEEP_PERIOD_SHIFT) | shift
    }

    fn full_swing_wave() -> [i8; 32] {
        WaveChannel::decode_wave_ram(&[FULL_SWING_WAVE_BYTE; 16])
    }

    #[test]
    fn noise_control_byte_sets_width_bit_only_for_odd_period() {
        assert_eq!(midi_key_to_noise_control(TEST_KEY) & NOISE_WIDTH_BIT, 0);
        assert_eq!(
            noise_control_byte(TEST_KEY, NARROW_NOISE) & NOISE_WIDTH_BIT,
            NOISE_WIDTH_BIT
        );
        assert_eq!(
            noise_control_byte(TEST_KEY, WIDE_NOISE) & NOISE_WIDTH_BIT,
            0
        );
    }

    #[test]
    fn noise_period_bit_drives_narrow_lfsr_mode() {
        let narrow = noise_voice(CgbAdsr::flat(), NARROW_NOISE, TestNote::default());
        assert_eq!(narrow.noise_is_narrow(), Some(true));
        let wide = noise_voice(CgbAdsr::flat(), WIDE_NOISE, TestNote::default());
        assert_eq!(wide.noise_is_narrow(), Some(false));
    }

    #[test]
    fn noise_retune_preserves_the_width_bit() {
        let mut narrow = noise_voice(CgbAdsr::flat(), NARROW_NOISE, TestNote::default());
        let octave_up = 12;
        narrow.set_track_pitch(octave_up, 0);
        assert_eq!(narrow.noise_is_narrow(), Some(true));
    }

    #[test]
    fn release_start_volume_write_retriggers_the_noise_lfsr() {
        // Release start is a volume write; upstream's channel-4 trigger
        // resets the LFSR (`m4a.c:1060-1069,1219-1226`; `mgba/src/gb/audio.c:374`).
        let adsr = CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: MAX_MASTER_VOLUME,
            release: 4,
        };
        let mut voice = noise_voice(adsr, WIDE_NOISE, TestNote::default());
        let at_note_on = voice.noise_lfsr();

        let mut acc = vec![(0i32, 0i32); 64];
        for _ in 0..3 {
            voice.begin_frame(MAX_MASTER_VOLUME, false);
            voice.render(&mut acc, &[]);
        }
        let walked_away = voice.noise_lfsr();
        assert_ne!(
            walked_away, at_note_on,
            "sanity: a sustained note must have walked the LFSR away from note-on"
        );

        voice.note_off();
        assert_eq!(
            voice.noise_lfsr(),
            walked_away,
            "sanity: note_off must not retrigger before the next begin_frame -- upstream applies \
             a tick's writes inside the following CgbSound call, not synchronously"
        );
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        assert_eq!(
            voice.noise_lfsr(),
            at_note_on,
            "note_off's release-start volume write (release != 0) must retrigger channel 4 at \
             the next begin_frame, clearing the LFSR"
        );
    }

    #[test]
    fn wave_note_with_active_envelope_is_audible() {
        let mut voice = wave_voice(false, TestNote::default());
        let mut acc = vec![(0i32, 0i32); 8];
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        voice.render(&mut acc, &[]);
        assert!(
            acc.iter().any(|&(l, r)| l != 0 || r != 0),
            "a live-envelope wave note must be audible"
        );
    }

    /// Hold a square voice's envelope at `expected_level` (the ADSR's zero
    /// attack and non-zero decay latch `volume` to the goal on entering decay,
    /// before any decay step consumes it) and return its peak render sample.
    fn square_peak_at_level(track_volume: u8, expected_level: u8) -> i32 {
        let held_at_goal = CgbAdsr {
            attack: 0,
            decay: 8,
            sustain: 15,
            release: 0,
        };
        let note = TestNote {
            track_right: track_volume,
            track_left: track_volume,
            ..TestNote::default()
        };
        let mut voice = square_voice_with_adsr(CgbChannelNumber::Square1, None, held_at_goal, note);
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        assert_eq!(
            voice.envelope_volume(),
            expected_level,
            "test setup must hold the envelope at the level under test"
        );
        let mut acc = vec![(0i32, 0i32); 64];
        voice.render(&mut acc, &[]);
        acc.iter()
            .map(|&(left, right)| left.abs().max(right.abs()))
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn square_envelope_levels_above_fifteen_render_through_the_nrx2_low_nibble() {
        // Pins the write half of `HardwareEnvelopeVolume`'s contract: levels
        // 16 and 30 must render through the low nibble, not the raw byte.
        let level_14_peak = square_peak_at_level(121, 14);
        let level_15_peak = square_peak_at_level(130, 15);

        assert!(level_14_peak > 0);
        assert!(level_15_peak > 0);
        assert_eq!(square_peak_at_level(131, 16), 0);
        assert_eq!(square_peak_at_level(243, 30), level_14_peak);
        assert_eq!(square_peak_at_level(255, 31), level_15_peak);
    }

    fn centred_goal_thirty_one_note() -> TestNote {
        TestNote {
            track_right: u8::MAX,
            track_left: u8::MAX,
            ..TestNote::default()
        }
    }

    /// Peak render sample of each of `frames` consecutive frames of a square
    /// voice, counting from its note-on. Square gain is the hardware nibble
    /// alone (`Oscillator::envelope_gain_256`), so the series reads the
    /// hardware envelope out directly.
    fn square_peaks_from_note_on(adsr: CgbAdsr, frames: usize) -> Vec<i32> {
        let mut voice = square_voice_with_adsr(
            CgbChannelNumber::Square1,
            None,
            adsr,
            centred_goal_thirty_one_note(),
        );
        (0..frames)
            .map(|_| {
                voice.begin_frame(MAX_MASTER_VOLUME, false);
                let mut acc = vec![(0i32, 0i32); 64];
                voice.render(&mut acc, &[]);
                acc.iter()
                    .map(|&(left, right)| left.abs().max(right.abs()))
                    .max()
                    .unwrap_or(0)
            })
            .collect()
    }

    #[test]
    fn note_on_arms_the_hardware_timer_from_the_decoded_nrx2_period() {
        // Pins the note-on half of `HardwareEnvelopeVolume`'s contract: the
        // note-on write programs NRx2 with the attack's decoded nibble, so a
        // fresh voice's timer starts from the decoded period, not the raw
        // attack byte.
        //
        // Attack 17 writes `(17 + CGB_NRx2_ENV_DIR_INC) & 0xF == 9`: period
        // 1, upward. The nibble leaves zero on the second frame, not on the
        // raw byte's eighteenth.
        let climbing = square_peaks_from_note_on(
            CgbAdsr {
                attack: 17,
                decay: 0,
                sustain: 15,
                release: 0,
            },
            4,
        );
        assert_eq!(
            climbing[0], 0,
            "the note-on write still owes one iteration on its own frame"
        );
        assert!(
            climbing[1] > 0 && climbing[2] > climbing[1] && climbing[3] > climbing[2],
            "a decoded period of 1 climbs a nibble per frame, got {climbing:?}"
        );
    }

    #[test]
    fn a_downward_note_on_write_at_level_zero_leaves_hardware_dead() {
        // The other half: the note-on timer takes the same saturation check
        // `write` applies. Attack 9 writes
        // `(9 + CGB_NRx2_ENV_DIR_INC) & 0xF == 1`: period 1, downward, and
        // the note-on level of zero is already saturated in that direction,
        // so the write leaves hardware dead rather than stepping the nibble
        // below zero.
        let dead = square_peaks_from_note_on(
            CgbAdsr {
                attack: 9,
                decay: 0,
                sustain: 15,
                release: 0,
            },
            12,
        );
        assert_eq!(
            dead,
            vec![0; 12],
            "a downward note-on write at level zero is dead on arrival"
        );
    }

    #[test]
    fn centred_goal_thirty_one_decay_falls_with_the_hardware_envelope_between_writes() {
        // Pins the continuation half of `HardwareEnvelopeVolume`'s contract:
        // the frame right after a write is already one hardware step
        // quieter, not still holding the write frame's loudness.
        let level_15_peak = square_peak_at_level(130, 15);
        let level_14_peak = square_peak_at_level(121, 14);

        let paced_decay = CgbAdsr {
            attack: 0,
            decay: 1,
            sustain: 0,
            release: 0,
        };
        let mut voice = square_voice_with_adsr(
            CgbChannelNumber::Square1,
            None,
            paced_decay,
            centred_goal_thirty_one_note(),
        );

        voice.begin_frame(MAX_MASTER_VOLUME, false);
        assert_eq!(
            voice.envelope_volume(),
            31,
            "write frame must hold the raw goal"
        );
        let mut write_frame_acc = vec![(0i32, 0i32); 64];
        voice.render(&mut write_frame_acc, &[]);
        let write_frame_peak = write_frame_acc
            .iter()
            .map(|&(left, right)| left.abs().max(right.abs()))
            .max()
            .unwrap_or(0);
        assert_eq!(write_frame_peak, level_15_peak);

        voice.begin_frame(MAX_MASTER_VOLUME, false);
        assert_eq!(
            voice.envelope_volume(),
            30,
            "one paced decay step without a write must have run"
        );
        let mut next_frame_acc = vec![(0i32, 0i32); 64];
        voice.render(&mut next_frame_acc, &[]);
        let next_frame_peak = next_frame_acc
            .iter()
            .map(|&(left, right)| left.abs().max(right.abs()))
            .max()
            .unwrap_or(0);
        assert_eq!(
            next_frame_peak, level_14_peak,
            "hardware's own envelope timer must already be one step quieter"
        );
    }

    /// Render one frame and return its peak sample magnitude.
    fn frame_peak(voice: &mut CgbVoice) -> i32 {
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        let mut acc = vec![(0i32, 0i32); 64];
        voice.render(&mut acc, &[]);
        acc.iter()
            .map(|&(left, right)| left.abs().max(right.abs()))
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn live_volume_write_mid_decay_restarts_the_hardware_envelope_timer() {
        // A live `MPT_FLG_VOLCHG` raises `CGB_CHANNEL_MO_VOL`
        // (`m4a_1.s:1394`..`:1400`) without touching `envelopeCounter`, and
        // that write ends in `*nrx4ptr = channels->n4 | 0x80`
        // (`m4a.c:1222`..`:1223`), whose trigger bit reloads the hardware
        // envelope timer from NRx2's step-time nibble (`_resetEnvelope`'s
        // `nextStep = stepTime`, `mgba/src/gb/audio.c:856`..`:859`, reached
        // from `GBAudioWriteNR14`'s restart branch, `:180`..`:181`). The
        // software counter keeps its own phase across that write, so the
        // hardware level must pace from the write, not from whatever the
        // stale software counter had left.
        let level_15_peak = square_peak_at_level(130, 15);
        let level_14_peak = square_peak_at_level(121, 14);
        assert_ne!(
            level_15_peak, level_14_peak,
            "sanity: the two levels differ"
        );

        let paced_decay = CgbAdsr {
            attack: 0,
            decay: 4,
            sustain: 0,
            release: 0,
        };
        let goal_fifteen_note = TestNote {
            track_right: 130,
            track_left: 130,
            ..TestNote::default()
        };
        let mut voice = square_voice_with_adsr(
            CgbChannelNumber::Square1,
            None,
            paced_decay,
            goal_fifteen_note,
        );

        assert_eq!(
            frame_peak(&mut voice),
            level_15_peak,
            "sanity: the attack-to-decay write latches level 15"
        );
        for frame in 0..2 {
            assert_eq!(
                frame_peak(&mut voice),
                level_15_peak,
                "sanity: frame {frame} is still inside the first decay period"
            );
        }

        // Two iterations short of the software counter's next step.
        voice.set_track_volume(130, 130);
        assert_eq!(
            frame_peak(&mut voice),
            level_15_peak,
            "sanity: the live volume write relatches the same level 15"
        );

        for frame in 0..(paced_decay.decay - 1) {
            assert_eq!(
                frame_peak(&mut voice),
                level_15_peak,
                "frame {frame} after the live volume write: the trigger restarted the hardware \
                 envelope timer, so a full step time must elapse before the next hardware step"
            );
        }
        assert_eq!(
            frame_peak(&mut voice),
            level_14_peak,
            "one full step time after the trigger, the hardware envelope steps once"
        );
    }

    #[test]
    fn centred_goal_thirty_one_attack_climbs_with_the_hardware_envelope_then_freezes_at_fifteen() {
        // Pins the climbing direction and the saturation ceiling of
        // `HardwareEnvelopeVolume`'s contract.
        let paced_attack = CgbAdsr {
            attack: 1,
            decay: 1,
            sustain: 15,
            release: 0,
        };
        let mut voice = square_voice_with_adsr(
            CgbChannelNumber::Square1,
            None,
            paced_attack,
            centred_goal_thirty_one_note(),
        );

        let mut previous_peak = -1i32;
        let mut peak_at_fifteen = None;
        for _ in 0..40 {
            voice.begin_frame(MAX_MASTER_VOLUME, false);
            let level = voice.envelope_volume();
            let mut acc = vec![(0i32, 0i32); 64];
            voice.render(&mut acc, &[]);
            let peak = acc
                .iter()
                .map(|&(left, right)| left.abs().max(right.abs()))
                .max()
                .unwrap_or(0);
            if level <= 15 {
                assert!(
                    peak >= previous_peak,
                    "loudness must not fall while the level is climbing in range"
                );
                if level == 15 {
                    peak_at_fifteen = Some(peak);
                }
            } else {
                assert_eq!(
                    Some(peak),
                    peak_at_fifteen,
                    "hardware must stay pinned at level 15's loudness once the level exceeds it"
                );
            }
            previous_peak = peak;
            if level >= 31 {
                break;
            }
        }
        assert!(
            peak_at_fifteen.is_some(),
            "test setup must run the attack far enough to reach level 15"
        );
    }

    #[test]
    fn centred_goal_thirty_one_decay_freezes_at_zero_before_software_reaches_it() {
        // Pins the saturation floor of `HardwareEnvelopeVolume`'s contract:
        // hardware goes silent well before the software envelope itself
        // reaches zero.
        let paced_decay_to_silence = CgbAdsr {
            attack: 0,
            decay: 1,
            sustain: 0,
            release: 0,
        };
        let mut voice = square_voice_with_adsr(
            CgbChannelNumber::Square1,
            None,
            paced_decay_to_silence,
            centred_goal_thirty_one_note(),
        );

        let mut saw_silence_while_still_in_range = false;
        for _ in 0..40 {
            voice.begin_frame(MAX_MASTER_VOLUME, false);
            let level = voice.envelope_volume();
            let mut acc = vec![(0i32, 0i32); 64];
            voice.render(&mut acc, &[]);
            let peak = acc
                .iter()
                .map(|&(left, right)| left.abs().max(right.abs()))
                .max()
                .unwrap_or(0);
            if level > 0 && level <= 15 && peak == 0 {
                saw_silence_while_still_in_range = true;
            }
            if level == 0 {
                assert_eq!(peak, 0, "hardware must be silent once it has bottomed out");
                break;
            }
        }
        assert!(
            saw_silence_while_still_in_range,
            "hardware must freeze at silence before the software level itself reaches zero"
        );
    }

    #[test]
    fn live_track_volume_drop_during_sustain_does_not_silence_the_channel() {
        // Sustain's periodic refresh (`CgbEnvelope::sustain_step`, every
        // `SUSTAIN_REFRESH_FRAMES`) only updates software bookkeeping and
        // never sets a hardware write -- upstream's own sustain-refresh
        // branch never raises `CGB_CHANNEL_MO_VOL` (`m4a.c:1128-1137`).
        // `set_track_volume` itself retriggers immediately, but that write
        // lands before the software goal has caught up (still the stale
        // value), so it changes nothing audible; the *later*, unwritten
        // refresh is the one that actually snaps to the new low goal.
        // `HardwareEnvelopeVolume::track` must not read that unwritten snap
        // as gradual hardware envelope motion and silence the channel.
        let held_at_full_sustain = CgbAdsr {
            attack: 0,
            decay: 0,
            sustain: MAX_MASTER_VOLUME,
            release: 0,
        };
        let mut voice = square_voice_with_adsr(
            CgbChannelNumber::Square1,
            None,
            held_at_full_sustain,
            centred_goal_thirty_one_note(),
        );
        voice.begin_frame(MAX_MASTER_VOLUME, false); // enters sustain at its write frame

        let mut sanity_acc = vec![(0i32, 0i32); 8];
        voice.render(&mut sanity_acc, &[]);
        assert!(
            sanity_acc.iter().any(|&(l, r)| l != 0 || r != 0),
            "sanity: the note starts audible in sustain"
        );

        let near_silent_track_volume = 1u8;
        voice.set_track_volume(near_silent_track_volume, near_silent_track_volume);

        for frame in 0..8 {
            voice.begin_frame(MAX_MASTER_VOLUME, false);
            let mut acc = vec![(0i32, 0i32); 8];
            voice.render(&mut acc, &[]);
            assert!(
                acc.iter().any(|&(l, r)| l != 0 || r != 0),
                "frame {frame}: hardware holds its last explicit write through the unwritten \
                 sustain refresh; it must not go silent before another real trigger"
            );
        }
    }

    #[test]
    fn cgb3_wave_level_pins_the_stepped_output_levels() {
        assert_eq!(cgb3_wave_gain_256(0), SILENT_GAIN_256);
        assert_eq!(cgb3_wave_gain_256(1), SILENT_GAIN_256);
        assert_eq!(cgb3_wave_gain_256(2), QUARTER_GAIN_256);
        assert_eq!(cgb3_wave_gain_256(6), HALF_GAIN_256);
        assert_eq!(cgb3_wave_gain_256(10), THREE_QUARTER_GAIN_256);
        assert_eq!(cgb3_wave_gain_256(15), FULL_GAIN_256);
        assert_eq!(cgb3_wave_gain_256(31), FULL_GAIN_256);
    }

    #[test]
    fn square_gain_at_the_nibble_ceiling_reaches_near_full_scale_like_the_wave_arm() {
        // `envelope_gain_256`'s square/noise arm must reach a ceiling
        // comparable to the wave arm's full-scale gain (`cgb3_wave_gain_256`,
        // above) once both read the same 0..=15 nibble domain. The bound
        // allows one hardware step (`LINEAR_ENVELOPE_SCALE`, the finest step
        // this linear law can represent) short of that ceiling.
        let square = Oscillator::Square(SquareChannel::new(HALF_DUTY, 0, None));
        let square_ceiling = square.envelope_gain_256(15, 15);
        assert!(
            square_ceiling >= FULL_GAIN_256 - LINEAR_ENVELOPE_SCALE,
            "the loudest square/noise nibble ({square_ceiling}/256) must land within one \
             hardware step of the wave arm's full-scale gain ({FULL_GAIN_256}/256)"
        );
    }

    #[test]
    fn square1_upward_sweep_overflow_is_born_muted_and_revives_on_a_safe_trigger() {
        // A note-on overflow clears the hardware channel-enable bit and nothing
        // else: upstream leaves SOUND_CHANNEL_SF_ON set and keeps running the
        // envelope, so the next volume write can bring the note back
        // (`m4a.c:1055-1058`, `mgba/src/gb/audio.c:180-186`).
        let high_key = 120;
        let safe_key = 48;
        for sweep_period in [0, 3] {
            let sweep = upward_sweep(sweep_period, 1);
            let mut muted = square_voice(
                CgbChannelNumber::Square1,
                Some(sweep),
                TestNote::at_key(high_key),
            );
            assert!(
                muted.is_active(),
                "the overflow mutes the hardware channel; the software voice keeps its slot"
            );
            let mut acc = vec![(0i32, 0i32); 8];
            muted.begin_frame(MAX_MASTER_VOLUME, false);
            muted.render(&mut acc, &[]);
            assert!(
                acc.iter().all(|&(l, r)| l == 0 && r == 0),
                "a born-muted channel must be silent from frame 0"
            );

            muted.set_track_pitch(i32::from(safe_key) - i32::from(high_key), 0);
            muted.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
            muted.begin_frame(MAX_MASTER_VOLUME, false);
            let mut revived = vec![(0i32, 0i32); 8];
            muted.render(&mut revived, &[]);
            assert!(
                revived.iter().any(|&(l, r)| l != 0 || r != 0),
                "a later safe trigger must revive the born-muted note"
            );
        }

        let mut normal = square_voice(
            CgbChannelNumber::Square1,
            Some(upward_sweep(0, 1)),
            TestNote::at_key(safe_key),
        );
        let mut acc = vec![(0i32, 0i32); 8];
        normal.begin_frame(MAX_MASTER_VOLUME, false);
        normal.render(&mut acc, &[]);
        assert!(
            acc.iter().any(|&(l, r)| l != 0 || r != 0),
            "a normal-frequency sweep note is audible from frame 0"
        );
    }

    #[test]
    fn with_pitch_key_overrides_the_base_used_for_mid_note_bend() {
        let rhythm_child_key = 72;
        assert_eq!(
            noise_voice(CgbAdsr::flat(), WIDE_NOISE, TestNote::default())
                .with_pitch_key(rhythm_child_key)
                .midi_key(),
            TEST_KEY,
            "EOT identity stays the played key"
        );

        let mut square = square_voice(CgbChannelNumber::Square1, None, TestNote::default())
            .with_pitch_key(rhythm_child_key);
        square.set_track_pitch(0, 0);
        let mut expected = square_voice(
            CgbChannelNumber::Square1,
            None,
            TestNote::at_key(rhythm_child_key),
        );
        let mut acc_a = vec![(0i32, 0i32); 16];
        let mut acc_b = vec![(0i32, 0i32); 16];
        square.begin_frame(MAX_MASTER_VOLUME, false);
        square.render(&mut acc_a, &[]);
        expected.begin_frame(MAX_MASTER_VOLUME, false);
        expected.render(&mut acc_b, &[]);
        assert_eq!(acc_a, acc_b);
    }

    #[test]
    fn rhythm_pan_shifts_the_stereo_split_on_construction_and_reruns() {
        let half_track_volume = 0x40;
        let rightward_pan = 63;
        let centred_note = TestNote {
            track_right: half_track_volume,
            track_left: half_track_volume,
            ..TestNote::default()
        };
        let panned_note = TestNote {
            rhythm_pan: rightward_pan,
            ..centred_note
        };
        let centred = noise_voice(CgbAdsr::flat(), WIDE_NOISE, centred_note);
        let panned = noise_voice(CgbAdsr::flat(), WIDE_NOISE, panned_note);
        assert!(
            panned.routing.right > centred.routing.right,
            "a positive rhythm pan should raise the right-channel base volume"
        );

        let mut voice = noise_voice(CgbAdsr::flat(), WIDE_NOISE, panned_note);
        voice.set_track_volume(half_track_volume, half_track_volume);
        assert!(
            voice.routing.right > voice.routing.left,
            "the rhythm-pan override must survive a mid-note VOL rerun"
        );
    }

    #[test]
    fn echo_volume_and_length_are_copied_into_the_envelope_at_construction() {
        let echo_note = TestNote {
            echo_volume: 128,
            echo_length: 3,
            ..TestNote::default()
        };
        let mut voice = noise_voice(
            CgbAdsr {
                attack: 0,
                decay: 0,
                sustain: MAX_MASTER_VOLUME,
                release: 0,
            },
            WIDE_NOISE,
            echo_note,
        );
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        voice.note_off();
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        assert!(
            voice.is_active(),
            "a nonzero echo_volume must hold the channel in its pseudo-echo tail"
        );
    }

    #[test]
    fn note_off_before_the_first_envelope_pass_produces_no_audible_samples() {
        // Note-off before any `begin_frame`: `CgbEnvelope`'s upstream
        // short-circuit retires it at once, skipping the pseudo-echo tail.
        let echo_note = TestNote {
            echo_volume: 128,
            echo_length: 3,
            ..TestNote::default()
        };
        let mut voice = noise_voice(
            CgbAdsr {
                attack: 0,
                decay: 0,
                sustain: MAX_MASTER_VOLUME,
                release: 0,
            },
            WIDE_NOISE,
            echo_note,
        );

        voice.note_off();
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        let mut acc = vec![(0i32, 0i32); 8];
        voice.render(&mut acc, &[]);

        assert!(
            !voice.is_active(),
            "must retire immediately, not hold a pseudo-echo tail"
        );
        assert!(
            acc.iter().all(|&(l, r)| l == 0 && r == 0),
            "must produce no audible samples"
        );
    }

    #[test]
    fn fixed_rate_dac_correction_matches_emeralds_8_bit_formula() {
        let correction = DacCorrection::FixedRate8Bit;
        assert_eq!(correction.apply(0), 0);
        assert_eq!(correction.apply(1), 2);
        assert_eq!(correction.apply(2), 2);
        assert_eq!(correction.apply(EVEN_FREQUENCY_REGISTER_MASK), 0x7FE);
        assert_eq!(DacCorrection::None.apply(0x555), 0x555);
    }

    #[test]
    fn fixed_rate_note_on_applies_the_dac_correction_before_the_sweep_mute_check() {
        let edge_note = TestNote {
            fine_pitch: 167,
            ..TestNote::at_key(54)
        };
        let raw_frequency = midi_key_to_cgb_freq_reg(edge_note.note_key, edge_note.fine_pitch);
        assert_eq!(raw_frequency, 0x555);
        assert_eq!(DacCorrection::FixedRate8Bit.apply(raw_frequency), 0x556);

        let sweep = upward_sweep(0, 1);
        let mut plain = square_voice(CgbChannelNumber::Square1, Some(sweep), edge_note);
        let mut plain_frame = vec![(0i32, 0i32); 8];
        plain.begin_frame(MAX_MASTER_VOLUME, false);
        plain.render(&mut plain_frame, &[]);
        assert!(
            plain_frame.iter().any(|&(l, r)| l != 0 || r != 0),
            "the uncorrected sum sits exactly at the threshold, not over it, so the note sounds"
        );

        let mut fixed = fixed_square_voice(CgbChannelNumber::Square1, Some(sweep), edge_note);
        let mut fixed_frame = vec![(0i32, 0i32); 8];
        fixed.begin_frame(MAX_MASTER_VOLUME, false);
        fixed.render(&mut fixed_frame, &[]);
        assert!(
            fixed_frame.iter().all(|&(l, r)| l == 0 && r == 0),
            "the DAC-corrected sum must overflow the sweep and mute the channel"
        );
        assert!(
            fixed.is_active(),
            "the overflow mutes the hardware channel without retiring the voice"
        );

        let safe_key: u8 = 48;
        fixed.set_track_pitch(i32::from(safe_key) - i32::from(edge_note.note_key), 0);
        fixed.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
        fixed.begin_frame(MAX_MASTER_VOLUME, false);
        let mut revived = vec![(0i32, 0i32); 8];
        fixed.render(&mut revived, &[]);
        assert!(
            revived.iter().any(|&(l, r)| l != 0 || r != 0),
            "a later safe trigger must revive the muted fixed-rate note"
        );
    }

    #[test]
    fn fixed_rate_wave_note_on_audibly_differs_from_the_uncorrected_register() {
        let edge_note = TestNote {
            fine_pitch: 167,
            ..TestNote::at_key(54)
        };
        let mut fixed = wave_voice(true, edge_note);
        let mut plain = wave_voice(false, edge_note);
        let mut acc_fixed = vec![(0i32, 0i32); 2048];
        let mut acc_plain = vec![(0i32, 0i32); 2048];
        fixed.begin_frame(MAX_MASTER_VOLUME, false);
        fixed.render(&mut acc_fixed, &[]);
        plain.begin_frame(MAX_MASTER_VOLUME, false);
        plain.render(&mut acc_plain, &[]);
        assert_ne!(
            acc_fixed, acc_plain,
            "the DAC-corrected register must audibly differ from the uncorrected one"
        );
    }

    #[test]
    fn set_track_pitch_reapplies_the_dac_correction_for_a_fixed_rate_channel() {
        let target_key = 54;
        let target_fine_pitch = 167;
        let target_note = TestNote {
            fine_pitch: target_fine_pitch,
            ..TestNote::at_key(target_key)
        };
        let mut direct = fixed_square_voice(CgbChannelNumber::Square2, None, target_note);
        let mut retuned = fixed_square_voice(
            CgbChannelNumber::Square2,
            None,
            TestNote {
                played_key: target_key,
                ..TestNote::default()
            },
        )
        .with_pitch_key(target_key);
        retuned.set_track_pitch(0, target_fine_pitch);

        let mut acc_direct = vec![(0i32, 0i32); 2048];
        let mut acc_retuned = vec![(0i32, 0i32); 2048];
        direct.begin_frame(MAX_MASTER_VOLUME, false);
        direct.render(&mut acc_direct, &[]);
        retuned.begin_frame(MAX_MASTER_VOLUME, false);
        retuned.render(&mut acc_retuned, &[]);
        assert_eq!(acc_direct, acc_retuned);
    }

    fn low_freq_sweep_voice(sweep_byte: u8) -> CgbVoice {
        square_voice(
            CgbChannelNumber::Square1,
            Some(sweep_byte),
            TestNote::at_key(0),
        )
    }

    fn sweep_frequency_after(sweep_byte: u8, len: usize, schedule: &[usize]) -> u16 {
        let mut voice = low_freq_sweep_voice(sweep_byte);
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        let ticks: Vec<usize> = schedule.iter().copied().filter(|&t| t < len).collect();
        let mut acc = vec![(0i32, 0i32); len];
        voice.render(&mut acc, &ticks);
        voice
            .sweep_frequency()
            .expect("still a square voice with a sweep configured")
    }

    fn sweep_steps(sweep_byte: u8, total: usize, schedule: &[usize]) -> Vec<(usize, u16)> {
        let mut steps = Vec::new();
        let mut previous = sweep_frequency_after(sweep_byte, 0, schedule);
        for len in 1..=total {
            let frequency = sweep_frequency_after(sweep_byte, len, schedule);
            if frequency != previous {
                steps.push((len - 1, frequency));
                previous = frequency;
            }
        }
        steps
    }

    #[test]
    fn square1_sweep_period_1_steps_once_per_scheduled_128hz_tick() {
        let mut clock = FrameSequencer128Hz::default();
        let schedule = clock.advance(600);
        assert_eq!(schedule, vec![104, 209, 313, 418, 522]);

        assert_eq!(
            sweep_steps(upward_sweep(1, 1), 600, &schedule),
            vec![(104, 66), (209, 99), (313, 148), (418, 222), (522, 333)],
        );
    }

    #[test]
    fn square1_sweep_period_2_steps_once_per_second_scheduled_tick() {
        let mut clock = FrameSequencer128Hz::default();
        let schedule = clock.advance(1200);
        assert_eq!(
            schedule,
            vec![104, 209, 313, 418, 522, 627, 731, 836, 940, 1045, 1149]
        );

        assert_eq!(
            sweep_steps(upward_sweep(2, 1), 1200, &schedule),
            vec![(209, 66), (418, 99), (627, 148), (836, 222), (1045, 333)],
        );
    }

    #[test]
    fn cgb_voice_render_is_chunk_boundary_invariant() {
        let make_voice = || low_freq_sweep_voice(upward_sweep(1, 1));

        let mut whole_voice = make_voice();
        whole_voice.begin_frame(MAX_MASTER_VOLUME, false);
        let mut whole_clock = FrameSequencer128Hz::default();
        let whole_ticks = whole_clock.advance(600);
        let mut whole_acc = vec![(0i32, 0i32); 600];
        whole_voice.render(&mut whole_acc, &whole_ticks);

        let mut split_voice = make_voice();
        split_voice.begin_frame(MAX_MASTER_VOLUME, false);
        let mut split_clock = FrameSequencer128Hz::default();
        let first_ticks = split_clock.advance(300);
        let mut first_half = vec![(0i32, 0i32); 300];
        split_voice.render(&mut first_half, &first_ticks);
        let second_ticks = split_clock.advance(300);
        let mut second_half = vec![(0i32, 0i32); 300];
        split_voice.render(&mut second_half, &second_ticks);
        let mut split_acc = first_half;
        split_acc.extend(second_half);

        assert_eq!(whole_acc, split_acc);
        assert!(
            whole_acc.iter().any(|&(l, r)| l != 0 || r != 0),
            "sanity: the sweeping voice must actually be audible"
        );
    }

    #[test]
    fn square1_sweep_overflow_mutes_the_voice_mid_buffer_until_the_next_safe_trigger() {
        // A running sweep's overflow clears the channel-enable bit exactly as
        // a trigger-time overflow does, and leaves the same later trigger able
        // to revive it (`mgba/src/gb/audio.c:667-672`, `:180-186`).
        let safe_key = 48;
        let mut voice = square_voice(
            CgbChannelNumber::Square1,
            Some(upward_sweep(1, 1)),
            TestNote::at_key(safe_key),
        );
        assert!(
            voice.is_active(),
            "not born dead: the trigger check alone doesn't overflow"
        );
        voice.begin_frame(MAX_MASTER_VOLUME, false);

        let mut clock = FrameSequencer128Hz::default();
        let ticks = clock.advance(300);
        let first_tick = ticks[0];
        let mut acc = vec![(0i32, 0i32); 300];
        voice.render(&mut acc, &ticks);

        assert!(
            acc[..first_tick].iter().any(|&(l, r)| l != 0 || r != 0),
            "samples before the overflowing tick must still be audible"
        );
        assert!(
            acc[first_tick..].iter().all(|&(l, r)| l == 0 && r == 0),
            "samples from the overflowing tick onward must be silent, not just \
             at the buffer end"
        );
        assert!(
            voice.is_active(),
            "the overflow mutes the hardware channel; the software voice lives on"
        );
        let mut still_muted = vec![(0i32, 0i32); 8];
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        voice.render(&mut still_muted, &[]);
        assert!(
            still_muted.iter().all(|&(l, r)| l == 0 && r == 0),
            "the mute holds across frames until a trigger rechecks the sweep"
        );

        voice.set_track_pitch(0, 0);
        voice.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        let mut revived = vec![(0i32, 0i32); 8];
        voice.render(&mut revived, &[]);
        assert!(
            revived.iter().any(|&(l, r)| l != 0 || r != 0),
            "a later safe trigger must revive the muted voice, not find it retired"
        );
    }

    #[test]
    fn attack_to_decay_volume_write_retriggers_channel1_sweep_from_the_current_frequency() {
        // The attack-to-decay transition is a volume write that reloads
        // channel 1's sweep from the channel's *current* played frequency,
        // not a pitch bend's stale shadow (`m4a.c:1150-1158,1219-1226`;
        // `mgba/src/gb/audio.c:180-186`).
        let safe_key: u8 = 48;
        let overflow_prone_key: u8 = 120; // shares its overflow fixture with
                                          // `square1_upward_sweep_overflow_is_born_muted_and_revives_on_a_safe_trigger`.
        let mut voice = CgbVoice::square(
            CgbChannelNumber::Square1,
            HALF_DUTY,
            Some(upward_sweep(0, 1)),
            CgbAdsr {
                attack: 0,
                decay: 1,
                sustain: MAX_MASTER_VOLUME,
                release: 0,
            },
            safe_key,
            0,
            FULL_TRACK_VOLUME,
            FULL_TRACK_VOLUME,
            FULL_VELOCITY,
            0,
            safe_key,
            0,
            0,
            0,
            0,
        );
        assert!(
            voice.is_active(),
            "constructed at a safe frequency, the sweep must not be born dead"
        );

        voice.set_track_pitch(i32::from(overflow_prone_key) - i32::from(safe_key), 0);
        assert!(
            voice.is_active(),
            "a pitch bend alone must not retrigger the sweep"
        );

        voice.begin_frame(MAX_MASTER_VOLUME, false); // attack==0 -> decay!=0: retriggers

        assert!(
            voice.is_active(),
            "an overflowing trigger mutes the hardware channel, not the software voice"
        );
        let mut acc = vec![(0i32, 0i32); 8];
        voice.render(&mut acc, &[]);
        assert!(
            acc.iter().all(|&(l, r)| l == 0 && r == 0),
            "the attack-to-decay volume write must retrigger channel 1, reloading the sweep \
             shadow from the bent frequency and finding it overflows"
        );
    }

    #[test]
    fn live_track_volume_update_retriggers_channel1_sweep_from_the_current_frequency() {
        // `set_track_volume` is itself a volume-write trigger, distinct from
        // an envelope transition (`m4a_1.s:1391-1400`, applied at `m4a.c:1219-1226`).
        let safe_key: u8 = 48;
        let overflow_prone_key: u8 = 120;
        let mut voice = square_voice(
            CgbChannelNumber::Square1,
            Some(upward_sweep(0, 1)),
            TestNote::at_key(safe_key),
        );
        // Settle `CgbAdsr::flat()`'s own instant retrigger first, so this
        // frame isolates `set_track_volume`'s retrigger below.
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        assert!(
            voice.is_active(),
            "the settling frame must not itself overflow"
        );

        voice.set_track_pitch(i32::from(overflow_prone_key) - i32::from(safe_key), 0);
        assert!(
            voice.is_active(),
            "a pitch bend alone must not retrigger the sweep"
        );

        voice.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
        assert!(
            voice.is_active(),
            "sanity: set_track_volume must not retrigger before the next begin_frame"
        );

        voice.begin_frame(MAX_MASTER_VOLUME, false); // steady in sustain now, so only
                                                     // set_track_volume's retrigger explains
                                                     // the mute below

        assert!(
            voice.is_active(),
            "an overflowing trigger mutes the hardware channel, not the software voice"
        );
        let mut acc = vec![(0i32, 0i32); 8];
        voice.render(&mut acc, &[]);
        assert!(
            acc.iter().all(|&(l, r)| l == 0 && r == 0),
            "a live volume update must retrigger channel 1, reloading the sweep shadow from \
             the bent frequency and finding it overflows"
        );
    }

    #[test]
    fn a_trigger_time_sweep_overflow_only_mutes_the_channel_until_the_next_safe_trigger() {
        let safe_key = 48;
        let overflowing_key = 120;
        let mut voice = square_voice(
            CgbChannelNumber::Square1,
            Some(upward_sweep(0, 1)),
            TestNote::at_key(safe_key),
        );
        assert!(voice.is_active(), "sanity: the note is born playing");

        // The volume write's trigger, after this bend, rechecks the sweep
        // and overflows (`mgba/src/gb/audio.c:180-196`).
        voice.set_track_pitch(i32::from(overflowing_key - safe_key), 0);
        voice.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        let mut muted = vec![(0i32, 0i32); 8];
        voice.render(&mut muted, &[]);
        assert!(
            muted.iter().all(|&(l, r)| l == 0 && r == 0),
            "the overflowing trigger must silence the hardware channel"
        );

        // The next trigger reruns the same recheck and finds no overflow
        // (`m4a.c:1053-1056`).
        voice.set_track_pitch(0, 0);
        voice.set_track_volume(FULL_TRACK_VOLUME, FULL_TRACK_VOLUME);
        voice.begin_frame(MAX_MASTER_VOLUME, false);
        let mut revived = vec![(0i32, 0i32); 8];
        voice.render(&mut revived, &[]);
        assert!(
            revived.iter().any(|&(l, r)| l != 0 || r != 0),
            "a later safe trigger must revive the muted voice, not find it retired"
        );
    }
}
