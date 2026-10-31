//! The 64-step sound-length counter of CGB channels 1, 2 and 4.
//!
//! The counter is a register-write machine: `NRx1` loads it, `NRx4` bit 6
//! enables it and bit 7 triggers, and the frame sequencer's even steps clock
//! it (`mgba/src/gb/audio.c:147-151,168-197,649-707`).

/// What an `NRx4` write did to the channel's playing state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthWrite {
    /// The write left the playing state alone.
    Unchanged,
    /// The write's extra clock took the counter to zero and stopped the channel.
    Expired,
    /// The write triggered the channel, which plays again.
    Triggered,
}

/// A sound-length counter and its `NRx4` length-enable bit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LengthCounter {
    remaining: u8,
    enabled: bool,
}

impl LengthCounter {
    /// The longest a counter runs, in 256 Hz clocks.
    pub const MAX: u8 = 64;
    const REGISTER_MASK: u8 = Self::MAX - 1;

    /// Steps left before the channel stops.
    #[must_use]
    pub fn remaining(self) -> u8 {
        self.remaining
    }

    /// Whether `NRx4` bit 6 is set.
    #[must_use]
    pub fn is_enabled(self) -> bool {
        self.enabled
    }

    /// An `NRx1` write: the low six bits load `64 - length` and a zero field
    /// loads the full 64 (`mgba/src/gb/audio.c:148-151`). It neither enables
    /// nor triggers the channel.
    pub fn load(&mut self, register: u8) {
        self.remaining = Self::MAX - (register & Self::REGISTER_MASK);
    }

    /// An `NRx4` write with the length-enable bit `enable` and trigger bit
    /// `trigger`. `skips_length` is the sequencer's next step not clocking
    /// length (`!(frame & 1)`).
    ///
    /// Switching the enable bit on while the counter is nonzero clocks it
    /// once at once, and a counter reaching zero stops the channel
    /// (`mgba/src/gb/audio.c:172-178`). A trigger then reloads a zero counter
    /// to 64, or to 63 when the channel is length-enabled and that extra
    /// clock is owed (`:187-192`).
    pub fn write_control(
        &mut self,
        enable: bool,
        trigger: bool,
        skips_length: bool,
    ) -> LengthWrite {
        let was_enabled = std::mem::replace(&mut self.enabled, enable);
        let mut result = LengthWrite::Unchanged;
        if !was_enabled && enable && self.remaining != 0 && skips_length {
            self.remaining -= 1;
            if self.remaining == 0 {
                result = LengthWrite::Expired;
            }
        }
        if trigger {
            if self.remaining == 0 {
                self.remaining = Self::MAX;
                if enable && skips_length {
                    self.remaining -= 1;
                }
            }
            result = LengthWrite::Triggered;
        }
        result
    }

    /// One 256 Hz length clock; returns whether it stopped the channel
    /// (`mgba/src/gb/audio.c:678-685`).
    pub fn clock(&mut self) -> bool {
        if self.enabled && self.remaining != 0 {
            self.remaining -= 1;
            return self.remaining == 0;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter(remaining: u8, enabled: bool) -> LengthCounter {
        LengthCounter { remaining, enabled }
    }

    #[test]
    fn nrx1_loads_sixty_four_minus_the_low_six_bits() {
        // `mgba/src/gb/audio.c:151` with `GBAudioRegisterDutyGetLength` = value & 0x3F.
        for (register, loaded) in [(0, 64), (1, 63), (60, 4), (63, 1), (64, 64), (255, 1)] {
            let mut length = LengthCounter::default();
            length.load(register);
            assert_eq!(length.remaining(), loaded, "NRx1 {register}");
            assert!(!length.is_enabled(), "a load does not enable the counter");
        }
    }

    #[test]
    fn enabling_clocks_a_nonzero_counter_once_when_the_next_step_skips_length() {
        for skips_length in [true, false] {
            for (before, even, odd) in [(2, 1, 2), (64, 63, 64)] {
                let mut length = counter(before, false);
                let result = length.write_control(true, false, skips_length);
                assert_eq!(result, LengthWrite::Unchanged);
                assert_eq!(length.remaining(), if skips_length { even } else { odd });
            }
        }
    }

    #[test]
    fn the_extra_clock_that_reaches_zero_stops_the_channel() {
        let mut even = counter(1, false);
        assert_eq!(even.write_control(true, false, true), LengthWrite::Expired);
        assert_eq!(even.remaining(), 0);

        let mut odd = counter(1, false);
        assert_eq!(
            odd.write_control(true, false, false),
            LengthWrite::Unchanged
        );
        assert_eq!(odd.remaining(), 1);
    }

    #[test]
    fn enabling_a_zero_counter_neither_clocks_nor_reloads_it() {
        let mut length = counter(0, false);
        assert_eq!(
            length.write_control(true, false, true),
            LengthWrite::Unchanged
        );
        assert_eq!(length.remaining(), 0);
    }

    #[test]
    fn rewriting_the_enable_bit_or_clearing_it_does_not_clock_again() {
        let mut length = counter(5, true);
        length.write_control(true, false, true);
        assert_eq!(length.remaining(), 5, "enable 1 -> 1 is not an edge");
        length.write_control(false, false, true);
        assert_eq!(length.remaining(), 5, "enable 1 -> 0 is not an edge");
        assert!(!length.clock(), "a disabled counter stops counting");
        assert_eq!(length.remaining(), 5);
    }

    #[test]
    fn a_trigger_reloads_only_a_zero_counter() {
        for skips_length in [true, false] {
            let mut disabled = counter(0, false);
            assert_eq!(
                disabled.write_control(false, true, skips_length),
                LengthWrite::Triggered
            );
            assert_eq!(disabled.remaining(), 64, "disabled reloads the full 64");

            let mut enabled = counter(0, true);
            enabled.write_control(true, true, skips_length);
            assert_eq!(
                enabled.remaining(),
                if skips_length { 63 } else { 64 },
                "an enabled reload owes the extra clock only on an even step"
            );

            let mut running = counter(7, true);
            running.write_control(true, true, skips_length);
            assert_eq!(running.remaining(), 7, "a nonzero counter is left alone");
        }
    }

    #[test]
    fn a_combined_enable_and_trigger_clocks_to_zero_then_reloads() {
        let mut length = counter(1, false);
        let result = length.write_control(true, true, true);
        assert_eq!(result, LengthWrite::Triggered);
        assert_eq!(length.remaining(), 63);
        length.write_control(true, true, true);
        assert_eq!(
            length.remaining(),
            63,
            "an identical second trigger changes nothing"
        );
    }

    #[test]
    fn a_length_clock_counts_down_and_reports_the_stop() {
        let mut length = counter(2, true);
        assert!(!length.clock());
        assert!(length.clock());
        assert!(!length.clock(), "an expired counter stays at zero");
        assert_eq!(length.remaining(), 0);
    }
}
