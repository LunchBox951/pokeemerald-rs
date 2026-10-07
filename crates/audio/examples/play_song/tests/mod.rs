//! Tests for the `play_song` example, split by seam.

use super::{
    build_song, classify_open_error, device_tail_wait, measured_drain_target, prefill_then_start,
    push_frame, start_playback, usable_callback_in_span, wait_for_device_tail,
    wait_for_device_tail_or_measured, wait_for_drain, wait_for_frame_deadline,
    wait_for_measured_tail, DrainError, OpenOutcome, PushError, RetryPolicy, StartOutcome,
    DEVICE_TAIL_FALLBACK, DEVICE_TAIL_MARGIN, DEVICE_TAIL_MAX,
};
use audio::Sequencer;
use platform::audio::{PlaybackProgress, UsableReading};
use platform::{AudioOutput, PlatformError};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

mod cadence;
mod device_tail;
mod fallback;
mod measured_wait;
mod retry;
mod startup;
mod validity;

/// A snapshot from a device whose callbacks never carry a usable timestamp.
fn progress(sounded_frames: u64, submitted_frames: u64) -> PlaybackProgress {
    PlaybackProgress {
        submitted_frames,
        sounded_frames,
        usable_through_frames: 0,
        usable_reading: None,
    }
}

/// When each usable mark first reached a poll in one test: a prompt poller
/// sees a callback within a poll of it, so tests whose polls are prompt stamp
/// a reading at that first sight; tests that need a callback's exact time use
/// [`progress_reading`].
#[derive(Default)]
struct FirstSeen(RefCell<Vec<(u64, Instant)>>);

impl FirstSeen {
    /// A snapshot whose usable callback ended at `usable_through_frames` and
    /// first reached a poll at `now`. Zero is no usable callback.
    fn progress(
        &self,
        sounded_frames: u64,
        submitted_frames: u64,
        usable_through_frames: u64,
        now: Instant,
    ) -> PlaybackProgress {
        let mut seen = self.0.borrow_mut();
        let observed_at =
            if let Some(&(_, at)) = seen.iter().find(|(mark, _)| *mark == usable_through_frames) {
                at
            } else {
                seen.push((usable_through_frames, now));
                now
            };
        progress_reading(
            sounded_frames,
            submitted_frames,
            usable_through_frames,
            observed_at,
        )
    }
}

/// A snapshot whose usable callback (when `usable_through_frames` is not
/// zero) ran at `observed_at` with `sounded_frames` as its own estimate.
fn progress_reading(
    sounded_frames: u64,
    submitted_frames: u64,
    usable_through_frames: u64,
    observed_at: Instant,
) -> PlaybackProgress {
    progress_started(
        sounded_frames,
        submitted_frames,
        usable_through_frames,
        observed_at,
        0,
    )
}

/// [`progress_reading`] for a callback that began before playback did, with
/// `startup_delay_frames` still to elapse before frame zero.
fn progress_started(
    sounded_frames: u64,
    submitted_frames: u64,
    usable_through_frames: u64,
    observed_at: Instant,
    startup_delay_frames: u64,
) -> PlaybackProgress {
    PlaybackProgress {
        submitted_frames,
        sounded_frames,
        usable_through_frames,
        usable_reading: (usable_through_frames > 0).then_some(UsableReading {
            observed_at,
            sounded_frames,
            startup_delay_frames,
        }),
    }
}
