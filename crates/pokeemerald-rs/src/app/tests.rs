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

/// S-3 (issue #185): a synthetic, pack-free song that loops forever via its
/// own `Goto` -- exactly like a real BGM (see `crate::music`'s module docs
/// on why continuous playback needs no extra restart logic beyond a song's
/// own jump commands). Mirrors `crate::music::tests`' own `looping_song`.
fn looping_song_for_test() -> audio::Song {
    use audio::{Adsr, Event, Instrument, Song, ToneData, WaveData};
    use std::sync::Arc;

    let wave = Arc::new(WaveData::one_shot(1 << 20, vec![100; 64]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let events = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(50),
        Event::Goto(0),
    ];
    Song::new(voices, vec![events], 150)
}

/// The App's own "stop" cue for its title BGM (Discussion #227's owner
/// decision, S-3 issue #185): [`App::step`]'s `advance_music` **fades** an
/// attached [`crate::music::MusicPlayer`] out once [`AppScene::Title`] is no
/// longer the active scene -- upstream's `FadeOutBGM(4)`
/// (`pokeemerald/src/title_screen.c:784`) -- and drops it (stopping the
/// stream) only when that fade completes, rather than hard-cutting it or
/// leaving it running unheard.
///
/// Uses [`App::new_headless`] (the pure I-1 boot-scene path, whose
/// `AppScene` is always `None` -- never `Title`) purely as a scaffold to
/// attach a synthetic player to without needing a real asset pack; the
/// "keeps playing (and never underruns) while `Title` stays active" half
/// needs a real `TitleScene` and lives in
/// `real_pack_boot_starts_title_music_and_sustains_it_without_underrun`,
/// below.
#[test]
fn leaving_the_title_scene_fades_the_attached_music_player_out_before_stopping_it() {
    /// `m4aMPlayFadeOut`'s 16 volume steps at `TITLE_FADE_OUT_SPEED` frames
    /// each -- see `crate::music::player`'s `FadeOut` docs.
    const FADE_FRAMES: usize = 64;

    let mut app = App::new_headless();
    let output = platform::AudioOutput::null(crate::music::RING_CAPACITY_FRAMES);
    let music = crate::music::MusicPlayer::start(looping_song_for_test(), output)
        .expect("null backend never errors");
    app.attach_music_for_test(music);
    assert!(app.has_music_for_test());

    // `new_headless`'s `AppScene` is always `None`, never `Title` -- exactly
    // the "scene left Title" case `advance_music` must react to.
    let mut drained = vec![0.0_f32; audio::Sequencer::FRAME_SAMPLES];
    for frame in 1..FADE_FRAMES {
        app.step().expect("headless step never errors");
        app.drain_music_for_test(&mut drained);
        assert!(
            app.has_music_for_test(),
            "frame {frame}: advance_music must fade the BGM out across {FADE_FRAMES} frames, not \
             cut it dead"
        );
    }

    app.step().expect("headless step never errors");
    assert!(
        !app.has_music_for_test(),
        "advance_music must stop the BGM once the fade-out has run to completion"
    );
    assert_eq!(
        app.music_underruns_for_test(),
        None,
        "a completed fade drops the player outright rather than leaving it paused"
    );
}

/// The sustained half: while `AppScene::Title` stays active, repeated
/// [`App::step`] calls must keep pushing audio without underrunning when
/// drained at the same cadence. Needs the real pack, like every other
/// `App::new_headless_real_title` test in this file.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_boot_starts_title_music_and_sustains_it_without_underrun() {
    let mut app = App::new_headless_real_title().expect("run `cargo xtask extract` first");
    assert!(
        app.has_music_for_test(),
        "App::boot must have started mus_title against the real pack + null audio backend"
    );

    let mut drained = vec![0.0_f32; audio::Sequencer::FRAME_SAMPLES];
    for _ in 0..120 {
        app.step().expect("headless step never errors");
        app.drain_music_for_test(&mut drained);
    }
    assert!(
        app.has_music_for_test(),
        "the title scene never left Title across these steps, so the BGM must still be playing"
    );
    assert_eq!(
        app.music_underruns_for_test(),
        Some(0),
        "120 steps of frame-driven playback, drained once per step, must not underrun the ring"
    );
}
