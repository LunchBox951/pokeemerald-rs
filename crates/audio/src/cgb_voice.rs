//! Live CGB PSG voice playback, envelopes, and stereo routing.

use crate::cgb_envelope::{cgb_envelope_goal, cgb_pan, CgbAdsr, CgbEnvelope, Panning};
use crate::cgb_pitch::{midi_key_to_cgb_freq_reg, midi_key_to_noise_control};
use crate::psg::{NoiseChannel, SquareChannel, WaveChannel};
use crate::voice::{channel_volume, pan_terms, StereoAcc};

const BIPOLAR_SAMPLE_SCALE: i32 = 127;
const WAVE_SAMPLE_SCALE: i32 = 16;
const LINEAR_ENVELOPE_SCALE: u32 = 8;
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

    fn envelope_gain_256(&self, envelope_volume: u8) -> u32 {
        match self {
            Self::Wave(_) => cgb3_wave_gain_256(envelope_volume),
            Self::Square(_) | Self::Noise(_) => u32::from(envelope_volume) * LINEAR_ENVELOPE_SCALE,
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
        let disabled_at_trigger = oscillator.disabled_at_trigger();
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
        if disabled_at_trigger {
            voice.envelope.retire();
        }
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
        Self {
            channel,
            oscillator,
            envelope: CgbEnvelope::new(adsr, routing.envelope_goal(), echo_volume, echo_length),
            adsr,
            routing,
            frame_gain: 0,
            gate: Gate::new(gate_time),
            identity: VoiceIdentity::new(track, midi_key),
            dac_correction,
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
        if self.gate.tick() {
            self.envelope.note_off();
        }
    }

    /// Request note-off immediately.
    pub fn note_off(&mut self) {
        self.envelope.note_off();
    }

    /// Update base volume, panning, and envelope goal from the owning track.
    pub fn set_track_volume(&mut self, vol_mr: u8, vol_ml: u8) {
        self.routing.update_from_track(vol_mr, vol_ml);
        self.envelope
            .set_goal(self.adsr, self.routing.envelope_goal());
    }

    /// Retune from the owning track while preserving note identity and noise width.
    pub fn set_track_pitch(&mut self, key_m: i32, pit_m: u8) {
        let translated_key = (i32::from(self.identity.pitch_key) + key_m).max(0) % MIDI_KEY_COUNT;
        let note_key = u8::try_from(translated_key).unwrap_or(0);
        self.oscillator.retune(note_key, pit_m, self.dac_correction);
    }

    /// Advance the software envelope and prepare gain for one render frame.
    pub fn begin_frame(&mut self, master_volume: u8, extra_envelope_iteration: bool) {
        self.envelope.step_frame(extra_envelope_iteration);
        let envelope_gain = self.oscillator.envelope_gain_256(self.envelope.volume());
        let effective = ((u32::from(master_volume) + 1) * envelope_gain) >> MASTER_VOLUME_BITS;
        self.frame_gain = i32::try_from(effective).unwrap_or(i32::MAX);
    }

    /// Accumulate this voice into one frame after [`Self::begin_frame`].
    /// `sweep_ticks` must contain ascending sample offsets from the shared
    /// 128 Hz CGB frame sequencer.
    pub fn render(&mut self, acc: &mut [StereoAcc], sweep_ticks: &[usize]) {
        let mut ticks = sweep_ticks.iter().copied().peekable();
        for (sample_offset, output) in acc.iter_mut().enumerate() {
            if !self.envelope.is_active() {
                break;
            }
            if ticks.peek() == Some(&sample_offset) {
                ticks.next();
                if !self.oscillator.step_sweep_tick() {
                    self.envelope.retire();
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

    fn square_voice(channel: CgbChannelNumber, sweep: Option<u8>, note: TestNote) -> CgbVoice {
        CgbVoice::square(
            channel,
            HALF_DUTY,
            sweep,
            CgbAdsr::flat(),
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
    fn square1_upward_sweep_overflow_is_born_dead() {
        let high_key = 120;
        for sweep_period in [0, 3] {
            let sweep = upward_sweep(sweep_period, 1);
            let mut dead = square_voice(
                CgbChannelNumber::Square1,
                Some(sweep),
                TestNote::at_key(high_key),
            );
            assert!(!dead.is_active(), "overflowing sweep note is born dead");
            let mut acc = vec![(0i32, 0i32); 8];
            dead.begin_frame(MAX_MASTER_VOLUME, false);
            dead.render(&mut acc, &[]);
            assert!(
                acc.iter().all(|&(l, r)| l == 0 && r == 0),
                "born-dead channel must be silent from frame 0"
            );
        }

        let normal = square_voice(
            CgbChannelNumber::Square1,
            Some(upward_sweep(0, 1)),
            TestNote::at_key(48),
        );
        assert!(
            normal.is_active(),
            "a normal-frequency sweep note keeps playing"
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
    fn fixed_rate_dac_correction_matches_emeralds_8_bit_formula() {
        let correction = DacCorrection::FixedRate8Bit;
        assert_eq!(correction.apply(0), 0);
        assert_eq!(correction.apply(1), 2);
        assert_eq!(correction.apply(2), 2);
        assert_eq!(correction.apply(EVEN_FREQUENCY_REGISTER_MASK), 0x7FE);
        assert_eq!(DacCorrection::None.apply(0x555), 0x555);
    }

    #[test]
    fn fixed_rate_note_on_applies_the_dac_correction_before_the_sweep_born_dead_check() {
        let edge_note = TestNote {
            fine_pitch: 167,
            ..TestNote::at_key(54)
        };
        let raw_frequency = midi_key_to_cgb_freq_reg(edge_note.note_key, edge_note.fine_pitch);
        assert_eq!(raw_frequency, 0x555);
        assert_eq!(DacCorrection::FixedRate8Bit.apply(raw_frequency), 0x556);

        let sweep = upward_sweep(0, 1);
        let plain = square_voice(CgbChannelNumber::Square1, Some(sweep), edge_note);
        assert!(
            plain.is_active(),
            "the uncorrected sum sits exactly at the threshold, not over it"
        );

        let fixed = fixed_square_voice(CgbChannelNumber::Square1, Some(sweep), edge_note);
        assert!(
            !fixed.is_active(),
            "the DAC-corrected sum must overflow the sweep, born dead"
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
    fn square1_sweep_overflow_retires_the_voice_mid_buffer() {
        let mut voice = square_voice(
            CgbChannelNumber::Square1,
            Some(upward_sweep(1, 1)),
            TestNote::at_key(48),
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
            !voice.is_active(),
            "the voice must retire once the sweep overflows"
        );
    }
}
