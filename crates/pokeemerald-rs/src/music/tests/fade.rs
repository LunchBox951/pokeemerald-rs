//! Pins PSG (CGB square-channel) fade-out behavior: silence on the fade's
//! terminal frame, and the `DirectSound` reverb ring's decaying tail
//! sounding past that same terminal step.

use audio::song::SquareTone;
use audio::{CgbAdsr, Event, Instrument, Sequencer, Song};
use platform::AudioOutput;

use super::super::{MusicPlayer, RING_CAPACITY_FRAMES, TITLE_FADE_OUT_SPEED};
use super::shared::{drain_everything, sustained_song};

/// The PSG counterpart to [`sustained_song`]: one CGB square voice holding a
/// tied note, so a fade's terminal frame can be inspected on a channel whose
/// gain runs through the CGB envelope rather than the `DirectSound` mixer.
fn sustained_cgb_song() -> Song {
    let voices = vec![Instrument::CgbSquare1(SquareTone {
        duty: 2,
        sweep: 0,
        adsr: CgbAdsr::flat(),
        fixed_rate: false,
    })];
    let track = vec![
        Event::Voice(0),
        Event::Volume(127),
        Event::Note {
            key: 60,
            velocity: 127,
            gate: 0,
        },
        Event::Wait(200),
        Event::Goto(0),
    ];
    Song::new(voices, vec![track], 150)
}

/// `FadeOutBody` stops every existing track outright on the step that reaches
/// `volX == 0` (`m4a.c:750`-`:757`), and `TrackStop` turns each CGB channel
/// off as it goes -- so the frame mixed after that step carries no PSG sound.
#[test]
fn a_finished_fade_silences_a_sustained_cgb_voice_on_its_terminal_frame() {
    const FADE_FRAMES: u32 = 64;

    let mut player = MusicPlayer::start(
        sustained_cgb_song(),
        AudioOutput::null(RING_CAPACITY_FRAMES),
    )
    .expect("null backend never errors");
    drain_everything(&mut player);
    player.fade_out(TITLE_FADE_OUT_SPEED);

    let mut frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut any_audible = false;
    for _ in 1..=FADE_FRAMES {
        player.advance_frame();
        player.drain_null_for_test(&mut frame);
        if frame.iter().any(|&s| s != 0.0) {
            any_audible = true;
        }
    }

    assert!(
        player.fade_finished(),
        "sanity: the fade must finish on frame {FADE_FRAMES}"
    );
    assert!(
        any_audible,
        "sanity: the CGB voice must actually have been producing sound to fade"
    );
    assert!(
        frame.iter().all(|&s| s == 0.0),
        "the terminal fade frame must be silent, got {:?}",
        &frame[..4]
    );
}

/// The `DirectSound` counterpart to
/// [`a_finished_fade_silences_a_sustained_cgb_voice_on_its_terminal_frame`]:
/// a reverbed song, whose master-mix feedback ring still holds seven frames
/// of delayed `DirectSound` samples when the terminal fade step stops every
/// track (`m4a.c:720`-`:735`). Upstream's mixer keeps running through the
/// paused player (`SoundMain` and `SoundMainRAM_Reverb`, `m4a_1.s:20`-`:119`),
/// so those delayed samples keep sounding as a decaying wet tail; stopping the
/// tracks silences the dry voices, not the ring. The player is droppable only
/// once that tail has rung down.
#[test]
fn a_finished_fade_keeps_sounding_the_reverb_tail_the_ring_still_holds() {
    const TITLE_REVERB_LEVEL: u8 = 50;
    const FRAMES: u32 = 96;

    let mut player = MusicPlayer::start(
        sustained_song(1).with_reverb(TITLE_REVERB_LEVEL),
        AudioOutput::null(RING_CAPACITY_FRAMES),
    )
    .expect("null backend never errors");
    drain_everything(&mut player);
    player.fade_out(TITLE_FADE_OUT_SPEED);

    let mut frame = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let mut audible_through_terminal = false;
    let mut audible_after_terminal = false;
    let mut past_terminal = false;
    for _ in 0..FRAMES {
        // The contract `App::advance_music` is built on -- and which
        // `app::tests::music_lifecycle::a_faded_reverbed_songs_tail_keeps_sounding_past_the_terminal_fade_step`
        // drives through the app itself: render frames until the fade has
        // finished and its tail is silent, then only poll `drained`.
        if player.fade_finished() && !player.tail_sounding() {
            let _ = player.drained();
        } else {
            player.advance_frame();
        }
        player.drain_null_for_test(&mut frame);
        let audible = frame.iter().any(|&sample| sample != 0.0);
        if past_terminal {
            audible_after_terminal |= audible;
        } else {
            audible_through_terminal |= audible;
        }
        past_terminal |= player.fade_finished();
    }

    assert!(
        past_terminal,
        "sanity: the fade must finish within {FRAMES} frames"
    );
    assert!(
        audible_through_terminal,
        "sanity: the song must actually have been sounding through the fade"
    );
    assert!(
        audible_after_terminal,
        "the reverb ring's delayed samples must keep sounding after the terminal fade step, \
         not be truncated when the fade finishes"
    );
    assert!(
        !player.tail_sounding(),
        "the tail must ring down within {FRAMES} frames, so the paused player can be dropped"
    );
}
