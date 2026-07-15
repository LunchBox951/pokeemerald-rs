//! GBA button state (S-1): bitmask, held/newly-pressed views, default keymap.
//!
//! [`Buttons`] mirrors the key-input bit layout from
//! `pokeemerald/include/gba/io_reg.h` (`A_BUTTON`..`L_BUTTON`, `KEYS_MASK`).
//! [`ButtonState`] mirrors the held-vs-newly-pressed distinction upstream
//! draws between `gMain.heldKeys` (`JOY_HELD`) and `gMain.newKeys`
//! (`JOY_NEW`) in `pokeemerald/include/global.h`: a button is "newly
//! pressed" only on the frame it transitions from not-held to held.
//! [`Keymap`] is the default (non-configurable; remapping is out of scope for
//! S-1) keyboard-to-button binding.

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

use winit::keyboard::KeyCode;

/// A bitmask of GBA buttons.
///
/// Bit positions mirror `pokeemerald/include/gba/io_reg.h` exactly (`A_BUTTON
/// = 0x0001` .. `L_BUTTON = 0x0200`), so the raw [`Buttons::bits`] value is
/// upstream's `KEYINPUT`-style encoding (with the hardware's active-low
/// polarity already resolved: a set bit here means "pressed").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Buttons(u16);

impl Buttons {
    /// No buttons pressed.
    pub const NONE: Self = Self(0);
    /// `A_BUTTON` (`0x0001`).
    pub const A: Self = Self(0x0001);
    /// `B_BUTTON` (`0x0002`).
    pub const B: Self = Self(0x0002);
    /// `SELECT_BUTTON` (`0x0004`).
    pub const SELECT: Self = Self(0x0004);
    /// `START_BUTTON` (`0x0008`).
    pub const START: Self = Self(0x0008);
    /// `DPAD_RIGHT` (`0x0010`).
    pub const RIGHT: Self = Self(0x0010);
    /// `DPAD_LEFT` (`0x0020`).
    pub const LEFT: Self = Self(0x0020);
    /// `DPAD_UP` (`0x0040`).
    pub const UP: Self = Self(0x0040);
    /// `DPAD_DOWN` (`0x0080`).
    pub const DOWN: Self = Self(0x0080);
    /// `R_BUTTON` (`0x0100`).
    pub const R: Self = Self(0x0100);
    /// `L_BUTTON` (`0x0200`).
    pub const L: Self = Self(0x0200);
    /// `KEYS_MASK` (`0x03FF`): every modelled button bit.
    pub const MASK: Self = Self(0x03FF);

    /// The raw bitmask, in upstream `KEYINPUT` bit order.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Whether every bit set in `other` is also set in `self`.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether `self` and `other` share at least one set bit.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl BitOr for Buttons {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Buttons {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for Buttons {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for Buttons {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl Not for Buttons {
    type Output = Self;

    /// Complement, masked to [`Buttons::MASK`] so the unused upper bits never
    /// come back set (mirrors `KEYS_MASK` gating in upstream key-input code).
    fn not(self) -> Self {
        Self(!self.0 & Self::MASK.0)
    }
}

/// Per-frame button state: the held set and the newly-pressed-this-frame set.
///
/// Mirrors upstream's `gMain.heldKeys` / `gMain.newKeys` pair: [`held`] is
/// every button currently down, [`newly_pressed`] is the subset that
/// transitioned from up to down since the previous [`update`].
///
/// [`held`]: ButtonState::held
/// [`newly_pressed`]: ButtonState::newly_pressed
/// [`update`]: ButtonState::update
#[derive(Debug, Clone, Copy, Default)]
pub struct ButtonState {
    held: Buttons,
    newly_pressed: Buttons,
}

impl ButtonState {
    /// A fresh state with nothing held or newly pressed.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance to a new frame given the raw set of buttons currently held
    /// down (e.g. accumulated from this frame's keyboard events).
    ///
    /// Equivalent to upstream's per-frame `newKeys = keyInput & ~heldKeys;
    /// heldKeys = keyInput;` transition, computed once per vblank.
    pub fn update(&mut self, currently_held: Buttons) {
        self.newly_pressed = currently_held & !self.held;
        self.held = currently_held;
    }

    /// Every button currently held down.
    #[must_use]
    pub const fn held(&self) -> Buttons {
        self.held
    }

    /// Buttons that transitioned from up to down on the most recent
    /// [`update`](Self::update) call.
    #[must_use]
    pub const fn newly_pressed(&self) -> Buttons {
        self.newly_pressed
    }

    /// Whether `button` is currently held down.
    #[must_use]
    pub const fn is_held(&self, button: Buttons) -> bool {
        self.held.intersects(button)
    }

    /// Whether `button` was newly pressed on the most recent
    /// [`update`](Self::update) call.
    #[must_use]
    pub const fn is_newly_pressed(&self, button: Buttons) -> bool {
        self.newly_pressed.intersects(button)
    }
}

/// A keyboard-to-[`Buttons`] binding.
///
/// Only the [`Keymap::default_keymap`] is provided — remappable/configurable
/// input is explicitly out of scope for S-1.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<(KeyCode, Buttons)>,
}

impl Keymap {
    /// The default binding: arrow keys for the D-pad, Z/X for A/B (the
    /// common two-button-emulator layout), Enter/right Shift for
    /// Start/Select, and A/S for the shoulder buttons L/R.
    #[must_use]
    pub fn default_keymap() -> Self {
        Self {
            bindings: vec![
                (KeyCode::ArrowUp, Buttons::UP),
                (KeyCode::ArrowDown, Buttons::DOWN),
                (KeyCode::ArrowLeft, Buttons::LEFT),
                (KeyCode::ArrowRight, Buttons::RIGHT),
                (KeyCode::KeyZ, Buttons::A),
                (KeyCode::KeyX, Buttons::B),
                (KeyCode::Enter, Buttons::START),
                (KeyCode::ShiftRight, Buttons::SELECT),
                (KeyCode::KeyA, Buttons::L),
                (KeyCode::KeyS, Buttons::R),
            ],
        }
    }

    /// The [`Buttons`] bound to a physical key, if any.
    #[must_use]
    pub fn lookup(&self, code: KeyCode) -> Option<Buttons> {
        self.bindings
            .iter()
            .find(|(bound, _)| *bound == code)
            .map(|(_, button)| *button)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_match_io_reg_h() {
        assert_eq!(Buttons::A.bits(), 0x0001);
        assert_eq!(Buttons::B.bits(), 0x0002);
        assert_eq!(Buttons::SELECT.bits(), 0x0004);
        assert_eq!(Buttons::START.bits(), 0x0008);
        assert_eq!(Buttons::RIGHT.bits(), 0x0010);
        assert_eq!(Buttons::LEFT.bits(), 0x0020);
        assert_eq!(Buttons::UP.bits(), 0x0040);
        assert_eq!(Buttons::DOWN.bits(), 0x0080);
        assert_eq!(Buttons::R.bits(), 0x0100);
        assert_eq!(Buttons::L.bits(), 0x0200);
        assert_eq!(Buttons::MASK.bits(), 0x03FF);
    }

    #[test]
    fn mask_covers_exactly_the_ten_buttons() {
        let all = Buttons::A
            | Buttons::B
            | Buttons::SELECT
            | Buttons::START
            | Buttons::RIGHT
            | Buttons::LEFT
            | Buttons::UP
            | Buttons::DOWN
            | Buttons::R
            | Buttons::L;
        assert_eq!(all, Buttons::MASK);
    }

    #[test]
    fn not_stays_within_mask() {
        // The complement of NONE should be exactly the mask, never spilling
        // into the six unused upper bits of the u16.
        assert_eq!(!Buttons::NONE, Buttons::MASK);
        assert_eq!((!Buttons::MASK).bits(), 0);
    }

    #[test]
    fn contains_and_intersects() {
        let dpad_up_and_a = Buttons::UP | Buttons::A;
        assert!(dpad_up_and_a.contains(Buttons::A));
        assert!(dpad_up_and_a.contains(Buttons::UP));
        assert!(!dpad_up_and_a.contains(Buttons::B));
        assert!(dpad_up_and_a.intersects(Buttons::UP | Buttons::DOWN));
        assert!(!dpad_up_and_a.intersects(Buttons::DOWN | Buttons::LEFT));
    }

    #[test]
    fn first_frame_press_is_both_held_and_newly_pressed() {
        let mut state = ButtonState::new();
        state.update(Buttons::A);
        assert!(state.is_held(Buttons::A));
        assert!(state.is_newly_pressed(Buttons::A));
    }

    #[test]
    fn holding_across_frames_is_not_repeatedly_new() {
        let mut state = ButtonState::new();
        state.update(Buttons::A);
        assert!(state.is_newly_pressed(Buttons::A));

        // Still held on the next frame: no longer "new".
        state.update(Buttons::A);
        assert!(state.is_held(Buttons::A));
        assert!(!state.is_newly_pressed(Buttons::A));
    }

    #[test]
    fn release_then_repress_is_newly_pressed_again() {
        let mut state = ButtonState::new();
        state.update(Buttons::A);
        state.update(Buttons::A);
        assert!(!state.is_newly_pressed(Buttons::A));

        // Released.
        state.update(Buttons::NONE);
        assert!(!state.is_held(Buttons::A));
        assert!(!state.is_newly_pressed(Buttons::A));

        // Pressed again: newly-pressed once more.
        state.update(Buttons::A);
        assert!(state.is_held(Buttons::A));
        assert!(state.is_newly_pressed(Buttons::A));
    }

    #[test]
    fn independent_buttons_transition_independently() {
        let mut state = ButtonState::new();
        state.update(Buttons::UP);
        state.update(Buttons::UP | Buttons::A); // A newly pressed, UP still held.
        assert!(state.is_held(Buttons::UP));
        assert!(!state.is_newly_pressed(Buttons::UP));
        assert!(state.is_held(Buttons::A));
        assert!(state.is_newly_pressed(Buttons::A));
    }

    #[test]
    fn default_keymap_covers_dpad_a_b_start_select_l_r() {
        let keymap = Keymap::default_keymap();
        assert_eq!(keymap.lookup(KeyCode::ArrowUp), Some(Buttons::UP));
        assert_eq!(keymap.lookup(KeyCode::ArrowDown), Some(Buttons::DOWN));
        assert_eq!(keymap.lookup(KeyCode::ArrowLeft), Some(Buttons::LEFT));
        assert_eq!(keymap.lookup(KeyCode::ArrowRight), Some(Buttons::RIGHT));
        assert_eq!(keymap.lookup(KeyCode::KeyZ), Some(Buttons::A));
        assert_eq!(keymap.lookup(KeyCode::KeyX), Some(Buttons::B));
        assert_eq!(keymap.lookup(KeyCode::Enter), Some(Buttons::START));
        assert_eq!(keymap.lookup(KeyCode::ShiftRight), Some(Buttons::SELECT));
        assert_eq!(keymap.lookup(KeyCode::KeyA), Some(Buttons::L));
        assert_eq!(keymap.lookup(KeyCode::KeyS), Some(Buttons::R));
    }

    #[test]
    fn unbound_key_has_no_mapping() {
        let keymap = Keymap::default_keymap();
        assert_eq!(keymap.lookup(KeyCode::F1), None);
    }
}
