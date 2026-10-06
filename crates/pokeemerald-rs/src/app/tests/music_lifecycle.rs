//! Title-music lifecycle tests: the attached player's fade-out, tail, and
//! underrun behaviour across scene changes, and the start-failure message.

use super::super::{title_music_start_failure_message, App, SaveSlot};
use crate::music::MusicError;
use assets::pack::PackError;
use platform::Buttons;

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
/// stream) only once that fade has completed AND the ring has drained
/// (issue #458), rather than hard-cutting it, leaving it running unheard, or
/// truncating the still-buffered tail the instant the fade reaches silence.
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
    /// Generous bound on the extra steps needed to drain the ring's queued
    /// tail after the fade reaches silence (issue #458) -- well above the
    /// handful of frames prefill can leave queued, so a regression that
    /// never drains fails this test instead of hanging it.
    const DRAIN_BUDGET: usize = 200;

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
        app.has_music_for_test(),
        "the fade reaches silence on frame {FADE_FRAMES}, but the ring still buffers queued \
         audio at that instant -- advance_music must keep the player alive until it drains \
         rather than dropping it the moment the fade finishes"
    );

    let mut dropped_within_budget = false;
    for _ in 0..DRAIN_BUDGET {
        app.step().expect("headless step never errors");
        if !app.has_music_for_test() {
            dropped_within_budget = true;
            break;
        }
        app.drain_music_for_test(&mut drained);
    }
    assert!(
        dropped_within_budget,
        "advance_music must eventually drop the player once its queued tail drains, within \
         {DRAIN_BUDGET} steps"
    );
    assert_eq!(
        app.music_underruns_for_test(),
        None,
        "a completed fade drops the player outright rather than leaving it paused"
    );
}

/// [`looping_song_for_test`]'s sustained counterpart: one tied note on a
/// looping wave, so the song is still sounding when the fade's terminal step
/// lands and the master-mix reverb ring is full of its frames. Its flat
/// envelope releases instantly, so that ring is the only thing left to sound
/// once the terminal step stops the track.
fn sustained_reverbed_song_for_test(reverb_level: u8) -> audio::Song {
    use audio::{Adsr, Event, Instrument, Song, ToneData, WaveData};
    use std::sync::Arc;

    let wave = Arc::new(WaveData::looping(1 << 20, 0, vec![100; 64]));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let events = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(200),
        Event::Goto(0),
    ];
    Song::new(voices, vec![events], 150).with_reverb(reverb_level)
}

/// The fade's terminal step stops every track, but the master-mix reverb
/// ring still holds the `DirectSound` frames it delayed, and upstream's mixer
/// keeps running through the paused player (`SoundMain` and
/// `SoundMainRAM_Reverb`, `m4a_1.s:20`-`:119`). So [`App::advance_music`]
/// must keep rendering frames past [`crate::music::MusicPlayer::fade_finished`]
/// until that tail has rung down, instead of polling `drained` the instant
/// the fade reports finished and cutting the wet tail short (issue #1281).
///
/// Drains the whole ring each step, so what the consumer hears on a step is
/// what that step rendered rather than prefill queued nine frames earlier.
#[test]
fn a_faded_reverbed_songs_tail_keeps_sounding_past_the_terminal_fade_step() {
    /// `m4aMPlayFadeOut`'s 16 volume steps at `TITLE_FADE_OUT_SPEED` frames
    /// each: the terminal step lands on this frame.
    const FADE_FRAMES: usize = 64;
    /// Generous bound on the frames the reverb ring needs to ring down, so a
    /// regression that renders forever fails here instead of hanging.
    const TAIL_BUDGET: usize = 200;
    /// `mus_title`'s own level (`crate::music`'s reverb tests use the same).
    const REVERB_LEVEL: u8 = 50;

    let mut app = App::new_headless();
    let output = platform::AudioOutput::null(crate::music::RING_CAPACITY_FRAMES);
    let music =
        crate::music::MusicPlayer::start(sustained_reverbed_song_for_test(REVERB_LEVEL), output)
            .expect("null backend never errors");
    app.attach_music_for_test(music);

    let mut drained = vec![
        0.0_f32;
        crate::music::RING_CAPACITY_FRAMES
            * usize::from(platform::AudioOutput::CHANNELS)
    ];
    for _ in 0..FADE_FRAMES {
        app.step().expect("headless step never errors");
        app.drain_music_for_test(&mut drained);
    }
    assert!(
        app.has_music_for_test(),
        "sanity: the terminal step lands on frame {FADE_FRAMES}, with the player still attached"
    );

    let audible_after_terminal = scan_post_terminal_tail(&mut drained, TAIL_BUDGET, |drained| {
        app.step().expect("headless step never errors");
        app.drain_music_for_test(drained);
        app.has_music_for_test()
    });

    assert!(
        audible_after_terminal,
        "advance_music must keep rendering the reverb tail the ring still holds after the \
         terminal fade step, not stop the moment the fade reports finished"
    );
    assert!(
        !app.has_music_for_test(),
        "the tail must ring down and the player be dropped within {TAIL_BUDGET} further steps"
    );
}

/// Steps the post-terminal tail up to `budget` times and reports whether a
/// step left the player attached with a nonzero sample. `step` runs one app
/// step plus its drain and returns whether the player is still attached.
///
/// Zeroes `drained` before every step: `drain_music_for_test` is a no-op once
/// the player is gone, so an unzeroed buffer would still hold the previous
/// step's samples and score a premature drop as an audible tail.
fn scan_post_terminal_tail(
    drained: &mut [f32],
    budget: usize,
    mut step: impl FnMut(&mut [f32]) -> bool,
) -> bool {
    let mut audible = false;
    for _ in 0..budget {
        drained.fill(0.0);
        let attached = step(drained);
        audible |= attached && drained.iter().any(|&sample| sample != 0.0);
        if !attached {
            break;
        }
    }
    audible
}

/// The scan must not count stale samples: a player dropped on the first
/// post-terminal step, with the terminal step's wet samples still in the
/// buffer, is no audible tail.
#[test]
fn a_first_post_terminal_drop_is_not_scored_as_an_audible_tail() {
    let mut app = App::new_headless();
    let output = platform::AudioOutput::null(crate::music::RING_CAPACITY_FRAMES);
    let music = crate::music::MusicPlayer::start(sustained_reverbed_song_for_test(50), output)
        .expect("null backend never errors");
    app.attach_music_for_test(music);

    let mut drained = vec![1.0_f32; 16];
    let audible = scan_post_terminal_tail(&mut drained, 4, |_| {
        app.music = None;
        app.has_music_for_test()
    });

    assert!(
        !audible,
        "stale terminal-step samples were scored as an audible tail"
    );
}

/// The ring must be fully drained, not merely silent by the fade math,
/// before [`App::advance_music`] drops the player, or the still-buffered
/// tail is truncated.
///
/// Deliberately opens a non-default ring capacity (not
/// [`crate::music::RING_CAPACITY_FRAMES`]) so this also catches a fix that
/// compares against that module constant instead of the ring the player was
/// actually given.
#[test]
fn every_queued_fade_frame_reaches_the_consumer_before_the_player_is_dropped() {
    const RING_CAPACITY_FRAMES: usize = 512;
    /// Bounds the test itself so a regression that never drops the player
    /// fails loudly instead of looping forever.
    const STEP_BUDGET: usize = 500;

    let mut app = App::new_headless();
    let output = platform::AudioOutput::null(RING_CAPACITY_FRAMES);
    let music = crate::music::MusicPlayer::start(looping_song_for_test(), output)
        .expect("null backend never errors");
    app.attach_music_for_test(music);

    let full_ring = RING_CAPACITY_FRAMES * usize::from(platform::AudioOutput::CHANNELS);
    let mut drained = vec![0.0_f32; audio::Sequencer::FRAME_SAMPLES];
    let mut ring_free_before_drop = None;
    let mut dropped = false;

    for _ in 0..STEP_BUDGET {
        app.step().expect("headless step never errors");
        if !app.has_music_for_test() {
            dropped = true;
            break;
        }
        app.drain_music_for_test(&mut drained);
        ring_free_before_drop = app.music_ring_free_for_test();
    }

    assert!(
        dropped,
        "the fading player must eventually be dropped within {STEP_BUDGET} steps"
    );
    assert_eq!(
        ring_free_before_drop,
        Some(full_ring),
        "every queued fade frame must reach the consumer: the ring must already be fully \
         drained the step before the player is dropped, not still holding a truncated tail"
    );
}

/// An empty ring means the output callback took the samples, not that the
/// device sounded them, and dropping the player closes the stream where it
/// stands instead of playing out its callback and OS buffers. So
/// [`App::advance_music`] must hold a healthy player past the empty ring for
/// its device tail rather than dropping it on the first empty poll, which
/// would still cut the quietest end of the fade on a real backend. The null
/// backend advertises no callback bound, so its tail is the floor every
/// device gets at least; it also sounds every sample the instant it is
/// removed, so this pins the policy the real device needs.
#[test]
fn the_fading_player_outlives_its_empty_ring_by_the_device_tail() {
    const RING_CAPACITY_FRAMES: usize = 512;
    /// Bounds the test itself so a regression that never drops the player
    /// fails loudly instead of looping forever.
    const STEP_BUDGET: usize = 500;

    let mut app = App::new_headless();
    let output = platform::AudioOutput::null(RING_CAPACITY_FRAMES);
    let music = crate::music::MusicPlayer::start(looping_song_for_test(), output)
        .expect("null backend never errors");
    app.attach_music_for_test(music);

    let full_ring = RING_CAPACITY_FRAMES * usize::from(platform::AudioOutput::CHANNELS);
    let mut drained = vec![0.0_f32; audio::Sequencer::FRAME_SAMPLES];
    let mut steps_survived_with_an_empty_ring = 0_usize;
    let mut dropped = false;

    for _ in 0..STEP_BUDGET {
        let ring_was_empty = app.music_ring_free_for_test() == Some(full_ring);
        app.step().expect("headless step never errors");
        if !app.has_music_for_test() {
            dropped = true;
            break;
        }
        if ring_was_empty {
            steps_survived_with_an_empty_ring += 1;
        }
        app.drain_music_for_test(&mut drained);
    }

    assert!(
        dropped,
        "the device tail is bounded: the player must still be dropped within {STEP_BUDGET} steps"
    );
    assert_eq!(
        steps_survived_with_an_empty_ring,
        crate::music::DEVICE_TAIL_FLOOR_FRAMES,
        "advance_music must keep the stream open for the whole device tail after the ring reads          empty, so the buffers the callback already took can sound before the drop"
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

/// Issue #902 regression: pins [`title_music_start_failure_message`]'s exact
/// output so a reintroduced `music: ` prefix (see its own doc comment) fails
/// loudly instead of rendering as `music: music: ...`.
#[test]
fn title_music_start_failure_names_its_subsystem_once() {
    let err = MusicError::Pack(PackError::UnknownAsset("audio/song/mus_title".into()));
    let message = title_music_start_failure_message(&err);
    assert_eq!(
        message,
        "music: asset pack: no entry with id `audio/song/mus_title` -- the title screen will \
         play without music"
    );
    assert_eq!(
        message.matches("music:").count(),
        1,
        "the recovery log must name its subsystem once, not once per error layer: {message}"
    );
}

/// A failed menu load after the title fade-wait leaves the restored title
/// with its music playing.
///
/// Needs the real pack, so it is `#[ignore]`d and runs in CI's `real-pack` job.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn failed_menu_load_after_title_fade_wait_leaves_title_music_playing() {
    let title_scene = crate::title::load_repo().expect("run `cargo xtask extract` first");
    let missing: &'static std::path::Path =
        Box::leak(std::path::PathBuf::from("/nonexistent/birch/menu.pack").into_boxed_path());
    let mut app = App::assemble(
        platform::Platform::new_headless(),
        crate::app::compose_title_scene(title_scene),
        SaveSlot::disabled(),
        crate::pack_source::PackSource::Test(missing),
    );
    let output = platform::AudioOutput::null(crate::music::RING_CAPACITY_FRAMES);
    let music = crate::music::MusicPlayer::start(looping_song_for_test(), output)
        .expect("null backend never errors");
    app.attach_music_for_test(music);

    let mut drained = vec![0.0_f32; audio::Sequencer::FRAME_SAMPLES];
    app.set_headless_buttons(Buttons::START).unwrap();
    app.step().unwrap();
    app.drain_music_for_test(&mut drained);
    app.set_headless_buttons(Buttons::NONE).unwrap();
    let mut back_on_title = false;
    for _ in 0..40 {
        app.step().unwrap();
        app.drain_music_for_test(&mut drained);
        if matches!(app.scene, Some(crate::flow::AppScene::Title(_))) {
            back_on_title = true;
            break;
        }
    }
    assert!(back_on_title, "failed menu load must restore the title");
    for _ in 0..200 {
        app.step().unwrap();
        app.drain_music_for_test(&mut drained);
    }
    assert!(matches!(app.scene, Some(crate::flow::AppScene::Title(_))));
    let music = app.music.as_ref();
    assert!(
        music.is_some_and(|m| !m.fade_finished()),
        "the recovered title must still have audible title music, got music present={} faded={:?}",
        music.is_some(),
        music.map(crate::music::MusicPlayer::fade_finished)
    );
}
