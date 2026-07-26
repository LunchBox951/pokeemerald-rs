//! The boot shell's top-level owned type (I-1 slice 1; I-2, issue #109):
//! wires `platform`'s window/input/pacing loop to a composed `rendering`
//! scene, converting and presenting each frame.
//!
//! [`App`] is intentionally the only thing `main` touches (`main` stays
//! thin, see the crate root docs) `(oop-boundaries)` -- this is the shell
//! future engine state plugs into.
//!
//! [`App::new_headless`] plus [`App::step`] (F-3, V-1) are the seam `xtask`'s
//! `e2e --suite smoke` run drives in-process: the exact same per-frame loop
//! body as [`App::run`], just against `platform`'s null window backend
//! instead of a real one (see `platform::Platform::new_headless`).
//! [`App::new_headless`] always composes the I-1 synthetic [`BootScene`]
//! (never the real title screen) so that suite's no-local-pack CI behaviour
//! stays exactly as it was before I-2 -- see [`crate::title`]'s module docs
//! for the real title screen, which only [`App::new`] (the real windowed
//! entry point) composes.
//!
//! # Animating the real title screen (I-2, issue #116)
//!
//! [`crate::title::TitleScene::compose`] takes a `frame` counter (the same
//! deterministic, wall-clock-free counter [`crate::title`]'s module docs
//! describe): [`App::new`] keeps the loaded [`TitleScene`] alive alongside a
//! running tick count, and every [`App::step`] recomposes the *next* tick's
//! frame right after presenting the current one -- so the windowed title
//! screen animates (cloud scroll, "Press Start" blink) using exactly the
//! same [`TitleScene::compose`] calls this crate's
//! tests and `xtask`'s smoke suite exercise headlessly, just one call per
//! real frame instead of at two fixed indices.

use platform::{ButtonState, Buttons, Frame, Platform, PlatformError};

use crate::frame::to_platform_frame;
use crate::scene::BootScene;
use crate::title::{self, TitleScene, TitleSceneError};

/// Compose a fresh [`BootScene`] into a `platform`-ready frame.
///
/// The synthetic placeholder scene: only [`App::new_headless`] uses this
/// now (see the module docs) -- it exists purely so headless tests and
/// `xtask`'s smoke suite keep a scene to render when no asset pack has been
/// extracted, without depending on one.
fn compose_boot_frame() -> Box<Frame> {
    to_platform_frame(&BootScene::new().compose())
}

/// Why [`App::new`] failed to start.
///
/// Concrete per-crate enum `(oop-boundaries)` -- no `anyhow`. Wraps both
/// halves of startup: opening the platform window, and loading/decoding the
/// real title screen ([`crate::title::load_default`]). The latter is the
/// I-2 "missing pack" diagnostic: [`AppError::Title`] with
/// [`TitleSceneError::is_pack_missing`] true prints exactly what to run
/// (`./init.sh` then `cargo xtask extract`) and lets `main` exit cleanly --
/// no panic, no window ever opened.
#[derive(Debug)]
pub enum AppError {
    /// Opening the platform window/event loop failed.
    Platform(PlatformError),
    /// Loading or decoding the real title screen failed -- see
    /// [`TitleSceneError`], most commonly "no pack extracted yet".
    Title(TitleSceneError),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Platform(err) => write!(f, "{err}"),
            Self::Title(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<PlatformError> for AppError {
    fn from(err: PlatformError) -> Self {
        Self::Platform(err)
    }
}

impl From<TitleSceneError> for AppError {
    fn from(err: TitleSceneError) -> Self {
        Self::Title(err)
    }
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
/// backend) and the current scene's already-composed frame, presented
/// unchanged every frame.
///
/// No engine/battle state yet (out of scope for this slice, see the crate
/// root docs). [`App::new`] composes the real title screen and keeps it
/// animating every frame (module docs' "Animating the real title screen"
/// section); [`App::new_headless`]'s synthetic [`BootScene`] stays a fixed
/// placeholder, unchanged from before I-2.
pub struct App {
    platform: Platform,
    frame: Box<Frame>,
    /// The real title screen's scene plus its running tick count, kept
    /// alive so [`App::step`] can recompose it every frame -- `None` for a
    /// headless `App` (module docs), whose [`BootScene`] frame never
    /// changes.
    title: Option<AnimatedTitle>,
}

/// [`App`]'s per-frame animation state for the real title screen (module
/// docs): the loaded scene, the tick most recently composed into
/// [`App`]'s cached `frame`, and whether that frame has been presented
/// yet (so [`App::step`] advances the tick *before* presenting every
/// frame after the first — keeping [`App::frame`]'s "most recently
/// presented" contract true at all times).
struct AnimatedTitle {
    scene: TitleScene,
    tick: u32,
    presented: bool,
}

impl App {
    /// Load the real title screen (I-2) from the local asset pack, then open
    /// a window titled `title` to present it.
    ///
    /// Loads/decodes the scene *before* opening the platform window, so a
    /// missing pack (or any other title-screen error) is surfaced cleanly
    /// without ever flashing a window open first.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Title`] if the asset pack has not been extracted
    /// yet (check [`TitleSceneError::is_pack_missing`] -- its rendered
    /// message names the exact `./init.sh`/`cargo xtask extract` commands to
    /// run) or is otherwise malformed; [`AppError::Platform`] if the
    /// platform's windowing event loop could not be created.
    pub fn new(title: impl Into<String>) -> Result<Self, AppError> {
        let scene = title::load_default()?;
        let frame = to_platform_frame(&scene.compose(0));
        let platform = Platform::new(title)?;
        Ok(Self {
            platform,
            frame,
            title: Some(AnimatedTitle {
                scene,
                tick: 0,
                presented: false,
            }),
        })
    }

    /// Build the I-1 synthetic placeholder scene against `platform`'s
    /// explicit headless/null backend (F-3, V-1) instead of a real window --
    /// deliberately *not* the real title screen (see the module docs), so
    /// this constructor's behaviour (and `xtask`'s smoke suite, which drives
    /// it) stays exactly as it was before I-2 regardless of whether a local
    /// asset pack happens to be present.
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
            title: None,
        }
    }

    /// Test-only: pair the headless/null platform backend with a real
    /// animated title scene, so the animated [`App::step`]/[`App::frame`]
    /// path is drivable without a window or display server (the public
    /// [`App::new`] always opens one).
    #[cfg(test)]
    fn new_headless_animated(scene: TitleScene) -> Self {
        let frame = to_platform_frame(&scene.compose(0));
        Self {
            platform: Platform::new_headless(),
            frame,
            title: Some(AnimatedTitle {
                scene,
                tick: 0,
                presented: false,
            }),
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
    /// Returns [`AppError::Platform`] if window/surface creation or
    /// presentation fails.
    pub fn run(&mut self) -> Result<(), AppError> {
        while self.step()? {}
        Ok(())
    }

    /// Run exactly one iteration of the frame loop body: pump input, log
    /// any newly-pressed buttons, then -- for a real title screen
    /// ([`App::new`]) whose current frame has already been presented --
    /// advance to and compose the next tick's frame, pace to the next GBA
    /// vblank (a no-op for a headless `App`, see
    /// `platform::Platform::wait_for_next_frame`), and present the
    /// composed scene (module docs' "Animating the real title screen"
    /// section). Composing *before* presenting keeps [`App::frame`]'s
    /// "most recently presented" contract true after every step, and
    /// pacing *before* presenting spaces consecutive presents one GBA
    /// frame apart -- including the first-to-second gap, which a
    /// present-then-pace order would collapse to zero on backends whose
    /// `present` doesn't block for vsync (the pacer's first tick
    /// establishes the deadline and returns immediately).
    ///
    /// Returns whether the loop should keep going -- `false` once
    /// `platform::Platform::pump` reports a close request, at which point
    /// the frame is *not* presented (mirroring the `while` loop this method
    /// replaces in [`App::run`]). Always `true` for a headless `App`, which
    /// has no window to close.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Platform`] if input pumping or presentation
    /// fails.
    pub fn step(&mut self) -> Result<bool, AppError> {
        if !self.platform.pump()? {
            return Ok(false);
        }
        if let Some(line) = describe_newly_pressed(*self.platform.buttons()) {
            eprintln!("{line}");
        }
        if let Some(title) = &mut self.title {
            if title.presented {
                title.tick = title.tick.wrapping_add(1);
                self.frame = to_platform_frame(&title.scene.compose(title.tick));
            }
            title.presented = true;
        }
        self.platform.wait_for_next_frame();
        self.platform.present(&self.frame)?;
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

    /// The animated path's `frame()` contract (I-2): after every step,
    /// `frame()` is the frame that step actually presented — the first
    /// step presents the initial tick-0 composition, the second tick 1's,
    /// and so on. Needs the real pack, like
    /// `title::tests::real_pack_composes_non_blank_deterministic_title_frames`.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn animated_frame_returns_the_presented_tick() {
        let scene = crate::title::load_default().expect("run `cargo xtask extract` first");
        let expected0 = super::to_platform_frame(&scene.compose(0));
        let expected1 = super::to_platform_frame(&scene.compose(1));
        let mut app = App::new_headless_animated(scene);

        assert_eq!(app.frame().to_vec(), expected0.to_vec());
        app.step().expect("headless step never errors");
        assert_eq!(
            app.frame().to_vec(),
            expected0.to_vec(),
            "the first step presents tick 0; frame() must still be tick 0's composition"
        );
        app.step().expect("headless step never errors");
        assert_eq!(
            app.frame().to_vec(),
            expected1.to_vec(),
            "the second step advances to and presents tick 1"
        );
    }

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
