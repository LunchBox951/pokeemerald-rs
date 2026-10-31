//! Playback-health check: did the device play any of the song as silence?

/// Samples the device played as silence because the ring ran dry, counted
/// against a snapshot taken before playback began.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UnderrunError {
    pub(super) samples: u64,
}

impl UnderrunError {
    pub(super) fn describe(self) -> String {
        format!(
            "{} sample(s) played as silence due to ring-buffer underrun during song rendering",
            self.samples
        )
    }
}

/// Fail if the midstream underrun count has grown past `baseline`.
///
/// `current` must be `AudioOutput::midstream_underruns`, which counts only
/// silence followed by a sample queued after it. The plain `underruns` total
/// also counts the padding a draining ring produces once the last frame is
/// queued, which can land before this runs if the producer is descheduled.
///
/// `baseline` is fixed for the run (a nonzero lifetime total is not a
/// failure). Only call this while the song is still being rendered: once the
/// last frame is queued the ring empties and the device pads silence by design.
pub(super) fn check_underruns(baseline: u64, current: u64) -> Result<(), UnderrunError> {
    match current.checked_sub(baseline) {
        Some(samples) if samples > 0 => Err(UnderrunError { samples }),
        _ => Ok(()),
    }
}
