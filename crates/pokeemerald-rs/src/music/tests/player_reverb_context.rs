//! Reverb-context contract: a song's reverb and the session context survive tails, failed starts, and restarts.

use platform::{AudioOutput, PlatformError};

use super::super::{MusicContext, MusicPlayer, RING_CAPACITY_FRAMES};
use super::player_shared::short_song_without_its_own_reverb;

const REVERB_TAIL_PROBE_FRAMES: usize = 25;

fn first_playthrough_finishes_within(player: &mut MusicPlayer, budget: usize) -> bool {
    for _ in 0..budget {
        player.advance_frame();
        if player.sequencer.is_finished() {
            return true;
        }
    }
    false
}

#[test]
fn a_songs_own_reverb_override_leaves_a_pending_tail() {
    let mut context = MusicContext::new();
    let song = short_song_without_its_own_reverb().with_reverb(100);
    assert_eq!(song.reverb_override(), Some(100));
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player = MusicPlayer::start_with_context(&mut context, song, output)
        .expect("null backend never errors");
    assert!(
        !first_playthrough_finishes_within(&mut player, REVERB_TAIL_PROBE_FRAMES),
        "an explicit reverb override must leave a tail pending well past the note's own end"
    );
}

#[test]
fn failed_output_start_preserves_music_context() {
    let mut context = MusicContext::new();
    let priming_song = short_song_without_its_own_reverb().with_reverb(77);
    let priming_output = AudioOutput::null(RING_CAPACITY_FRAMES);
    MusicPlayer::start_with_context(&mut context, priming_song, priming_output)
        .expect("null backend never errors");
    assert_eq!(context.master_reverb, 77);

    let song = short_song_without_its_own_reverb().with_reverb(100);
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);

    let result = MusicPlayer::start_with_context_and_starter(&mut context, song, output, |_| {
        Err(PlatformError::NoAudioDevice)
    });

    assert!(matches!(result, Err(PlatformError::NoAudioDevice)));
    assert_eq!(context.master_reverb, 77);
}

#[test]
fn a_song_with_no_reverb_override_inherits_the_sessions_previous_level() {
    let mut context = MusicContext::new();
    let priming = short_song_without_its_own_reverb().with_reverb(100);
    let priming_output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut priming_player = MusicPlayer::start_with_context(&mut context, priming, priming_output)
        .expect("null backend never errors");
    assert!(!first_playthrough_finishes_within(
        &mut priming_player,
        REVERB_TAIL_PROBE_FRAMES
    ));

    let inheriting = short_song_without_its_own_reverb();
    assert_eq!(inheriting.reverb_override(), None);
    let inheriting_output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut inheriting_player =
        MusicPlayer::start_with_context(&mut context, inheriting, inheriting_output)
            .expect("null backend never errors");
    assert!(
        !first_playthrough_finishes_within(&mut inheriting_player, REVERB_TAIL_PROBE_FRAMES),
        "a header-less song must inherit the session's previously configured reverb level"
    );
}

#[test]
fn an_explicit_zero_reverb_overrides_the_sessions_previous_level() {
    let mut context = MusicContext::new();
    let priming = short_song_without_its_own_reverb().with_reverb(100);
    let priming_output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut priming_player = MusicPlayer::start_with_context(&mut context, priming, priming_output)
        .expect("null backend never errors");
    assert!(!first_playthrough_finishes_within(
        &mut priming_player,
        REVERB_TAIL_PROBE_FRAMES
    ));

    let disabling = short_song_without_its_own_reverb().with_reverb(0);
    assert_eq!(disabling.reverb_override(), Some(0));
    let disabling_output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut disabling_player =
        MusicPlayer::start_with_context(&mut context, disabling, disabling_output)
            .expect("null backend never errors");
    assert!(
        first_playthrough_finishes_within(&mut disabling_player, REVERB_TAIL_PROBE_FRAMES),
        "an explicit reverb of 0 must disable the tail even though the session had one"
    );
}

#[test]
fn an_inheriting_song_keeps_its_resolved_reverb_across_the_defensive_restart() {
    let mut context = MusicContext::new();
    let priming = short_song_without_its_own_reverb().with_reverb(100);
    let priming_output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut priming_player = MusicPlayer::start_with_context(&mut context, priming, priming_output)
        .expect("null backend never errors");
    assert!(!first_playthrough_finishes_within(
        &mut priming_player,
        REVERB_TAIL_PROBE_FRAMES
    ));

    let inheriting = short_song_without_its_own_reverb();
    assert_eq!(inheriting.reverb_override(), None);
    let output = AudioOutput::null(RING_CAPACITY_FRAMES);
    let mut player = MusicPlayer::start_with_context(&mut context, inheriting, output)
        .expect("null backend never errors");

    let mut frames = 0;
    while !player.sequencer.is_finished() {
        player.advance_frame();
        frames += 1;
        assert!(
            frames < 5_000,
            "the inherited reverb tail must eventually drain"
        );
    }

    assert!(
        !first_playthrough_finishes_within(&mut player, REVERB_TAIL_PROBE_FRAMES),
        "the defensive restart must reuse the resolved reverb level, not the song header's"
    );
}
