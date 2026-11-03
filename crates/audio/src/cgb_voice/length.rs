//! The sound-length counter writes of a square or noise voice.
//!
//! `CgbSound` loads the instrument's length into `NRx1` on a note's first pass
//! and keeps `NRx4` bit 6 set only for a nonzero length
//! (`pokeemerald/src/m4a.c:1001,1014-1018,1021-1028`). The counter then runs
//! on the hardware's 256 Hz clock, and a trigger reloads it only from zero, so
//! a long envelope outlives the counter until a later volume write retriggers
//! it (`mgba/src/gb/audio.c:168-197,649-707`).

use super::{CgbChannelNumber, CgbVoice};
use crate::psg::{FrameSequencer, LengthWrite};

impl CgbVoice {
    /// Set the instrument's raw `ToneData::length`, which a nonzero value
    /// enables as `NRx4` bit 6 (`pokeemerald/src/m4a_1.s:1771-1772`). The
    /// wave channel's 256-step counter is not modelled and ignores it.
    #[must_use]
    pub(crate) fn with_length(mut self, register: u8) -> Self {
        self.length_register = register;
        self
    }

    /// Set the frame-sequencer step completed before the next
    /// [`Self::begin_frame`], whose register writes see its parity.
    pub(crate) fn set_sequencer_step(&mut self, step: u8) {
        self.sequencer_step = step;
    }

    fn has_length_counter(&self) -> bool {
        self.channel != CgbChannelNumber::Wave
    }

    fn skips_length(&self) -> bool {
        FrameSequencer::next_step_skips_length(self.sequencer_step)
    }

    fn length_enable_bit(&self) -> bool {
        self.length_register != 0
    }

    fn apply_length_write(&mut self, result: LengthWrite) {
        match result {
            LengthWrite::Unchanged => {}
            LengthWrite::Expired => self.length_stopped = true,
            LengthWrite::Triggered => self.length_stopped = false,
        }
    }

    /// The `SF_START` pass's `NRx1` store: `(duty << 6) + length` in one byte
    /// (`m4a.c:1001`), whose bits 6-7 select a square's duty and whose bits
    /// 0-5 load the counter (`mgba/src/gb/audio.c:885-888`). The enable bit
    /// still follows the raw instrument length ([`Self::with_length`]).
    pub(super) fn load_length(&mut self) {
        if let Some(register) = self.oscillator.write_nrx1(self.length_register) {
            self.length.load(register);
        }
    }

    /// The pitch block's `NRx4` store, which carries the enable bit with no
    /// trigger (`m4a.c:1198-1203`).
    pub(super) fn write_length_enable(&mut self) {
        if self.has_length_counter() {
            let result =
                self.length
                    .write_control(self.length_enable_bit(), false, self.skips_length());
            self.apply_length_write(result);
        }
    }

    /// A `CGB_CHANNEL_MO_VOL` trigger (`m4a.c:1219-1226`).
    pub(super) fn trigger_length(&mut self) {
        if self.has_length_counter() {
            let result =
                self.length
                    .write_control(self.length_enable_bit(), true, self.skips_length());
            self.apply_length_write(result);
        }
    }

    /// The idling slot's `NRx4 = 0x80` off-write: a trigger with the enable bit clear.
    pub(super) fn off_write_length(&mut self) {
        if self.has_length_counter() {
            let result = self.length.write_control(false, true, self.skips_length());
            self.apply_length_write(result);
        }
    }

    /// Counts the 256 Hz clocks a channel that is not rendering still receives.
    pub(super) fn clock_length_silently(&mut self, ticks: impl IntoIterator<Item = usize>) {
        for _ in ticks {
            self.clock_length();
        }
    }

    /// One 256 Hz clock; an expiry stops the channel without retiring the note.
    pub(super) fn clock_length(&mut self) {
        if self.has_length_counter() && self.length.clock() {
            self.length_stopped = true;
            if self.hardware_muted {
                self.last_contribution = 0;
            }
        }
    }

    /// Inherit the counter and enable bit of the voice this one replaces on the
    /// slot; only the new note's `NRx1` store reloads the count.
    pub(super) fn carry_length_from(&mut self, previous: &Self) {
        self.length = previous.length;
    }

    /// The counter, for the regressions that pin it.
    #[cfg(test)]
    pub(crate) fn length_counter(&self) -> crate::psg::LengthCounter {
        self.length
    }

    /// Whether the counter has stopped the channel's oscillation.
    #[cfg(test)]
    pub(crate) fn is_length_stopped(&self) -> bool {
        self.length_stopped
    }
}
