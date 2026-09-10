//! The NRx2 envelope register a live square or noise channel runs on: what a
//! `CGB_CHANNEL_MO_VOL` store puts there, and how the APU's own timer paces
//! it until the next one. Owns that contract for [`super::CgbEnvelope`] and
//! [`crate::cgb_voice`].

/// The nibble upstream masks its step-time-and-direction field to before
/// adding the volume nibble into one NRx2 byte (`m4a.c:1221`..`:1222`).
const NRX2_ENV_FIELD_MASK: u8 = 0x0F;
/// NRx2 bits 0-2 (`GBAudioRegisterSweep`,
/// `mgba/include/mgba/internal/gb/audio.h:24`..`:25`).
const NRX2_ENV_STEP_TIME_MASK: u8 = 0x07;
/// NRx2 bit 3: upstream's `CGB_NRx2_ENV_DIR_INC`, whose `_DEC` counterpart is
/// zero (`m4a_internal.h:84`..`:85`).
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
    /// Decode a store's step-time-and-direction field (`_writeEnvelope`,
    /// `mgba/src/gb/audio.c:891`..`:893`). A zero step time is a dead
    /// envelope rather than a slow one, whatever bit 3 says
    /// (`_updateEnvelopeDead`, `mgba/src/gb/audio.c:948`..`:950`), so a phase
    /// byte above 7 can pace at another period, in the other direction, or
    /// not at all.
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
/// A store latches the level and restarts the timer from that store's own
/// step time (`m4a.c:1219`..`:1223`; `_resetEnvelope`,
/// `mgba/src/gb/audio.c:856`..`:860`); between stores the timer steps the
/// nibble one level in NRx2's direction and dies at 0 or 15 rather than
/// wrapping (`_updateEnvelope`, `mgba/src/gb/audio.c:931`..`:945`). It is
/// never the software envelope's counter: a live volume write lands mid-phase
/// without disturbing upstream's `envelopeCounter` (`m4a_1.s:1394`..`:1400`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct HardwareEnvelopeVolume {
    volume: u8,
    iterations_until_step: u8,
    note_on_write_owed: bool,
}

impl HardwareEnvelopeVolume {
    /// A fresh voice: silent, and owing the note-on store its first
    /// `CgbSound` pass ends with (`m4a.c:993`, `:1206`..`:1226`).
    pub(crate) fn at_note_on() -> Self {
        Self {
            volume: 0,
            iterations_until_step: 0,
            note_on_write_owed: true,
        }
    }

    /// Settle one render frame, `iterations` software iterations long. A
    /// frame that stores NRx2 does so once, at the pass's end after those
    /// iterations (`m4a.c:1206`..`:1226`); any other lets the timer run.
    pub(crate) fn end_frame(
        &mut self,
        iterations: u8,
        wrote: bool,
        level: u8,
        pacing: Option<HardwareEnvelopePacing>,
    ) {
        let note_on = std::mem::take(&mut self.note_on_write_owed);
        if wrote || note_on {
            self.write(level, pacing);
        } else {
            self.advance(iterations, pacing);
        }
    }

    /// The nibble the volume register currently holds.
    pub(crate) fn volume(self) -> u8 {
        self.volume
    }

    fn write(&mut self, level: u8, pacing: Option<HardwareEnvelopePacing>) {
        self.volume = level & NIBBLE_MASK;
        // A level already saturated in the store's own direction is dead on
        // arrival (`_updateEnvelopeDead`, `mgba/src/gb/audio.c:948`..`:954`).
        self.iterations_until_step = match pacing {
            Some(pacing) if !self.is_saturated_toward(pacing) => pacing.step_time,
            _ => 0,
        };
    }

    fn advance(&mut self, iterations: u8, pacing: Option<HardwareEnvelopePacing>) {
        let Some(pacing) = pacing else { return };
        for _ in 0..iterations {
            if self.iterations_until_step == 0 {
                return;
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
            if self.is_saturated_toward(pacing) {
                return;
            }
            self.iterations_until_step = pacing.step_time;
        }
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
        // The note-on store lands after the pass's own envelope stepping,
        // doubling re-entry included (`m4a.c:1176`..`:1180`), so however many
        // iterations that pass ran, it ends with a freshly armed timer and a
        // nibble still at the level it stored.
        for iterations in [1, 2] {
            let mut hardware = HardwareEnvelopeVolume::at_note_on();

            hardware.end_frame(iterations, false, 0, Some(UP_EVERY_ITERATION));
            assert_eq!(hardware.volume(), 0, "{iterations} iterations");

            hardware.end_frame(1, false, 0, Some(UP_EVERY_ITERATION));
            assert_eq!(hardware.volume(), 1, "{iterations} iterations");
        }
    }
}
