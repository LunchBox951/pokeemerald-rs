use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::*;

#[test]
fn delay_to_frames_converts_an_exact_delay_at_the_device_rate() {
    assert_eq!(delay_to_frames(Duration::from_millis(10), 48_000), 480);
}

/// Rounding a fractional-frame delay down would let a caller read a
/// frame as sounded one frame before it actually is.
#[test]
fn delay_to_frames_rounds_a_fractional_frame_up() {
    assert_eq!(delay_to_frames(Duration::from_nanos(1), 48_000), 1);
}

#[test]
fn estimate_sounded_frames_treats_an_exact_zero_delay_as_valid() {
    let ts = cpal::OutputStreamTimestamp {
        callback: cpal::StreamInstant::new(1, 0),
        playback: cpal::StreamInstant::new(1, 0),
    };
    assert_eq!(
        estimate_sounded_frames(1_000, ts, 48_000),
        Some(SoundedEstimate {
            sounded_frames: 1_000,
            startup_delay_frames: 0,
        })
    );
}

#[test]
fn estimate_sounded_frames_is_none_for_an_inverted_timestamp_pair() {
    let ts = cpal::OutputStreamTimestamp {
        callback: cpal::StreamInstant::new(2, 0),
        playback: cpal::StreamInstant::new(1, 0),
    };
    assert_eq!(estimate_sounded_frames(1_000, ts, 48_000), None);
}

#[test]
fn estimate_sounded_frames_keeps_the_startup_delay_saturation_hides() {
    // 9,600 frames submitted, 450 ms (21,600 frames) of delay: nothing has
    // sounded, and playback begins 12,000 frames past the callback's start.
    let ts = cpal::OutputStreamTimestamp {
        callback: cpal::StreamInstant::new(1, 0),
        playback: cpal::StreamInstant::new(1, 450_000_000),
    };
    assert_eq!(
        estimate_sounded_frames(9_600, ts, 48_000),
        Some(SoundedEstimate {
            sounded_frames: 0,
            startup_delay_frames: 12_000,
        })
    );
}

#[test]
fn every_usable_callback_restamps_the_reading_and_a_stale_one_keeps_it() {
    let clock = PlaybackClock::new();
    clock.record_reading(100, at(&clock, 10), usable(60));
    clock.record_reading(100, at(&clock, 20), usable(60)); // repeated estimate
    clock.record_reading(100, at(&clock, 30), usable(40)); // regressed estimate
    assert_eq!(
        clock.snapshot().usable_reading,
        Some(reading(&clock, 30, 40))
    );
    assert_eq!(clock.snapshot().sounded_frames, 60);
    clock.record(100, |_| None);
    assert_eq!(
        clock.snapshot().usable_reading,
        Some(reading(&clock, 30, 40))
    );
}

#[test]
fn a_callback_observed_before_the_origin_is_stale() {
    let clock = PlaybackClock::new();
    let Some(before) = clock.origin.checked_sub(Duration::from_millis(1)) else {
        return;
    };
    clock.record_reading(100, before, usable(60));
    assert_eq!(clock.snapshot().usable_reading, None);
    assert_eq!(clock.snapshot().usable_through_frames, 0);
}

#[test]
fn playback_clock_publishes_a_monotonic_estimate_under_host_jitter() {
    let clock = PlaybackClock::new();
    let ts_with_delay = |delay_ms: u64| cpal::OutputStreamTimestamp {
        callback: cpal::StreamInstant::ZERO,
        playback: cpal::StreamInstant::from_millis(delay_ms),
    };

    // Callback 1: nothing submitted yet outsizes a 10ms (480-frame)
    // delay, so the estimate floors at zero.
    clock.record_callback(
        1_000,
        &cpal::OutputCallbackInfo::new(ts_with_delay(10)),
        48_000,
    );
    assert_eq!(clock.sounded_frames.load(Ordering::Acquire), 0);

    // Callback 2: 1000 frames now submitted outsize the same delay.
    clock.record_callback(
        1_000,
        &cpal::OutputCallbackInfo::new(ts_with_delay(10)),
        48_000,
    );
    assert_eq!(clock.sounded_frames.load(Ordering::Acquire), 520);

    // Callback 3: a jitter spike inflates the reported delay past the
    // submitted position, so the naive estimate would fall back to
    // zero -- the published estimate must hold at 520 instead.
    clock.record_callback(
        1_000,
        &cpal::OutputCallbackInfo::new(ts_with_delay(60)),
        48_000,
    );
    assert_eq!(
        clock.sounded_frames.load(Ordering::Acquire),
        520,
        "a jittery spike in the reported delay must not move the published position backward"
    );

    // Callback 4: delay returns to normal, and the estimate resumes
    // advancing past the held value.
    clock.record_callback(
        1_000,
        &cpal::OutputCallbackInfo::new(ts_with_delay(10)),
        48_000,
    );
    assert_eq!(clock.sounded_frames.load(Ordering::Acquire), 2_520);
}

#[test]
fn a_usable_callback_is_counted_even_when_its_estimate_does_not_grow() {
    let clock = PlaybackClock::new();
    clock.record(100, |_| Some(60));
    clock.record(100, |_| Some(60)); // repeated estimate
    clock.record(100, |_| Some(40)); // regressed estimate
    assert_eq!(clock.sounded_frames.load(Ordering::Acquire), 60);
    assert_eq!(clock.usable_through_frames.load(Ordering::Acquire), 300);
    assert_eq!(clock.submitted_frames.load(Ordering::Acquire), 300);
}

#[test]
fn a_stale_callback_advances_submitted_frames_without_the_usable_mark() {
    let clock = PlaybackClock::new();
    clock.record(100, |_| Some(60));
    clock.record(100, |_| None);
    assert_eq!(clock.usable_through_frames.load(Ordering::Acquire), 100);
    assert_eq!(clock.submitted_frames.load(Ordering::Acquire), 200);
}

#[test]
fn the_submitted_store_lands_after_the_estimate_and_usable_mark_stores() {
    let clock = PlaybackClock::new();
    clock.record(100, |start| {
        // Mid-callback, between the estimate and the stores: the frames are
        // not yet published, so a reader cannot see them ahead of the mark.
        assert_eq!(start, 0);
        assert_eq!(clock.submitted_frames.load(Ordering::Acquire), 0);
        Some(10)
    });
    assert_eq!(clock.submitted_frames.load(Ordering::Acquire), 100);
    assert_eq!(clock.usable_through_frames.load(Ordering::Acquire), 100);
}

/// `ms` after the clock's origin: an explicit callback observation time.
fn at(clock: &PlaybackClock, ms: u64) -> Instant {
    clock.origin + Duration::from_millis(ms)
}

/// A usable estimate of `sounded` frames with no startup delay.
fn usable(sounded: u64) -> impl FnOnce(u64) -> Option<SoundedEstimate> {
    move |_| {
        Some(SoundedEstimate {
            sounded_frames: sounded,
            startup_delay_frames: 0,
        })
    }
}

/// The reading a usable callback at `ms` with `sounded` frames publishes.
fn reading(clock: &PlaybackClock, ms: u64, sounded: u64) -> UsableReading {
    UsableReading {
        observed_at: at(clock, ms),
        sounded_frames: sounded,
        startup_delay_frames: 0,
    }
}

#[test]
fn a_snapshot_never_pairs_a_new_estimate_with_an_older_mark() {
    // Stale callbacks have left the mark behind the submitted frames; a new
    // usable callback then completes between the reader's mark and estimate
    // loads. Loading the live counters would see the old submitted total and
    // mark with the new, still-short estimate.
    let clock = PlaybackClock::new();
    clock.record_reading(100, at(&clock, 10), usable(10));
    clock.record(100, |_| None);
    let mut interleaved = false;
    let snapshot = clock.snapshot_observing(|loads| {
        if loads == 2 && !std::mem::replace(&mut interleaved, true) {
            clock.record_reading(100, at(&clock, 150), usable(150));
        }
    });

    assert!(interleaved);
    assert_eq!(
        snapshot,
        PlaybackProgress {
            submitted_frames: 300,
            sounded_frames: 150,
            usable_through_frames: 300,
            usable_reading: Some(reading(&clock, 150, 150)),
        },
        "the retried read takes the new callback whole"
    );
}

#[test]
fn a_snapshot_two_callbacks_overtake_is_retried_whole() {
    // Two callbacks complete during one read, rewriting the copy it was
    // reading: it must retry rather than mix the two.
    let clock = PlaybackClock::new();
    clock.record_reading(100, at(&clock, 10), usable(10));
    clock.record(100, |_| None);
    let mut interleaved = false;
    let snapshot = clock.snapshot_observing(|loads| {
        if loads == 2 && !std::mem::replace(&mut interleaved, true) {
            clock.record_reading(100, at(&clock, 150), usable(150));
            clock.record_reading(100, at(&clock, 160), usable(160));
        }
    });

    assert!(interleaved);
    assert_eq!(
        snapshot,
        PlaybackProgress {
            submitted_frames: 400,
            sounded_frames: 160,
            usable_through_frames: 400,
            usable_reading: Some(reading(&clock, 160, 160)),
        }
    );
}

#[test]
fn a_snapshot_during_a_callback_mid_store_reads_the_last_completed_one() {
    // No snapshot taken before: the callback is descheduled mid-store, with
    // its estimate published to the live counters but not its mark.
    let clock = PlaybackClock::new();
    clock.record_reading(100, at(&clock, 10), usable(10));
    clock.record(100, |_| None);
    let mut during = None;
    clock.write(|| {
        clock.sounded_frames.fetch_max(150, Ordering::Release);
        during = Some(clock.snapshot());
        clock.usable_through_frames.store(300, Ordering::Release);
        clock.submitted_frames.store(300, Ordering::Release);
    });

    assert_eq!(
        during,
        Some(PlaybackProgress {
            submitted_frames: 200,
            sounded_frames: 10,
            usable_through_frames: 100,
            usable_reading: Some(reading(&clock, 10, 10)),
        })
    );
    assert_eq!(
        clock.snapshot(),
        PlaybackProgress {
            submitted_frames: 300,
            sounded_frames: 150,
            usable_through_frames: 300,
            usable_reading: Some(reading(&clock, 10, 10)),
        }
    );
}

#[test]
fn a_snapshot_never_takes_a_rewritten_slot_before_it_is_published() {
    // The reader takes the published slot's index; a callback then publishes
    // the other slot, and the next one rewrites the first and is descheduled
    // before naming it published. At either pause point the read must return
    // only published counters, so the following read cannot go backward.
    for pause_at in [0_u32, 3] {
        let clock = PlaybackClock::new();
        clock.record_reading(100, at(&clock, 10), usable(10));
        let mut interleaved = false;
        let first = clock.snapshot_observing(|loads| {
            if loads == pause_at && !std::mem::replace(&mut interleaved, true) {
                clock.record_reading(100, at(&clock, 20), usable(20));
                clock.submitted_frames.store(300, Ordering::Release);
                let unpublished = 1 - clock.published_slot.load(Ordering::Relaxed);
                clock.fill_slot(unpublished);
            }
        });
        let second = clock.snapshot();

        assert!(interleaved);
        assert!(
            first.submitted_frames <= second.submitted_frames,
            "pause at {pause_at}: {first:?} then {second:?}"
        );
        assert_eq!(
            second,
            PlaybackProgress {
                submitted_frames: 200,
                sounded_frames: 20,
                usable_through_frames: 200,
                usable_reading: Some(reading(&clock, 20, 20)),
            },
            "pause at {pause_at}: only the published callback is read"
        );
    }
}
