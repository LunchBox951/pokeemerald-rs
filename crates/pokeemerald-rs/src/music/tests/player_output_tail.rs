//! Output-tail contract: how long the player waits for the device and resampler to drain queued audio.

use platform::AudioOutput;

use super::super::player::{
    callback_cadence_frames, device_tail_millis, game_frames_in, max_drain_wait_frames,
    measured_drain_target, MusicPlayer, DEVICE_TAIL_FLOOR_FRAMES, DEVICE_TAIL_FLOOR_MILLIS,
    DEVICE_TAIL_MARGIN_MILLIS, DEVICE_TAIL_MAX_MILLIS, RING_CAPACITY_FRAMES,
};
use super::player_shared::short_song_without_its_own_reverb;

/// A device that advertises no concrete buffer range says nothing about
/// its latency, so the floor is the whole wait.
#[test]
fn an_unadvertised_callback_bound_waits_the_floor() {
    assert_eq!(device_tail_millis(None, 48_000), DEVICE_TAIL_FLOOR_MILLIS);
}

/// A rate of zero cannot turn a frame count into a duration; the floor
/// stands rather than a division by zero or a nonsense wait.
#[test]
fn a_rateless_device_waits_the_floor() {
    assert_eq!(device_tail_millis(Some(4_096), 0), DEVICE_TAIL_FLOOR_MILLIS);
}

/// A small callback buffer derives a wait under the floor, and the floor
/// wins: the advertised bound covers the callback, not the OS queueing
/// behind it.
#[test]
fn a_short_callback_bound_still_waits_the_floor() {
    assert_eq!(
        device_tail_millis(Some(512), 48_000),
        DEVICE_TAIL_FLOOR_MILLIS
    );
}

/// A callback buffer that outlasts the floor widens the wait rather
/// than letting it expire mid-buffer, and the wait covers every period
/// the host keeps queued behind the callback, not the one it fills:
/// 7,680 frames at 48 kHz is 160 ms per period, so two periods plus the
/// margin is 370 ms, the same figure the `play_song` example derives.
#[test]
fn a_callback_bound_past_the_floor_widens_the_wait_by_every_queued_period() {
    let tail = device_tail_millis(Some(7_680), 48_000);
    assert_eq!(tail, 2 * 160 + DEVICE_TAIL_MARGIN_MILLIS);
    assert!(tail > DEVICE_TAIL_FLOOR_MILLIS);
}

/// An advertised maximum the device may never reach must not hold the
/// title -> main menu transition open for as long as it claims.
#[test]
fn an_outsized_callback_bound_is_capped() {
    assert_eq!(
        device_tail_millis(Some(480_000), 48_000),
        DEVICE_TAIL_MAX_MILLIS
    );
}

/// The pre-empty bound governs a ring that has not drained yet. Derived
/// from the ring alone it assumes the consumer drains about as often as
/// the game renders: at the production ring that expires after 38 game
/// frames, roughly 0.64 s. A device whose callback period outlasts that
/// leaves the ring nonempty until its next callback, so the bound must
/// widen to that device's tail rather than call a healthy stream stalled
/// and drop the queued fade.
#[test]
fn a_long_advertised_callback_widens_the_pre_empty_bound() {
    let ring_capacity = RING_CAPACITY_FRAMES * usize::from(AudioOutput::CHANNELS);
    let ring_only = max_drain_wait_frames(ring_capacity, 0);
    let device_tail = game_frames_in(device_tail_millis(Some(48_000), 48_000));

    assert!(
        device_tail > ring_only,
        "a one-second callback bound must outlast the {ring_only}-frame ring-only figure, \
         or this test proves nothing"
    );
    assert_eq!(
        max_drain_wait_frames(ring_capacity, device_tail),
        device_tail,
        "the pre-empty bound must cover the device's own callback cadence"
    );
}

/// A device advertising no callback bound has an unknown cadence, not a
/// short one, so the pre-empty wait takes the cap rather than the ring's
/// own figure.
#[test]
fn an_unadvertised_callback_bounds_the_pre_empty_wait_at_the_cap() {
    let ring_capacity = RING_CAPACITY_FRAMES * usize::from(AudioOutput::CHANNELS);
    let ring_only = max_drain_wait_frames(ring_capacity, 0);
    let cadence = callback_cadence_frames(None, 48_000);

    assert_eq!(cadence, game_frames_in(DEVICE_TAIL_MAX_MILLIS));
    assert!(cadence > ring_only);
    assert_eq!(max_drain_wait_frames(ring_capacity, cadence), cadence);
}

/// A short advertised callback leaves the ring's own figure in charge.
#[test]
fn a_short_advertised_callback_leaves_the_pre_empty_bound_on_the_ring() {
    let ring_capacity = RING_CAPACITY_FRAMES * usize::from(AudioOutput::CHANNELS);
    let cadence = callback_cadence_frames(Some(512), 48_000);

    assert_eq!(cadence, DEVICE_TAIL_FLOOR_FRAMES);
    assert_eq!(
        max_drain_wait_frames(ring_capacity, cadence),
        max_drain_wait_frames(ring_capacity, 0)
    );
}

/// [`MusicPlayer::start`] is public and takes any [`AudioOutput`], so a
/// caller may have queued through [`AudioOutput::producer`] first. The
/// ring's capacity is what `drained` compares against to call the ring
/// empty, so it must come from the ring rather than from the free space
/// left at construction -- otherwise those pre-queued samples set the
/// bar low and their own tail could be dropped undelivered.
#[test]
fn a_pre_queued_producer_still_records_the_full_ring_capacity() {
    const RING_FRAMES: usize = 512;
    let full_ring = RING_FRAMES * usize::from(AudioOutput::CHANNELS);

    let output = AudioOutput::null(RING_FRAMES);
    let queued = output.producer().push(&[0.25; 64]);
    assert_eq!(queued, 64, "the null ring must accept this priming push");

    let player = MusicPlayer::start(short_song_without_its_own_reverb(), output)
        .expect("null backend never errors");

    assert_eq!(
        player.ring_capacity_for_test(),
        full_ring,
        "capacity must be the ring's own, not the free space a pre-queued producer left"
    );
}

/// A producer clone retained from before [`MusicPlayer::start`] can
/// refill the ring after `drained` has seen it empty; the device tail
/// then restarts from the latest drain rather than resuming its count.
#[test]
fn a_ring_that_refills_restarts_the_device_tail() {
    const RING_FRAMES: usize = 512;
    let full_ring = RING_FRAMES * usize::from(AudioOutput::CHANNELS);
    let output = AudioOutput::null(RING_FRAMES);
    let retained = output.producer();
    let mut player = MusicPlayer::start(short_song_without_its_own_reverb(), output)
        .expect("null backend never errors");
    let mut sink = vec![0.0_f32; full_ring];
    let tail = player.max_device_tail_frames;
    assert!(
        tail > 2,
        "the null backend's floor must leave room for a partial count"
    );

    player.drain_null_for_test(&mut sink);
    assert_eq!(
        player.ring_free_for_test(),
        full_ring,
        "sanity: the ring is empty"
    );
    for _ in 0..tail - 1 {
        assert!(!player.drained(), "still inside the device tail");
    }

    assert_eq!(
        retained.push(&[0.25; 64]),
        64,
        "the retained producer refills the ring"
    );
    assert!(!player.drained(), "a nonempty ring is never drained");
    player.drain_null_for_test(&mut sink);

    let mut polls_after_refill = 0;
    while !player.drained() {
        polls_after_refill += 1;
        assert!(polls_after_refill <= tail + 1, "the device tail is bounded");
    }
    assert_eq!(
        polls_after_refill, tail,
        "the tail must run in full from the latest drain, not resume the earlier count"
    );
}

/// With a measured playback-position signal available, `drained` must
/// wait on it rather than on the poll count: it must stay `false` while
/// the measured sounded-frame position is stationary, however many polls
/// elapse short of the device-tail cap, and must return `true` the
/// instant the fake clock reaches the latched target -- not merely
/// because enough polls have gone by.
#[test]
fn drained_waits_on_the_measured_playback_position_not_the_poll_count() {
    const RING_FRAMES: usize = 512;
    let full_ring = RING_FRAMES * usize::from(AudioOutput::CHANNELS);
    let output = AudioOutput::null(RING_FRAMES);
    let mut player = MusicPlayer::start(short_song_without_its_own_reverb(), output)
        .expect("null backend never errors");
    let mut sink = vec![0.0_f32; full_ring];

    player.drain_null_for_test(&mut sink);
    assert_eq!(
        player.ring_free_for_test(),
        full_ring,
        "sanity: the ring is empty"
    );

    // Enabled only after draining, so the latched target below is the
    // real submitted-frame count `drain_null_for_test` just advanced.
    player.output.enable_playback_progress_for_test();
    let tail = player.max_device_tail_frames;
    assert!(
        tail > 2,
        "the null backend's floor must leave room for a partial count"
    );

    // Poll up to (but not past) the poll-count cap without ever
    // advancing the fake sounded-frame clock: every poll must stay
    // false, driven by the unmet measured target rather than by the
    // ring/poll bookkeeping `a_ring_that_refills_restarts_the_device_tail`
    // already pins.
    for _ in 0..tail - 1 {
        assert!(
            !player.drained(),
            "must not report drained while the measured playback position is stationary"
        );
    }

    // Advance the fake clock to the latched submitted-frame target
    // (`drain_null_for_test` submitted `RING_FRAMES` stereo frames, not
    // `full_ring` samples) on this very poll -- one short of the
    // poll-count cap (`tail`) that would otherwise still have to elapse
    // -- and `drained` must report true immediately, proving the
    // measured signal decided it.
    player
        .output
        .advance_sounded_frames_for_test(RING_FRAMES as u64);
    assert!(
        player.drained(),
        "must report drained once the measured playback position reaches the target"
    );
}

#[test]
fn measured_drain_target_adds_the_settle_margin() {
    assert_eq!(measured_drain_target(100, 8), 108);
    assert_eq!(measured_drain_target(100, 0), 100);
}

#[test]
fn measured_drain_target_saturates_rather_than_overflowing() {
    assert_eq!(measured_drain_target(u64::MAX, 5), u64::MAX);
}

/// A resampled output keeps buffered `prev`/`next` interpolation state,
/// so real, audible content can still be sounding in the callback AFTER
/// the source ring first reads empty (see `platform::Resampler`'s
/// deferred-lookahead module docs, and its
/// `a_ring_emptied_mid_callback_still_sounds_real_audio_in_the_next_one`
/// regression, which reproduces this exact rate ratio at the `Resampler`
/// level). `AudioOutput::null`'s source plays samples straight through
/// with no such buffering, so it cannot exercise this path --
/// `AudioOutput::null_resampled` stands in for a real device instead.
///
/// This must fail against the pre-fix `drained`, which latched the raw
/// submitted-frame count with no settle margin: reaching that raw count
/// alone must NOT report drained once a nonzero margin is in play, only
/// reaching the raw count plus the margin may.
#[test]
fn drained_waits_past_the_resamplers_deferred_lookahead_tail() {
    const RING_FRAMES: usize = 512;
    // Same rate ratio (step = 25/100 = 0.25) as the `Resampler`-level
    // regression this margin exists to cover.
    let output = AudioOutput::null_resampled(RING_FRAMES, 25.0, 100, 16)
        .expect("a real rate ratio must always construct a Resampler");
    output.enable_playback_progress_for_test();
    let margin = output.playback_settle_margin_frames();
    assert!(
        margin > 0,
        "an up-sampling resampler must report a nonzero settle margin"
    );

    let mut player = MusicPlayer::start(short_song_without_its_own_reverb(), output)
        .expect("a resampled null backend never errors");
    let full_ring = player.ring_capacity_for_test();
    // Upsampling (step < 1.0) drains far more than `full_ring` output
    // frames' worth of *device* frames from the source ring per pull, so
    // one `full_ring`-sized pull is not guaranteed to empty it; poll
    // until it genuinely does, bounded generously against a runaway loop.
    let mut sink = vec![0.0_f32; full_ring];
    let mut pulls = 0;
    while player.ring_free_for_test() < full_ring {
        player.drain_null_for_test(&mut sink);
        pulls += 1;
        assert!(
            pulls <= 64,
            "the ring must empty well within this many pulls"
        );
    }
    assert_eq!(
        player.ring_free_for_test(),
        full_ring,
        "sanity: the ring is empty"
    );

    assert!(
        !player.drained(),
        "the fake sounded-frame clock starts at zero, well short of any target"
    );
    let raw_target = player
        .output
        .playback_progress()
        .expect("the test hook was enabled")
        .submitted_frames;

    player.output.advance_sounded_frames_for_test(raw_target);
    assert!(
        !player.drained(),
        "reaching the raw submitted-frame count must not read as drained: the resampler's \
         buffered interpolation state can still be sounding real audio for `margin` more \
         device frames"
    );

    player
        .output
        .advance_sounded_frames_for_test(raw_target + margin);
    assert!(
        player.drained(),
        "must report drained once the measured position reaches the raw target plus the \
         resampler's settle margin"
    );
}

/// cpal's ALSA worker reports a recoverable XRUN through the error
/// callback, then re-prepares and keeps running, so `stream_errors`
/// counts errors the stream survived. One must not drop the queued fade.
#[test]
fn a_recovered_stream_error_still_drains_the_queued_fade() {
    const RING_FRAMES: usize = 512;
    let full_ring = RING_FRAMES * usize::from(AudioOutput::CHANNELS);
    let output = AudioOutput::null(RING_FRAMES);
    let mut player = MusicPlayer::start(short_song_without_its_own_reverb(), output)
        .expect("null backend never errors");
    let queued = full_ring - player.ring_free_for_test();
    assert!(queued > 0, "the prefill must leave samples to drain");

    player.output.record_stream_error_for_test();

    assert!(
        !player.drained(),
        "a recovered stream error must not drop {queued} queued samples unplayed"
    );
}

/// The pre-empty bound guards against a consumer that never takes the
/// queued fade. An empty ring disproves that stall, so audio a retained
/// producer queues afterwards gets its own wait rather than the remainder
/// of a bound the disproven suspicion already spent.
#[test]
fn a_ring_that_refills_after_a_full_drain_gets_a_fresh_drain_wait() {
    const RING_FRAMES: usize = 512;
    let full_ring = RING_FRAMES * usize::from(AudioOutput::CHANNELS);
    let output = AudioOutput::null(RING_FRAMES);
    let retained = output.producer();
    let mut player = MusicPlayer::start(short_song_without_its_own_reverb(), output)
        .expect("null backend never errors");
    let mut sink = vec![0.0_f32; full_ring];
    player.drain_null_for_test(&mut sink);
    assert_eq!(
        player.ring_free_for_test(),
        full_ring,
        "sanity: the ring starts empty"
    );

    let bound = player.max_drain_wait_frames;
    assert_eq!(
        retained.push(&[0.25; 64]),
        64,
        "the ring holds queued audio"
    );
    for _ in 0..bound - 1 {
        assert!(!player.drained(), "the pre-empty bound has not elapsed yet");
    }

    player.drain_null_for_test(&mut sink);
    assert_eq!(
        player.ring_free_for_test(),
        full_ring,
        "the consumer took everything"
    );
    assert!(
        !player.drained(),
        "an empty ring is still inside the device tail"
    );

    assert_eq!(
        retained.push(&[0.5; 64]),
        64,
        "the retained producer refills the ring"
    );
    assert!(
        !player.drained(),
        "the refilled queue must get its own wait, not be dropped on the poll that sees it"
    );
}
