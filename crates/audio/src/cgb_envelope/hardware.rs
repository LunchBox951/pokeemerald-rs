//! The NRx2 envelope register of a live square or noise channel: the level a
//! `CGB_CHANNEL_MO_VOL` store latches and how the APU's own timer steps it
//! until the next store. Contract for [`super::CgbEnvelope`] and
//! [`crate::cgb_voice`].

const NRX2_ENV_FIELD_MASK: u8 = 0x0F;
const NRX2_ENV_STEP_TIME_MASK: u8 = 0x07;
pub(super) const NRX2_ENV_DIR_INC: u8 = 0x08;
const NIBBLE_MASK: u8 = 0x0F;

/// How a live channel's hardware envelope paces itself between NRx2 stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HardwareEnvelopePacing {
    /// Phase-7 envelope events between steps, from NRx2 bits 0-2; never zero.
    pub(crate) step_time: u8,
    /// Whether the envelope counts up rather than down, from NRx2 bit 3.
    pub(crate) increasing: bool,
}

impl HardwareEnvelopePacing {
    /// Decodes a store's step-time-and-direction field. A zero step time
    /// yields `None`: the hardware treats it as a dead envelope, not a slow
    /// one, whatever the direction bit says.
    pub(super) fn from_nrx2_step_time_and_dir(step_time_and_dir: u8) -> Option<Self> {
        let field = step_time_and_dir & NRX2_ENV_FIELD_MASK;
        let step_time = field & NRX2_ENV_STEP_TIME_MASK;
        (step_time != 0).then_some(Self {
            step_time,
            increasing: field & NRX2_ENV_DIR_INC != 0,
        })
    }
}

/// The NRx2 volume nibble a live square or noise channel holds, and the
/// hardware timer pacing it.
///
/// A store latches the level and restarts the timer; between stores the timer
/// steps the nibble one level in NRx2's direction and stops at 0 or 15 rather
/// than wrapping. Only a phase-7 event of the shared frame sequencer clocks the
/// timer (`mgba/src/gb/audio.c:710-732`); it is independent of the software
/// envelope's counter, and a live volume write does not disturb it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HardwareEnvelopeVolume {
    volume: u8,
    events_until_step: u8,
}

impl HardwareEnvelopeVolume {
    /// A store of NRx2: latches `level`'s low nibble and restarts the timer,
    /// leaving the shared frame sequencer untouched.
    pub(crate) fn write(&mut self, level: u8, pacing: Option<HardwareEnvelopePacing>) {
        self.volume = level & NIBBLE_MASK;
        // A level already saturated in the store's own direction never steps.
        self.events_until_step = match pacing {
            Some(pacing) if !self.is_saturated_toward(pacing) => pacing.step_time,
            _ => 0,
        };
    }

    /// Counts one phase-7 event of a playing channel. Returns whether the
    /// timer stepped the nibble, because only a step re-resolves a noise latch.
    pub(crate) fn tick(&mut self, pacing: Option<HardwareEnvelopePacing>) -> bool {
        let Some(pacing) = pacing else { return false };
        if self.events_until_step == 0 {
            return false;
        }
        self.events_until_step -= 1;
        if self.events_until_step != 0 {
            return false;
        }
        if pacing.increasing {
            self.volume += 1;
        } else {
            self.volume -= 1;
        }
        if !self.is_saturated_toward(pacing) {
            self.events_until_step = pacing.step_time;
        }
        true
    }

    /// The off-write's `NR42 = 8` store: volume zero and no timer.
    pub(crate) fn write_off(&mut self) {
        self.volume = 0;
        self.events_until_step = 0;
    }

    /// The nibble the volume register currently holds.
    pub(crate) fn volume(self) -> u8 {
        self.volume
    }

    fn is_saturated_toward(self, pacing: HardwareEnvelopePacing) -> bool {
        if pacing.increasing {
            self.volume >= NIBBLE_MASK
        } else {
            self.volume == 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn every(step_time: u8, increasing: bool) -> HardwareEnvelopePacing {
        HardwareEnvelopePacing {
            step_time,
            increasing,
        }
    }

    /// Writes `level`, then returns what each of `events` phase-7 events did.
    fn run(level: u8, pacing: Option<HardwareEnvelopePacing>, events: usize) -> Vec<(bool, u8)> {
        let mut hardware = HardwareEnvelopeVolume::default();
        hardware.write(level, pacing);
        (0..events)
            .map(|_| (hardware.tick(pacing), hardware.volume()))
            .collect()
    }

    #[test]
    fn a_store_alone_never_moves_the_volume() {
        let mut hardware = HardwareEnvelopeVolume::default();
        hardware.write(9, Some(every(1, true)));
        assert_eq!(hardware.volume(), 9);
    }

    #[test]
    fn a_period_of_two_steps_on_every_second_event_and_saturates_without_wrapping() {
        assert_eq!(
            run(14, Some(every(2, true)), 6),
            [
                (false, 14),
                (true, 15),
                (false, 15),
                (false, 15),
                (false, 15),
                (false, 15)
            ]
        );
        assert_eq!(
            run(1, Some(every(2, false)), 6),
            [
                (false, 1),
                (true, 0),
                (false, 0),
                (false, 0),
                (false, 0),
                (false, 0)
            ]
        );
    }

    #[test]
    fn a_level_saturated_in_the_stores_direction_never_steps() {
        assert!(run(0, Some(every(1, false)), 4)
            .iter()
            .all(|&(stepped, v)| !stepped && v == 0));
        assert!(run(15, Some(every(1, true)), 4)
            .iter()
            .all(|&(stepped, v)| !stepped && v == 15));
    }

    #[test]
    fn the_far_end_of_the_range_is_alive_in_the_opposite_direction() {
        assert_eq!(run(0, Some(every(1, true)), 2), [(true, 1), (true, 2)]);
        assert_eq!(run(15, Some(every(1, false)), 2), [(true, 14), (true, 13)]);
    }

    #[test]
    fn a_period_zero_store_is_dead_not_a_slow_envelope() {
        // NRx2 with a zero step time decodes to no pacing, whatever its direction.
        let dead = HardwareEnvelopePacing::from_nrx2_step_time_and_dir(NRX2_ENV_DIR_INC);
        assert!(dead.is_none());
        assert!(run(0, dead, 20)
            .iter()
            .all(|&(stepped, v)| !stepped && v == 0));
        assert!(run(7, dead, 20)
            .iter()
            .all(|&(stepped, v)| !stepped && v == 7));
    }

    #[test]
    fn a_store_restarts_the_countdown_even_at_the_same_level() {
        let pacing = Some(every(4, false));
        let mut hardware = HardwareEnvelopeVolume::default();
        hardware.write(15, pacing);
        for _ in 0..3 {
            assert!(!hardware.tick(pacing));
        }
        hardware.write(15, pacing);
        for _ in 0..3 {
            assert!(!hardware.tick(pacing));
        }
        assert_eq!(hardware.volume(), 15);
        assert!(hardware.tick(pacing));
        assert_eq!(hardware.volume(), 14);
    }

    #[test]
    fn the_off_write_zeroes_the_volume_and_disarms_the_timer() {
        let pacing = Some(every(1, true));
        let mut hardware = HardwareEnvelopeVolume::default();
        hardware.write(9, pacing);
        hardware.write_off();
        assert_eq!(hardware.volume(), 0);
        assert!(!hardware.tick(pacing));
        assert_eq!(hardware.volume(), 0);
    }
}
