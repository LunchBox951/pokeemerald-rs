//! Audio output device: opens the default output device — or a
//! headless-friendly null backend for tests/CI, since CI runners have no
//! audio device — and streams PCM pulled from a [`crate::ring`] ring buffer
//! that its caller fills with the `audio` crate's rendered M4A output (in
//! practice the integration crate's frame-driven music player).
//!
//! `cpal` is owner-approved for exactly this crate and exactly this use:
//! open the default output device, one stream, a ring-buffer callback. No
//! decoding, no effects — see [`Resampler`] below for the one deliberate
//! exception (bridging a sample-rate mismatch is format adaptation, not an
//! effect).
//!
//! ## Design
//!
//! - **Sample format**: the ring buffer always carries interleaved `f32`
//!   samples (cpal's most portable format, and natural headroom for
//!   downstream mixing). If the device's negotiated stream format is `i16`
//!   instead (common on Linux/ALSA), the device callback converts on the
//!   way out; the ring buffer and its producers never need to know.
//! - **Sample rate**: [`AudioOutput::M4A_MIXER_RATE`] (13379 Hz, the rate
//!   upstream's M4A engine actually renders PCM at — see the const's docs) is
//!   the ring buffer's nominal rate for pitch/synthesis purposes; the `audio`
//!   crate renders at this rate unconditionally. The ring buffer's *actual*
//!   production cadence is `AudioOutput::source_cadence_hz` instead.
//!   Devices virtually never advertise 13379 Hz, so a
//!   [`crate::resample::Resampler`] bridging nominal to actual rate inside
//!   the callback (`Source::Resampled`) is the common path; direct 1:1
//!   streaming (`Source::Direct`) is reserved for the null backend.
//! - **Channels**: fixed at [`AudioOutput::CHANNELS`] (stereo), matching the
//!   GBA's Direct Sound A/B stereo output. A device with no stereo output
//!   config at all is out of scope and reported as
//!   [`PlatformError::UnsupportedAudioConfig`].
//! - **Underruns**: `Source::fill` always fills its output buffer
//!   completely; any shortfall is silence, counted via
//!   [`crate::ring::Consumer::fill`]'s non-blocking bulk drain (see
//!   `crate::ring` and `crate::resample`) so audio-health checks can
//!   observe shortfalls.
//! - **Stream health**: underruns cover the producer-outran-consumer case,
//!   but a `cpal` stream can also fail asynchronously (device disconnect,
//!   driver error) on its own callback thread. Those are counted separately
//!   via [`AudioOutput::stream_errors`]; a nonzero count means the stream is
//!   unhealthy even when [`AudioOutput::underruns`] stays flat.
//! - **Playback position**: a real device's callback captures
//!   `cpal::OutputCallbackInfo::timestamp()` on every invocation and derives
//!   a monotonic, best-effort "device frames actually sounded" estimate from
//!   it, published lock-free via atomics and exposed as
//!   [`AudioOutput::playback_progress`]. A host that never reports a usable
//!   timestamp pair (including [`AudioOutput::null`], unless a test enables
//!   one by hand) leaves this `None` forever, so callers that wait on
//!   playback position keep a derived-bound fallback for that case — see
//!   `pokeemerald_rs::music::player::MusicPlayer::drained` and the
//!   `play_song` example's `device_tail_wait`.
//!
//! CI is headless, so nothing here opens a real cpal stream in a test: only
//! [`AudioOutput::open`] and the private `negotiate`/stream-building helpers
//! touch `cpal` directly. Both take their cpal calls behind
//! `OutputDevice`, so each stage's error mapping is testable against a
//! fake that opens no device. The ring buffer and resampler — the logic that
//! actually matters for correctness — are pure and fully unit tested
//! against [`AudioOutput::null`] and the `ring`/`resample` modules directly.

use std::sync::atomic::{fence, AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{HostTrait, StreamTrait};

use crate::error::PlatformError;
use crate::resample::Resampler;
use crate::ring::{ring_buffer, Consumer, Producer};

mod config;
mod stream;
#[cfg(test)]
// Tests compare PCM sample arrays for exact equality on purpose: every
// value here is the deliberate, exactly-representable output of a ring
// buffer push/fill, not the result of accumulated floating-point math.
#[allow(clippy::float_cmp)]
mod tests;

use config::{max_buffer_frames, negotiate};
use stream::build_stream;

/// Either play ring-buffer samples straight through, or bridge a sample-rate
/// mismatch via [`Resampler`] — see the module docs.
///
/// `Direct` is reserved for [`AudioOutput::null`], which its caller clocks
/// by hand. A real device, built through [`source_for_device`], is always
/// `Resampled`, even at exactly [`AudioOutput::M4A_MIXER_RATE`] Hz: its
/// clock free-runs at that integer rate, not at
/// [`AudioOutput::source_cadence_hz`].
///
/// Both variants bottom out in [`crate::ring::Consumer::fill`]'s non-blocking
/// bulk drain, so the underrun-safe behaviour tested against the null backend
/// below is exactly what the real device callback runs.
enum Source {
    Direct(Consumer),
    Resampled(Resampler),
}

impl Source {
    fn fill(&mut self, out: &mut [f32]) {
        match self {
            Self::Direct(consumer) => consumer.fill(out),
            Self::Resampled(resampler) => resampler.fill(out),
        }
    }

    /// See [`AudioOutput::playback_settle_margin_frames`]: zero for `Direct`
    /// (no interpolation buffering to defer a lookahead pull through), the
    /// resampler's own [`Resampler::settle_margin_frames`] for `Resampled`.
    fn settle_margin_frames(&self) -> u64 {
        match self {
            Self::Direct(_) => 0,
            Self::Resampled(resampler) => resampler.settle_margin_frames(),
        }
    }
}

/// Build the [`Source`] a real device's stream is driven through — see
/// [`Source`]'s docs for why this is always [`Source::Resampled`].
///
/// Extracted from [`AudioOutput::open`] so this selection is unit-testable
/// without a `cpal` device (only `open` itself touches `cpal` to get here —
/// see the module docs).
///
/// # Errors
///
/// See [`crate::resample::Resampler::new`]'s `UnsupportedResampleRatio` doc.
fn source_for_device(
    consumer: Consumer,
    channels: u16,
    device_sample_rate: u32,
    max_output_frames: usize,
) -> Result<Source, PlatformError> {
    Ok(Source::Resampled(Resampler::new(
        consumer,
        channels,
        AudioOutput::source_cadence_hz(),
        device_sample_rate,
        max_output_frames,
    )?))
}

/// The open output stream/device, or the null stand-in used by tests and
/// headless environments.
enum Backend {
    Null(Source),
    Device(cpal::Stream),
}

/// Shared playback-position signal, written only by the real device's
/// callback thread and read by any number of other threads — atomics, not a
/// lock, so the real-time callback (see the module docs' "no allocation"
/// rule and `crates/platform/tests/realtime_alloc.rs`) never blocks on a
/// reader.
///
/// [`AudioOutput::null`] starts [`Self::timestamp_available`] `false` and
/// leaves it there unless a test enables it by hand (see
/// `AudioOutput::enable_playback_progress_for_test`), so
/// [`AudioOutput::playback_progress`] reads `None` for the whole lifetime of
/// an unmodified null-backed instance — exactly like a real host that never
/// reports a usable timestamp.
struct PlaybackClock {
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
    fn new() -> Self {
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
    /// called *before* [`Source::fill`] consumes them from the ring.
    /// Allocation-free (atomic loads/stores only), so it is safe to call
    /// from the real-time callback thread.
    ///
    /// A callback whose timestamp pair does not support an estimate this
    /// time (see [`estimate_sounded_frames`]) simply leaves the published
    /// estimate and the usable frame mark where they were.
    fn record_callback(
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

/// A snapshot of [`AudioOutput`]'s measured playback-position signal — see
/// [`AudioOutput::playback_progress`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackProgress {
    /// Device frames submitted to the output callback so far (monotonic
    /// non-decreasing across calls against the same [`AudioOutput`]).
    pub submitted_frames: u64,
    /// The most recent conservative estimate of device frames that have
    /// actually sounded, derived from the host's own callback timestamps
    /// (monotonic non-decreasing; see the module docs). All three fields come
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

/// An owned audio-output subsystem: opens (at most) one output stream and
/// exposes a [`Producer`] handle its caller fills with rendered PCM.
///
/// No global state: every [`AudioOutput`] owns its own device/stream (or
/// null stand-in) and ring buffer. Dropping it tears the backend down
/// cleanly — `cpal::Stream`'s own `Drop` stops the stream and releases the
/// device; the null backend holds no OS resources to release.
pub struct AudioOutput {
    backend: Backend,
    producer: Producer,
    /// The rate the `audio` crate should always render at; see the module
    /// docs. Not necessarily the device's physical rate — see
    /// `device_sample_rate`.
    sample_rate: u32,
    device_sample_rate: u32,
    channels: u16,
    running: bool,
    /// Count of asynchronous `cpal` stream errors reported to the callback's
    /// error function (device disconnect, driver failure, …). Shared with the
    /// stream's error closure; a nonzero value means the stream is unhealthy.
    /// Always zero for the null backend, which owns no `cpal` stream.
    stream_errors: Arc<AtomicU64>,
    /// `max_buffer_frames` of the negotiated config; `0` for the null backend
    /// and for a device advertising no concrete range.
    max_callback_frames: usize,
    /// Measured playback-position signal — see [`Self::playback_progress`].
    playback_clock: Arc<PlaybackClock>,
    /// See [`Self::playback_settle_margin_frames`]. Fixed at construction
    /// (a function of the source's resample step, which never changes).
    settle_margin_frames: u64,
}

impl AudioOutput {
    /// The rate upstream's M4A engine actually renders PCM at — the nominal
    /// producer contract for the ring buffer and the `audio` crate, which
    /// renders at exactly this rate.
    ///
    /// Derived from `pokeemerald/src/m4a.c`: `m4aSoundInit` selects
    /// `SOUND_MODE_FREQ_13379` (m4a.c:79), and `SoundInit` calls
    /// `SampleFreqSet(SOUND_MODE_FREQ_13379)` (m4a.c:395). `SampleFreqSet`
    /// (m4a.c:400) looks up `gPcmSamplesPerVBlank = gPcmSamplesPerVBlankTable[
    /// freq - 1]` — with the `13379` frequency index that is table entry `224`
    /// (`m4a_tables.c:107`) — then computes
    /// `pcmFreq = (597275 * pcmSamplesPerVBlank + 5000) / 10000`
    /// (m4a.c:410) = `(597275 * 224 + 5000) / 10000` = **13379 Hz**.
    ///
    /// Note: 32768 Hz (a value seen elsewhere in GBA audio docs) is only the
    /// `SOUNDBIAS` DAC/PWM carrier frequency, *not* the mixer's PCM render
    /// rate. Using it here would bake in a ~2.45x pitch/timing error, so this
    /// contract is the mixer rate. Devices virtually never advertise 13379 Hz,
    /// so resampling to the device rate is the norm — see the module docs.
    pub const M4A_MIXER_RATE: u32 = 13_379;

    /// Stereo frames the ring buffer's producer (in practice the integration
    /// crate's frame-driven music player) pushes per game frame — one call to
    /// `MusicPlayer::advance_frame` per `App::step`, each rendering
    /// `Sequencer::FRAME_SAMPLES / CHANNELS` frames. A duplicate of the
    /// `audio` crate's `SAMPLES_PER_FRAME`, not an import of it: `platform`
    /// has no dependency on `audio`, only the reverse, as a dev dependency
    /// (see `crates/audio/Cargo.toml`). Used only by
    /// [`Self::source_cadence_hz`].
    const M4A_SAMPLES_PER_GAME_FRAME: u32 = 224;

    /// Interleaved channel count the ring buffer and device stream use
    /// (stereo, matching the GBA's Direct Sound A/B output).
    pub const CHANNELS: u16 = 2;

    /// The ring buffer's real production cadence, about 13378.96 Hz:
    /// [`Self::M4A_SAMPLES_PER_GAME_FRAME`] frames per
    /// [`crate::pacing::GBA_FRAME_PERIOD`]. The [`Resampler`] drains at this
    /// rate, not the rounded [`Self::M4A_MIXER_RATE`] upstream uses for
    /// pitch; the difference is a rate bias that drains the ring over hours.
    fn source_cadence_hz() -> f64 {
        f64::from(Self::M4A_SAMPLES_PER_GAME_FRAME) / crate::pacing::GBA_FRAME_PERIOD.as_secs_f64()
    }

    /// Open the default output device and negotiate [`Self::M4A_MIXER_RATE`]
    /// or the nearest supported rate (falling back to on-the-fly resampling
    /// if the exact rate is unavailable — the common case, see the module
    /// docs). The stream is created but not started; call
    /// [`AudioOutput::start`].
    ///
    /// `ring_capacity_frames` sizes the ring buffer in stereo frames (e.g.
    /// `4096` is ~306ms of headroom at the nominal 13379 Hz rate).
    ///
    /// # Errors
    ///
    /// - [`PlatformError::NoAudioDevice`] if there is no default output
    ///   device, or the one the host named cannot be reached at all
    ///   (headless CI, no audio hardware, no driver running) — see
    ///   `classify_query_error`.
    /// - [`PlatformError::UnsupportedAudioConfig`] if the device has no
    ///   usable stereo output configuration.
    /// - [`PlatformError::Audio`] if `cpal` fails to query a reachable
    ///   device, or fails to build the stream. A device lost *after* the
    ///   query stays here rather than collapsing into `NoAudioDevice`: the
    ///   device was real, so losing it is a failure, not a headless run.
    /// - [`PlatformError::UnsupportedResampleRatio`] if the negotiated
    ///   device rate pairs with `Self::source_cadence_hz` into a ratio the
    ///   resampler's bounded scratch cannot carry (see
    ///   [`crate::resample::Resampler::new`]).
    pub fn open(ring_capacity_frames: usize) -> Result<Self, PlatformError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(PlatformError::NoAudioDevice)?;
        let config = negotiate(&device)?;

        let device_sample_rate = config.sample_rate();
        let channels = config.channels();
        let (producer, consumer) = ring_buffer(ring_capacity_frames * channels as usize);
        let source = source_for_device(
            consumer,
            channels,
            device_sample_rate,
            max_buffer_frames(&config),
        )?;
        let settle_margin_frames = source.settle_margin_frames();

        let stream_errors = Arc::new(AtomicU64::new(0));
        let playback_clock = Arc::new(PlaybackClock::new());
        let stream = build_stream(
            &device,
            &config,
            source,
            Arc::clone(&stream_errors),
            Arc::clone(&playback_clock),
        )?;

        Ok(Self {
            backend: Backend::Device(stream),
            producer,
            sample_rate: Self::M4A_MIXER_RATE,
            device_sample_rate,
            channels,
            running: false,
            stream_errors,
            max_callback_frames: max_buffer_frames(&config),
            playback_clock,
            settle_margin_frames,
        })
    }

    /// An explicit headless/null backend: opens no OS audio device.
    ///
    /// Always available (no hardware required), and the only backend unit
    /// tests may construct — CI runners have no audio device, so `cargo
    /// test` must never open a real `cpal` stream. Drive it by hand with
    /// [`AudioOutput::pull_null`]. Always plays samples straight through with
    /// no interpolation buffering (see the module docs), so
    /// [`Self::playback_settle_margin_frames`] is always `0`; a test that
    /// needs to exercise a resampler's deferred-lookahead tail without a real
    /// `cpal` device wants [`Self::null_resampled`] instead.
    #[must_use]
    pub fn null(ring_capacity_frames: usize) -> Self {
        let (producer, consumer) = ring_buffer(ring_capacity_frames * usize::from(Self::CHANNELS));
        Self {
            backend: Backend::Null(Source::Direct(consumer)),
            producer,
            sample_rate: Self::M4A_MIXER_RATE,
            device_sample_rate: Self::M4A_MIXER_RATE,
            channels: Self::CHANNELS,
            running: false,
            stream_errors: Arc::new(AtomicU64::new(0)),
            max_callback_frames: 0,
            playback_clock: Arc::new(PlaybackClock::new()),
            settle_margin_frames: 0,
        }
    }

    /// Test-only headless backend that resamples like a real device instead
    /// of playing samples straight through, so a test can exercise
    /// [`Self::playback_settle_margin_frames`]'s nonzero (real-device) case
    /// through [`Self::pull_null`] without a real `cpal` device. Otherwise
    /// identical to [`Self::null`]: `source_rate`/`device_rate` feed
    /// [`crate::resample::Resampler::new`] exactly as a real device's
    /// negotiated rate would.
    ///
    /// # Errors
    ///
    /// See [`crate::resample::Resampler::new`]'s `UnsupportedResampleRatio` doc.
    #[doc(hidden)]
    pub fn null_resampled(
        ring_capacity_frames: usize,
        source_rate: f64,
        device_rate: u32,
        max_output_frames: usize,
    ) -> Result<Self, PlatformError> {
        let channels = Self::CHANNELS;
        let (producer, consumer) = ring_buffer(ring_capacity_frames * usize::from(channels));
        // Unlike `source_for_device` (which always resamples from the fixed
        // M4A source cadence), this test seam takes `source_rate` directly so
        // a test can reproduce an exact rate ratio (e.g. the reviewer's
        // step = 0.25 regression) without depending on that constant.
        let source = Source::Resampled(Resampler::new(
            consumer,
            channels,
            source_rate,
            device_rate,
            max_output_frames,
        )?);
        let settle_margin_frames = source.settle_margin_frames();
        Ok(Self {
            backend: Backend::Null(source),
            producer,
            sample_rate: Self::M4A_MIXER_RATE,
            device_sample_rate: device_rate,
            channels,
            running: false,
            stream_errors: Arc::new(AtomicU64::new(0)),
            max_callback_frames: 0,
            playback_clock: Arc::new(PlaybackClock::new()),
            settle_margin_frames,
        })
    }

    /// Start (or resume) playback.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Audio`] if `cpal` refuses to play the
    /// stream (e.g. the device was disconnected). Always succeeds for the
    /// null backend.
    pub fn start(&mut self) -> Result<(), PlatformError> {
        if let Backend::Device(stream) = &self.backend {
            stream.play()?;
        }
        self.running = true;
        Ok(())
    }

    /// Pause playback; the device/stream stays open and can be
    /// [`AudioOutput::start`]ed again.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Audio`] if `cpal` refuses to pause the
    /// stream. Always succeeds for the null backend.
    pub fn stop(&mut self) -> Result<(), PlatformError> {
        if let Backend::Device(stream) = &self.backend {
            stream.pause()?;
        }
        self.running = false;
        Ok(())
    }

    /// Whether [`AudioOutput::start`] has been called more recently than
    /// [`AudioOutput::stop`].
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// The nominal PCM sample rate the `audio` crate should always render
    /// at ([`Self::M4A_MIXER_RATE`]), regardless of the device's physical
    /// rate.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The audio device's actual negotiated sample rate. Equal to
    /// [`AudioOutput::sample_rate`] unless a [`Resampler`] is bridging a
    /// mismatch (see the module docs); always equal for the null backend.
    #[must_use]
    pub fn device_sample_rate(&self) -> u32 {
        self.device_sample_rate
    }

    /// Interleaved channel count of the ring buffer / device stream.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// The largest callback buffer the device advertises, in frames at
    /// [`Self::device_sample_rate`], or `None` when it advertises no concrete
    /// range. It bounds one callback period only, not the periods the host
    /// keeps queued behind it, so a caller waiting for the tail to sound
    /// derives its wait from this bound rather than sleeping it verbatim.
    /// Always `None` for the null backend.
    #[must_use]
    pub fn max_callback_frames(&self) -> Option<usize> {
        match self.max_callback_frames {
            0 => None,
            frames => Some(frames),
        }
    }

    /// A cloneable producer handle for filling the ring buffer with
    /// rendered PCM. See [`crate::ring::Producer`].
    #[must_use]
    pub fn producer(&self) -> Producer {
        self.producer.clone()
    }

    /// Total samples played as silence so far due to ring-buffer underrun
    /// (producer outran consumer). Distinct from [`Self::stream_errors`],
    /// which covers asynchronous device/driver failures.
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.producer.underruns()
    }

    /// Count of asynchronous `cpal` stream errors reported since the stream
    /// was built (device disconnect, driver failure, …).
    ///
    /// A nonzero value means the stream is unhealthy: playback may have
    /// stopped at the OS level even though [`Self::is_running`] still reports
    /// `true` and [`Self::underruns`] is flat, because the callback that
    /// drains the ring buffer is no longer being invoked. Always zero for the
    /// null backend, which owns no `cpal` stream.
    #[must_use]
    pub fn stream_errors(&self) -> u64 {
        self.stream_errors.load(Ordering::Relaxed)
    }

    /// Records one asynchronous stream error as the `cpal` error closure in
    /// `build_stream` does, so a null-backend test can stand in for a device.
    #[doc(hidden)]
    pub fn record_stream_error_for_test(&self) {
        self.stream_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// A measured playback-position signal derived from the host's own
    /// callback timestamps (see the module docs' "Playback position"
    /// bullet), or `None` if no callback has yet published a usable one —
    /// including for the whole lifetime of an unmodified [`AudioOutput::null`]
    /// instance. A caller waiting for playback to catch up to a target frame
    /// count should fall back to a derived bound when this is `None`; see
    /// `pokeemerald_rs::music::player::MusicPlayer::drained` and the
    /// `play_song` example's `device_tail_wait`.
    #[must_use]
    pub fn playback_progress(&self) -> Option<PlaybackProgress> {
        if !self
            .playback_clock
            .timestamp_available
            .load(Ordering::Acquire)
        {
            return None;
        }
        // One callback's stores whole: see `PlaybackClock::snapshot`.
        Some(self.playback_clock.snapshot())
    }

    /// Extra device frames, beyond a [`Self::playback_progress`] snapshot's
    /// `submitted_frames`, that may still carry real, audible content sounded
    /// from a [`crate::resample::Resampler`]'s buffered interpolation state —
    /// see that type's module docs' "deferred lookahead pull". Always `0` for
    /// the null backend (see [`Self::null`]) and any device whose source
    /// plays samples straight through with no interpolation buffering.
    ///
    /// A caller latching a "done" target from `submitted_frames` the instant
    /// its own ring reads empty (as `pokeemerald_rs::music::player::MusicPlayer::drained`
    /// and the `play_song` example's tail wait both do) must add this margin
    /// to that target before waiting for `sounded_frames` to reach it, or it
    /// can end the wait one callback before the resampler's real, decaying
    /// tail actually finishes sounding.
    #[must_use]
    pub fn playback_settle_margin_frames(&self) -> u64 {
        self.settle_margin_frames
    }

    /// Marks this instance's playback-position signal available, as a real
    /// device's first callback with a usable timestamp would, so a
    /// null-backed test can drive [`Self::playback_progress`] without a
    /// `cpal` device. See [`Self::advance_sounded_frames_for_test`].
    #[doc(hidden)]
    pub fn enable_playback_progress_for_test(&self) {
        self.playback_clock
            .timestamp_available
            .store(true, Ordering::Release);
    }

    /// Advances this instance's fake sounded-frame position for a
    /// null-backed test, the way a real device callback's own estimate
    /// would — clamped to monotonic non-decreasing, so a lower `frames`
    /// never moves it backward. Also marks every frame submitted so far
    /// as covered by a usable callback (`PlaybackProgress::usable_through_frames`),
    /// moved or not. Has no effect on
    /// [`Self::playback_progress`] until
    /// [`Self::enable_playback_progress_for_test`] has been called.
    #[doc(hidden)]
    pub fn advance_sounded_frames_for_test(&self, frames: u64) {
        let clock = &self.playback_clock;
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

    /// Drive the null backend by hand, filling `out` through the exact same
    /// underrun-safe path the real device callback runs (see the module
    /// docs and [`crate::ring::Consumer::fill`]).
    ///
    /// A no-op (leaves `out` untouched) if this instance was opened against
    /// a real device via [`AudioOutput::open`] — the OS drives consumption
    /// there instead, on its own callback thread. Advances the submitted
    /// device-frame count read back through [`Self::playback_progress`]
    /// before filling, exactly as a real device callback does, so it tracks
    /// a null-backed test's own draining regardless of whether the test has
    /// enabled [`Self::playback_progress`] via the hooks above.
    pub fn pull_null(&mut self, out: &mut [f32]) {
        if let Backend::Null(source) = &mut self.backend {
            let channels = usize::from(self.channels).max(1);
            let frames = u64::try_from(out.len() / channels).unwrap_or(u64::MAX);
            self.playback_clock.record(frames, |_| None);
            source.fill(out);
        }
    }
}
