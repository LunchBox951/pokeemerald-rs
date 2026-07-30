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
//!
//! # Game flow: title -> main menu -> intro -> overworld (I-3, issue #149)
//!
//! [`App::new`]'s real (windowed) path is a small state machine owned by
//! [`crate::flow`]: every [`App::step`] hands the current
//! [`crate::flow::AppScene`] to [`crate::flow::advance_scene`], which
//! advances it by one frame and returns the (possibly transitioned) next
//! scene plus its composed frame. See [`crate::flow`]'s module docs for
//! the full title -> main menu -> intro -> overworld transition diagram and
//! its "log-or-ignore is fine" failure policy for a transition's own pack
//! load.
//!
//! # The headless real-boot check (I-2, issue #168)
//!
//! [`App::new`]'s title-load wiring is factored into [`load_real_title`]
//! precisely so it has a second, test-only caller:
//! `tests::real_pack_boots_to_the_title_screen_through_app_new` builds an
//! `App` through [`App::new_headless_real_title`] -- the same
//! [`load_real_title`] call [`App::new`] itself makes, paired with
//! `platform`'s null backend instead of a real window -- and asserts the
//! resulting [`App::frame`] matches the real title screen composed
//! independently via [`title::load_default`]. Before this, the only
//! pack-backed coverage of the real title screen (this module's own
//! `animated_frame_returns_the_presented_tick`, `xtask`'s
//! `check_title_screen`, and [`crate::title`]'s own tests) called
//! [`title::load_default`]/`compose` directly and never went through
//! [`App::new`]'s construction at all -- so a regression in *that* wiring
//! (forgetting to load the title, composing the wrong tick, dropping the
//! `to_platform_frame` conversion) could stay green. This is the honest
//! evidence for I-2 "boots to the title screen": the exact construction
//! `main` uses, driven headlessly against real extracted assets.

use platform::{ButtonState, Buttons, Frame, Platform, PlatformError};

use crate::flow::{self, AnimatedTitle, AppScene};
use crate::frame::to_platform_frame;
use crate::scene::BootScene;
use crate::title::{self, TitleSceneError};

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
    /// The real game-flow state ([`crate::flow`], module docs' "Game flow"
    /// section) -- `None` for a headless `App` (module docs), whose
    /// [`BootScene`] frame never changes.
    scene: Option<AppScene>,
}

/// Compose an already-loaded [`title::TitleScene`] at tick 0 into the
/// `(frame, scene)` pair [`App::new`] (and its headless test counterparts)
/// store: the tick-0 composed frame, plus the fresh [`AppScene::Title`] /
/// [`AnimatedTitle`] state [`App::step`] advances from there.
///
/// The one spot that builds an [`AnimatedTitle`], so [`App::load_real_title`]
/// (which also loads the scene) and [`App::new_headless_animated`] (which
/// takes an already-loaded scene, for tests that need their own reference
/// copy to compare ticks against) never hand-write these same three lines
/// twice `(oop-boundaries)`.
fn compose_title_scene(scene: title::TitleScene) -> (Box<Frame>, AppScene) {
    let frame = to_platform_frame(&scene.compose(0));
    let app_scene = AppScene::Title(Box::new(AnimatedTitle {
        scene,
        tick: 0,
        presented: false,
    }));
    (frame, app_scene)
}

impl App {
    /// Load the real title screen (I-2) -- the exact wiring [`App::new`]
    /// runs -- into a `(frame, scene)` pair ready to move into an `App`
    /// alongside whichever `Platform` backend the caller opens.
    ///
    /// Factored out so [`App::new`] (a real window) and
    /// [`App::new_headless_real_title`] (`platform`'s null backend, test-only)
    /// share this exact code path (I-2, issue #168): before this split, the
    /// headless test coverage for the real title screen called
    /// [`title::load_default`] and composed the scene itself, a *separate*
    /// hand-written copy of these same steps that a regression in `App::new`'s
    /// own wiring could slip past. Deliberately returns before any `Platform`
    /// exists, preserving [`App::new`]'s "surfaced cleanly without ever
    /// flashing a window open first" contract for a missing/malformed pack.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Title`] if the asset pack has not been extracted
    /// yet (check [`TitleSceneError::is_pack_missing`] -- its rendered
    /// message names the exact `./init.sh`/`cargo xtask extract` commands to
    /// run) or is otherwise malformed.
    fn load_real_title() -> Result<(Box<Frame>, AppScene), AppError> {
        let scene = title::load_default()?;
        Ok(compose_title_scene(scene))
    }

    /// Load the real title screen (I-2) from the local asset pack, then open
    /// a window titled `title` to present it.
    ///
    /// Loads/decodes the scene ([`App::load_real_title`]) *before* opening
    /// the platform window, so a missing pack (or any other title-screen
    /// error) is surfaced cleanly without ever flashing a window open first.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Title`] if the asset pack has not been extracted
    /// yet (check [`TitleSceneError::is_pack_missing`] -- its rendered
    /// message names the exact `./init.sh`/`cargo xtask extract` commands to
    /// run) or is otherwise malformed; [`AppError::Platform`] if the
    /// platform's windowing event loop could not be created.
    pub fn new(title: impl Into<String>) -> Result<Self, AppError> {
        let (frame, scene) = Self::load_real_title()?;
        let platform = Platform::new(title)?;
        Ok(Self {
            platform,
            frame,
            scene: Some(scene),
        })
    }

    /// Test-only: [`App::new`]'s real title-load wiring
    /// ([`App::load_real_title`]), paired with `platform`'s headless/null
    /// backend instead of a real window -- the I-2 headless real-boot check
    /// (issue #168): the same construction `main` uses via [`App::new`], up
    /// to and including the first composed frame, minus only the one part
    /// that categorically cannot run in CI (opening an actual OS window).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Title`] under the same conditions as
    /// [`App::new`] -- most commonly [`TitleSceneError::is_pack_missing`]
    /// when no local asset pack has been extracted yet.
    #[cfg(test)]
    fn new_headless_real_title() -> Result<Self, AppError> {
        let (frame, scene) = Self::load_real_title()?;
        Ok(Self {
            platform: Platform::new_headless(),
            frame,
            scene: Some(scene),
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
            scene: None,
        }
    }

    /// Test-only: pair the headless/null platform backend with a real
    /// animated title scene, so the animated [`App::step`]/[`App::frame`]
    /// path is drivable without a window or display server (the public
    /// [`App::new`] always opens one).
    #[cfg(test)]
    fn new_headless_animated(scene: title::TitleScene) -> Self {
        let (frame, app_scene) = compose_title_scene(scene);
        Self {
            platform: Platform::new_headless(),
            frame,
            scene: Some(app_scene),
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
    /// any newly-pressed buttons, then -- for a real game-flow `App`
    /// ([`App::new`]) -- advance [`crate::flow::AppScene`] by one frame via
    /// [`crate::flow::advance_scene`] (module docs' "Game flow" section;
    /// for the title phase specifically this is "whose current frame has
    /// already been presented, advance to and compose the next tick's
    /// frame," exactly as before I-3), pace to the next GBA vblank (a
    /// no-op for a headless `App`, see
    /// `platform::Platform::wait_for_next_frame`), and present the
    /// composed scene. Composing *before* presenting keeps [`App::frame`]'s
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
        let buttons = *self.platform.buttons();
        if let Some(line) = describe_newly_pressed(buttons) {
            eprintln!("{line}");
        }
        if let Some(scene) = self.scene.take() {
            let (next, frame) = flow::advance_scene(scene, buttons);
            self.scene = Some(next);
            self.frame = frame;
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

    /// The I-2 headless real-boot check (issue #168): builds an `App`
    /// through [`App::new_headless_real_title`] -- [`App::load_real_title`],
    /// the exact wiring `main` reaches via the public [`App::new`], paired
    /// with `platform`'s null backend instead of a real window (see the
    /// module docs' "The headless real-boot check" section) -- and asserts
    /// the resulting [`App::frame`] is the real title screen's tick-0
    /// composition, computed independently via `title::load_default` so
    /// this assertion can never trivially pass by comparing a value against
    /// itself. Unlike `animated_frame_returns_the_presented_tick` above
    /// (which hands `App::new_headless_animated` an already-loaded scene),
    /// this test never touches `title::load_default`/`compose` itself on
    /// the `App`-construction side -- only [`App::new`]'s own wiring does,
    /// which is the whole point: a regression there (the wrong tick, a
    /// dropped `to_platform_frame` conversion, `App::new` never calling
    /// [`title::load_default`] at all) fails *this* test even though every
    /// other real-pack check in this crate composes the title screen
    /// directly and would stay green. Needs the real pack, like
    /// `animated_frame_returns_the_presented_tick`.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn real_pack_boots_to_the_title_screen_through_app_new() {
        let expected = crate::title::load_default()
            .expect("run `cargo xtask extract` first")
            .compose_frame(0);

        let app = App::new_headless_real_title().expect("run `cargo xtask extract` first");

        assert_eq!(
            app.frame().to_vec(),
            expected.to_vec(),
            "App::new's real construction path must present the real title screen's tick-0 frame"
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
