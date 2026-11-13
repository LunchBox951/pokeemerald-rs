//! The sound-length counter writes of a square, noise or wave voice.
//!
//! `CgbSound` loads the instrument's length into `NRx1` on a note's first pass
//! and keeps `NRx4` bit 6 set only for a nonzero length
//! (`pokeemerald/src/m4a.c:1001,1014-1018,1021-1028`). The counter then runs
//! on the hardware's 256 Hz clock and, on expiry, stops the channel without
//! retiring the note (`mgba/src/gb/audio.c:168-197,649-707`).
//!
//! A square or noise voice (channels 1, 2 and 4) has a 64-step counter loaded
//! from the low six bits of `NRx1`. A trigger reloads it only from zero, so a
//! long envelope outlives the counter until a later envelope transition or
//! live volume write retriggers the channel.
//!
//! A wave voice (channel 3) differs in four ways:
//! - its counter is 256 steps wide and `NR31` loads `256 - length` from the
//!   whole byte (`mgba/src/gb/audio.c:255-258`);
//! - its first pitch block is the only trigger, because the volume block
//!   clears `n4` bit 7 after the first pass, so later transitions and live
//!   volume writes never revive an expired counter (`m4a.c:1013-1018,
//!   1201-1216`);
//! - `CgbOscOff(3)` writes `NR30` alone, leaving the count and enable bit
//!   (`m4a.c:869-871`);
//! - on expiry it presents the latch its last clock left
//!   (`mgba/src/gb/audio.c:768-774`).

use super::{CgbChannelNumber, CgbVoice};
use crate::psg::{FrameSequencer, LengthWrite};

impl CgbVoice {
    /// Set the instrument's raw `ToneData::length`, which a nonzero value
    /// enables as `NRx4` bit 6 (`pokeemerald/src/m4a_1.s:1771-1772`). A wave's
    /// byte is the whole `NR31` length; a square or noise keeps six bits.
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

    fn is_wave(&self) -> bool {
        self.channel == CgbChannelNumber::Wave
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
    /// 0-5 load the counter (`mgba/src/gb/audio.c:885-888`); a wave stores the
    /// byte as `NR31` (`m4a.c:1014`) and loads `256 - length`
    /// (`mgba/src/gb/audio.c:255-258`). The enable bit still follows the raw
    /// instrument length ([`Self::with_length`]).
    pub(super) fn load_length(&mut self) {
        let register = self.oscillator.write_nrx1(self.length_register);
        self.length.load(register);
    }

    /// The first pitch block's `NRx4` store (`m4a.c:1198-1203`). A square or
    /// noise carries the enable bit with no trigger; a wave's `n4` also holds
    /// the trigger bit until its volume block clears it (`m4a.c:1014-1018,
    /// 1213-1216`), so this store is the one trigger a wave ever gets, and its
    /// repeat in the volume block finds a nonzero count.
    pub(super) fn write_first_pitch_length(&mut self) {
        let result = self.length.write_control(
            self.length_enable_bit(),
            self.is_wave(),
            self.skips_length(),
        );
        self.apply_length_write(result);
    }

    /// A `CGB_CHANNEL_MO_VOL` trigger (`m4a.c:1219-1226`). A wave's volume
    /// block only rewrites `NR32` once its first trigger has cleared `n4` bit 7.
    pub(super) fn trigger_length(&mut self) {
        if !self.is_wave() {
            let result =
                self.length
                    .write_control(self.length_enable_bit(), true, self.skips_length());
            self.apply_length_write(result);
        }
    }

    /// The idling slot's `NRx4 = 0x80` off-write: a trigger with the enable bit
    /// clear. `CgbOscOff(3)` writes only `NR30` (`m4a.c:857-876`).
    pub(super) fn off_write_length(&mut self) {
        if !self.is_wave() {
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
        if self.length.clock() {
            self.length_stopped = true;
            if self.hardware_muted && !self.sweep_held {
                self.last_contribution = 0;
            }
        }
    }

    /// The latch a square would hold if its length clock at this sample stops
    /// it. `GBAudioUpdateFrame` runs channels 1 and 2 to the frame-event
    /// timestamp while they still play, so `_updateSquareSample` re-latches the
    /// duty level before the length clock clears `playing`
    /// (`mgba/src/gb/audio.c:649-657,493-512,678-684`). Read it without
    /// stepping: the sample's deferred idle time still steps the phase. `None`
    /// unless a length clock is due at a rendering square.
    pub(super) fn expiry_square_latch(&mut self, clock_due: bool) -> Option<i32> {
        if !clock_due || self.length_stopped || self.hardware_muted {
            return None;
        }
        self.oscillator.square_held_contribution(self.frame_gain)
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
