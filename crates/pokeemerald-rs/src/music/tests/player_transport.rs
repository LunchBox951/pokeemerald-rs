//! Transport-degradation contract: the player keeps sequencing and fading on
//! game frames while no stream is attached, refreshes its device-tail bounds
//! when one attaches, and reports each transition once.
//!
//! The null backend cannot change transport state, so the player's scripted
//! transition queue stands in for the device worker; manually draining a
//! null ring stands in for the platform discarding history on re-attach.

use audio::{Adsr, Event, Instrument, Sequencer, Song, ToneData, WaveData};
use platform::audio::{TransportFailure, TransportState};
use platform::AudioOutput;
use std::sync::Arc;

use super::super::player::{game_frames_in, MusicPlayer, DEVICE_TAIL_FLOOR_FRAMES};
use super::player_shared::short_song_without_its_own_reverb;

/// Ring small enough that its own drain figure (6 frames) does not mask the
/// callback-cadence term of `max_drain_wait_frames`.
const SMALL_RING_FRAMES: usize = 512;
/// Largest callback buffer of a 160 ms period at 48 kHz.
const LONG_CALLBACK_FRAMES: usize = 7_680;
/// Two 160 ms periods plus the 50 ms margin.
const LONG_TAIL_MILLIS: usize = 370;
/// A cadence figure for a device that advertises no bound.
const UNADVERTISED_CADENCE_FRAMES: usize = 60;

fn small_ring_player() -> MusicPlayer {
    let output = AudioOutput::null(SMALL_RING_FRAMES);
    MusicPlayer::start(short_song_without_its_own_reverb(), output)
        .expect("null backend never errors")
}

/// A looping melody whose every frame differs from its neighbours, so a
/// player that fell behind or restarted cannot match a continuous one.
fn rising_melody() -> Song {
    let wave: Vec<i8> = (-32..32_i8).map(|i| i * 4).collect();
    let wave = Arc::new(WaveData::looping(1 << 20, 0, wave));
    let voices = vec![Instrument::DirectSound(ToneData::new(wave, Adsr::flat()))];
    let mut events = vec![Event::Voice(0), Event::Volume(127)];
    for key in 40..100 {
        events.push(Event::Note {
            key,
            velocity: 127,
            gate: 1,
        });
        events.push(Event::Wait(6));
    }
    events.push(Event::Goto(0));
    Song::new(voices, vec![events], 150)
}

fn drain_all(player: &mut MusicPlayer) -> Vec<f32> {
    let queued = player.ring_capacity_for_test() - player.ring_free_for_test();
    let mut out = vec![0.0_f32; queued];
    player.drain_null_for_test(&mut out);
    out
}

fn next_frame(player: &mut MusicPlayer) -> Vec<f32> {
    player.advance_frame();
    drain_all(player)
}

fn polls_until_drained(player: &mut MusicPlayer, limit: usize) -> Option<usize> {
    (1..=limit).find(|_| player.drained())
}

/// Device facts are provisional until the first `Ready`: the tail bound set
/// at startup is the 200 ms floor, and a device that attaches later with a
/// longer callback buffer must widen it before `drained` closes the stream.
#[test]
fn a_late_ready_widens_the_device_tail_to_the_attached_stream() {
    let mut player = small_ring_player();
    assert_eq!(
        player.max_device_tail_frames_for_test(),
        DEVICE_TAIL_FLOOR_FRAMES
    );
    assert_eq!(
        player.max_drain_wait_frames_for_test(),
        UNADVERTISED_CADENCE_FRAMES
    );

    player.script_transport_for_test(TransportState::Ready, Some(LONG_CALLBACK_FRAMES), 48_000);
    assert_eq!(
        player.poll_transport(),
        Some((TransportState::Null, TransportState::Ready))
    );

    let tail = game_frames_in(LONG_TAIL_MILLIS);
    assert_eq!(tail, 23);
    assert_eq!(player.max_device_tail_frames_for_test(), tail);
    assert_eq!(player.max_drain_wait_frames_for_test(), tail);
}

/// The widened bound is what `drained` observes, for a ring the consumer
/// has not emptied (stall bound) and for one it has (device tail).
#[test]
fn drained_waits_the_refreshed_bound_for_both_ring_states() {
    let mut stalled = small_ring_player();
    stalled.script_transport_for_test(TransportState::Ready, Some(LONG_CALLBACK_FRAMES), 48_000);
    let _ = stalled.poll_transport();
    assert_eq!(polls_until_drained(&mut stalled, 100), Some(23));

    let mut emptied = small_ring_player();
    emptied.script_transport_for_test(TransportState::Ready, Some(LONG_CALLBACK_FRAMES), 48_000);
    let _ = emptied.poll_transport();
    drop(drain_all(&mut emptied));
    assert_eq!(polls_until_drained(&mut emptied, 100), Some(24));
}

/// Every `Ready` describes the stream it attached: a second attach with a
/// different rate or no advertised bound replaces the first's figures.
#[test]
fn each_ready_recomputes_the_bounds_from_the_current_device_facts() {
    let mut player = small_ring_player();
    player.script_transport_for_test(TransportState::Ready, Some(LONG_CALLBACK_FRAMES), 48_000);
    player.script_transport_for_test(
        TransportState::Unavailable(TransportFailure::StreamLost),
        None,
        0,
    );
    player.script_transport_for_test(TransportState::Ready, Some(LONG_CALLBACK_FRAMES), 96_000);
    let mut seen = Vec::new();
    while let Some(transition) = player.poll_transport() {
        seen.push((transition, player.max_device_tail_frames_for_test()));
    }
    // 7680 frames at 96 kHz: two 80 ms periods plus the margin.
    let faster = game_frames_in(210);
    assert_eq!(seen[0].1, 23);
    assert_eq!(
        seen[1].1, 23,
        "a loss leaves the last stream's bounds alone"
    );
    assert_eq!(seen[2].1, faster);
    assert_eq!(player.max_drain_wait_frames_for_test(), faster);

    player.script_transport_for_test(
        TransportState::Unavailable(TransportFailure::NoOutput),
        None,
        0,
    );
    player.script_transport_for_test(TransportState::Ready, None, 48_000);
    while player.poll_transport().is_some() {}
    assert_eq!(
        player.max_device_tail_frames_for_test(),
        DEVICE_TAIL_FLOOR_FRAMES
    );
    assert_eq!(
        player.max_drain_wait_frames_for_test(),
        UNADVERTISED_CADENCE_FRAMES
    );
}

/// Polls counted against a stream that is gone must not eat into the
/// allowance of the one that replaced it; retries while unavailable must
/// not restart the wait.
#[test]
fn ready_restarts_the_drain_wait_and_a_retrying_transport_does_not() {
    let mut player = small_ring_player();
    player.script_transport_for_test(TransportState::Ready, Some(LONG_CALLBACK_FRAMES), 48_000);
    let _ = player.poll_transport();
    for _ in 0..10 {
        assert!(!player.drained());
    }
    player.script_transport_for_test(
        TransportState::Unavailable(TransportFailure::NoOutput),
        None,
        0,
    );
    let _ = player.poll_transport();
    assert_eq!(
        polls_until_drained(&mut player, 100),
        Some(13),
        "the unavailable retry kept the ten polls already spent of 23"
    );

    player.script_transport_for_test(TransportState::Ready, Some(LONG_CALLBACK_FRAMES), 48_000);
    let _ = player.poll_transport();
    assert_eq!(
        polls_until_drained(&mut player, 100),
        Some(23),
        "the attach gives the new stream its whole allowance"
    );
}

/// Each polled transition reports where it came from, once, in order, and
/// an empty poll reports nothing.
#[test]
fn transitions_are_returned_once_in_order_with_their_origin() {
    let mut player = small_ring_player();
    let lost = TransportState::Unavailable(TransportFailure::StreamLost);
    for state in [TransportState::Connecting, lost, TransportState::Ready] {
        player.script_transport_for_test(state, None, 48_000);
    }
    assert_eq!(
        [(); 4].map(|()| player.poll_transport()),
        [
            Some((TransportState::Null, TransportState::Connecting)),
            Some((TransportState::Connecting, lost)),
            Some((lost, TransportState::Ready)),
            None,
        ]
    );
}

/// The null backend has no transport to report on.
#[test]
fn a_null_output_reports_no_transitions() {
    let mut player = small_ring_player();
    assert_eq!(player.poll_transport(), None);
}

/// Rendering never waits for a stream: a player nothing drains keeps
/// advancing its sequencer, so when a stream returns and history is
/// discarded, the next frame is the continuous player's frame, not the
/// stale queue and not a restart.
#[test]
fn recovery_resumes_at_the_current_position() {
    const LOST_FRAMES: usize = 300;

    let mut continuous = MusicPlayer::start(
        rising_melody(),
        AudioOutput::null(super::super::player::RING_CAPACITY_FRAMES),
    )
    .expect("null backend never errors");
    let mut lost = MusicPlayer::start(
        rising_melody(),
        AudioOutput::null(super::super::player::RING_CAPACITY_FRAMES),
    )
    .expect("null backend never errors");
    drop(drain_all(&mut continuous));
    let startup = drain_all(&mut lost);

    lost.script_transport_for_test(
        TransportState::Unavailable(TransportFailure::StreamLost),
        None,
        0,
    );
    let _ = lost.poll_transport();
    for _ in 0..LOST_FRAMES {
        lost.advance_frame();
        drop(next_frame(&mut continuous));
    }
    assert!(
        lost.overruns_for_test() > 0,
        "nothing drained the lost ring"
    );

    lost.script_transport_for_test(TransportState::Ready, None, 48_000);
    let _ = lost.poll_transport();
    drop(drain_all(&mut lost));

    let resumed = next_frame(&mut lost);
    let expected = next_frame(&mut continuous);
    assert_eq!(resumed.len(), Sequencer::FRAME_SAMPLES);
    assert_eq!(resumed, expected);
    assert_ne!(resumed, startup[..Sequencer::FRAME_SAMPLES]);
}

/// A fade is counted in game frames, so a lost transport neither stalls nor
/// hastens it, and the terminal step still lands on the 64th frame.
#[test]
fn a_fade_runs_on_game_frames_while_the_transport_is_lost() {
    let mut player = small_ring_player();
    player.script_transport_for_test(
        TransportState::Unavailable(TransportFailure::NoOutput),
        None,
        0,
    );
    let _ = player.poll_transport();
    player.fade_out(super::super::player::TITLE_FADE_OUT_SPEED);
    for frame in 1..64 {
        player.advance_frame();
        assert!(!player.fade_finished(), "frame {frame}");
    }
    player.advance_frame();
    assert!(player.fade_finished());
    assert!(player.is_running(), "a lost transport is not a stop");
}
