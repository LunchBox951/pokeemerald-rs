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
    /// Iterations between steps, from NRx2 bits 0-2; never zero.
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
/// than wrapping. It is independent of the software envelope's counter: a live
/// volume write does not disturb it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HardwareEnvelopeVolume {
    volume: u8,
    iterations_until_step: u8,
    note_on_write_owed: bool,
}

impl HardwareEnvelopeVolume {
    /// A fresh voice: silent, and owing the note-on store that ends its
    /// first pass.
    pub(crate) fn at_note_on() -> Self {
        Self {
            volume: 0,
            iterations_until_step: 0,
            note_on_write_owed: true,
        }
    }

    /// Settles one render frame of `iterations` software iterations. A frame
    /// that stores NRx2 does so once, after those iterations; any other lets
    /// the timer run. Returns whether the timer stepped the nibble rather than
    /// a direct store, because only a step re-resolves a noise latch.
    pub(crate) fn end_frame(
        &mut self,
        iterations: u8,
        wrote: bool,
        level: u8,
        pacing: Option<HardwareEnvelopePacing>,
    ) -> bool {
        let note_on = std::mem::take(&mut self.note_on_write_owed);
        if wrote || note_on {
            self.write(level, pacing);
            false
        } else {
            self.advance(iterations, pacing)
        }
    }

    /// The off-write's `NR42 = 8` store: volume zero, no timer, and no
    /// note-on store left owing.
    pub(crate) fn write_off(&mut self) {
        self.volume = 0;
        self.iterations_until_step = 0;
        self.note_on_write_owed = false;
    }

    /// The nibble the volume register currently holds.
    pub(crate) fn volume(self) -> u8 {
        self.volume
    }

    fn write(&mut self, level: u8, pacing: Option<HardwareEnvelopePacing>) {
        self.volume = level & NIBBLE_MASK;
        // A level already saturated in the store's own direction never steps.
        self.iterations_until_step = match pacing {
            Some(pacing) if !self.is_saturated_toward(pacing) => pacing.step_time,
            _ => 0,
        };
    }

    fn advance(&mut self, iterations: u8, pacing: Option<HardwareEnvelopePacing>) -> bool {
        let Some(pacing) = pacing else { return false };
        let mut stepped = false;
        for _ in 0..iterations {
            if self.iterations_until_step == 0 {
                return stepped;
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
            stepped = true;
            if self.is_saturated_toward(pacing) {
                return stepped;
            }
            self.iterations_until_step = pacing.step_time;
        }
        stepped
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

    const UP_EVERY_ITERATION: HardwareEnvelopePacing = HardwareEnvelopePacing {
        step_time: 1,
        increasing: true,
    };

    #[test]
    fn a_note_on_pass_arms_the_timer_only_once_it_ends() {
        // The note-on store lands after the pass's own stepping, so however
        // many iterations ran, the pass ends with a freshly armed timer.
        for iterations in [1, 2] {
            let mut hardware = HardwareEnvelopeVolume::at_note_on();

            let stepped = hardware.end_frame(iterations, false, 0, Some(UP_EVERY_ITERATION));
            assert!(
                !stepped,
                "{iterations} iterations: the note-on store is direct"
            );
            assert_eq!(hardware.volume(), 0, "{iterations} iterations");

            let stepped = hardware.end_frame(1, false, 0, Some(UP_EVERY_ITERATION));
            assert!(stepped, "{iterations} iterations");
            assert_eq!(hardware.volume(), 1, "{iterations} iterations");
        }
    }

    #[test]
    fn a_direct_store_to_zero_is_not_a_step() {
        let mut hardware = HardwareEnvelopeVolume::at_note_on();
        hardware.end_frame(1, true, 9, None);
        assert_eq!(hardware.volume(), 9);
        let stepped = hardware.end_frame(1, true, 0, None);
        assert!(!stepped);
        assert_eq!(hardware.volume(), 0);
    }

    #[test]
    fn the_off_write_zeroes_the_volume_and_disarms_the_timer() {
        let mut hardware = HardwareEnvelopeVolume::at_note_on();
        hardware.end_frame(1, true, 9, Some(UP_EVERY_ITERATION));
        hardware.write_off();
        assert_eq!(hardware.volume(), 0);
        assert!(!hardware.end_frame(4, false, 9, Some(UP_EVERY_ITERATION)));
        assert_eq!(hardware.volume(), 0);
    }
}
