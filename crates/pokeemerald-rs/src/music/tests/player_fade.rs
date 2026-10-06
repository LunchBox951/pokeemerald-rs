//! Fade contract: a cancelled fade-out renders at the unfaded level again.

use std::sync::Arc;

use audio::{Adsr, Event, Instrument, Song, ToneData, WaveData};
use platform::AudioOutput;

use super::super::{MusicPlayer, RING_CAPACITY_FRAMES, TITLE_FADE_OUT_SPEED};

/// One note held for the whole song over a looping constant wave at full
/// sustain, so every rendered frame's peak tracks the track's volume.
fn sustained_song() -> Song {
    let adsr = Adsr {
        attack: 255,
        decay: 255,
        sustain: 255,
        release: 0,
    };
    let wave = Arc::new(WaveData::looping(
        1 << 20,
        0,
        vec![100; audio::SAMPLES_PER_FRAME],
    ));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, adsr))];
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

/// Renders one game frame, drains everything `player` has queued so the
/// ring never overruns, and returns that frame's peak absolute sample.
fn advance_and_peak(player: &mut MusicPlayer) -> f32 {
    player.advance_frame();
    let queued = player.ring_capacity_for_test() - player.ring_free_for_test();
    let mut drained = vec![0.0_f32; queued];
    player.drain_null_for_test(&mut drained);
    drained[queued - audio::Sequencer::FRAME_SAMPLES..]
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
}

/// A fade cancelled partway (the title fade-wait's failed menu load)
/// must render at the unfaded level afterwards, not stay at the `volX`
/// the fade's last step staged.
#[test]
fn a_cancelled_fade_renders_at_the_unfaded_level_again() {
    const FADE_FRAMES: usize = 22;
    const SETTLE_FRAMES: usize = 60;

    let start = || {
        MusicPlayer::start(sustained_song(), AudioOutput::null(RING_CAPACITY_FRAMES))
            .expect("the null backend never fails to start")
    };
    let mut reference = start();
    let mut cancelled = start();
    for _ in 0..FADE_FRAMES {
        cancelled.fade_out(TITLE_FADE_OUT_SPEED);
        advance_and_peak(&mut cancelled);
        advance_and_peak(&mut reference);
    }
    cancelled.cancel_fade();
    let (mut reference_peak, mut cancelled_peak) = (0.0, 0.0);
    for _ in 0..SETTLE_FRAMES {
        cancelled_peak = advance_and_peak(&mut cancelled);
        reference_peak = advance_and_peak(&mut reference);
    }

    assert!(reference_peak > 0.0, "sanity: the song must be audible");
    assert!(
        (cancelled_peak - reference_peak).abs() <= reference_peak * 0.01,
        "a cancelled fade must restore the unfaded level: peak {cancelled_peak} vs \
         {reference_peak} never faded"
    );
}
