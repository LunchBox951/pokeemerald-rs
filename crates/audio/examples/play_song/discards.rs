//! Samples the output transport throws away unplayed. A replacement stream
//! discards the queue it attaches over (a default-output change, a lost
//! stream), which empties the ring and raises no stream error, so neither the
//! ring nor `stream_errors` can tell it from a clean drain. The smoke check
//! reads `AudioOutput::discarded_samples` instead.

use std::time::{Duration, Instant};

use platform::audio::PlaybackProgress;
use platform::AudioOutput;

use super::{
    measured_drain_target, wait_for_device_tail_or_measured, wait_for_drain, DrainError,
    RetryPolicy,
};

/// The lifetime [`AudioOutput::discarded_samples`] count when playback began,
/// so a discard from before it (a reconnect during setup) does not fail it.
#[derive(Clone, Copy, Debug)]
pub(super) struct DiscardBaseline(u64);

impl DiscardBaseline {
    pub(super) fn of(output: &AudioOutput) -> Self {
        Self(output.discarded_samples())
    }

    /// Samples discarded since the baseline was taken.
    pub(super) fn since(self, output: &AudioOutput) -> u64 {
        output.discarded_samples().saturating_sub(self.0)
    }
}

/// `Ok` while no sample was discarded since the baseline; otherwise
/// [`DrainError::QueueDiscarded`].
pub(super) fn queue_intact(discarded: u64) -> Result<(), DrainError> {
    if discarded > 0 {
        Err(DrainError::QueueDiscarded { samples: discarded })
    } else {
        Ok(())
    }
}

/// `result` unless samples were `discarded`, which outranks an `Ok` and a
/// timeout (a replaced stream stalls the waits and explains them) but not a
/// reported stream error, the more specific cause.
fn unless_discarded(result: Result<(), DrainError>, discarded: u64) -> Result<(), DrainError> {
    match result {
        Err(
            err @ (DrainError::StreamStopped { .. } | DrainError::StreamStoppedDuringTail { .. }),
        ) => Err(err),
        other => queue_intact(discarded).and(other),
    }
}

/// `main`'s end of playback: wait for the ring to drain, then hold the stream
/// open until the last samples sound, failing if the transport discarded any
/// queued sample meanwhile.
///
/// The discard count is read after each wait, so it is monotonic evidence: a
/// discard at any point since the baseline, including one followed by a
/// recovered `Ready` transport, fails the run. The ring reads empty after a
/// discard, so the count is read after the emptiness (the transport raises it
/// first), and a drain that saw the ring empty also sees the discard.
///
/// Prefers the device's own measured playback position when it has one (see
/// `platform::AudioOutput::playback_progress`); the derived `derived_tail`
/// is only the fallback for a host that never reports a usable timestamp. The
/// target is widened by the resampler's own settle margin
/// (`playback_settle_margin_frames`), or a resampled device's buffered
/// interpolation tail could still be sounding real audio after the wait
/// already ended -- see [`measured_drain_target`]. Hooks are injected as in
/// `push_frame`.
#[expect(
    clippy::too_many_arguments,
    reason = "the injected clock, sleep, and stream hooks are the seam the tests drive"
)]
pub(super) fn finish_playback(
    ring_capacity_samples: usize,
    derived_tail: Duration,
    device_sample_rate: u32,
    settle_margin_frames: u64,
    policy: &RetryPolicy,
    mut available_space: impl FnMut() -> usize,
    mut progress: impl FnMut() -> Option<PlaybackProgress>,
    mut stream_errors: impl FnMut() -> u64,
    mut discarded: impl FnMut() -> u64,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<(), DrainError> {
    let drained = wait_for_drain(
        ring_capacity_samples,
        policy,
        &mut available_space,
        &mut stream_errors,
        &mut now,
        &mut sleep,
    );
    unless_discarded(drained, discarded())?;
    let submitted_target = progress()
        .map(|progress| measured_drain_target(progress.submitted_frames, settle_margin_frames));
    let tail = wait_for_device_tail_or_measured(
        submitted_target,
        derived_tail,
        device_sample_rate,
        settle_margin_frames,
        policy,
        progress,
        stream_errors,
        now,
        sleep,
    );
    unless_discarded(tail, discarded())
}
