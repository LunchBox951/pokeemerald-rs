use super::common::{phase_delta, register_frequency_hz, MAX_FREQUENCY_REGISTER, PHASE_ONE};
use super::sweep::{Sweep, SweepResult};

const FREQUENCY_LOW_BYTE: u16 = 0x00FF;
const FREQUENCY_HIGH_BITS: u16 = 0x0700;
const SQUARE_CLOCK_HZ: f64 = 131_072.0;
const SQUARE_STEPS_PER_CYCLE: f64 = 8.0;

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
    /// The step rate `phase`'s fractional remainder was encoded at when the
    /// current deferral began. While set, sweep retunes leave `phase`
    /// untouched (the hardware keeps the index and last-update time while
    /// dead, `mgba/src/gb/audio.c:493-503,975-979`) and
    /// [`Self::settled_phase`] retimes it once at the final rate.
    idle_step_delta: Option<u32>,
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
            idle_step_delta: None,
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
        self.idle_step_delta
            .map_or(self.phase, |from_delta| {
                retime_step_remainder(self.phase, from_delta, self.step_delta)
            })
            .wrapping_add(self.step_delta.wrapping_mul(self.idle_samples))
    }

    /// Catches the duty position up over the deferred idle samples at the
    /// current frequency, as a register write does (`mgba/src/gb/audio.c:162-171`).
    fn settle_idle_samples(&mut self) {
        self.phase = self.settled_phase();
        self.idle_samples = 0;
        self.idle_step_delta = None;
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
        self.idle_step_delta = None;
    }

    /// Records `samples` of an idle slot's silence without advancing the duty
    /// position. The disabled channel's catch-up is skipped at every frame and
    /// sweep tick (`GBAudioRun`'s `dead != 2` gate, `mgba/src/gb/audio.c:493-510`)
    /// and runs once at the next register write, at whatever frequency the
    /// sweep has reached by then (`:162-171`), not at each intermediate one.
    pub(crate) fn defer_idle_samples(&mut self, samples: usize) {
        let samples = u32::try_from(samples).unwrap_or(u32::MAX);
        self.idle_step_delta.get_or_insert(self.step_delta);
        self.idle_samples = self.idle_samples.wrapping_add(samples);
    }

    /// Advances the duty position through `samples` of silence. A disabled
    /// channel's index still catches up over that time at the next write to
    /// its registers, including the note-on of whichever note replaces it
    /// (`mgba/src/gb/audio.c:140-141,493-501`).
    pub(crate) fn advance_silently(&mut self, samples: usize) {
        self.settle_idle_samples();
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
        if self.idle_step_delta.is_none() {
            self.phase = retime_step_remainder(self.phase, self.step_delta, step_delta);
        }
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

#[cfg(test)]
mod tests {
    use super::super::common::{FREQUENCY_REGISTER_RANGE, MAX_FREQUENCY_REGISTER};
    use super::super::sweep::{test_support::*, SweepDirection};
    use super::*;

    const HALF_DUTY_REGISTER: u8 = 2;

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

#[cfg(test)]
mod deferred_idle_settlement_tests {
    use super::*;

    const DEFERRED: usize = 37;

    fn pair() -> (SquareChannel, SquareChannel) {
        let mut deferred = SquareChannel::new(2, 0x400, None);
        let mut eager = deferred.clone();
        deferred.defer_idle_samples(DEFERRED);
        eager.advance_silently(DEFERRED);
        (deferred, eager)
    }

    #[test]
    fn a_pitch_write_settles_deferred_silence_at_the_old_frequency() {
        let (mut deferred, mut eager) = pair();
        deferred.set_frequency(0x700);
        eager.set_frequency(0x700);
        assert_eq!(deferred.idle_samples, 0);
        assert_eq!(deferred.phase, eager.phase);
    }

    #[test]
    fn a_trigger_settles_deferred_silence_at_the_old_frequency() {
        let (mut deferred, mut eager) = pair();
        let _ = deferred.retrigger();
        let _ = eager.retrigger();
        assert_eq!(deferred.idle_samples, 0);
        assert_eq!(deferred.phase, eager.phase);
    }

    #[test]
    fn the_first_audible_sample_settles_deferred_silence() {
        let (mut deferred, mut eager) = pair();
        let a: Vec<i8> = (0..64).map(|_| deferred.sample()).collect();
        let b: Vec<i8> = (0..64).map(|_| eager.sample()).collect();
        assert_eq!(a, b);
        assert_eq!(deferred.idle_samples, 0);
        assert_eq!(deferred.phase, eager.phase);
    }

    #[test]
    fn silent_sweep_ticks_rate_the_inherited_remainder_at_the_final_frequency() {
        const FIRST_FREQUENCY: u16 = 0x400;
        const FINAL_FREQUENCY: u16 = 0x640;
        const FIRST_SPAN: usize = 8;
        const SECOND_SPAN: usize = 9;
        const TOTAL_SAMPLES: u32 = 17;

        let sweep = Sweep::from_byte((1 << 4) | 2, FIRST_FREQUENCY);
        let mut square = SquareChannel::new(2, FIRST_FREQUENCY, Some(sweep));
        square.phase = PHASE_ONE - 1;
        let inherited_phase = square.phase;
        let inherited_delta = square.step_delta;

        square.defer_idle_samples(FIRST_SPAN);
        assert!(square.step_sweep_tick());
        assert_eq!(square.frequency, 0x500);
        square.defer_idle_samples(SECOND_SPAN);
        assert!(square.step_sweep_tick());
        assert_eq!(square.frequency, FINAL_FREQUENCY);

        let expected = retime_step_remainder(inherited_phase, inherited_delta, square.step_delta)
            .wrapping_add(square.step_delta.wrapping_mul(TOTAL_SAMPLES));
        assert_eq!(expected / PHASE_ONE, 5);
        assert_eq!(square.duty_phase() / PHASE_ONE, expected / PHASE_ONE);
        assert_eq!(square.duty_phase(), expected);

        square.settle_idle_samples();
        assert_eq!(square.idle_samples, 0);
        assert_eq!(square.phase, expected);
        let _ = square.sample();
        assert_eq!(square.phase, expected.wrapping_add(square.step_delta));
    }
}
