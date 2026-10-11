use super::common::{phase_delta, PHASE_ONE};

const NOISE_CLOCK_HZ: f64 = 524_288.0;

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
///
/// Besides the LFSR it holds the channel's output latch, `ch4.sample`: the
/// volume-resolved level (0..=15) the last LFSR clock or envelope step left
/// (`mgba/src/gb/audio.c:641,730-732`). Triggers, retunes, and direct volume
/// writes never rewrite it.
#[derive(Clone, Debug)]
pub struct NoiseChannel {
    lfsr: u16,
    width: LfsrWidth,
    /// Elapsed time encoded at `step_delta`; after a retune it can hold whole
    /// periods owed to the next clock-enabled sample.
    phase: u64,
    step_delta: u32,
    output: i8,
    level: u8,
    /// The volume the latch was last resolved against: the render path's
    /// DC centre, rewritten only where the latch itself is.
    centre: u8,
    clocking: bool,
}

impl NoiseChannel {
    /// Creates a retriggered noise channel from an `NR43` control byte.
    #[must_use]
    pub fn from_control_byte(byte: u8) -> Self {
        let control = NoiseControl::from_byte(byte);
        Self {
            lfsr: 0,
            width: control.width,
            phase: 0,
            step_delta: control.step_delta,
            output: -1,
            level: 0,
            centre: 0,
            clocking: true,
        }
    }

    /// Retunes the clock without resetting the LFSR or its trigger-time width.
    ///
    /// M4A preserves the `NR43` width bit during pitch writes
    /// (`pokeemerald/src/m4a.c:1197-1201`).
    ///
    /// mGBA settles the old rate before an `NR43` write and keeps `lastEvent`,
    /// so the new period is measured against the clock time already elapsed
    /// (`mgba/src/gb/audio.c:354-358,602-607,630-644`). The phase is therefore
    /// rescaled by the rate ratio, whole periods included; the next
    /// clock-enabled sample, or a trigger first ([`Self::settle_owed_clocks`]),
    /// clocks them, so the retune itself leaves the latch untouched.
    pub fn retune(&mut self, byte: u8) {
        let step_delta = NoiseControl::from_byte(byte).step_delta;
        self.phase = (self.phase * u64::from(step_delta))
            .checked_div(u64::from(self.step_delta))
            .unwrap_or(0);
        self.step_delta = step_delta;
    }

    /// Clocks the whole periods a retune left owed, at hardware `volume`.
    ///
    /// mGBA runs channel 4 before each `NR42`/`NR44` write
    /// (`mgba/src/gb/audio.c:347,362,602-644`), so a trigger following a
    /// retune first clocks every whole new-rate period already elapsed, and
    /// the latch they leave survives the trigger. A stopped channel owes
    /// nothing (`audio.c:585`).
    pub fn settle_owed_clocks(&mut self, volume: u8) {
        if !self.clocking {
            return;
        }
        while self.phase >= u64::from(PHASE_ONE) {
            self.phase -= u64::from(PHASE_ONE);
            self.shift_lfsr(volume);
        }
    }

    /// Resets the LFSR and clock phase, exactly as at note-on
    /// (`mgba/src/gb/audio.c:374,381-382`). The output latch is untouched.
    pub fn retrigger(&mut self) {
        self.phase = 0;
        self.lfsr = 0;
    }

    /// The retirement off-write (`NR42 = 8; NR44 = 0x80`, `m4a.c:873-874`):
    /// restarts the LFSR and phase and leaves the channel clocking, keeping a
    /// raised latch for the first clock to settle. A latch already at zero
    /// renders the same zero level at every volume, so its DC centre drops
    /// with the volume rather than inventing a settling edge at the next
    /// clock (`mgba/src/gb/audio.c:371-383,641`).
    pub fn off_write(&mut self) {
        self.retrigger();
        self.clocking = true;
        if self.level == 0 {
            self.centre = 0;
        }
    }

    /// Keeps the previous note's output latch across a trigger: `ch4.sample`
    /// is only rewritten when the LFSR clocks or the envelope steps, so the
    /// predecessor's resolved level persists into the next note until then
    /// (`mgba/src/gb/audio.c:371-383,602-644`). The level is carried as
    /// resolved, whatever the predecessor's current hardware volume: a
    /// volume write or zero-volume trigger does not recompute it.
    pub fn continue_output_from(&mut self, previous: &Self) {
        self.level = previous.level;
        self.centre = previous.centre;
    }

    /// Re-resolves the latch at an envelope step: the sample is rewritten as
    /// `(sample > 0) * currentVolume` (`mgba/src/gb/audio.c:730-732`), so a
    /// raised latch follows the stepped volume while a low one stays low.
    pub fn apply_envelope_step(&mut self, volume: u8) {
        // The step rewrites `ch4.sample = (sample > 0) * currentVolume`
        // (`mgba/src/gb/audio.c:730-732`), so the centre follows every step
        // while a low latch stays at zero.
        self.centre = volume & 0x0F;
        if self.level > 0 {
            self.level = self.centre;
        }
    }

    /// Whether the last trigger left the channel running. A trigger with
    /// initial volume zero and a decreasing envelope disables it
    /// (`mgba/src/gb/audio.c:371-372,856-860`): the LFSR stops clocking
    /// (`audio.c:585`) while the held latch is still output (`audio.c:782`).
    pub fn set_clocking(&mut self, clocking: bool) {
        self.clocking = clocking;
    }

    /// The signed render level, `2 * level - centre`: the latch measured from
    /// the volume it was resolved against, so it is unchanged by anything but
    /// the writes that rewrite the latch itself.
    #[must_use]
    pub fn centred_level(&self) -> i32 {
        2 * i32::from(self.level) - i32::from(self.centre)
    }

    /// Whether the held latch still renders a nonzero level that a clock
    /// has yet to settle.
    #[must_use]
    pub fn has_unsettled_output(&self) -> bool {
        self.level != 0 || self.centre != 0
    }

    /// The resolved output latch, 0..=15.
    #[must_use]
    pub fn level(&self) -> u8 {
        self.level
    }

    fn shift_lfsr(&mut self, volume: u8) {
        let feedback_is_high = (self.lfsr ^ (self.lfsr >> 1)) & 1 == 0;
        let feedback_bits = self.width.feedback_bits();
        self.lfsr = (self.lfsr >> 1) & !feedback_bits;
        if feedback_is_high {
            self.lfsr |= feedback_bits;
        }
        self.output = if feedback_is_high { 1 } else { -1 };
        // `ch4.sample = lsb * currentVolume` (`mgba/src/gb/audio.c:641`).
        self.level = if feedback_is_high { volume & 0x0F } else { 0 };
        self.centre = volume & 0x0F;
    }

    #[cfg(test)]
    pub(crate) fn is_narrow(&self) -> bool {
        self.width == LfsrWidth::SevenBit
    }

    #[cfg(test)]
    pub(crate) fn lfsr(&self) -> u16 {
        self.lfsr
    }

    /// Advances one output sample at hardware `volume`, clocking the LFSR
    /// whenever its phase wraps, and returns the resolved latch (0..=15).
    pub fn clock_sample(&mut self, volume: u8) -> u8 {
        if !self.clocking {
            return self.level;
        }
        self.phase += u64::from(self.step_delta);
        while self.phase >= u64::from(PHASE_ONE) {
            self.phase -= u64::from(PHASE_ONE);
            self.shift_lfsr(volume);
        }
        self.level
    }

    /// Produces the next bipolar sample, clocking the LFSR when its phase advances.
    pub fn sample(&mut self) -> i8 {
        self.clock_sample(NOISE_UNIT_VOLUME);
        self.output
    }
}

const NOISE_UNIT_VOLUME: u8 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    const SEVEN_BIT_LFSR_PERIOD: usize = 127;

    fn lfsr_repeats_within(mut noise: NoiseChannel, steps: usize) -> bool {
        noise.step_delta = PHASE_ONE;
        noise.sample();
        let initial_state = noise.lfsr;
        (0..steps).any(|_| {
            noise.sample();
            noise.lfsr == initial_state
        })
    }

    #[test]
    fn noise_retunes_preserve_elapsed_time_and_the_next_clock() {
        // `mgba/src/gb/audio.c:354-358,602-607,630-644`: `lastEvent` survives
        // an `NR43` write, so elapsed time (not a period fraction) carries over.
        for (old, new, samples, old_phase, scaled_phase, next_clock, final_phase) in [
            (0x65, 0x75, 26, 38_776, 19_388, 24, 1_996),
            (0x75, 0x65, 43, 20_722, 41_444, 7, 3_992),
        ] {
            let mut noise = NoiseChannel::from_control_byte(old);
            for _ in 0..samples {
                noise.clock_sample(11);
            }
            assert_eq!((noise.lfsr(), noise.phase), (0x4000, old_phase));
            let held = (
                noise.lfsr,
                noise.width,
                noise.output,
                noise.level,
                noise.centre,
                noise.clocking,
            );

            noise.retune(new);
            assert_eq!(noise.phase, scaled_phase);
            assert_eq!(
                (
                    noise.lfsr,
                    noise.width,
                    noise.output,
                    noise.level,
                    noise.centre,
                    noise.clocking
                ),
                held,
            );
            for _ in 1..next_clock {
                assert_eq!(noise.clock_sample(7), 11);
                assert_eq!(noise.lfsr(), 0x4000);
            }
            assert_eq!(noise.clock_sample(7), 7);
            assert_eq!((noise.lfsr(), noise.phase), (0x6000, final_phase));
        }
    }

    #[test]
    fn noise_retune_keeps_whole_periods_owed_until_the_next_clock() {
        let mut noise = NoiseChannel::from_control_byte(0x75);
        for _ in 0..30 {
            noise.clock_sample(11);
        }
        assert_eq!((noise.lfsr(), noise.phase), (0, 60_180));
        noise.retune(0x5D);
        assert_eq!(noise.phase, 240_750);
        assert_eq!(noise.width, LfsrWidth::FifteenBit);

        noise.clocking = false;
        noise.clock_sample(0);
        assert_eq!((noise.lfsr(), noise.phase), (0, 240_750));
        noise.clocking = true;
        assert_eq!(noise.clock_sample(7), 7);
        assert_eq!((noise.lfsr(), noise.phase), (0x7000, 52_167));
    }

    #[test]
    fn noise_narrow_mode_repeats_much_sooner_than_wide_mode() {
        let narrow = NoiseChannel::from_control_byte(NoiseControl::WIDTH_BIT);
        assert!(lfsr_repeats_within(narrow, SEVEN_BIT_LFSR_PERIOD));

        let wide = NoiseChannel::from_control_byte(0);
        assert!(!lfsr_repeats_within(wide, SEVEN_BIT_LFSR_PERIOD));
    }

    #[test]
    fn noise_construction_does_not_clock_before_a_full_period() {
        for (byte, first_lfsr) in [(0, 0x4000), (NoiseControl::WIDTH_BIT, 0x4040)] {
            let mut noise = NoiseChannel::from_control_byte(byte);
            assert_eq!((noise.lfsr(), noise.phase, noise.output), (0, 0, -1));
            noise.step_delta = PHASE_ONE / 2;
            assert_eq!(noise.sample(), -1);
            assert_eq!(noise.lfsr(), 0);
            assert_eq!(noise.sample(), 1);
            assert_eq!((noise.lfsr(), noise.phase), (first_lfsr, 0));
        }
    }

    #[test]
    fn a_noise_clock_resolves_the_latch_to_the_current_volume() {
        for byte in [0, NoiseControl::WIDTH_BIT] {
            let mut noise = NoiseChannel::from_control_byte(byte);
            assert_eq!(noise.level(), 0, "construction latches low");
            while noise.level() == 0 {
                noise.clock_sample(11);
            }
            assert_eq!(noise.level(), 11);
        }
    }

    #[test]
    fn a_disabled_channel_holds_its_latch_without_clocking() {
        let mut noise = NoiseChannel::from_control_byte(0);
        while noise.level() == 0 {
            noise.clock_sample(9);
        }
        noise.retrigger();
        noise.set_clocking(false);
        for _ in 0..10_000 {
            assert_eq!(noise.clock_sample(0), 9);
        }
        assert_eq!(noise.lfsr(), 0);
        noise.set_clocking(true);
        noise.retrigger();
        while noise.clock_sample(0) != 0 {}
    }

    #[test]
    fn a_trigger_and_a_volume_change_leave_the_resolved_latch_alone() {
        let mut noise = NoiseChannel::from_control_byte(0);
        while noise.level() == 0 {
            noise.clock_sample(9);
        }
        noise.retrigger();
        assert_eq!(noise.level(), 9);
        let mut successor = NoiseChannel::from_control_byte(0x25);
        successor.continue_output_from(&noise);
        assert_eq!(successor.level(), 9);
        // The first sample after the trigger (no clock yet) still reads it.
        assert_eq!(successor.clock_sample(0), 9);
    }

    #[test]
    fn a_zero_volume_clock_settles_the_latch_and_a_step_cannot_raise_it() {
        let mut noise = NoiseChannel::from_control_byte(0);
        while noise.level() == 0 {
            noise.clock_sample(9);
        }
        noise.apply_envelope_step(7);
        assert_eq!(noise.level(), 7, "a raised latch follows the step");
        noise.retrigger();
        let mut samples = 0;
        while noise.clock_sample(0) != 0 || samples == 0 {
            samples += 1;
            assert!(samples < 1000, "the first zero-volume clock settles it");
        }
        noise.apply_envelope_step(12);
        assert_eq!(noise.level(), 0, "a low latch stays low through a step");
    }

    #[test]
    fn an_envelope_step_recentres_a_low_latch_at_the_stepped_volume() {
        let mut noise = NoiseChannel::from_control_byte(0);
        while noise.level() == 0 {
            noise.clock_sample(9);
        }
        while noise.level() != 0 {
            noise.clock_sample(9);
        }
        assert_eq!(noise.centred_level(), -9, "sanity: low at volume 9");
        noise.apply_envelope_step(5);
        assert_eq!(noise.level(), 0, "the level stays zero");
        assert_eq!(noise.centred_level(), -5, "the centre follows the step");
    }

    #[test]
    fn noise_retrigger_resets_without_clocking_and_keeps_the_output_latch() {
        for byte in [0, NoiseControl::WIDTH_BIT] {
            for latch in [-1i8, 1] {
                let mut noise = NoiseChannel::from_control_byte(byte);
                noise.step_delta = PHASE_ONE * 3 / 4;
                while noise.lfsr() == 0 || noise.output != latch || noise.phase == 0 {
                    noise.sample();
                }
                let (width, delta) = (noise.width, noise.step_delta);
                noise.retrigger();
                assert_eq!((noise.lfsr(), noise.phase), (0, 0));
                assert_eq!(
                    (noise.width, noise.step_delta, noise.output),
                    (width, delta, latch)
                );
                assert_eq!(noise.sample(), latch);
                assert_eq!(noise.lfsr(), 0);
                assert_eq!(noise.sample(), 1);
                assert_ne!(noise.lfsr(), 0);
            }
        }
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
}
