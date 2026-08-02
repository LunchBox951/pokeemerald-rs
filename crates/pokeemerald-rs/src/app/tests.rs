//! Unit tests for [`super::App`]: the headless boot/step/present paths and
//! the input-log formatter.
//!
//! The I-2 real-boot checks in here (and what each of them does and does not
//! cover) are described once, in [`super`]'s module docs under "The headless
//! real-boot check" -- each test's own comment says only what it adds
//! `(lean-docs)`. Tests needing an extracted asset pack are `#[ignore]`d and
//! run by CI's `real-pack` job.

use super::{describe_newly_pressed, App, AppError};
use platform::{ButtonState, Buttons};

/// The animated path's `frame()` contract (I-2): after every step,
/// `frame()` is the frame that step actually presented — the first
/// step presents the initial tick-0 composition, the second tick 1's,
/// the third tick 2's, and so on. Ticks 0 and 1 compose pixel-identically
/// for the real title (the clouds scroll once every four ticks, "Press
/// Start" blinks every sixteen -- see `title`'s module docs), so a
/// tick-1 check alone would pass even for a frozen-tick `advance_scene`
/// regression; the tick-2 check below is what actually exercises the
/// tick counter, behind an `assert_ne!` non-vacuity guard exactly like
/// `real_pack_boots_to_the_title_screen_through_app_boot`'s tick checks,
/// so it cannot silently become vacuous again. Needs the real pack, like
/// `real_pack_boots_to_the_title_screen_through_app_boot`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn animated_frame_returns_the_presented_tick() {
    let scene = crate::title::load_default().expect("run `cargo xtask extract` first");
    let expected0 = super::to_platform_frame(&scene.compose(0));
    let expected1 = super::to_platform_frame(&scene.compose(1));
    let expected2 = super::to_platform_frame(&scene.compose(2));
    assert_ne!(
        expected2.to_vec(),
        expected0.to_vec(),
        "tick 2 must differ from tick 0, or the third-step check below proves nothing -- ticks \
         0 and 1 compose pixel-identically and so cannot guard it themselves"
    );
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
    app.step().expect("headless step never errors");
    assert_eq!(
        app.frame().to_vec(),
        expected2.to_vec(),
        "the third step advances to and presents tick 2 -- pixel-distinguishable from tick 0 \
         (asserted above), so this step's tick tracking is not vacuous"
    );
}

/// The I-2 headless real-boot check (issues #168 and #175): boots an
/// `App` through `App::boot` -- `App::new`'s own body -- and asserts the
/// frames it *presents* are the real title screen's, at exactly ticks 0,
/// 2 and 14. See the module docs' "The headless real-boot check" for
/// what that does and does not cover, why the presented frames are read
/// back from the null backend rather than from `App::frame`, and why
/// tick 14 is the tick that pins the counter. Needs the real pack, like
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
        "App::boot -- App::new's own construction body -- must compose the real title screen's \
         tick-0 frame"
    );
    assert!(
        app.frame().iter().any(|&pixel| pixel != 0),
        "the real title screen booted through App::boot must be non-blank"
    );

    assert!(
        app.platform.last_presented().is_none(),
        "construction alone must not present anything yet"
    );

    app.step().expect("headless step never errors");
    assert_eq!(
        presented(&app),
        expected0.to_vec(),
        "the first step must present that same tick-0 frame (flow's Title arm)"
    );
    for _ in 0..2 {
        app.step().expect("headless step never errors");
    }
    assert_eq!(
        presented(&app),
        expected2.to_vec(),
        "the booted App must keep animating: the third step presents tick 2"
    );
    for _ in 0..12 {
        app.step().expect("headless step never errors");
    }
    assert_eq!(
        presented(&app),
        expected14.to_vec(),
        "the fifteenth step must present tick 14 exactly -- one tick either way composes a \
         different frame (asserted above)"
    );
}

/// The frame `app`'s headless backend actually received from
/// `Platform::present` (module docs: not `App::frame`, which is set
/// before presenting and so cannot witness a dropped present).
fn presented(app: &App) -> Vec<u32> {
    app.platform
        .last_presented()
        .expect("step must have presented a frame")
        .to_vec()
}

/// Whether the two pack-less checks below can run here: they need
/// `title::load_default` to fail with exactly the "no pack extracted yet"
/// diagnostic they then assert on. An extracted pack (CI's `real-pack` job)
/// or a *malformed* one is a different situation entirely -- skip loudly,
/// naming `test`, rather than fail it and blame `App::new`.
fn no_pack_here(test: &str) -> bool {
    match crate::title::load_default() {
        Ok(_) => {
            eprintln!("skipped {test}: a local asset pack is extracted");
            false
        }
        Err(err) if err.is_pack_missing() => true,
        Err(err) => {
            eprintln!("skipped {test}: the local pack exists but does not load: {err}");
            false
        }
    }
}

/// Covers the public `App::new` itself -- the exact function `main`
/// calls, compiled exactly as it ships -- via its missing-pack path: it
/// must fail in the title load and so never reach `Platform::new`
/// (module docs' "Which code the tests actually run").
#[test]
fn without_a_pack_app_new_fails_the_load_before_opening_a_window() {
    if !no_pack_here("without_a_pack_app_new_fails_the_load_before_opening_a_window") {
        return;
    }
    let Err(err) = App::new("pokeemerald-rs (test)") else {
        panic!("with no pack extracted, App::new must fail");
    };
    assert!(
        matches!(&err, AppError::Title(title) if title.is_pack_missing()),
        "App::new must surface the missing pack as AppError::Title -- an AppError::Platform \
         here would mean it tried to open the window first: {err}"
    );
}

/// `App::boot`'s load-before-open ordering, pinned directly: the opener
/// it is handed must not run at all when the title load fails.
#[test]
fn boot_opens_no_platform_when_the_title_screen_fails_to_load() {
    if !no_pack_here("boot_opens_no_platform_when_the_title_screen_fails_to_load") {
        return;
    }
    let mut opened = false;
    let Err(err) = App::boot(|| {
        opened = true;
        Ok(platform::Platform::new_headless())
    }) else {
        panic!("with no pack extracted, boot must fail");
    };
    assert!(
        !opened,
        "boot must not open a platform when the title load failed"
    );
    assert!(matches!(err, AppError::Title(_)), "got: {err}");
}

/// The other half of that ordering: once the title screen *has* loaded,
/// `App::boot` does reach its opener, and a failure there surfaces as
/// `AppError::Platform`. The error variant is arbitrary -- any
/// `PlatformError` would do, and `NoAudioDevice` is simply the one that
/// can be constructed without a real winit/softbuffer failure.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_boot_propagates_a_platform_opener_error() {
    let Err(err) = App::boot(|| Err(platform::PlatformError::NoAudioDevice)) else {
        panic!("the opener failed, so boot must fail");
    };
    assert!(matches!(err, AppError::Platform(_)), "got: {err}");
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
