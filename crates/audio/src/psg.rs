//! Sample-rate waveform generators for the four CGB programmable sound channels.

use crate::pitch::MIXER_RATE;

const PHASE_ONE: u32 = 1 << 16;
const FREQUENCY_REGISTER_RANGE: u16 = 1 << 11;
const MAX_FREQUENCY_REGISTER: u16 = FREQUENCY_REGISTER_RANGE - 1;
const SQUARE_CLOCK_HZ: f64 = 131_072.0;
const SQUARE_STEPS_PER_CYCLE: f64 = 8.0;
const WAVE_CLOCK_HZ: f64 = 65_536.0;
const WAVE_STEPS_PER_CYCLE: f64 = 32.0;
const NOISE_CLOCK_HZ: f64 = 524_288.0;
const WAVE_RAM_BYTES: usize = 16;
const WAVE_SAMPLES: usize = WAVE_RAM_BYTES * 2;
const NIBBLE_ZERO: i8 = 8;

// CGB duty patterns in register order (mgba/src/gb/audio.c:47-52).
const DUTY_TABLE: [[bool; 8]; 4] = [
    [false, false, false, false, false, false, false, true],
    [true, false, false, false, false, false, false, true],
    [true, false, false, false, false, true, true, true],
    [false, true, true, true, true, true, true, false],
];

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
enum SquareDuty {
    OneEighth,
    OneQuarter,
    OneHalf,
    ThreeQuarters,
}

impl SquareDuty {
    const REGISTER_MASK: u8 = 0b11;

    fn from_register(value: u8) -> Self {
        match value & Self::REGISTER_MASK {
            0 => Self::OneEighth,
            1 => Self::OneQuarter,
            2 => Self::OneHalf,
            3 => Self::ThreeQuarters,
            _ => unreachable!(),
        }
    }

    fn pattern(self) -> &'static [bool; 8] {
        &DUTY_TABLE[self as usize]
    }
}

fn register_frequency_hz(frequency_register: u16, clock_hz: f64) -> f64 {
    let frequency_register = f64::from(frequency_register.min(MAX_FREQUENCY_REGISTER));
    clock_hz / (f64::from(FREQUENCY_REGISTER_RANGE) - frequency_register)
}

fn phase_delta(hz: f64, steps_per_cycle: f64) -> u32 {
    let delta = (hz * steps_per_cycle * f64::from(PHASE_ONE)) / f64::from(MIXER_RATE);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "register frequencies produce positive deltas within u32"
    )]
    {
        delta as u32
    }
}

/// Schedules channel-1 sweep ticks without quantizing them to render buffers.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameSequencer128Hz {
    tick_accumulator: u32,
}

impl FrameSequencer128Hz {
    // A 512 Hz CGB frame sequencer clocks sweep on phases 2 and 6
    // (mgba/src/gb/audio.c:659-668).
    const TICK_HZ: u32 = 128;

    /// Replaces `ticks` with the ascending sample offsets where sweep ticks occur.
    pub fn advance_into(&mut self, samples: usize, ticks: &mut Vec<usize>) {
        ticks.clear();
        for sample_offset in 0..samples {
            self.tick_accumulator += Self::TICK_HZ;
            if self.tick_accumulator >= MIXER_RATE {
                self.tick_accumulator -= MIXER_RATE;
                ticks.push(sample_offset);
            }
        }
    }

    /// Returns the sweep-tick offsets for `samples` output samples.
    #[must_use]
    pub fn advance(&mut self, samples: usize) -> Vec<usize> {
        let mut ticks = Vec::new();
        self.advance_into(samples, &mut ticks);
        ticks
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SweepDirection {
    Increase,
    Decrease,
}

/// Channel-1 frequency sweep state.
#[derive(Clone, Copy, Debug)]
pub struct Sweep {
    shift: u8,
    direction: SweepDirection,
    period_ticks: u8,
    ticks_until_step: u8,
    shadow_frequency: u16,
}

/// Result of advancing a [`Sweep`] by one 128 Hz tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SweepResult {
    /// No frequency change.
    Unchanged,
    /// Retune the channel to this frequency register value.
    Changed(u16),
    /// Silence the channel after a frequency overflow.
    Disable,
}

impl Sweep {
    const SHIFT_MASK: u8 = 0b111;
    const DIRECTION_BIT: u8 = 1 << 3;
    const PERIOD_SHIFT: u32 = 4;

    /// Decodes an `NR10` byte: shift in bits 0..=2, direction in bit 3, and
    /// period in bits 4..=6.
    #[must_use]
    pub fn from_byte(byte: u8, initial_freq_reg: u16) -> Self {
        let direction = if byte & Self::DIRECTION_BIT == 0 {
            SweepDirection::Increase
        } else {
            SweepDirection::Decrease
        };
        let period_ticks = (byte >> Self::PERIOD_SHIFT) & Self::SHIFT_MASK;
        Self {
            shift: byte & Self::SHIFT_MASK,
            direction,
            period_ticks,
            ticks_until_step: period_ticks,
            shadow_frequency: initial_freq_reg.min(MAX_FREQUENCY_REGISTER),
        }
    }

    fn next_frequency(self) -> Option<u16> {
        let delta = self.shadow_frequency >> self.shift;
        match self.direction {
            SweepDirection::Increase => self
                .shadow_frequency
                .checked_add(delta)
                .filter(|&frequency| frequency <= MAX_FREQUENCY_REGISTER),
            SweepDirection::Decrease => Some(self.shadow_frequency - delta),
        }
    }

    /// Reports whether the hardware's trigger-time sweep calculation overflows.
    ///
    /// The check runs for an upward nonzero shift even when the period is zero
    /// (`mgba/src/gb/audio.c:180-186`).
    #[must_use]
    pub fn overflows_at_trigger(&self) -> bool {
        self.direction == SweepDirection::Increase
            && self.shift != 0
            && self.next_frequency().is_none()
    }

    /// Reloads the shadow frequency and timer from a hardware trigger
    /// (`mgba/src/gb/audio.c:182,863-867`); recheck
    /// [`Self::overflows_at_trigger`] after (`:184-186`).
    pub(crate) fn retrigger(&mut self, freq_reg: u16) {
        self.shadow_frequency = freq_reg.min(MAX_FREQUENCY_REGISTER);
        self.ticks_until_step = self.period_ticks;
    }

    /// Advances the sweep by one 128 Hz tick.
    pub fn tick(&mut self) -> SweepResult {
        if self.period_ticks == 0 {
            return SweepResult::Unchanged;
        }
        if self.ticks_until_step == 0 {
            self.ticks_until_step = self.period_ticks;
        }
        self.ticks_until_step -= 1;
        if self.ticks_until_step != 0 {
            return SweepResult::Unchanged;
        }
        self.ticks_until_step = self.period_ticks;

        let Some(frequency) = self.next_frequency() else {
            return SweepResult::Disable;
        };

        // The increase branch's write-back is gated on a non-zero shift; the
        // decrease branch always writes back (mgba/src/gb/audio.c:965-989).
        let writes_back = match self.direction {
            SweepDirection::Increase => self.shift != 0,
            SweepDirection::Decrease => true,
        };
        if !writes_back {
            return SweepResult::Unchanged;
        }

        self.shadow_frequency = frequency;

        // Hardware checks the next upward calculation before playing this one
        // (mgba/src/gb/audio.c:975-985).
        if self.direction == SweepDirection::Increase && self.next_frequency().is_none() {
            return SweepResult::Disable;
        }

        SweepResult::Changed(self.shadow_frequency)
    }
}

/// CGB channel 1 or 2 square-wave generator.
#[derive(Clone, Debug)]
pub struct SquareChannel {
    duty: SquareDuty,
    phase: u32,
    step_delta: u32,
    freq_reg: u16,
    sweep: Option<Sweep>,
    disabled_at_trigger: bool,
}

impl SquareChannel {
    /// Creates a square channel from its duty and frequency register values.
    /// The low two duty bits select the pattern; `sweep` is present only for channel 1.
    #[must_use]
    pub fn new(duty: u8, freq_reg: u16, sweep: Option<Sweep>) -> Self {
        let disabled_at_trigger = sweep.as_ref().is_some_and(Sweep::overflows_at_trigger);
        let mut chan = Self {
            duty: SquareDuty::from_register(duty),
            phase: 0,
            step_delta: 0,
            freq_reg: 0,
            sweep,
            disabled_at_trigger,
        };
        chan.set_frequency(freq_reg);
        chan
    }

    /// Reports whether the trigger-time sweep check disabled the channel.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.disabled_at_trigger
    }

    #[cfg(test)]
    pub(crate) fn sweep_frequency(&self) -> Option<u16> {
        self.sweep.as_ref().map(|s| s.shadow_frequency)
    }

    /// Retunes the channel from an 11-bit frequency register value.
    pub fn set_frequency(&mut self, freq_reg: u16) {
        self.freq_reg = freq_reg;
        let hz = register_frequency_hz(freq_reg, SQUARE_CLOCK_HZ);
        self.step_delta = phase_delta(hz, SQUARE_STEPS_PER_CYCLE);
    }

    /// Reloads the sweep shadow/timer from the current played frequency
    /// and reruns the overflow check (`mgba/src/gb/audio.c:180-186`).
    #[must_use]
    pub fn retrigger(&mut self) -> bool {
        let Some(sweep) = self.sweep.as_mut() else {
            return true;
        };
        sweep.retrigger(self.freq_reg);
        !sweep.overflows_at_trigger()
    }

    /// Advances channel 1's sweep, returning `false` when it disables the channel.
    pub fn step_sweep_tick(&mut self) -> bool {
        let Some(sweep) = self.sweep.as_mut() else {
            return true;
        };
        match sweep.tick() {
            SweepResult::Unchanged => true,
            SweepResult::Changed(freq) => {
                self.set_frequency(freq);
                true
            }
            SweepResult::Disable => false,
        }
    }

    /// Produces the next bipolar unit sample.
    pub fn sample(&mut self) -> i8 {
        let pattern = self.duty.pattern();
        let step = (self.phase / PHASE_ONE) as usize % pattern.len();
        self.phase = self.phase.wrapping_add(self.step_delta);
        if pattern[step] {
            1
        } else {
            -1
        }
    }
}

/// CGB channel 3 generator over 32 decoded wave-RAM samples.
///
/// Samples remain unscaled because M4A applies `gCgb3Vol` through `NR32`
/// (`pokeemerald/src/m4a.c:1205-1212`).
#[derive(Clone, Debug)]
pub struct WaveChannel {
    samples: [i8; WAVE_SAMPLES],
    phase: u32,
    step_delta: u32,
}

impl WaveChannel {
    /// Decodes two high-nibble-first samples per wave-RAM byte, centered on zero.
    #[must_use]
    pub fn decode_wave_ram(bytes: &[u8; WAVE_RAM_BYTES]) -> [i8; WAVE_SAMPLES] {
        let mut samples = [0i8; WAVE_SAMPLES];
        for (i, &byte) in bytes.iter().enumerate() {
            #[expect(clippy::cast_possible_wrap, reason = "a nibble fits in i8")]
            let high = (byte >> 4) as i8 - NIBBLE_ZERO;
            #[expect(clippy::cast_possible_wrap, reason = "a nibble fits in i8")]
            let low = (byte & 0x0F) as i8 - NIBBLE_ZERO;
            samples[i * 2] = high;
            samples[i * 2 + 1] = low;
        }
        samples
    }

    /// Creates a wave channel from decoded samples and a frequency register value.
    #[must_use]
    pub fn new(samples: [i8; WAVE_SAMPLES], freq_reg: u16) -> Self {
        let mut chan = Self {
            samples,
            phase: 0,
            step_delta: 0,
        };
        chan.set_frequency(freq_reg);
        chan
    }

    /// Retunes the channel from an 11-bit frequency register value.
    pub fn set_frequency(&mut self, freq_reg: u16) {
        let hz = register_frequency_hz(freq_reg, WAVE_CLOCK_HZ);
        self.step_delta = phase_delta(hz, WAVE_STEPS_PER_CYCLE);
    }

    /// Produces the next unscaled decoded sample in `-8..=7`.
    pub fn sample(&mut self) -> i8 {
        let step = (self.phase / PHASE_ONE) as usize % self.samples.len();
        self.phase = self.phase.wrapping_add(self.step_delta);
        self.samples[step]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LfsrWidth {
    FifteenBit,
    SevenBit,
}

impl LfsrWidth {
    const WIDE_FEEDBACK_BIT: u16 = 1 << 14;
    const NARROW_FEEDBACK_BIT: u16 = 1 << 6;

    fn feedback_bits(self) -> u16 {
        Self::WIDE_FEEDBACK_BIT
            | match self {
                Self::FifteenBit => 0,
                Self::SevenBit => Self::NARROW_FEEDBACK_BIT,
            }
    }
}

#[derive(Clone, Copy, Debug)]
struct NoiseControl {
    step_delta: u32,
    width: LfsrWidth,
}

impl NoiseControl {
    const DIVISOR_MASK: u8 = 0b111;
    const WIDTH_BIT: u8 = 1 << 3;
    const CLOCK_SHIFT: u32 = 4;

    fn from_byte(byte: u8) -> Self {
        let clock_shift = byte >> Self::CLOCK_SHIFT;
        let divisor_code = byte & Self::DIVISOR_MASK;
        let divisor = if divisor_code == 0 {
            0.5
        } else {
            f64::from(divisor_code)
        };
        let hz = NOISE_CLOCK_HZ / divisor / f64::from(1u32 << (clock_shift + 1));
        let width = if byte & Self::WIDTH_BIT == 0 {
            LfsrWidth::FifteenBit
        } else {
            LfsrWidth::SevenBit
        };
        Self {
            step_delta: phase_delta(hz, 1.0),
            width,
        }
    }
}

/// CGB channel 4 noise generator.
#[derive(Clone, Debug)]
pub struct NoiseChannel {
    lfsr: u16,
    width: LfsrWidth,
    phase: u32,
    step_delta: u32,
    output: i8,
}

impl NoiseChannel {
    /// Creates a retriggered noise channel from an `NR43` control byte.
    #[must_use]
    pub fn from_control_byte(byte: u8) -> Self {
        let control = NoiseControl::from_byte(byte);
        let mut chan = Self {
            lfsr: 0,
            width: control.width,
            phase: 0,
            step_delta: control.step_delta,
            output: -1,
        };
        chan.shift_lfsr();
        chan
    }

    /// Retunes the clock without resetting the LFSR or its trigger-time width.
    ///
    /// M4A preserves the `NR43` width bit during pitch writes
    /// (`pokeemerald/src/m4a.c:1197-1201`).
    pub fn retune(&mut self, byte: u8) {
        self.step_delta = NoiseControl::from_byte(byte).step_delta;
    }

    /// Resets the LFSR and clock phase, exactly as at note-on
    /// (`mgba/src/gb/audio.c:374,381-382`).
    pub fn retrigger(&mut self) {
        self.phase = 0;
        self.lfsr = 0;
        self.shift_lfsr();
    }

    fn shift_lfsr(&mut self) {
        let feedback_is_high = (self.lfsr ^ (self.lfsr >> 1)) & 1 == 0;
        let feedback_bits = self.width.feedback_bits();
        self.lfsr = (self.lfsr >> 1) & !feedback_bits;
        if feedback_is_high {
            self.lfsr |= feedback_bits;
        }
        self.output = if feedback_is_high { 1 } else { -1 };
    }

    #[cfg(test)]
    pub(crate) fn is_narrow(&self) -> bool {
        self.width == LfsrWidth::SevenBit
    }

    #[cfg(test)]
    pub(crate) fn lfsr(&self) -> u16 {
        self.lfsr
    }

    /// Produces the next bipolar sample, clocking the LFSR when its phase advances.
    pub fn sample(&mut self) -> i8 {
        self.phase = self.phase.wrapping_add(self.step_delta);
        while self.phase >= PHASE_ONE {
            self.phase -= PHASE_ONE;
            self.shift_lfsr();
        }
        self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF_DUTY_REGISTER: u8 = 2;
    const HIGH_FREQUENCY_REGISTER: u16 = 0x700;
    const LOOKAHEAD_OVERFLOW_FREQUENCY: u16 = 1046;
    const SEVEN_BIT_LFSR_PERIOD: usize = 127;

    fn sweep(
        period_ticks: u8,
        direction: SweepDirection,
        shift: u8,
        initial_frequency: u16,
    ) -> Sweep {
        let direction_bit = match direction {
            SweepDirection::Increase => 0,
            SweepDirection::Decrease => Sweep::DIRECTION_BIT,
        };
        let byte = (period_ticks << Sweep::PERIOD_SHIFT) | direction_bit | shift;
        Sweep::from_byte(byte, initial_frequency)
    }

    fn lfsr_repeats_within(mut noise: NoiseChannel, steps: usize) -> bool {
        noise.step_delta = PHASE_ONE;
        let initial_state = noise.lfsr;
        (0..steps).any(|_| {
            noise.sample();
            noise.lfsr == initial_state
        })
    }

    #[test]
    fn duty_pattern_matches_the_hardware_table() {
        let mut square = SquareChannel::new(HALF_DUTY_REGISTER, 0, None);
        square.step_delta = PHASE_ONE;
        let steps: Vec<bool> = (0..DUTY_TABLE[0].len())
            .map(|_| square.sample() > 0)
            .collect();
        assert_eq!(
            steps,
            vec![true, false, false, false, false, true, true, true]
        );
    }

    #[test]
    fn higher_frequency_register_raises_pitch() {
        let low = SquareChannel::new(HALF_DUTY_REGISTER, 0, None);
        let high = SquareChannel::new(HALF_DUTY_REGISTER, 1900, None);
        assert!(high.step_delta > low.step_delta);
    }

    #[test]
    fn sweep_up_raises_frequency_by_the_shift_formula() {
        let mut sweep = sweep(1, SweepDirection::Increase, 1, 100);
        assert_eq!(sweep.tick(), SweepResult::Changed(150));
        assert_eq!(sweep.tick(), SweepResult::Changed(225));
    }

    #[test]
    fn sweep_down_lowers_frequency() {
        let mut sweep = sweep(1, SweepDirection::Decrease, 1, 100);
        assert_eq!(sweep.tick(), SweepResult::Changed(50));
    }

    #[test]
    fn sweep_overflow_disables_the_channel() {
        let mut sweep = sweep(1, SweepDirection::Increase, 1, HIGH_FREQUENCY_REGISTER);
        assert_eq!(sweep.tick(), SweepResult::Disable);
    }

    #[test]
    fn sweep_disables_on_post_update_lookahead_overflow() {
        let mut sweep = sweep(1, SweepDirection::Increase, 1, LOOKAHEAD_OVERFLOW_FREQUENCY);
        assert_eq!(sweep.tick(), SweepResult::Disable);
    }

    #[test]
    fn square_channel_retires_on_lookahead_overflow() {
        let sweep = sweep(1, SweepDirection::Increase, 1, LOOKAHEAD_OVERFLOW_FREQUENCY);
        assert!(!sweep.overflows_at_trigger());
        let mut square = SquareChannel::new(
            HALF_DUTY_REGISTER,
            LOOKAHEAD_OVERFLOW_FREQUENCY,
            Some(sweep),
        );
        assert!(!square.is_disabled());
        assert!(!square.step_sweep_tick());
    }

    #[test]
    fn zero_period_sweep_never_fires() {
        let mut sweep = sweep(0, SweepDirection::Increase, 1, 100);
        for _ in 0..10 {
            assert_eq!(sweep.tick(), SweepResult::Unchanged);
        }
    }

    #[test]
    fn zero_shift_upward_sweep_ticks_without_changing_frequency() {
        let mut sweep = sweep(1, SweepDirection::Increase, 0, 100);
        assert_eq!(sweep.tick(), SweepResult::Unchanged);
    }

    #[test]
    fn zero_shift_downward_sweep_writes_back_to_zero() {
        let mut sweep = sweep(1, SweepDirection::Decrease, 0, 100);
        assert_eq!(sweep.tick(), SweepResult::Changed(0));
    }

    #[test]
    fn zero_shift_upward_sweep_disables_the_channel_when_the_doubling_overflows() {
        // A shift-0 upward sweep still computes `frequency + (frequency >> 0)`,
        // and 2048 or higher retires channel 1; only the write-back is gated on
        // a non-zero shift (mgba/src/gb/audio.c:965-990).
        let mut sweep = sweep(1, SweepDirection::Increase, 0, 1024);
        assert_eq!(sweep.tick(), SweepResult::Disable);
    }

    #[test]
    fn upward_sweep_overflow_disables_the_channel_at_trigger() {
        let period_zero = sweep(0, SweepDirection::Increase, 1, HIGH_FREQUENCY_REGISTER);
        assert!(period_zero.overflows_at_trigger());
        assert!(SquareChannel::new(
            HALF_DUTY_REGISTER,
            HIGH_FREQUENCY_REGISTER,
            Some(period_zero)
        )
        .is_disabled());

        let with_period = sweep(3, SweepDirection::Increase, 1, HIGH_FREQUENCY_REGISTER);
        assert!(with_period.overflows_at_trigger());
    }

    #[test]
    fn trigger_overflow_spares_normal_and_downward_sweeps() {
        let normal_frequency = 0x100;
        let normal = sweep(0, SweepDirection::Increase, 1, normal_frequency);
        assert!(!normal.overflows_at_trigger());
        assert!(
            !SquareChannel::new(HALF_DUTY_REGISTER, normal_frequency, Some(normal)).is_disabled()
        );

        let downward = sweep(0, SweepDirection::Decrease, 1, HIGH_FREQUENCY_REGISTER);
        assert!(!downward.overflows_at_trigger());

        let no_shift = sweep(0, SweepDirection::Increase, 0, MAX_FREQUENCY_REGISTER);
        assert!(!no_shift.overflows_at_trigger());
    }

    #[test]
    fn wave_ram_decodes_high_nibble_first() {
        let bytes = [0xF0, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let samples = WaveChannel::decode_wave_ram(&bytes);
        assert_eq!(&samples[..4], &[7, -8, -8, 0]);
    }

    #[test]
    fn wave_channel_emits_raw_decoded_nibbles() {
        let full_amplitude_wave = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(full_amplitude_wave, 0);
        wave.step_delta = 0;
        assert_eq!(wave.sample(), 7);
    }

    #[test]
    fn noise_narrow_mode_repeats_much_sooner_than_wide_mode() {
        let narrow = NoiseChannel::from_control_byte(NoiseControl::WIDTH_BIT);
        assert!(lfsr_repeats_within(narrow, SEVEN_BIT_LFSR_PERIOD));

        let wide = NoiseChannel::from_control_byte(0);
        assert!(!lfsr_repeats_within(wide, SEVEN_BIT_LFSR_PERIOD));
    }

    #[test]
    fn noise_channel_is_deterministic() {
        let mut a = NoiseChannel::from_control_byte(0x25);
        let mut b = NoiseChannel::from_control_byte(0x25);
        a.step_delta = PHASE_ONE;
        b.step_delta = PHASE_ONE;
        let seq_a: Vec<i8> = (0..32).map(|_| a.sample()).collect();
        let seq_b: Vec<i8> = (0..32).map(|_| b.sample()).collect();
        assert_eq!(seq_a, seq_b);
    }

    #[test]
    fn frame_sequencer_128hz_pins_the_hardware_tick_offsets() {
        let mut clock = FrameSequencer128Hz::default();
        let ticks = clock.advance(1200);
        assert_eq!(
            ticks,
            vec![104, 209, 313, 418, 522, 627, 731, 836, 940, 1045, 1149]
        );
        let spacings: Vec<usize> = ticks
            .windows(2)
            .map(|window| window[1] - window[0])
            .collect();
        let floor_spacing = usize::try_from(MIXER_RATE / FrameSequencer128Hz::TICK_HZ)
            .expect("sample spacing fits usize");
        assert!(
            spacings
                .iter()
                .all(|&spacing| spacing == floor_spacing || spacing == floor_spacing + 1),
            "unexpected tick spacing: {spacings:?}"
        );
    }

    #[test]
    fn frame_sequencer_128hz_does_not_drift_over_long_runs() {
        let one_second = usize::try_from(MIXER_RATE).expect("MIXER_RATE fits a usize");

        let mut one_second_clock = FrameSequencer128Hz::default();
        assert_eq!(one_second_clock.advance(one_second).len(), 128);

        let mut ten_second_clock = FrameSequencer128Hz::default();
        assert_eq!(ten_second_clock.advance(10 * one_second).len(), 1280);
    }

    #[test]
    fn frame_sequencer_128hz_chunk_boundary_invariance() {
        let mut whole = FrameSequencer128Hz::default();
        let whole_ticks = whole.advance(600);

        let mut split = FrameSequencer128Hz::default();
        let first = split.advance(300);
        let second = split.advance(300);
        let mut combined = first;
        combined.extend(second.into_iter().map(|t| t + 300));

        assert_eq!(whole_ticks, combined);
    }
}
