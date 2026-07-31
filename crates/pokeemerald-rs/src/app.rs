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
//! [`App::new`]'s whole body lives in the private `App::boot` (load and
//! compose the real title screen, open a platform backend, `App::assemble`
//! the two), which takes the window-opening call as a parameter -- so
//! [`App::new`] is `Platform::new` handed to `boot`, and nothing else.
//! `tests::real_pack_boots_to_the_title_screen_through_app_boot` builds an
//! `App` through `App::new_headless_real_title`, which calls that *same*
//! `boot` with `Platform::new_headless`: the test therefore executes
//! [`App::new`]'s construction body itself, and the only line of booting it
//! does not cover is the `Platform::new` call that opens a real OS window
//! (categorically un-runnable in CI). A construction regression that has
//! any effect on the frames a booted `App` shows -- never loading the
//! title, composing the wrong tick, dropping the `to_platform_frame`
//! conversion, dropping the scene -- therefore fails the test. (The
//! load-before-open ordering is the one thing `boot` does that the test
//! cannot judge: `Platform::new_headless` opens nothing and cannot fail.
//! It is kept honest by being written exactly once, in `boot`.)
//!
//! The test then drives [`App::step`] (which pumps input, advances
//! `flow::advance_scene`'s title arm, and *presents*) and asserts
//! [`App::frame`] matches the real title screen composed independently via
//! [`title::load_default`]: tick 0 for the first presented frame, tick 2
//! after the third step, and tick 14 after the fifteenth. Tick 14 is the
//! one that pins the tick counter to *exactly* zero offset, in both
//! directions: [`crate::title`]'s animation is coarse (the clouds move once
//! every four ticks, "Press Start" blinks every sixteen), so most adjacent
//! ticks compose bit-identical frames, but tick 14 differs from tick 13
//! (the cloud scroll steps 3 -> 4) *and* from tick 15 ("Press Start" blinks
//! on) -- both asserted as `assert_ne!` guards. An `App` running one tick
//! ahead or behind cannot pass, over 15 consecutive `step` calls.
//!
//! Before this, the only pack-backed coverage of the real title screen
//! (this module's own `animated_frame_returns_the_presented_tick`, `xtask`'s
//! `check_title_screen`, and [`crate::title`]'s own tests) called
//! [`title::load_default`]/`compose` directly and never went through
//! [`App::new`]'s construction at all. This is the honest evidence for I-2
//! "boots to the title screen": the construction `main` uses, minus only
//! the window handle, driven headlessly against real extracted assets and
//! through a real present.

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
/// The one production spot that builds an [`AnimatedTitle`] (`flow`'s own
/// tests build one directly), so [`App::boot`] (which also loads the scene)
/// and [`App::new_headless_animated`] (which takes an already-loaded scene,
/// for tests that need their own reference copy to compare ticks against)
/// never hand-write these same three lines twice `(oop-boundaries)`.
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
    /// Move an already-loaded `(frame, scene)` pair (from [`App::boot`] or
    /// [`compose_title_scene`]) into a running `App` around `platform`.
    ///
    /// The *only* place a real-game-flow `App` is assembled, so [`App::new`]
    /// and its headless counterparts cannot drift apart in what they store
    /// `(oop-boundaries)`: with the struct literal written once here, a
    /// regression in what a booted `App` holds (dropping the scene, say, so
    /// the title never animates) is a regression in code the I-2 headless
    /// real-boot check (module docs) runs.
    fn assemble(platform: Platform, (frame, scene): (Box<Frame>, AppScene)) -> Self {
        Self {
            platform,
            frame,
            scene: Some(scene),
        }
    }

    /// The whole of [`App::new`]'s body: load and compose the real title
    /// screen (I-2), open a platform backend via `open_platform`, and
    /// assemble the two into a running `App`.
    ///
    /// The window-opening call is a parameter precisely because it is the
    /// one step that categorically cannot run in CI -- everything else
    /// [`App::new`] does happens here, so the test-only
    /// [`App::new_headless_real_title`] (which passes
    /// `Platform::new_headless`) executes this body *itself*, not a
    /// hand-written copy of it (I-2, issue #168; PR #175 review). That
    /// leaves [`App::new`] a bare `Platform::new` plus a call to this
    /// function: nothing of its own left to regress.
    ///
    /// Loads and composes the scene *before* calling `open_platform`, so a
    /// missing pack (or any other title-screen error) is surfaced cleanly
    /// without ever flashing a window open first -- an ordering that,
    /// written once here, holds for every constructor that boots the real
    /// game flow.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Title`] if the asset pack has not been extracted
    /// yet (check [`TitleSceneError::is_pack_missing`] -- its rendered
    /// message names the exact `./init.sh`/`cargo xtask extract` commands to
    /// run) or is otherwise malformed; whatever `open_platform` fails with
    /// (for [`App::new`], [`AppError::Platform`] if the platform's windowing
    /// event loop could not be created) otherwise.
    fn boot(
        open_platform: impl FnOnce() -> Result<Platform, PlatformError>,
    ) -> Result<Self, AppError> {
        // Load first: no window is opened if the pack is missing.
        let loaded = compose_title_scene(title::load_default()?);
        let platform = open_platform()?;
        Ok(Self::assemble(platform, loaded))
    }

    /// Load the real title screen (I-2) from the local asset pack, then open
    /// a window titled `title` to present it.
    ///
    /// Deliberately nothing but `Platform::new` handed to [`App::boot`],
    /// which holds the entire construction body -- and which the I-2
    /// headless real-boot check (module docs) runs directly. Opening the
    /// real OS window is therefore the only part of booting that this
    /// constructor does not share with that test.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Title`] if the asset pack has not been extracted
    /// yet (check [`TitleSceneError::is_pack_missing`] -- its rendered
    /// message names the exact `./init.sh`/`cargo xtask extract` commands to
    /// run) or is otherwise malformed; [`AppError::Platform`] if the
    /// platform's windowing event loop could not be created.
    pub fn new(title: impl Into<String>) -> Result<Self, AppError> {
        Self::boot(|| Platform::new(title))
    }

    /// Test-only: [`App::new`]'s own body -- [`App::boot`] -- with
    /// `platform`'s headless/null backend substituted for a real window.
    /// This is the I-2 headless real-boot check's constructor (issue #168):
    /// the same construction `main` uses via [`App::new`], up to and
    /// including the first composed frame, differing in nothing but the one
    /// part that categorically cannot run in CI (opening an actual OS
    /// window).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Title`] under the same conditions as
    /// [`App::new`] -- most commonly [`TitleSceneError::is_pack_missing`]
    /// when no local asset pack has been extracted yet.
    #[cfg(test)]
    fn new_headless_real_title() -> Result<Self, AppError> {
        Self::boot(|| Ok(Platform::new_headless()))
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
        Self::assemble(Platform::new_headless(), compose_title_scene(scene))
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
    /// through `App::new_headless_real_title`, which calls the very
    /// `App::boot` the public `App::new` calls -- so this test executes
    /// `App::new`'s whole construction body, all but its `Platform::new`
    /// call (see the module docs' "The headless real-boot check" section)
    /// -- and asserts both the frame that construction composes and the
    /// frame the first `App::step` actually *presents* are the real title
    /// screen's tick-0 composition, computed independently via
    /// `title::load_default` so these assertions can never trivially pass by
    /// comparing a value against itself.
    ///
    /// Stepping (rather than only inspecting the constructed frame) is what
    /// makes "boots to the title screen" honest: it reaches
    /// `platform::Platform::present` and runs `flow::advance_scene`'s
    /// `Title` arm on the real path, whose "first step presents tick 0"
    /// contract is the same one `animated_frame_returns_the_presented_tick`
    /// above pins down against a hand-loaded scene.
    ///
    /// Stepping on to tick 2 shows the boot keeps animating rather than
    /// freezing (tick 2 is the first tick whose composition differs from
    /// tick 0 at all: `title::cloud_scroll_y` moves the clouds only every
    /// other tick, then halves that again). Stepping on to tick 14 is what
    /// pins the counter to exactly the right tick: tick 14 differs from
    /// *both* neighbours -- from 13 because the cloud scroll steps 3 -> 4,
    /// and from 15 because "Press Start" blinks on
    /// (`title::press_start_visible`) -- and both differences are asserted
    /// below, so an `App` presenting one tick ahead of or behind the frame
    /// it should (a dropped `AnimatedTitle::presented` guard, an extra
    /// advance during construction) fails here. Every intermediate tick is
    /// stepped through, so the count has to be exact for all 15 steps.
    ///
    /// Unlike `animated_frame_returns_the_presented_tick` (which hands
    /// `App::new_headless_animated` an already-loaded scene), this test
    /// never touches `title::load_default`/`compose` on the
    /// `App`-construction side -- only `App::new`'s own shared body does,
    /// which is the whole point: a regression there (the wrong tick, a
    /// dropped `to_platform_frame` conversion, a dropped scene, never
    /// calling `title::load_default` at all) fails *this* test even though
    /// every other real-pack check in this crate composes the title screen
    /// directly and would stay green. Needs the real pack, like
    /// `animated_frame_returns_the_presented_tick`.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn real_pack_boots_to_the_title_screen_through_app_boot() {
        let reference = crate::title::load_default().expect("run `cargo xtask extract` first");
        let expected0 = reference.compose_frame(0);
        let expected2 = reference.compose_frame(2);
        let expected13 = reference.compose_frame(13);
        let expected14 = reference.compose_frame(14);
        let expected15 = reference.compose_frame(15);
        assert_ne!(
            expected0.to_vec(),
            expected2.to_vec(),
            "tick 2 must differ from tick 0, or the animation check below proves nothing"
        );
        assert_ne!(
            expected14.to_vec(),
            expected13.to_vec(),
            "tick 14 must differ from tick 13 (the clouds scroll), or the tick-14 check below \
             would pass for an App running one tick behind"
        );
        assert_ne!(
            expected14.to_vec(),
            expected15.to_vec(),
            "tick 14 must differ from tick 15 (\"Press Start\" blinks on), or the tick-14 check \
             below would pass for an App running one tick ahead"
        );

        let mut app = App::new_headless_real_title().expect("run `cargo xtask extract` first");

        assert_eq!(
            app.frame().to_vec(),
            expected0.to_vec(),
            "App::new's real construction path must compose the real title screen's tick-0 frame"
        );
        assert!(
            app.frame().iter().any(|&pixel| pixel != 0),
            "the real title screen booted through App::new must be non-blank"
        );

        app.step().expect("headless step never errors");
        assert_eq!(
            app.frame().to_vec(),
            expected0.to_vec(),
            "the first step must present that same tick-0 frame (flow's Title arm)"
        );
        for _ in 0..2 {
            app.step().expect("headless step never errors");
        }
        assert_eq!(
            app.frame().to_vec(),
            expected2.to_vec(),
            "the booted App must keep animating: the third step presents tick 2"
        );
        for _ in 0..12 {
            app.step().expect("headless step never errors");
        }
        assert_eq!(
            app.frame().to_vec(),
            expected14.to_vec(),
            "the fifteenth step must present tick 14 exactly -- one tick either way composes a \
             different frame (asserted above)"
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
