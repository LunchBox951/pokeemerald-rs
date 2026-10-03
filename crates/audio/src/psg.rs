//! Sample-rate waveform generators for the four CGB programmable sound channels.

use crate::pitch::MIXER_RATE;

const PHASE_ONE: u32 = 1 << 16;
const FREQUENCY_REGISTER_RANGE: u16 = 1 << 11;
const MAX_FREQUENCY_REGISTER: u16 = FREQUENCY_REGISTER_RANGE - 1;
const FREQUENCY_LOW_BYTE: u16 = 0x00FF;
const FREQUENCY_HIGH_BITS: u16 = 0x0700;
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
    /// Silence the channel after a frequency overflow, first retuning to the
    /// frequency the sweep committed before its overflowing look-ahead
    /// (`mgba/src/gb/audio.c:975-985`).
    Disable(Option<u16>),
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
            return SweepResult::Disable(None);
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
            return SweepResult::Disable(Some(self.shadow_frequency));
        }

        SweepResult::Changed(self.shadow_frequency)
    }
}

/// Re-expresses the time since a square channel's last duty step, held as a
/// phase remainder accumulated at `from_delta` per sample, at `to_delta` per
/// sample, keeping the duty index. Whole steps that time now covers advance
/// the index (`mgba/src/gb/audio.c:493-510`).
fn retime_step_remainder(phase: u32, from_delta: u32, to_delta: u32) -> u32 {
    let index = phase & !(PHASE_ONE - 1);
    let remainder = u64::from(phase & (PHASE_ONE - 1));
    let retimed = (remainder * u64::from(to_delta))
        .checked_div(u64::from(from_delta))
        .unwrap_or(0);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the phase accumulator wraps modulo 2^32, a whole number of duty cycles"
    )]
    let phase = (u64::from(index) + retimed) as u32;
    phase
}

/// CGB channel 1 or 2 square-wave generator.
#[derive(Clone, Debug)]
pub struct SquareChannel {
    duty: SquareDuty,
    phase: u32,
    step_delta: u32,
    /// The played 11-bit frequency: `NR13`'s low byte and `NR14`'s high three
    /// bits, both of which a sweep write-back replaces
    /// (`mgba/src/gb/audio.c:160-171`, `:978-979`).
    frequency: u16,
    /// The high three bits a pitch write last left in `NR14`. A volume-only
    /// trigger rewrites `NR14` without `NR13`, so it rejoins these bits to
    /// whatever low byte the sweep has reached (`pokeemerald/src/m4a.c:1198-1203`,
    /// `:1219-1225`).
    note_high_bits: u16,
    sweep: Option<Sweep>,
    disabled_at_trigger: bool,
    /// Samples an idle slot has spent since `phase` was last settled: the
    /// hardware defers the duty catch-up to the next register write, so a
    /// sweep retuning meanwhile does not rate those samples
    /// ([`Self::defer_idle_samples`]'s doc).
    idle_samples: u32,
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
            frequency: 0,
            note_high_bits: 0,
            sweep,
            disabled_at_trigger,
            idle_samples: 0,
        };
        chan.set_frequency(freq_reg);
        chan
    }

    /// Reports whether the most recent trigger's sweep check disabled the channel.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.disabled_at_trigger
    }

    #[cfg(test)]
    pub(crate) fn sweep_frequency(&self) -> Option<u16> {
        self.sweep.as_ref().map(|s| s.shadow_frequency)
    }

    #[cfg(test)]
    pub(crate) fn duty_phase(&self) -> u32 {
        self.settled_phase()
    }

    /// The duty position once the deferred idle samples are rated at the
    /// current frequency.
    fn settled_phase(&self) -> u32 {
        self.phase
            .wrapping_add(self.step_delta.wrapping_mul(self.idle_samples))
    }

    /// Catches the duty position up over the deferred idle samples at the
    /// current frequency, as a register write does (`mgba/src/gb/audio.c:162-171`).
    fn settle_idle_samples(&mut self) {
        self.phase = self.settled_phase();
        self.idle_samples = 0;
    }

    /// Continues the duty position of `previous`, the note this one replaces
    /// on the same hardware slot: a restart keeps the duty index and the time
    /// since the last step (`mgba/src/gb/audio.c:168-194`, `:493-510`).
    pub(crate) fn continue_duty_from(&mut self, previous: &Self) {
        self.phase = retime_step_remainder(
            previous.settled_phase(),
            previous.step_delta,
            self.step_delta,
        );
        self.idle_samples = 0;
    }

    /// Records `samples` of an idle slot's silence without advancing the duty
    /// position. The disabled channel's catch-up is skipped at every frame and
    /// sweep tick (`GBAudioRun`'s `dead != 2` gate, `mgba/src/gb/audio.c:493-510`)
    /// and runs once at the next register write, at whatever frequency the
    /// sweep has reached by then (`:162-171`), not at each intermediate one.
    pub(crate) fn defer_idle_samples(&mut self, samples: usize) {
        let samples = u32::try_from(samples).unwrap_or(u32::MAX);
        self.idle_samples = self.idle_samples.wrapping_add(samples);
    }

    /// Advances the duty position through `samples` of silence. A disabled
    /// channel's index still catches up over that time at the next write to
    /// its registers, including the note-on of whichever note replaces it
    /// (`mgba/src/gb/audio.c:140-141,493-501`).
    pub(crate) fn advance_silently(&mut self, samples: usize) {
        let samples = u32::try_from(samples).unwrap_or(u32::MAX);
        self.phase = self
            .phase
            .wrapping_add(self.step_delta.wrapping_mul(samples));
    }

    /// Applies the off-write an idling slot's `NR14`/`NR24` restart makes:
    /// it clears the frequency register's high three bits, so a later
    /// catch-up rates against the low byte alone
    /// (`pokeemerald/src/m4a.c:857-868`, `mgba/src/gb/audio.c:168-171,219-222`).
    /// The write is also a trigger, so channel 1's sweep reloads from the
    /// truncated frequency and rechecks overflow (`mgba/src/gb/audio.c:168-186`);
    /// returns whether the channel still plays.
    pub(crate) fn apply_hardware_off_write(&mut self) -> bool {
        self.set_frequency(self.frequency & FREQUENCY_LOW_BYTE);
        self.retrigger()
    }

    /// Retunes the channel from an 11-bit frequency register value, as a pitch
    /// write does through both `NR13` and `NR14`
    /// (`pokeemerald/src/m4a.c:1198-1203`).
    pub fn set_frequency(&mut self, freq_reg: u16) {
        self.settle_idle_samples();
        let freq_reg = freq_reg.min(MAX_FREQUENCY_REGISTER);
        self.note_high_bits = freq_reg & FREQUENCY_HIGH_BITS;
        self.play_frequency(freq_reg);
    }

    /// Every frequency write catches the duty index up at the old rate and
    /// keeps the time since its last step, which the new rate then measures
    /// (`mgba/src/gb/audio.c:162-171,493-510,648-668`).
    fn play_frequency(&mut self, freq_reg: u16) {
        self.frequency = freq_reg;
        let hz = register_frequency_hz(freq_reg, SQUARE_CLOCK_HZ);
        let step_delta = phase_delta(hz, SQUARE_STEPS_PER_CYCLE);
        self.phase = retime_step_remainder(self.phase, self.step_delta, step_delta);
        self.step_delta = step_delta;
    }

    /// Applies a volume-only trigger: its `NR14` write restores the note's high
    /// bits over the swept low byte, and the sweep reloads from that rebuilt
    /// frequency and rechecks overflow (`mgba/src/gb/audio.c:170-186`).
    ///
    /// Returns whether the channel still plays.
    #[must_use]
    pub fn retrigger(&mut self) -> bool {
        self.settle_idle_samples();
        self.play_frequency(self.note_high_bits | (self.frequency & FREQUENCY_LOW_BYTE));
        let Some(sweep) = self.sweep.as_mut() else {
            return true;
        };
        sweep.retrigger(self.frequency);
        self.disabled_at_trigger = sweep.overflows_at_trigger();
        !self.disabled_at_trigger
    }

    /// Advances channel 1's sweep, returning `false` when it disables the channel.
    pub fn step_sweep_tick(&mut self) -> bool {
        let Some(sweep) = self.sweep.as_mut() else {
            return true;
        };
        match sweep.tick() {
            SweepResult::Unchanged => true,
            SweepResult::Changed(freq) => {
                self.play_frequency(freq);
                true
            }
            SweepResult::Disable(committed) => {
                if let Some(freq) = committed {
                    self.play_frequency(freq);
                }
                false
            }
        }
    }

    /// Produces the next bipolar unit sample.
    pub fn sample(&mut self) -> i8 {
        self.settle_idle_samples();
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

    /// Retunes the channel from an 11-bit frequency register value, keeping
    /// the sample index but starting a fresh period at the new rate
    /// (`mgba/src/gb/audio.c:331-335`).
    pub fn set_frequency(&mut self, freq_reg: u16) {
        let hz = register_frequency_hz(freq_reg, WAVE_CLOCK_HZ);
        self.step_delta = phase_delta(hz, WAVE_STEPS_PER_CYCLE);
        self.phase &= !(PHASE_ONE - 1);
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
    const LOOKAHEAD_OVERFLOW_INTERMEDIATE: u16 = 1569;
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
        assert_eq!(
            sweep.tick(),
            SweepResult::Disable(None),
            "a first-calculation overflow writes nothing back"
        );
    }

    #[test]
    fn sweep_disables_on_post_update_lookahead_overflow() {
        let mut sweep = sweep(1, SweepDirection::Increase, 1, LOOKAHEAD_OVERFLOW_FREQUENCY);
        assert_eq!(
            sweep.tick(),
            SweepResult::Disable(Some(LOOKAHEAD_OVERFLOW_INTERMEDIATE)),
            "the look-ahead disables only after hardware has already written the \
             non-overflowing intermediate frequency"
        );
    }

    #[test]
    fn lookahead_overflow_plays_the_committed_intermediate_before_disabling() {
        let sweep = sweep(1, SweepDirection::Increase, 1, LOOKAHEAD_OVERFLOW_FREQUENCY);
        assert!(!sweep.overflows_at_trigger());
        let mut square = SquareChannel::new(
            HALF_DUTY_REGISTER,
            LOOKAHEAD_OVERFLOW_FREQUENCY,
            Some(sweep),
        );
        assert!(!square.is_disabled());
        assert!(!square.step_sweep_tick());
        assert_eq!(
            square.frequency, LOOKAHEAD_OVERFLOW_INTERMEDIATE,
            "the disabling tick still leaves the intermediate frequency in NR13/NR14"
        );
        assert_eq!(
            square.sweep_frequency(),
            Some(LOOKAHEAD_OVERFLOW_INTERMEDIATE),
            "the shadow tracks the same write"
        );
    }

    #[test]
    fn a_volume_only_trigger_rejoins_the_note_high_bits_to_the_swept_low_byte() {
        // A volume write rewrites NR14 alone, so the note's high bits return
        // over the sweep's low byte rather than reloading the full swept
        // frequency (`pokeemerald/src/m4a.c:1198-1203,1219-1225`;
        // `mgba/src/gb/audio.c:170-171,182`).
        let sweep = sweep(1, SweepDirection::Increase, 1, LOOKAHEAD_OVERFLOW_FREQUENCY);
        let mut square = SquareChannel::new(
            HALF_DUTY_REGISTER,
            LOOKAHEAD_OVERFLOW_FREQUENCY,
            Some(sweep),
        );
        assert!(!square.step_sweep_tick());

        let rebuilt = (LOOKAHEAD_OVERFLOW_FREQUENCY & FREQUENCY_HIGH_BITS)
            | (LOOKAHEAD_OVERFLOW_INTERMEDIATE & FREQUENCY_LOW_BYTE);
        assert_eq!(rebuilt, 1057);
        assert!(
            square.retrigger(),
            "1057 + (1057 >> 1) stays under 2048, so this channel plays again"
        );
        assert_eq!(square.frequency, rebuilt);
        assert_eq!(square.sweep_frequency(), Some(rebuilt));
        assert!(!square.is_disabled());
    }

    #[test]
    fn a_volume_only_trigger_revives_a_note_that_a_full_reload_would_leave_muted() {
        // Reloading the whole swept frequency (1912) would overflow again;
        // only the NR13/NR14 split explains the note coming back.
        const NOTE: u16 = 1700;
        const SWEPT: u16 = 1912;
        let mut square = SquareChannel::new(
            HALF_DUTY_REGISTER,
            NOTE,
            Some(sweep(1, SweepDirection::Increase, 3, NOTE)),
        );
        assert!(!square.step_sweep_tick());
        assert_eq!(
            square.frequency, SWEPT,
            "1700 + (1700 >> 3) is written back"
        );
        const {
            assert!(
                SWEPT + (SWEPT >> 3) >= FREQUENCY_REGISTER_RANGE,
                "sanity: the look-ahead from 1912 is what disabled the channel"
            );
        }

        assert!(square.retrigger());
        assert_eq!(
            square.frequency,
            (NOTE & FREQUENCY_HIGH_BITS) | (SWEPT & FREQUENCY_LOW_BYTE)
        );
        assert_eq!(square.frequency, 1656);
        assert_eq!(square.sweep_frequency(), Some(1656));
    }

    #[test]
    fn a_volume_only_trigger_leaves_an_overflowing_channel_muted() {
        let mut square = SquareChannel::new(
            HALF_DUTY_REGISTER,
            HIGH_FREQUENCY_REGISTER,
            Some(sweep(
                1,
                SweepDirection::Increase,
                1,
                HIGH_FREQUENCY_REGISTER,
            )),
        );
        assert!(!square.step_sweep_tick());
        assert!(
            !square.retrigger(),
            "no sweep write reached NR13, so the trigger rebuilds the same \
             overflowing frequency"
        );
        assert!(square.is_disabled());
    }

    #[test]
    fn a_pitch_write_replaces_both_register_halves() {
        let mut square = SquareChannel::new(HALF_DUTY_REGISTER, 0x555, None);
        square.set_frequency(0x0AA);
        assert!(square.retrigger());
        assert_eq!(
            square.frequency, 0x0AA,
            "the later pitch write owns the high bits a trigger restores"
        );
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
        assert_eq!(sweep.tick(), SweepResult::Disable(None));
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

    fn samples_until_next_wave_step(mut chan: WaveChannel) -> u32 {
        let index = chan.phase / PHASE_ONE;
        (1..=PHASE_ONE)
            .find(|_| {
                chan.sample();
                chan.phase / PHASE_ONE != index
            })
            .expect("a wave channel always reaches its next step")
    }

    /// A retune mid-step keeps the sample index but discards the fraction
    /// of the old step already elapsed, so the new rate's first step lands
    /// one new period after the retune itself, not after the step it
    /// interrupted (`WaveChannel::set_frequency`'s doc).
    #[test]
    fn a_wave_retune_mid_step_lands_the_next_step_one_new_period_after_the_retune() {
        const FIRST_FREQUENCY: u16 = 0x400;
        const RETUNED_FREQUENCY: u16 = 0x200;
        const SAMPLES_BEFORE_RETUNE: u32 = 3;

        let wave_ram = WaveChannel::decode_wave_ram(&[0xF0; WAVE_RAM_BYTES]);
        let mut wave = WaveChannel::new(wave_ram, FIRST_FREQUENCY);
        // Walk into the middle of a step so a remainder is actually carried.
        while wave.phase / PHASE_ONE == 0 {
            wave.sample();
        }
        let last_step_index = wave.phase / PHASE_ONE;
        for _ in 0..SAMPLES_BEFORE_RETUNE {
            wave.sample();
        }
        assert_eq!(
            wave.phase / PHASE_ONE,
            last_step_index,
            "the retune case must stay within the same step",
        );

        wave.set_frequency(RETUNED_FREQUENCY);
        assert_eq!(
            wave.phase % PHASE_ONE,
            0,
            "the retune must discard the elapsed fraction of the old step",
        );
        let retuned_period = f64::from(PHASE_ONE) / f64::from(wave.step_delta);

        let next_step = f64::from(samples_until_next_wave_step(wave));
        assert!(
            (next_step - retuned_period).abs() <= 1.0,
            "the retuned wave steps after {next_step} samples, expected about {retuned_period}",
        );
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

    fn samples_until_next_duty_step(mut chan: SquareChannel) -> u32 {
        let index = chan.phase / PHASE_ONE;
        (1..=PHASE_ONE)
            .find(|_| {
                chan.sample();
                chan.phase / PHASE_ONE != index
            })
            .expect("a square channel always reaches its next duty step")
    }

    /// A note replacing another at a different pitch keeps the duty index and
    /// the time elapsed since its last step, so its first step lands one new
    /// period after the outgoing note's last step
    /// (`SquareChannel::continue_duty_from`'s doc).
    #[test]
    fn a_replacement_at_another_pitch_steps_one_new_period_after_the_last_step() {
        const OUTGOING_FREQUENCY: u16 = 0x700;
        const REPLACEMENT_FREQUENCY: u16 = 0x000;
        const ELAPSED_SAMPLES: u32 = 2;

        let mut outgoing = SquareChannel::new(HALF_DUTY_REGISTER, OUTGOING_FREQUENCY, None);
        // Walk into the middle of a step so a remainder is actually carried.
        while outgoing.phase / PHASE_ONE == 0 {
            outgoing.sample();
        }
        let last_step_index = outgoing.phase / PHASE_ONE;
        for _ in 0..ELAPSED_SAMPLES {
            outgoing.sample();
        }
        assert_eq!(outgoing.phase / PHASE_ONE, last_step_index);

        let mut replacement = SquareChannel::new(HALF_DUTY_REGISTER, REPLACEMENT_FREQUENCY, None);
        let replacement_period = samples_until_next_duty_step(replacement.clone());
        replacement.continue_duty_from(&outgoing);

        assert_eq!(
            replacement.phase / PHASE_ONE,
            last_step_index,
            "the duty index must carry over",
        );
        let first_step = samples_until_next_duty_step(replacement);
        let expected = replacement_period - ELAPSED_SAMPLES;
        assert!(
            first_step.abs_diff(expected) <= 1,
            "first step after {first_step} samples, expected about {expected}",
        );
    }

    /// A higher replacement whose period is shorter than the time elapsed
    /// since the outgoing note's last step advances the index by every whole
    /// new period that time covers, as mGBA's catch-up does
    /// (`SquareChannel::continue_duty_from`'s doc).
    #[test]
    fn a_higher_replacement_advances_the_index_by_the_new_periods_already_elapsed() {
        const OUTGOING_FREQUENCY: u16 = 0x000;
        const REPLACEMENT_FREQUENCY: u16 = 0x700;
        const ELAPSED_SAMPLES: u32 = 13;

        let mut outgoing = SquareChannel::new(HALF_DUTY_REGISTER, OUTGOING_FREQUENCY, None);
        while outgoing.phase / PHASE_ONE == 0 {
            outgoing.sample();
        }
        let last_step_index = outgoing.phase / PHASE_ONE;
        let overshoot = outgoing.phase % PHASE_ONE;
        for _ in 0..ELAPSED_SAMPLES {
            outgoing.sample();
        }
        assert_eq!(outgoing.phase / PHASE_ONE, last_step_index);

        let mut replacement = SquareChannel::new(HALF_DUTY_REGISTER, REPLACEMENT_FREQUENCY, None);
        let replacement_period = f64::from(PHASE_ONE) / f64::from(replacement.step_delta);
        let elapsed =
            f64::from(ELAPSED_SAMPLES) + f64::from(overshoot) / f64::from(outgoing.step_delta);
        assert!(
            elapsed > 2.0 * replacement_period,
            "the case must cover whole replacement periods",
        );
        replacement.continue_duty_from(&outgoing);

        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a handful of whole periods"
        )]
        let covered_steps = (elapsed / replacement_period).floor() as u32;
        assert_eq!(
            replacement.phase / PHASE_ONE,
            last_step_index + covered_steps,
            "the index advances by each whole new period already elapsed",
        );
        let first_step = f64::from(samples_until_next_duty_step(replacement));
        let expected = replacement_period - elapsed % replacement_period;
        assert!(
            (first_step - expected).abs() <= 1.0,
            "next step after {first_step} samples, expected about {expected}",
        );
    }

    /// A retune keeps the time since the last duty step, so a later
    /// replacement measures the whole time, spent at both rates, against its
    /// own period (`SquareChannel::play_frequency`'s doc).
    #[test]
    fn a_replacement_after_a_retune_measures_the_time_spent_at_both_rates() {
        const FIRST_FREQUENCY: u16 = 0x400;
        const RETUNED_FREQUENCY: u16 = 0x200;
        const REPLACEMENT_FREQUENCY: u16 = 0x000;
        const SAMPLES_BEFORE_RETUNE: u32 = 10;
        const SAMPLES_AFTER_RETUNE: u32 = 5;

        let mut outgoing = SquareChannel::new(HALF_DUTY_REGISTER, FIRST_FREQUENCY, None);
        while outgoing.phase / PHASE_ONE == 0 {
            outgoing.sample();
        }
        let last_step_index = outgoing.phase / PHASE_ONE;
        let overshoot = f64::from(outgoing.phase % PHASE_ONE) / f64::from(outgoing.step_delta);
        for _ in 0..SAMPLES_BEFORE_RETUNE {
            outgoing.sample();
        }
        outgoing.set_frequency(RETUNED_FREQUENCY);
        for _ in 0..SAMPLES_AFTER_RETUNE {
            outgoing.sample();
        }
        assert_eq!(outgoing.phase / PHASE_ONE, last_step_index);
        let elapsed =
            overshoot + f64::from(SAMPLES_BEFORE_RETUNE) + f64::from(SAMPLES_AFTER_RETUNE);

        let retuned_period = f64::from(PHASE_ONE) / f64::from(outgoing.step_delta);
        let outgoing_next = f64::from(samples_until_next_duty_step(outgoing.clone()));
        assert!(
            (outgoing_next - (retuned_period - elapsed)).abs() <= 1.0,
            "the retuned note steps after {outgoing_next} samples, expected about {}",
            retuned_period - elapsed,
        );

        let mut replacement = SquareChannel::new(HALF_DUTY_REGISTER, REPLACEMENT_FREQUENCY, None);
        let replacement_period = f64::from(PHASE_ONE) / f64::from(replacement.step_delta);
        replacement.continue_duty_from(&outgoing);
        assert_eq!(replacement.phase / PHASE_ONE, last_step_index);
        let first_step = f64::from(samples_until_next_duty_step(replacement));
        assert!(
            (first_step - (replacement_period - elapsed)).abs() <= 1.0,
            "the replacement steps after {first_step} samples, expected about {}",
            replacement_period - elapsed,
        );
    }

    /// The off-write rates the channel as if only its low frequency byte
    /// survived, without itself moving the duty index
    /// (`SquareChannel::apply_hardware_off_write`'s doc).
    #[test]
    fn the_hardware_off_write_truncates_the_frequency_to_its_low_byte() {
        let mut square = SquareChannel::new(HALF_DUTY_REGISTER, HIGH_FREQUENCY_REGISTER, None);
        while square.phase / PHASE_ONE == 0 {
            square.sample();
        }
        let index_before = square.phase / PHASE_ONE;

        square.apply_hardware_off_write();

        assert_eq!(
            square.phase / PHASE_ONE,
            index_before,
            "the off-write must not itself move the duty index",
        );
        let truncated = SquareChannel::new(
            HALF_DUTY_REGISTER,
            HIGH_FREQUENCY_REGISTER & FREQUENCY_LOW_BYTE,
            None,
        );
        assert_eq!(
            square.step_delta, truncated.step_delta,
            "the off-write must rate the channel as if only its low frequency byte survived",
        );
    }
}
