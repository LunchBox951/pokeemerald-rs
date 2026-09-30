//! Pins frame-driven `MusicPlayer` playback: ring prefill/drain accounting,
//! the upstream fade-out volume schedule, and restart-after-finish or
//! after-tail-drain behavior.

use std::sync::Arc;

use audio::{Adsr, Event, Instrument, Sequencer, Song, ToneData, WaveData};
use platform::AudioOutput;

use super::super::{MusicPlayer, RING_CAPACITY_FRAMES, TITLE_FADE_OUT_SPEED};
use super::shared::{drain_everything, sustained_song, RING_CAPACITY_SAMPLES};

fn loud_wave() -> Arc<WaveData> {
    Arc::new(WaveData::one_shot(1 << 20, vec![100; 64]))
}

fn looping_song() -> Song {
    let voices = vec![Instrument::DirectSound(ToneData::new(
        loud_wave(),
        Adsr::flat(),
    ))];
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

fn short_one_shot_song() -> Song {
    let voices = vec![Instrument::DirectSound(ToneData::new(
        loud_wave(),
        Adsr::flat(),
    ))];
    let events = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 4,
        },
        Event::Wait(8),
        Event::Fine,
    ];
    Song::new(voices, vec![events], 150)
}

fn finite_reverbed_song() -> Song {
    let voices = vec![Instrument::DirectSound(ToneData::new(
        loud_wave(),
        Adsr::flat(),
    ))];
    let events = vec![
        Event::Voice(0),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 1,
        },
        Event::Wait(2),
        Event::Fine,
    ];
    Song::new(voices, vec![events], 150).with_reverb(100)
}

#[test]
fn advance_frame_produces_audible_output_and_never_underruns_when_drained_each_frame() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player = MusicPlayer::start(looping_song(), output).expect("null backend never errors");
    assert!(player.is_running());

    let mut any_audible = false;
    let mut drained = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    for _ in 0..64 {
        player.advance_frame();
        player.drain_null_for_test(&mut drained);
        if drained.iter().any(|&s| s != 0.0) {
            any_audible = true;
        }
    }
    assert!(any_audible, "a looping song must produce audible output");
    assert_eq!(
        player.underruns(),
        0,
        "draining exactly one frame per advance must never starve the ring"
    );
    assert_eq!(
        player.overruns(),
        0,
        "draining exactly one frame per advance must never overflow the ring either"
    );
}

#[test]
fn start_prefills_about_half_the_ring_and_leaves_the_rest_as_headroom() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    assert_eq!(
        output.producer().available_space(),
        RING_CAPACITY_SAMPLES,
        "a fresh ring is empty, so this test's capacity constant is the real one"
    );
    let player = MusicPlayer::start(looping_song(), output).expect("null backend never errors");

    let free = player.ring_free_for_test();
    let queued = RING_CAPACITY_SAMPLES - free;
    let half = RING_CAPACITY_SAMPLES / 2;
    assert!(
        queued <= half && queued + Sequencer::FRAME_SAMPLES > half,
        "prefill queued {queued} samples: expected the largest whole number of \
         {}-sample frames that fits in half of {RING_CAPACITY_SAMPLES}",
        Sequencer::FRAME_SAMPLES
    );
    assert!(
        free >= half,
        "prefill must leave at least half the ring ({half} samples) free as drift headroom, \
         left {free}"
    );
    assert_eq!(player.overruns(), 0, "the prefill must never drop a sample");
}

#[test]
fn overruns_count_the_samples_a_full_ring_drops() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player = MusicPlayer::start(looping_song(), output).expect("null backend never errors");
    assert_eq!(player.overruns(), 0);

    let free_at_start = player.ring_free_for_test();
    let frames_that_fit = free_at_start / Sequencer::FRAME_SAMPLES;
    for _ in 0..frames_that_fit {
        player.advance_frame();
    }
    assert_eq!(
        player.overruns(),
        0,
        "frames that still fit must not be counted as dropped"
    );

    player.advance_frame();
    let first_drop = player.overruns();
    assert_eq!(
        first_drop,
        (Sequencer::FRAME_SAMPLES - free_at_start % Sequencer::FRAME_SAMPLES) as u64,
        "the first overflowing push must drop exactly the part that did not fit"
    );

    player.advance_frame();
    assert_eq!(
        player.overruns(),
        first_drop + Sequencer::FRAME_SAMPLES as u64,
        "a completely full ring drops a whole frame"
    );
    assert_eq!(
        player.underruns(),
        0,
        "overflowing is not underflowing -- the two counters must stay independent"
    );
}

#[test]
fn fade_out_follows_upstreams_speed_4_volume_schedule_and_then_stops() {
    const FULL_VOLUME: u32 = 64;
    const VOLUME_PER_STEP: u32 = 4;
    const FADE_FRAMES: u32 = 64;
    // A faded sample is now `dry * gain` truncated through several integer
    // `>>` stages (`sequencer::track_volume`, `voice::channel_volume`)
    // instead of one exact float multiply (issue #1243); `1.5/128` covers
    // this fixture's worst observed truncation (`1.4375/128` at `volX ==
    // 4`) with headroom.
    const TRUNCATION_TOLERANCE: f32 = 1.5 / 128.0;
    // (frame, left, right) mix units at representative steps, independently
    // derived from those same integer stages rather than observed from this
    // player: catches a wrong stage, or a reintroduced post-mix scale, that
    // TRUNCATION_TOLERANCE's headroom alone would miss.
    const EXACT_MIX_UNITS: [(u32, u8, u8); 3] = [(4, 36, 37), (32, 19, 19), (60, 1, 1)];

    let mut plain = MusicPlayer::start(sustained_song(1), AudioOutput::null(RING_CAPACITY_FRAMES))
        .expect("null backend never errors");
    let mut fading = MusicPlayer::start(sustained_song(1), AudioOutput::null(RING_CAPACITY_FRAMES))
        .expect("null backend never errors");
    drain_everything(&mut plain);
    drain_everything(&mut fading);

    assert!(!fading.fade_finished(), "no fade has been started yet");
    fading.fade_out(TITLE_FADE_OUT_SPEED);
    fading.fade_out(TITLE_FADE_OUT_SPEED);

    let mut plain_frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut fading_frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut any_audible = false;
    for frame in 1..=FADE_FRAMES {
        plain.advance_frame();
        fading.advance_frame();
        plain.drain_null_for_test(&mut plain_frame);
        fading.drain_null_for_test(&mut fading_frame);

        let volume = FULL_VOLUME - VOLUME_PER_STEP * (frame / u32::from(TITLE_FADE_OUT_SPEED));
        #[expect(
            clippy::cast_precision_loss,
            reason = "fade volume values from zero through 64 are exact in f32"
        )]
        let gain = volume as f32 / FULL_VOLUME as f32;
        for (i, (&dry, &wet)) in plain_frame.iter().zip(&fading_frame).enumerate() {
            assert!(
                (wet - dry * gain).abs() < TRUNCATION_TOLERANCE,
                "frame {frame}, sample {i}: expected about {dry} * {gain} = {}, got {wet}",
                dry * gain
            );
            if dry != 0.0 {
                any_audible = true;
            }
        }
        if let Some(&(_, left_units, right_units)) =
            EXACT_MIX_UNITS.iter().find(|&&(f, ..)| f == frame)
        {
            assert_eq!(
                (fading_frame[0], fading_frame[1]),
                (f32::from(left_units) / 128.0, f32::from(right_units) / 128.0),
                "frame {frame}: exact fade level check (independent of TRUNCATION_TOLERANCE) failed"
            );
        }

        assert_eq!(
            fading.fade_finished(),
            frame == FADE_FRAMES,
            "frame {frame}: the fade must finish on frame {FADE_FRAMES}, not before or after"
        );
    }
    assert!(
        any_audible,
        "the reference player must actually have been producing sound to fade"
    );
    assert!(
        fading_frame.iter().all(|&s| s == 0.0),
        "the last fade frame must be silent"
    );
}

/// Renders `song` for `frames` game frames past the prefill, fading from the
/// first frame when `fade_speed` is given, and returns the last frame's
/// first sample.
fn first_sample_after_frames(song: Song, frames: u32, fade_speed: Option<u16>) -> f32 {
    let mut player = MusicPlayer::start(song, AudioOutput::null(RING_CAPACITY_FRAMES))
        .expect("null backend never errors");
    drain_everything(&mut player);
    if let Some(speed) = fade_speed {
        player.fade_out(speed);
    }
    let mut frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    for _ in 0..frames {
        player.advance_frame();
        player.drain_null_for_test(&mut frame);
    }
    frame[0]
}

/// Upstream scales each voice, then sums, then clips (`FadeOutBody`,
/// `m4a.c:750`-`:757`; `TrkVolPitSet`, `m4a.c:765`-`:788`); four full-volume
/// tracks overflow signed 8-bit together, so halving their volume must
/// relieve that clipping, not just halve its clipped remainder.
#[test]
fn a_fade_scales_each_voice_before_the_mixer_clips_their_sum() {
    const CLIPPING_TRACKS: u8 = 4;
    const FADE_SPEED: u16 = 1;
    const HALF_VOLUME_FRAME: u32 = 8;
    const FULL_SCALE: f32 = 127.0 / 128.0;

    let one_voice = first_sample_after_frames(sustained_song(1), 1, None);
    let unclipped_sum = one_voice * f32::from(CLIPPING_TRACKS);
    assert!(
        unclipped_sum > FULL_SCALE,
        "the test needs voices whose sum overflows the mix: \
         {CLIPPING_TRACKS} x {one_voice} = {unclipped_sum}"
    );

    // Per-voice volume truncation costs at most one mix unit per voice.
    let tolerance = f32::from(CLIPPING_TRACKS) / 128.0;
    let unfaded = first_sample_after_frames(sustained_song(usize::from(CLIPPING_TRACKS)), 1, None);
    assert!(
        (unfaded - FULL_SCALE).abs() < tolerance,
        "the unfaded frame must sit at clipped full scale, got {unfaded}"
    );

    let faded = first_sample_after_frames(
        sustained_song(usize::from(CLIPPING_TRACKS)),
        HALF_VOLUME_FRAME,
        Some(FADE_SPEED),
    );
    let expected = unclipped_sum * 0.5;
    assert!(
        expected < FULL_SCALE,
        "at half volume the faded voices must fit in the mix without clipping"
    );
    assert!(
        (faded - expected).abs() < tolerance,
        "half-volume fade of {CLIPPING_TRACKS} clipping voices: expected about {expected}, \
         got {faded} (scaling the clipped frame instead would give {})",
        FULL_SCALE * 0.5
    );
}

#[test]
fn a_finished_song_restarts_instead_of_falling_permanently_silent() {
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player =
        MusicPlayer::start(short_one_shot_song(), output).expect("null backend never errors");

    let mut drained = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut audible_after_expected_finish = false;
    for frame in 0..200 {
        player.advance_frame();
        player.drain_null_for_test(&mut drained);
        if frame > 100 && drained.iter().any(|&s| s != 0.0) {
            audible_after_expected_finish = true;
        }
    }
    assert!(
        audible_after_expected_finish,
        "a one-shot song must restart rather than staying silent forever once finished"
    );
}

#[test]
fn finite_reverbed_song_restarts_only_after_tail_drains() {
    let song = finite_reverbed_song();
    let capacity_frames = Sequencer::FRAME_SAMPLES / usize::from(AudioOutput::CHANNELS);
    let output = AudioOutput::null(capacity_frames);
    let mut player = MusicPlayer::start(song.clone(), output).expect("null backend never errors");
    let mut reference = Sequencer::new(song.clone());
    let mut expected = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut actual = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut heard_tail_without_voice = false;

    for frame in 0..1000 {
        reference.render_frame(&mut expected);
        player.advance_frame();
        player.drain_null_for_test(&mut actual);
        assert_eq!(
            actual, expected,
            "MusicPlayer restarted before the reference tail drained on frame {frame}"
        );

        if reference.voice_count() == 0 && expected.iter().any(|&sample| sample != 0.0) {
            heard_tail_without_voice = true;
        }
        if reference.is_finished() {
            break;
        }
    }

    assert!(
        heard_tail_without_voice,
        "the finite song must render a wet tail after its dry voice stops"
    );
    assert!(
        reference.is_finished(),
        "the finite song's reverb tail must eventually drain"
    );

    let mut restarted = Sequencer::new(song);
    restarted.render_frame(&mut expected);
    player.advance_frame();
    player.drain_null_for_test(&mut actual);
    assert_eq!(
        actual, expected,
        "MusicPlayer must restart on the frame after the drained tail completes"
    );
}
