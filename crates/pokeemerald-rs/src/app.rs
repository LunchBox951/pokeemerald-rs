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
//! [`App`] also owns the session's save medium (I-6, issue #214): the
//! [`SaveSlot`] it hands to every [`crate::flow::advance_scene`] call, and to
//! [`crate::flow::save_on_exit`] on the one frame [`App::step`] observes a
//! close request. That shutdown write is the only place this shell touches
//! save state directly -- see [`crate::flow`]'s module docs for both ends.
//!
//! # The headless real-boot check (I-2, issues #168 and #175)
//!
//! The single home for this rationale: the items below cross-reference it
//! rather than restating it `(lean-docs)`.
//!
//! ## Which code the tests actually run
//!
//! [`App::new`]'s whole body lives in the private `App::boot` -- load and
//! compose the real title screen, open a platform backend (passed in as a
//! closure), `App::assemble` the two -- leaving [`App::new`] itself that
//! closure (`Platform::new`) and nothing else. What covers it, in this
//! module's `tests` submodule:
//!
//! - `tests::real_pack_boots_to_the_title_screen_through_app_boot` reaches
//!   `boot` via `App::new_headless_real_title`, i.e. with
//!   `Platform::new_headless`, so every construction step [`App::new`]
//!   performs runs under test. The one line it cannot run is `Platform::new`
//!   itself: no CI job may open an OS window.
//! - `tests::without_a_pack_app_new_fails_the_load_before_opening_a_window`
//!   calls the real, production-compiled [`App::new`] -- the exact function
//!   `main` calls -- where there is no extracted pack, so it must fail in
//!   the load and never reach `Platform::new`. That is the wrapper's
//!   *delegation* coverage: an [`App::new`] that stopped handing off to
//!   `boot` fails it. Deliberately *not* a `#[cfg(test)]`-substituted
//!   opener, which would make the [`App::new`] under test a different
//!   compilation from the shipped one.
//! - `tests::boot_opens_no_platform_when_the_title_screen_fails_to_load`
//!   and `tests::real_pack_boot_propagates_a_platform_opener_error` are the
//!   deterministic proof of the "no window is flashed open when the pack is
//!   missing" ordering, passing `boot` openers that record whether they ran
//!   (never, when the load fails) and that fail (surfacing as
//!   [`AppError::Platform`], so the opener *is* reached once the load
//!   succeeds). Ordering is checked here, at the seam, rather than through
//!   [`App::new`], where a reordered open would only misbehave on a machine
//!   with no display -- and where a `cfg(test)` headless opener would hide
//!   it entirely, since such an opener cannot fail.
//!
//! ## What the boot check asserts
//!
//! It drives [`App::step`] (pump input, advance `flow::advance_scene`'s
//! title arm, pace, present) and compares against title frames composed
//! independently via [`title::load_default`], so no assertion can pass by
//! comparing a value with itself. The presented-frame assertions read
//! `platform::Platform::last_presented` -- the frame the null backend
//! actually received -- rather than [`App::frame`], which `step` sets
//! *before* presenting and which would therefore stay convincing even if
//! `present` were never called at all.
//!
//! Ticks asserted: 0 (the first `step`), 2 (the third), 14 (the fifteenth).
//! Tick 14 is what pins the tick counter to exactly zero offset in both
//! directions. [`crate::title`]'s animation is coarse -- the clouds move
//! once every four ticks, "Press Start" blinks every sixteen -- so most
//! adjacent ticks compose bit-identical frames; tick 14 differs from tick 13
//! (cloud scroll 3 -> 4) *and* from tick 15 ("Press Start" blinks on), and
//! both differences are asserted as `assert_ne!` guards. Across 15
//! consecutive steps an `App` running one tick ahead or behind cannot pass.
//!
//! This is the evidence for I-2 "boots to the title screen". Before it, the
//! pack-backed title coverage (`animated_frame_returns_the_presented_tick`,
//! `xtask`'s `check_title_screen`, [`crate::title`]'s own tests) all called
//! [`title::load_default`]/`compose` directly, and went through neither
//! construction nor presentation.

use platform::{ButtonState, Buttons, Frame, Platform, PlatformError};

use crate::flow::{self, AnimatedTitle, AppScene};
use crate::frame::to_platform_frame;
use crate::game_save::SaveSlot;
use crate::music::MusicPlayer;
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
    /// This session's save medium (I-6, issue #214): read by the `Title` ->
    /// `MainMenu` transition, written by [`flow::save_on_exit`] on the way
    /// out. Owned here, at the one place with a whole-session lifetime, and
    /// passed down `(oop-boundaries)` -- upstream's equivalent is flash plus
    /// the `gSaveCounter`/`gLastWrittenSector` globals.
    save_slot: SaveSlot,
    /// This session's title-screen BGM (S-3, issue #185): `Some` for exactly
    /// as long as [`AppScene::Title`] is the active scene (see
    /// [`Self::advance_music`]) and a pack/audio device were both available
    /// at boot -- `None` otherwise, including for every headless `App` this
    /// module's other constructors build, which never attempt to open one.
    /// Best-effort by design: a missing pack or audio device silences the
    /// BGM rather than failing the whole boot (module docs' "log-or-ignore
    /// is fine" policy, matching issue #70's precedent for input).
    music: Option<MusicPlayer>,
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
    /// `(oop-boundaries)` -- and so this struct literal is code the I-2
    /// real-boot check (module docs) runs.
    fn assemble(platform: Platform, (frame, scene): (Box<Frame>, AppScene)) -> Self {
        Self {
            platform,
            frame,
            scene: Some(scene),
            save_slot: SaveSlot::default_location(),
            music: None,
        }
    }

    /// The whole of [`App::new`]'s body: load and compose the real title
    /// screen (I-2), open a platform backend via `open_platform`, and
    /// assemble the two into a running `App`.
    ///
    /// Loads *before* calling `open_platform`, so a missing pack (or any
    /// other title-screen error) is surfaced cleanly without ever flashing a
    /// window open first. Taking the opener as a parameter is what lets the
    /// headless tests run this body itself rather than a copy -- see the
    /// module docs' "The headless real-boot check".
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
    /// which holds the entire construction body -- see the module docs' "The
    /// headless real-boot check" for how both halves are covered.
    ///
    /// Also starts the title screen's BGM (S-3, issue #185) via
    /// [`Self::start_title_music`] -- best-effort, so a missing pack or audio
    /// device silences the BGM rather than failing the boot the title
    /// screen's own graphics already succeeded at (see [`Self::music`]'s
    /// field docs).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Title`] if the asset pack has not been extracted
    /// yet (check [`TitleSceneError::is_pack_missing`] -- its rendered
    /// message names the exact `./init.sh`/`cargo xtask extract` commands to
    /// run) or is otherwise malformed; [`AppError::Platform`] if the
    /// platform's windowing event loop could not be created.
    pub fn new(title: impl Into<String>) -> Result<Self, AppError> {
        let mut app = Self::boot(|| Platform::new(title))?;
        app.music = Self::start_title_music(|| {
            platform::AudioOutput::open(crate::music::RING_CAPACITY_FRAMES)
        });
        Ok(app)
    }

    /// Test-only: [`App::boot`] -- [`App::new`]'s own body -- with
    /// `platform`'s headless/null backend substituted for a real window, the
    /// I-2 real-boot check's constructor (module docs). Also attempts to
    /// start the title BGM against `platform`'s headless/null audio backend
    /// (mirroring [`App::new`]'s real path, minus the real device), so the
    /// I-2 real-boot check's own frame assertions run against exactly the
    /// same per-step body [`App::new`] does.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Title`] under the same conditions as
    /// [`App::new`] -- most commonly [`TitleSceneError::is_pack_missing`]
    /// when no local asset pack has been extracted yet.
    #[cfg(test)]
    fn new_headless_real_title() -> Result<Self, AppError> {
        let mut app = Self::boot(|| Ok(Platform::new_headless()))?;
        app.music = Self::start_title_music(|| {
            Ok(platform::AudioOutput::null(
                crate::music::RING_CAPACITY_FRAMES,
            ))
        });
        Ok(app)
    }

    /// Best-effort: load the asset pack a second time (deliberately -- see
    /// [`Self::boot`]'s own load, which does not expose the [`assets::AssetPack`]
    /// it built) and start `mus_title` through `open_audio`, logging and
    /// returning `None` on any failure instead of propagating it -- see
    /// [`Self::music`]'s field docs on why this stays best-effort.
    fn start_title_music(
        open_audio: impl FnOnce() -> Result<platform::AudioOutput, PlatformError>,
    ) -> Option<MusicPlayer> {
        let pack = match assets::AssetPack::load_default() {
            Ok(pack) => pack,
            Err(err) => {
                eprintln!("music: {err} -- the title screen will play without music");
                return None;
            }
        };
        match MusicPlayer::start_from_pack(&pack, "mus_title", open_audio) {
            Ok(player) => Some(player),
            Err(err) => {
                eprintln!("music: {err} -- the title screen will play without music");
                None
            }
        }
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
            save_slot: SaveSlot::default_location(),
            music: None,
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
            // I-6's write trigger, and the only one this slice ships: on the
            // way out, persist whatever save state the current scene holds.
            // See `flow::save_on_exit` for why this stands in for upstream's
            // start-menu SAVE flow, and why a failure here is logged rather
            // than turned into an `AppError` -- the window is already
            // closing, and there is nothing left to show a diagnostic on.
            if let Some(Err(err)) = self
                .scene
                .as_ref()
                .and_then(|scene| flow::save_on_exit(scene, &mut self.save_slot))
            {
                eprintln!("save: {err} -- the game was not saved on exit");
            }
            return Ok(false);
        }
        let buttons = *self.platform.buttons();
        if let Some(line) = describe_newly_pressed(buttons) {
            eprintln!("{line}");
        }
        if let Some(scene) = self.scene.take() {
            let (next, frame) = flow::advance_scene(scene, buttons, &mut self.save_slot);
            self.scene = Some(next);
            self.frame = frame;
        }
        self.advance_music();
        self.platform.wait_for_next_frame();
        self.platform.present(&self.frame)?;
        Ok(true)
    }

    /// The title flow's own "play/stop" cue for its BGM (Discussion #227's
    /// owner decision): render and push one more frame of `mus_title` while
    /// [`AppScene::Title`] is the active scene, or drop the player (stopping
    /// the stream -- [`Self::music`]'s field docs) the moment it stops being
    /// one. A no-op either way when [`Self::music`] is already `None` (no
    /// pack/audio device at boot, or a headless `App` that never requested
    /// one).
    fn advance_music(&mut self) {
        if matches!(self.scene, Some(AppScene::Title(_))) {
            if let Some(music) = &mut self.music {
                music.advance_frame();
            }
        } else {
            self.music = None;
        }
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

#[cfg(test)]
impl App {
    /// Test-only: attach an already-started [`MusicPlayer`] directly (bypassing
    /// [`Self::start_title_music`]'s pack load), for tests that want to drive
    /// [`Self::advance_music`] without a real asset pack.
    fn attach_music_for_test(&mut self, music: MusicPlayer) {
        self.music = Some(music);
    }

    /// Test-only: whether [`Self::music`] is currently playing.
    fn has_music_for_test(&self) -> bool {
        self.music.is_some()
    }

    /// Test-only: drain this session's music ring by hand (mirrors
    /// `platform::AudioOutput::pull_null`/`MusicPlayer::drain_null_for_test`),
    /// so a test can prove the frame-driven push [`Self::step`] performs
    /// never underruns when paired with a steady drain -- exactly what a
    /// real device callback provides. A no-op if no music is playing.
    fn drain_music_for_test(&mut self, out: &mut [f32]) {
        if let Some(music) = &mut self.music {
            music.drain_null_for_test(out);
        }
    }

    /// Test-only: this session's music underrun count, or `None` if no
    /// music is playing.
    fn music_underruns_for_test(&self) -> Option<u64> {
        self.music.as_ref().map(MusicPlayer::underruns)
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
mod tests;
