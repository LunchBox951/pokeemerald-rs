use super::common::MAX_FREQUENCY_REGISTER;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SweepDirection {
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
    pub(super) shadow_frequency: u16,
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

/// Shared fixtures for the sweep and square tests.
#[cfg(test)]
pub(super) mod test_support {
    use super::*;

    pub(in crate::psg) const HIGH_FREQUENCY_REGISTER: u16 = 0x700;
    pub(in crate::psg) const LOOKAHEAD_OVERFLOW_FREQUENCY: u16 = 1046;
    pub(in crate::psg) const LOOKAHEAD_OVERFLOW_INTERMEDIATE: u16 = 1569;

    pub(in crate::psg) fn sweep(
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
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

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
}
