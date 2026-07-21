//! The boot shell's top-level owned type (I-1 slice 1): wires `platform`'s
//! window/input/pacing loop to a composed `rendering` scene, converting and
//! presenting each frame.
//!
//! [`App`] is intentionally the only thing `main` touches (`main` stays
//! thin, see the crate root docs) `(oop-boundaries)` -- this is the shell
//! future engine state plugs into.

use platform::{ButtonState, Buttons, Platform, PlatformError};

use crate::frame::to_platform_frame;
use crate::scene::BootScene;

/// The GBA button names in [`Buttons`] bit order, used only to format a
/// human-readable input log line (see [`describe_newly_pressed`]).
const BUTTON_NAMES: [(Buttons, &str); 10] = [
    (Buttons::A, "A"),
    (Buttons::B, "B"),
    (Buttons::SELECT, "SELECT"),
    (Buttons::START, "START"),
    (Buttons::RIGHT, "RIGHT"),
    (Buttons::LEFT, "LEFT"),
    (Buttons::UP, "UP"),
    (Buttons::DOWN, "DOWN"),
    (Buttons::R, "R"),
    (Buttons::L, "L"),
];

/// The running game shell: an open window and the (currently static)
/// placeholder scene it presents every frame.
///
/// No engine/battle state yet (out of scope for this slice, see the crate
/// root docs) -- `scene` is a fixed placeholder recomposited unchanged each
/// frame; a future slice replaces it with real per-frame game state.
pub struct App {
    platform: Platform,
    scene: BootScene,
}

impl App {
    /// Open a window titled `title` and build the placeholder scene.
    ///
    /// # Errors
    ///
    /// Returns a [`PlatformError`] if the platform's windowing event loop
    /// could not be created.
    pub fn new(title: impl Into<String>) -> Result<Self, PlatformError> {
        Ok(Self {
            platform: Platform::new(title)?,
            scene: BootScene::new(),
        })
    }

    /// Run the frame loop until the window is closed or Escape is pressed
    /// (see `platform::window`'s docs): pump input, present the composed
    /// scene, pace to the next GBA vblank.
    ///
    /// The scene is static for this slice, so it is composed once up front
    /// rather than every frame; a future slice recomposites per frame once
    /// there is engine state to reflect.
    ///
    /// # Errors
    ///
    /// Returns a [`PlatformError`] if window/surface creation or
    /// presentation fails.
    pub fn run(&mut self) -> Result<(), PlatformError> {
        let frame = to_platform_frame(&self.scene.compose());

        while self.platform.pump()? {
            if let Some(line) = describe_newly_pressed(*self.platform.buttons()) {
                eprintln!("{line}");
            }
            self.platform.present(&frame)?;
            self.platform.wait_for_next_frame();
        }
        Ok(())
    }
}

/// Format a log line naming every button that transitioned to held this
/// frame, or `None` if nothing changed.
///
/// No engine exists yet to act on GBA input (that wiring is future work);
/// this is the "log" half of "log-or-ignore is fine" (issue #70) -- proving
/// the keymap -> [`ButtonState`] pipeline is live end to end without
/// pretending it drives anything yet. Kept pure (no I/O) so it is
/// headless-unit-testable; [`App::run`] is the only (windowed, untestable)
/// caller.
fn describe_newly_pressed(state: ButtonState) -> Option<String> {
    let pressed = state.newly_pressed();
    if pressed == Buttons::NONE {
        return None;
    }
    let mut names = Vec::new();
    for &(button, name) in &BUTTON_NAMES {
        if pressed.intersects(button) {
            names.push(name);
        }
    }
    Some(format!("input: {}", names.join("+")))
}

#[cfg(test)]
mod tests {
    use super::describe_newly_pressed;
    use platform::{ButtonState, Buttons};

    #[test]
    fn no_input_yields_no_log_line() {
        let state = ButtonState::new();
        assert_eq!(describe_newly_pressed(state), None);
    }

    #[test]
    fn single_button_press_is_named() {
        let mut state = ButtonState::new();
        state.update(Buttons::A);
        assert_eq!(describe_newly_pressed(state).as_deref(), Some("input: A"));
    }

    #[test]
    fn multiple_simultaneous_presses_are_all_named_in_bit_order() {
        let mut state = ButtonState::new();
        state.update(Buttons::UP | Buttons::A);
        assert_eq!(
            describe_newly_pressed(state).as_deref(),
            Some("input: A+UP")
        );
    }

    #[test]
    fn holding_across_frames_is_not_logged_again() {
        let mut state = ButtonState::new();
        state.update(Buttons::A);
        state.update(Buttons::A); // still held, not newly pressed this frame.
        assert_eq!(describe_newly_pressed(state), None);
    }

    #[test]
    fn release_then_repress_is_logged_again() {
        let mut state = ButtonState::new();
        state.update(Buttons::B);
        state.update(Buttons::NONE);
        state.update(Buttons::B);
        assert_eq!(describe_newly_pressed(state).as_deref(), Some("input: B"));
    }
}
