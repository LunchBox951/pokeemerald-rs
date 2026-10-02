//! Tests for the `play_song` example, split by seam.

use super::{
    build_song, classify_open_error, device_tail_wait, measured_drain_target, prefill_then_start,
    push_frame, start_playback, wait_for_device_tail, wait_for_device_tail_or_measured,
    wait_for_drain, wait_for_frame_deadline, wait_for_measured_tail, DrainError, OpenOutcome,
    PushError, RetryPolicy, StartOutcome, DEVICE_TAIL_FALLBACK, DEVICE_TAIL_MARGIN,
    DEVICE_TAIL_MAX,
};
use audio::Sequencer;
use platform::audio::PlaybackProgress;
use platform::{AudioOutput, PlatformError};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

mod cadence;
mod device_tail;
mod fallback;
mod measured_wait;
mod retry;
mod startup;
mod validity;

/// A snapshot from a device whose callbacks never carry a usable timestamp.
fn progress(sounded_frames: u64, submitted_frames: u64) -> PlaybackProgress {
    progress_usable(sounded_frames, submitted_frames, 0)
}

fn progress_usable(
    sounded_frames: u64,
    submitted_frames: u64,
    usable_through_frames: u64,
) -> PlaybackProgress {
    PlaybackProgress {
        submitted_frames,
        sounded_frames,
        usable_through_frames,
    }
}
