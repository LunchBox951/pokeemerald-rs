//! `ReadKeys`' held-button repeat (`pokeemerald/src/main.c:251-275`), kept per
//! screen because the platform input carries only held and fresh buttons.

use platform::input::{ButtonState, Buttons};

/// `InitKeys`' `gKeyRepeatStartDelay` (`main.c:241`): frames a button is held
/// before its first repeat.
pub(crate) const START_DELAY: u8 = 40;
/// `InitKeys`' `gKeyRepeatContinueDelay` (`main.c:240`): frames between
/// repeats after the first.
pub(crate) const CONTINUE_DELAY: u8 = 5;

/// The `keyRepeatCounter` and the held set it was counting.
#[derive(Debug)]
pub(crate) struct KeyRepeat {
    start_delay: u8,
    counter: u8,
    previous_held: Buttons,
}

impl KeyRepeat {
    /// A counter at rest for a screen whose `gKeyRepeatStartDelay` is
    /// `start_delay`.
    pub(crate) fn new(start_delay: u8) -> Self {
        Self {
            start_delay,
            counter: start_delay,
            previous_held: Buttons::NONE,
        }
    }

    /// `gMain.newAndRepeatedKeys` for this frame. Call it once per frame,
    /// whether or not the screen reads the result: upstream counts every frame.
    pub(crate) fn advance(&mut self, buttons: ButtonState) -> Buttons {
        let held = buttons.held();
        let mut repeated = buttons.newly_pressed();
        if held != Buttons::NONE && held == self.previous_held {
            self.counter -= 1;
            if self.counter == 0 {
                repeated = held;
                self.counter = CONTINUE_DELAY;
            }
        } else {
            self.counter = self.start_delay;
        }
        self.previous_held = held;
        repeated
    }
}
