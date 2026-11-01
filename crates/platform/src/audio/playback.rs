//! The lock-free playback clock behind `AudioOutput::playback_progress`.
//!
//! Owns the single-writer counters the real device callback advances, the
//! two-slot publication that lets any thread read them whole, and the
//! timestamp estimator that turns a host callback/playback pair into a
//! conservative sounded-frame count. The transport that feeds the callback
//! stays in the parent `audio` module.

use std::sync::atomic::{fence, AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

/// Shared playback-position signal, written only by the real device's
/// callback thread and read by any number of other threads — atomics, not a
/// lock, so the real-time callback (see the audio module docs' "no allocation"
/// rule and `crates/platform/tests/realtime_alloc.rs`) never blocks on a
/// reader.
///
/// [`crate::audio::AudioOutput::null`] starts [`Self::timestamp_available`] `false` and
/// leaves it there unless a test enables it by hand (see
/// `crate::audio::AudioOutput::enable_playback_progress_for_test`), so
/// [`crate::audio::AudioOutput::playback_progress`] reads `None` for the whole lifetime of
/// an unmodified null-backed instance — exactly like a real host that never
/// reports a usable timestamp.
pub(super) struct PlaybackClock {
    /// Device frames handed to the callback so far — advanced *before* the
    /// callback consumes them from the ring (see [`Self::record_callback`]),
    /// so a reader that observes the ring newly empty already sees the frame
    /// count for the callback holding the last samples, not the one before
    /// it.
    submitted_frames: AtomicU64,
    /// The most recent conservative estimate of device frames that have
    /// actually sounded (see [`estimate_sounded_frames`]). Published via
    /// `fetch_max` so a jittery host estimate can never move it backward.
    sounded_frames: AtomicU64,
    /// Whether at least one callback has published a usable host timestamp.
    /// Sticky: once observed, never reset — a single callback whose
    /// timestamp pair doesn't support an estimate (see
    /// [`estimate_sounded_frames`]) does not undo an earlier one that did.
    timestamp_available: AtomicBool,
    /// The `submitted_frames` total at the end of the most recent callback
    /// that carried a usable timestamp (see [`estimate_sounded_frames`]).
    /// Unlike `sounded_frames`, which `fetch_max` leaves flat for an estimate
    /// that repeats or moves backward, this advances for every usable
    /// callback. Stored before that callback's `submitted_frames`, so the live
    /// mark can lead the live submitted total; readers take both from the
    /// published copy (see [`PlaybackClock::write`]), where it cannot.
    usable_through_frames: AtomicU64,
    /// When the most recent usable callback was observed, as nanoseconds
    /// since `origin` plus one; zero while no callback has been usable (see
    /// [`UsableReading`]). Advances with `usable_through_frames`.
    reading_stamp: AtomicU64,
    /// That callback's own sounded estimate, before `fetch_max` folds it into
    /// `sounded_frames`.
    reading_sounded_frames: AtomicU64,
    /// That callback's playback delay beyond the frames submitted before it.
    reading_startup_delay_frames: AtomicU64,
    /// The instant `reading_stamp` counts from.
    origin: Instant,
    /// Two published copies of the counters, each under its own
    /// sequence (see [`Self::write`]); `published_slot` names the one a
    /// reader takes.
    published: [PublishedSlot; 2],
    /// Which of `published` holds the latest completed callback's counters.
    published_slot: AtomicUsize,
}

/// One published copy of [`PlaybackClock`]'s counters, guarded by a
/// sequence that is odd while the writer is storing into it.
struct PublishedSlot {
    sequence: AtomicU64,
    submitted_frames: AtomicU64,
    sounded_frames: AtomicU64,
    usable_through_frames: AtomicU64,
    reading_stamp: AtomicU64,
    reading_sounded_frames: AtomicU64,
    reading_startup_delay_frames: AtomicU64,
}

impl PublishedSlot {
    fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            submitted_frames: AtomicU64::new(0),
            sounded_frames: AtomicU64::new(0),
            usable_through_frames: AtomicU64::new(0),
            reading_stamp: AtomicU64::new(0),
            reading_sounded_frames: AtomicU64::new(0),
            reading_startup_delay_frames: AtomicU64::new(0),
        }
    }
}

impl PlaybackClock {
    pub(super) fn new() -> Self {
        Self {
            submitted_frames: AtomicU64::new(0),
            sounded_frames: AtomicU64::new(0),
            timestamp_available: AtomicBool::new(false),
            usable_through_frames: AtomicU64::new(0),
            reading_stamp: AtomicU64::new(0),
            reading_sounded_frames: AtomicU64::new(0),
            reading_startup_delay_frames: AtomicU64::new(0),
            origin: Instant::now(),
            published: [PublishedSlot::new(), PublishedSlot::new()],
            published_slot: AtomicUsize::new(0),
        }
    }

    /// The latest completed callback's counters, or `None` while no callback
    /// has published a usable timestamp (see [`Self::snapshot`]).
    pub(super) fn progress(&self) -> Option<PlaybackProgress> {
        if !self.timestamp_available.load(Ordering::Acquire) {
            return None;
        }
        // One callback's stores whole: see `PlaybackClock::snapshot`.
        Some(self.snapshot())
    }

    /// Marks the clock available, as a real device's first usable callback
    /// would, for null-backed tests.
    pub(super) fn enable_playback_progress_for_test(&self) {
        self.timestamp_available.store(true, Ordering::Release);
    }

    /// Advances the fake sounded-frame position for a null-backed test: see
    /// `AudioOutput::advance_sounded_frames_for_test`.
    pub(super) fn advance_sounded_frames_for_test(&self, frames: u64) {
        let clock = self;
        clock.write(|| {
            // Stamped as a callback observing `frames` now would be.
            let stamp = clock
                .origin
                .elapsed()
                .as_nanos()
                .try_into()
                .map_or(0, |nanos: u64| nanos.saturating_add(1));
            clock.reading_stamp.store(stamp, Ordering::Relaxed);
            clock
                .reading_sounded_frames
                .store(frames, Ordering::Relaxed);
            clock
                .reading_startup_delay_frames
                .store(0, Ordering::Relaxed);
            clock.sounded_frames.fetch_max(frames, Ordering::Release);
            let submitted = clock.submitted_frames.load(Ordering::Acquire);
            clock
                .usable_through_frames
                .fetch_max(submitted, Ordering::Release);
        });
    }

    /// Records `frame_count` frames handed to a null-backed pull, with no
    /// timestamp estimate.
    pub(super) fn record_submitted(&self, frame_count: u64) {
        self.record(frame_count, |_| None);
    }

    /// Runs `update`, the single writer's stores to the live counters, then
    /// publishes the result whole: into the slot readers are *not* taking,
    /// under that slot's sequence (odd before any store, even after all of
    /// them), and only then names it the published slot. A reader therefore
    /// always finds the latest completed callback's counters intact, even
    /// while the next callback is descheduled mid-store, and a read can tear
    /// only if two more callbacks complete during it, which it detects and
    /// retries. Allocation- and lock-free, so safe on the real-time callback.
    fn write(&self, update: impl FnOnce()) {
        update();
        let next = 1 - self.published_slot.load(Ordering::Relaxed);
        self.fill_slot(next);
        self.published_slot.store(next, Ordering::Release);
    }

    /// Copies the live counters into `published[index]` under its sequence;
    /// [`Self::write`] then names it the published slot.
    fn fill_slot(&self, index: usize) {
        // The single writer reads back its own stores.
        let submitted = self.submitted_frames.load(Ordering::Relaxed);
        let sounded = self.sounded_frames.load(Ordering::Relaxed);
        let usable_through = self.usable_through_frames.load(Ordering::Relaxed);
        let reading_stamp = self.reading_stamp.load(Ordering::Relaxed);
        let reading_sounded = self.reading_sounded_frames.load(Ordering::Relaxed);
        let reading_startup = self.reading_startup_delay_frames.load(Ordering::Relaxed);
        let slot = &self.published[index];
        let start = slot.sequence.load(Ordering::Relaxed);
        slot.sequence
            .store(start.wrapping_add(1), Ordering::Relaxed);
        // Orders the odd sequence before the stores below: a reader whose
        // acquire fence follows a load of one of them sees it.
        fence(Ordering::Release);
        slot.submitted_frames.store(submitted, Ordering::Relaxed);
        slot.sounded_frames.store(sounded, Ordering::Relaxed);
        slot.usable_through_frames
            .store(usable_through, Ordering::Relaxed);
        slot.reading_stamp.store(reading_stamp, Ordering::Relaxed);
        slot.reading_sounded_frames
            .store(reading_sounded, Ordering::Relaxed);
        slot.reading_startup_delay_frames
            .store(reading_startup, Ordering::Relaxed);
        slot.sequence
            .store(start.wrapping_add(2), Ordering::Release);
    }

    /// The latest completed callback's counters, whole: never one field from
    /// before a callback and another from after it (see [`Self::write`]).
    fn snapshot(&self) -> PlaybackProgress {
        self.snapshot_observing(|_| {})
    }

    /// [`Self::snapshot`], calling `between_loads` after the slot index load
    /// (`0`) and after each field load (`1` to `6`), so a test can interleave
    /// a callback's stores deterministically. A read counts only if its slot
    /// was neither rewritten nor replaced as the published one meanwhile: a
    /// slot rewritten but not yet published holds counters newer than the
    /// published ones, and taking them would let the next read go backward.
    /// Retries until a read lands between two completed callbacks, which
    /// only a writer completing a callback every few loads could prevent.
    fn snapshot_observing(&self, mut between_loads: impl FnMut(u32)) -> PlaybackProgress {
        loop {
            let index = self.published_slot.load(Ordering::Acquire);
            between_loads(0);
            let slot = &self.published[index];
            let before = slot.sequence.load(Ordering::Acquire);
            if before.is_multiple_of(2) {
                let submitted_frames = slot.submitted_frames.load(Ordering::Relaxed);
                between_loads(1);
                let usable_through_frames = slot.usable_through_frames.load(Ordering::Relaxed);
                between_loads(2);
                let sounded_frames = slot.sounded_frames.load(Ordering::Relaxed);
                between_loads(3);
                let reading_stamp = slot.reading_stamp.load(Ordering::Relaxed);
                between_loads(4);
                let reading_sounded = slot.reading_sounded_frames.load(Ordering::Relaxed);
                between_loads(5);
                let reading_startup = slot.reading_startup_delay_frames.load(Ordering::Relaxed);
                between_loads(6);
                fence(Ordering::Acquire);
                if slot.sequence.load(Ordering::Relaxed) == before
                    && self.published_slot.load(Ordering::Relaxed) == index
                {
                    return PlaybackProgress {
                        submitted_frames,
                        sounded_frames,
                        usable_through_frames,
                        usable_reading: self.reading(
                            reading_stamp,
                            reading_sounded,
                            reading_startup,
                        ),
                    };
                }
            }
            std::hint::spin_loop();
        }
    }

    /// The published reading fields as a [`UsableReading`]; `None` before any
    /// usable callback.
    fn reading(
        &self,
        stamp: u64,
        sounded_frames: u64,
        startup_delay_frames: u64,
    ) -> Option<UsableReading> {
        let observed = Duration::from_nanos(stamp.checked_sub(1)?);
        Some(UsableReading {
            observed_at: self.origin.checked_add(observed)?,
            sounded_frames,
            startup_delay_frames,
        })
    }

    /// Records one real-device callback of `frame_count` device frames,
    /// called *before* [`super::Source::fill`] consumes them from the ring.
    /// Allocation-free (atomic loads/stores only), so it is safe to call
    /// from the real-time callback thread.
    ///
    /// A callback whose timestamp pair does not support an estimate this
    /// time (see [`estimate_sounded_frames`]) simply leaves the published
    /// estimate and the usable frame mark where they were.
    pub(super) fn record_callback(
        &self,
        frame_count: u64,
        info: &cpal::OutputCallbackInfo,
        device_sample_rate: u32,
    ) {
        self.record_reading(frame_count, Instant::now(), |callback_start_frame| {
            estimate_sounded_frames(callback_start_frame, info.timestamp(), device_sample_rate)
        });
    }

    /// The store order behind [`Self::record_callback`]. The callback thread
    /// is the only writer, so `submitted_frames` is loaded and stored rather
    /// than `fetch_add`ed, and stored *last*: a reader that loads
    /// `submitted_frames` first and sees the callback's frames therefore also
    /// sees its estimate and usable mark, however long the callback thread is
    /// preempted between the stores. Likewise the mark is stored after the
    /// estimate, so a reader that loads the mark before the estimate never
    /// pairs a callback's mark with an older estimate. The mark may be seen
    /// *before* the frames it covers; see `PlaybackProgress::usable_through_frames`.
    /// A reader takes all three only once the callback has finished, through
    /// [`Self::write`]'s published copy, so a [`Self::snapshot`] never pairs
    /// this callback's estimate with an older mark either, the one torn read
    /// the store order alone cannot exclude.
    fn record(&self, frame_count: u64, estimate: impl FnOnce(u64) -> Option<u64>) {
        self.record_reading(frame_count, Instant::now(), |start| {
            estimate(start).map(|sounded| SoundedEstimate {
                sounded_frames: sounded,
                startup_delay_frames: 0,
            })
        });
    }

    /// [`Self::record`] for a callback observed at `observed_at`, whose
    /// estimate also carries the playback delay saturation hid. An
    /// `observed_at` before the clock's origin leaves the callback stale.
    fn record_reading(
        &self,
        frame_count: u64,
        observed_at: Instant,
        estimate: impl FnOnce(u64) -> Option<SoundedEstimate>,
    ) {
        let mut usable = false;
        self.write(|| {
            let callback_start_frame = self.submitted_frames.load(Ordering::Relaxed);
            let callback_end_frame = callback_start_frame.saturating_add(frame_count);
            let stamp = observed_at
                .checked_duration_since(self.origin)
                .and_then(|elapsed| u64::try_from(elapsed.as_nanos()).ok())
                .and_then(|nanos| nanos.checked_add(1));
            if let Some((reading, stamp)) = estimate(callback_start_frame).zip(stamp) {
                let sounded = reading.sounded_frames;
                self.reading_sounded_frames
                    .store(sounded, Ordering::Relaxed);
                self.reading_startup_delay_frames
                    .store(reading.startup_delay_frames, Ordering::Relaxed);
                self.reading_stamp.store(stamp, Ordering::Relaxed);
                self.sounded_frames.fetch_max(sounded, Ordering::Release);
                self.usable_through_frames
                    .store(callback_end_frame, Ordering::Release);
                usable = true;
            }
            self.submitted_frames
                .store(callback_end_frame, Ordering::Release);
        });
        // After the publish, so a reader that sees a usable timestamp also
        // finds the callback that carried it in the published counters.
        if usable {
            self.timestamp_available.store(true, Ordering::Release);
        }
    }
}

/// A snapshot of [`crate::audio::AudioOutput`]'s measured playback-position signal — see
/// [`crate::audio::AudioOutput::playback_progress`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackProgress {
    /// Device frames submitted to the output callback so far (monotonic
    /// non-decreasing across calls against the same [`crate::audio::AudioOutput`]).
    pub submitted_frames: u64,
    /// The most recent conservative estimate of device frames that have
    /// actually sounded, derived from the host's own callback timestamps
    /// (monotonic non-decreasing; see the audio module docs). All three fields come
    /// from the same completed callback, so their relationship within one
    /// snapshot holds as well as each field's history across calls.
    pub sounded_frames: u64,
    /// The `submitted_frames` total at the end of the latest callback with a
    /// usable timestamp, whether or not it moved `sounded_frames` (an estimate
    /// that repeats or regresses leaves that flat). A usable callback ended at or past
    /// this mark. For a submitted span `(from, to]`, `from < mark <= to` means
    /// the span `(from, mark]` held one; `mark <= from` means every callback
    /// in the span had a stale timestamp. Taken with the other fields from one
    /// completed callback, it never leads `submitted_frames` within a
    /// snapshot, and it trails them across stale callbacks.
    pub usable_through_frames: u64,
    /// The latest usable callback's own reading and when it was observed, from
    /// the same completed callback as the counters; `None` before any usable
    /// callback. Unlike `sounded_frames` it moves for every usable callback,
    /// so a reading followed by any number of stale callbacks keeps its time.
    pub usable_reading: Option<UsableReading>,
}

/// One usable callback's reading of the playback position, stamped with when
/// the callback observed it, so a reader can compute the wall-clock time the
/// reading still owed however many stale callbacks followed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsableReading {
    /// When the callback ran, on the process's monotonic [`Instant`] clock
    /// (the callback's own time, not a reader's poll).
    pub observed_at: Instant,
    /// This callback's estimate of device frames sounded, not folded into the
    /// monotonic `sounded_frames`: zero when the first buffer had not begun.
    pub sounded_frames: u64,
    /// Device frames past this callback's start that elapse before playback
    /// reaches frame zero: the delay `sounded_frames` saturated away while
    /// fewer frames than the delay had been submitted. Zero once playback
    /// has begun.
    pub startup_delay_frames: u64,
}

/// What a usable callback's host timestamp pair says: see [`UsableReading`].
#[derive(Debug, Eq, PartialEq)]
struct SoundedEstimate {
    sounded_frames: u64,
    startup_delay_frames: u64,
}

/// `delay` (a callback-to-playback gap from a [`cpal::OutputStreamTimestamp`])
/// as whole device frames at `device_sample_rate`, rounded up: undercounting
/// the delay would let a caller read a frame as sounded before it actually
/// is.
fn delay_to_frames(delay: Duration, device_sample_rate: u32) -> u64 {
    let nanos = delay.as_nanos();
    let rate = u128::from(device_sample_rate);
    u64::try_from(nanos.saturating_mul(rate).div_ceil(1_000_000_000)).unwrap_or(u64::MAX)
}

/// Estimate how many device frames have actually sounded as of a callback's
/// invocation, given `callback_start_frame` (device frames submitted before
/// this callback — the position its first sample occupies) and the host's
/// own callback/playback timestamp pair: `timestamp.playback` is the
/// predicted instant the data this callback writes (starting at
/// `callback_start_frame`) will sound, so subtracting that delay's frame
/// count from `callback_start_frame` estimates what is sounding right now.
///
/// `None` if the pair does not support an estimate: `playback` earlier than
/// `callback` is a nonsensical (or deliberately unimplemented) pair, not a
/// real zero-latency device — [`cpal::StreamInstant::checked_duration_since`]
/// already treats an exactly-equal pair (delay zero) as valid, so this only
/// excludes the inverted case.
fn estimate_sounded_frames(
    callback_start_frame: u64,
    timestamp: cpal::OutputStreamTimestamp,
    device_sample_rate: u32,
) -> Option<SoundedEstimate> {
    let delay = timestamp
        .playback
        .checked_duration_since(timestamp.callback)?;
    let delay_frames = delay_to_frames(delay, device_sample_rate);
    Some(SoundedEstimate {
        sounded_frames: callback_start_frame.saturating_sub(delay_frames),
        startup_delay_frames: delay_frames.saturating_sub(callback_start_frame),
    })
}
