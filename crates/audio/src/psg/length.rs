//! The sound-length counter of the CGB channels: 64 steps on channels 1, 2 and
//! 4, 256 on channel 3.
//!
//! The counter is a register-write machine: `NRx1` loads it, `NRx4` bit 6
//! enables it and bit 7 triggers, and the frame sequencer's even steps clock
//! it (`mgba/src/gb/audio.c:147-151,168-197,255-258,290-313,649-707`).

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LengthCounter {
    remaining: u16,
    /// The counter's width in 256 Hz clocks, a power of two.
    max: u16,
    enabled: bool,
}

impl Default for LengthCounter {
    fn default() -> Self {
        Self::sixty_four_step()
    }
}

impl LengthCounter {
    /// Channels 1, 2 and 4: a 6-bit `NRx1` length (`mgba/src/gb/audio.c:151,343`).
    const SQUARE_NOISE_STEPS: u16 = 64;
    /// Channel 3: the full `NR31` byte (`mgba/src/gb/audio.c:257`).
    const WAVE_STEPS: u16 = 256;

    /// A counter of channels 1, 2 and 4.
    #[must_use]
    pub fn sixty_four_step() -> Self {
        Self::with_steps(Self::SQUARE_NOISE_STEPS)
    }

    /// A counter of channel 3.
    #[must_use]
    pub fn two_hundred_fifty_six_step() -> Self {
        Self::with_steps(Self::WAVE_STEPS)
    }

    fn with_steps(max: u16) -> Self {
        Self {
            remaining: 0,
            max,
            enabled: false,
        }
    }

    /// Steps left before the channel stops.
    #[must_use]
    pub fn remaining(self) -> u16 {
        self.remaining
    }

    /// Whether `NRx4` bit 6 is set.
    #[must_use]
    pub fn is_enabled(self) -> bool {
        self.enabled
    }

    /// An `NRx1` write: the low bits (six, or all eight for channel 3) load
    /// `steps - length` and a zero field loads the full width
    /// (`mgba/src/gb/audio.c:148-151,257`). It neither enables nor triggers
    /// the channel.
    pub fn load(&mut self, register: u8) {
        self.remaining = self.max - (u16::from(register) & (self.max - 1));
    }

    /// An `NRx4` write with the length-enable bit `enable` and trigger bit
    /// `trigger`. `skips_length` is the sequencer's next step not clocking
    /// length (`!(frame & 1)`).
    ///
    /// Switching the enable bit on while the counter is nonzero clocks it
    /// once at once, and a counter reaching zero stops the channel
    /// (`mgba/src/gb/audio.c:172-178`). A trigger then reloads a zero counter
    /// to the full width, or one less when the channel is length-enabled and that extra
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
                self.remaining = self.max;
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

    fn counter(remaining: u16, enabled: bool) -> LengthCounter {
        LengthCounter {
            remaining,
            enabled,
            ..LengthCounter::default()
        }
    }

    fn wave_counter(remaining: u16, enabled: bool) -> LengthCounter {
        LengthCounter {
            remaining,
            enabled,
            ..LengthCounter::two_hundred_fifty_six_step()
        }
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

    #[test]
    fn nr31_loads_two_hundred_fifty_six_minus_the_whole_byte() {
        // `mgba/src/gb/audio.c:257`.
        for (register, loaded) in [(0, 256), (1, 255), (64, 192), (240, 16), (255, 1)] {
            let mut length = LengthCounter::two_hundred_fifty_six_step();
            length.load(register);
            assert_eq!(length.remaining(), loaded, "NR31 {register}");
            assert!(!length.is_enabled(), "a load does not enable the counter");
        }
    }

    #[test]
    fn the_wave_enable_edge_clocks_once_and_can_stop_the_channel() {
        let mut odd = wave_counter(1, false);
        assert_eq!(
            odd.write_control(true, false, false),
            LengthWrite::Unchanged
        );
        assert_eq!(odd.remaining(), 1);

        let mut even = wave_counter(1, false);
        assert_eq!(even.write_control(true, false, true), LengthWrite::Expired);
        assert_eq!(even.remaining(), 0);

        let mut running = wave_counter(200, true);
        running.write_control(true, false, true);
        assert_eq!(running.remaining(), 200, "enable 1 -> 1 is not an edge");
    }

    #[test]
    fn a_wave_trigger_reloads_a_zero_counter_to_256_or_255() {
        // `mgba/src/gb/audio.c:308-313`.
        for (enabled, skips_length, reloaded) in [
            (false, true, 256),
            (false, false, 256),
            (true, false, 256),
            (true, true, 255),
        ] {
            let mut length = wave_counter(0, enabled);
            assert_eq!(
                length.write_control(enabled, true, skips_length),
                LengthWrite::Triggered
            );
            assert_eq!(length.remaining(), reloaded);
        }
        let mut running = wave_counter(100, true);
        running.write_control(true, true, true);
        assert_eq!(running.remaining(), 100, "a nonzero counter is left alone");
    }

    #[test]
    fn an_enable_edge_that_empties_a_wave_counter_is_reloaded_by_its_trigger() {
        let mut length = wave_counter(1, false);
        assert_eq!(
            length.write_control(true, true, true),
            LengthWrite::Triggered
        );
        assert_eq!(length.remaining(), 255);
    }

    #[test]
    fn a_wave_counter_counts_all_256_steps() {
        let mut length = wave_counter(256, true);
        for _ in 0..255 {
            assert!(!length.clock());
        }
        assert!(length.clock());
        assert_eq!(length.remaining(), 0);
    }
}
