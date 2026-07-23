//! The boot shell's top-level owned type (I-1 slice 1): wires `platform`'s
//! window/input/pacing loop to a composed `rendering` scene, converting and
//! presenting each frame.
//!
//! [`App`] is intentionally the only thing `main` touches (`main` stays
//! thin, see the crate root docs) `(oop-boundaries)` -- this is the shell
//! future engine state plugs into.
//!
//! [`App::new_headless`] plus [`App::step`] (F-3, V-1) are the seam `xtask`'s
//! `e2e --suite smoke` run drives in-process: the exact same composed scene
//! and per-frame loop body as [`App::run`], just against `platform`'s null
//! window backend instead of a real one (see
//! `platform::Platform::new_headless`).

use platform::{ButtonState, Buttons, Frame, Platform, PlatformError};

use crate::frame::to_platform_frame;
use crate::scene::BootScene;

/// Compose a fresh [`BootScene`] into a `platform`-ready frame.
///
/// Shared by both constructors, which each call this exactly once: the
/// scene is static for this slice (no engine state exists yet to reflect
/// per frame, see the module docs), so composing it once up front and
/// caching the result on [`App`] is correct and avoids re-allocating a
/// 240x160 frame every [`App::step`].
fn compose_boot_frame() -> Box<Frame> {
    to_platform_frame(&BootScene::new().compose())
}

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

/// The running game shell: an open window (or, headlessly, `platform`'s null
/// backend) and the (currently static) placeholder scene's already-composed
/// frame, presented unchanged every frame.
///
/// No engine/battle state yet (out of scope for this slice, see the crate
/// root docs) -- the scene is a fixed placeholder; a future slice replaces
/// `frame` with real per-frame recomposition once there is engine state to
/// reflect (see [`compose_boot_frame`]).
pub struct App {
    platform: Platform,
    frame: Box<Frame>,
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
            frame: compose_boot_frame(),
        })
    }

    /// Build the same placeholder scene as [`App::new`], but against
    /// `platform`'s explicit headless/null backend (F-3, V-1) instead of a
    /// real window.
    ///
    /// Always succeeds (mirrors `platform::Platform::new_headless`, which
    /// opens no OS resources), unlike [`App::new`]. This is the constructor
    /// `xtask`'s `e2e --suite smoke` run uses to drive the boot shell
    /// in-process without a display server.
    #[must_use]
    pub fn new_headless() -> Self {
        Self {
            platform: Platform::new_headless(),
            frame: compose_boot_frame(),
        }
    }

    /// Run the frame loop until the window is closed or Escape is pressed
    /// (see `platform::window`'s docs), or (for a headless `App`, which
    /// never signals a close) forever -- callers that need a bounded run
    /// (e.g. `xtask`'s smoke suite) should call [`App::step`] directly
    /// instead of `run`.
    ///
    /// # Errors
    ///
    /// Returns a [`PlatformError`] if window/surface creation or
    /// presentation fails.
    pub fn run(&mut self) -> Result<(), PlatformError> {
        while self.step()? {}
        Ok(())
    }

    /// Run exactly one iteration of the frame loop body: pump input, log
    /// any newly-pressed buttons, present the composed scene, and pace to
    /// the next GBA vblank (a no-op for a headless `App`, see
    /// `platform::Platform::wait_for_next_frame`).
    ///
    /// Returns whether the loop should keep going -- `false` once
    /// `platform::Platform::pump` reports a close request, at which point
    /// the frame is *not* presented (mirroring the `while` loop this method
    /// replaces in [`App::run`]). Always `true` for a headless `App`, which
    /// has no window to close.
    ///
    /// # Errors
    ///
    /// Returns a [`PlatformError`] if input pumping or presentation fails.
    pub fn step(&mut self) -> Result<bool, PlatformError> {
        if !self.platform.pump()? {
            return Ok(false);
        }
        if let Some(line) = describe_newly_pressed(*self.platform.buttons()) {
            eprintln!("{line}");
        }
        self.platform.present(&self.frame)?;
        self.platform.wait_for_next_frame();
        Ok(true)
    }

    /// The composed placeholder scene's frame, as most recently handed to
    /// `platform::Platform::present` (or about to be, on the first
    /// [`App::step`]).
    ///
    /// Exposed for e2e assertions (`xtask`'s `e2e --suite smoke` checks it
    /// is non-blank as proof the boot scene actually rendered something,
    /// not just that the loop ran) -- not needed by [`App::run`]/`step`
    /// themselves, which read the cached `frame` field directly.
    #[must_use]
    pub fn frame(&self) -> &Frame {
        &self.frame
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
    use super::{describe_newly_pressed, App};
    use platform::{ButtonState, Buttons};

    #[test]
    fn headless_frame_is_non_blank() {
        let app = App::new_headless();
        assert!(
            app.frame().iter().any(|&pixel| pixel != 0),
            "the composed boot scene must produce a non-blank frame"
        );
    }

    #[test]
    fn headless_step_keeps_going_and_never_errors() {
        let mut app = App::new_headless();
        for _ in 0..10 {
            assert!(app.step().expect("headless step never errors"));
        }
    }

    #[test]
    fn headless_frame_is_stable_across_steps() {
        // The scene is static for this slice (see the module docs), so the
        // composed frame must not change between steps.
        let mut app = App::new_headless();
        let before = app.frame().to_vec();
        for _ in 0..5 {
            app.step().expect("headless step never errors");
        }
        assert_eq!(app.frame().to_vec(), before);
    }

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
