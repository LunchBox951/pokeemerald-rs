//! `ReadKeys`' held-button repeat (`pokeemerald/src/main.c:251-275`), kept per
//! screen because the platform input carries only held and fresh buttons.
//! Upstream's `gKeyRepeatStartDelay` is session-global and a screen that
//! changes it (the naming screen to 16, `naming_screen.c:483`; the storage
//! system to 20, `pokemon_storage_system.c:2058`) leaves it changed; each
//! screen here opens with the delay upstream would hold on a fresh boot.

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
    ///
    /// The compare is upstream's, bug included (`main.c:257-261`): this
    /// frame's raw input against the previous frame's remapped held set, and
    /// the repeated set is never remapped. Under L=A a held `L` therefore
    /// never equals the remembered `L|A`, so nothing repeats while `L` is down.
    pub(crate) fn advance(&mut self, buttons: ButtonState) -> Buttons {
        let key_input = buttons.raw_held();
        let mut repeated = buttons.raw_newly_pressed();
        if key_input != Buttons::NONE && key_input == self.previous_held {
            self.counter -= 1;
            if self.counter == 0 {
                repeated = key_input;
                self.counter = CONTINUE_DELAY;
            }
        } else {
            self.counter = self.start_delay;
        }
        self.previous_held = buttons.held();
        repeated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames_repeating(buttons: Buttons, l_equals_a: bool, frames: u32) -> Vec<u32> {
        let mut raw = ButtonState::new();
        let mut repeat = KeyRepeat::new(START_DELAY);
        let mut out = Vec::new();
        for frame in 0..frames {
            raw.update(buttons);
            let state = if l_equals_a {
                raw.with_l_equals_a()
            } else {
                raw
            };
            let repeated = repeat.advance(state);
            if frame > 0 && repeated.intersects(Buttons::DOWN) {
                out.push(frame);
            }
        }
        out
    }

    #[test]
    fn a_held_direction_repeats_after_the_start_delay_then_every_continue_delay() {
        assert_eq!(frames_repeating(Buttons::DOWN, false, 51), [40, 45, 50]);
    }

    /// `ReadKeys` compares raw `keyInput` (`L|Down`) against the previous
    /// remapped `heldKeys` (`L|A|Down`), so under L=A a held `L` suppresses
    /// every repeat (`main.c:257-261`).
    #[test]
    fn held_l_in_l_equals_a_mode_suppresses_repeat() {
        assert_eq!(frames_repeating(Buttons::L | Buttons::DOWN, true, 60), []);
        assert_eq!(
            frames_repeating(Buttons::L | Buttons::DOWN, false, 51),
            [40, 45, 50]
        );
    }

    /// `newAndRepeatedKeys` is never remapped: a fresh `L` under L=A does not
    /// report `A` through the repeat view.
    #[test]
    fn the_repeated_set_is_not_remapped() {
        let mut raw = ButtonState::new();
        let mut repeat = KeyRepeat::new(START_DELAY);
        raw.update(Buttons::L);
        let repeated = repeat.advance(raw.with_l_equals_a());
        assert_eq!(repeated, Buttons::L);
    }
}
