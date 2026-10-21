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
//!   via [`AudioOutput::stream_errors`], a lifetime count across every stream
//!   the transport builds: a rise during an operation means a stream failed
//!   then, even when [`AudioOutput::underruns`] stays flat, while
//!   [`AudioOutput::transport_state`] says whether a stream is attached now.
//! - **Transport**: [`AudioOutput::open`] never waits for or fails on a
//!   missing device. A worker thread (see the private `transport` module)
//!   discovers the OS default output, builds the stream, and rebuilds it
//!   after a stream failure or a default-output change, with bounded
//!   retries. Producers keep the same ring and handle throughout; samples
//!   queued while playback was started but nothing was draining are discarded
//!   when a stream re-attaches, and a queue left while stopped is kept. Distinct [`TransportState`] transitions are
//!   available from [`AudioOutput::poll_transport_state`]; neither the device
//!   callbacks nor the game loop connect, wait, or log.
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

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::error::PlatformError;
use crate::resample::Resampler;
use crate::ring::{ring_buffer, Consumer, Producer};

pub use self::playback::{PlaybackProgress, UsableReading};
pub use self::transport::{TransportFailure, TransportState};

mod config;
mod playback;
mod stream;
#[cfg(test)]
// Tests compare PCM sample arrays for exact equality on purpose: every
// value here is the deliberate, exactly-representable output of a ring
// buffer push/fill, not the result of accumulated floating-point math.
#[allow(clippy::float_cmp)]
mod tests;
mod transport;

use transport::{CpalHost, OutputHost, Shared, Worker};

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
    Device(Worker),
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
    /// [`Self::device_sample_rate`].
    sample_rate: u32,
    channels: u16,
    /// Playback intent, negotiated device facts, error counter and
    /// playback clock, shared with the transport worker.
    shared: Arc<Shared>,
}

/// Build a ring holding `frames` interleaved frames of `channels` samples.
///
/// The ring counts samples, so the frame capacity is multiplied by the
/// channel count; an overflowing product is refused rather than wrapped into
/// a smaller ring.
fn frame_ring(frames: usize, channels: u16) -> Result<(Producer, Consumer), PlatformError> {
    let samples = frames
        .checked_mul(usize::from(channels))
        .ok_or(PlatformError::AudioRingCapacityOverflow { frames, channels })?;
    Ok(ring_buffer(samples))
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

    /// Open the default output transport. Returns without waiting for a
    /// device: a worker thread discovers the OS default output, negotiates
    /// [`Self::M4A_MIXER_RATE`] or the nearest supported rate (resampling if
    /// the exact rate is unavailable — the common case, see the module
    /// docs), and keeps the stream attached across output loss. Playback
    /// begins once the stream exists and [`AudioOutput::start`] has been
    /// called; follow [`Self::poll_transport_state`] to see when.
    ///
    /// `ring_capacity_frames` sizes the ring buffer in stereo frames (e.g.
    /// `4096` is ~306ms of headroom at the nominal 13379 Hz rate). Until a
    /// stream attaches, pushes fill the ring and then drop, as with any full
    /// ring. Device facts ([`Self::device_sample_rate`] and friends) are
    /// provisional until the transport reports [`TransportState::Ready`]
    /// and describe the current stream afterwards.
    ///
    /// # Errors
    ///
    /// - [`PlatformError::AudioRingCapacityOverflow`] if
    ///   `ring_capacity_frames` times [`Self::CHANNELS`] overflows `usize`.
    /// - [`PlatformError::Audio`] if the worker thread cannot be spawned.
    ///
    /// A missing device, an unusable configuration, or an unsupported rate
    /// ratio is not an error here: it is a [`TransportState::Unavailable`]
    /// the worker keeps retrying.
    pub fn open(ring_capacity_frames: usize) -> Result<Self, PlatformError> {
        Self::open_with_host(CpalHost, ring_capacity_frames)
    }

    /// [`Self::open`] against any host, so tests can supply a fake.
    fn open_with_host<H: OutputHost>(
        host: H,
        ring_capacity_frames: usize,
    ) -> Result<Self, PlatformError> {
        let (producer, consumer) = frame_ring(ring_capacity_frames, Self::CHANNELS)?;
        // The transport claims the consumer back for every stream it builds.
        drop(consumer);
        let shared = Arc::new(Shared::new(
            TransportState::Connecting,
            Self::M4A_MIXER_RATE,
            0,
        ));
        let worker = Worker::spawn(host, producer.clone(), Arc::clone(&shared))?;
        Ok(Self {
            backend: Backend::Device(worker),
            producer,
            sample_rate: Self::M4A_MIXER_RATE,
            channels: Self::CHANNELS,
            shared,
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
    ///
    /// # Panics
    ///
    /// If `ring_capacity_frames` times [`Self::CHANNELS`] overflows `usize`.
    #[must_use]
    pub fn null(ring_capacity_frames: usize) -> Self {
        let (producer, consumer) = frame_ring(ring_capacity_frames, Self::CHANNELS)
            .expect("null ring capacity in frames overflows the interleaved sample count");
        Self {
            backend: Backend::Null(Source::Direct(consumer)),
            producer,
            sample_rate: Self::M4A_MIXER_RATE,
            channels: Self::CHANNELS,
            shared: Arc::new(Shared::new(TransportState::Null, Self::M4A_MIXER_RATE, 0)),
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
    /// Also [`PlatformError::AudioRingCapacityOverflow`] if
    /// `ring_capacity_frames` times the channel count overflows `usize`.
    #[doc(hidden)]
    pub fn null_resampled(
        ring_capacity_frames: usize,
        source_rate: f64,
        device_rate: u32,
        max_output_frames: usize,
    ) -> Result<Self, PlatformError> {
        let channels = Self::CHANNELS;
        let (producer, consumer) = frame_ring(ring_capacity_frames, channels)?;
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
            channels,
            shared: Arc::new(Shared::new(
                TransportState::Null,
                device_rate,
                settle_margin_frames,
            )),
        })
    }

    /// Start (or resume) playback.
    ///
    /// Records the intent and returns at once: the transport worker plays the
    /// stream when one exists, now or after it next attaches. Idempotent.
    ///
    /// # Errors
    ///
    /// Never fails today; the `Result` is kept for callers written against
    /// the earlier synchronous contract. A stream that refuses to play is
    /// reported as a [`TransportState::Unavailable`] transition instead.
    pub fn start(&mut self) -> Result<(), PlatformError> {
        self.set_running(true);
        Ok(())
    }

    /// Pause playback; the transport stays up and can be
    /// [`AudioOutput::start`]ed again. Idempotent.
    ///
    /// # Errors
    ///
    /// Never fails; see [`Self::start`].
    pub fn stop(&mut self) -> Result<(), PlatformError> {
        self.set_running(false);
        Ok(())
    }

    fn set_running(&self, running: bool) {
        self.shared.set_running(running, &self.producer);
        if let Backend::Device(worker) = &self.backend {
            worker.wake();
        }
    }

    /// Whether [`AudioOutput::start`] has been called more recently than
    /// [`AudioOutput::stop`].
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    /// The transport's current state: [`TransportState::Null`] for the
    /// headless backend, otherwise whether a stream is attached.
    #[must_use]
    pub fn transport_state(&self) -> TransportState {
        self.shared.state()
    }

    /// The next not-yet-reported transport transition, oldest first, or
    /// `None` when there is nothing new. Each distinct state is reported
    /// once however often the worker retries, and polling does no device
    /// work, so it is safe in the game loop. Always `None` for the null
    /// backend.
    #[must_use]
    pub fn poll_transport_state(&mut self) -> Option<TransportState> {
        match &self.backend {
            Backend::Device(worker) => worker.poll(),
            Backend::Null(_) => None,
        }
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
        self.shared.device_sample_rate.load(Ordering::Acquire)
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
        match self.shared.max_callback_frames.load(Ordering::Acquire) {
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

    /// Count of asynchronous `cpal` stream errors (device disconnect, driver
    /// failure, xrun, …) reported during this output's lifetime, including
    /// by streams the transport has since replaced.
    ///
    /// A lifetime diagnostic, not the current transport state: compare it
    /// with a value read when an operation began to learn whether a stream
    /// failed during that operation, which [`Self::is_running`] and a flat
    /// [`Self::underruns`] cannot show, and use [`Self::transport_state`] to
    /// learn whether a stream is attached now. Always zero for the null
    /// backend, which owns no `cpal` stream.
    #[must_use]
    pub fn stream_errors(&self) -> u64 {
        self.shared.stream_errors.load(Ordering::Relaxed)
    }

    /// Records one asynchronous stream error as the `cpal` error closure in
    /// `build_stream` does, so a null-backend test can stand in for a device.
    #[doc(hidden)]
    pub fn record_stream_error_for_test(&self) {
        self.shared.stream_errors.fetch_add(1, Ordering::Relaxed);
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
        self.shared.playback_clock.progress()
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
        self.shared.settle_margin_frames.load(Ordering::Acquire)
    }

    /// Marks this instance's playback-position signal available, as a real
    /// device's first callback with a usable timestamp would, so a
    /// null-backed test can drive [`Self::playback_progress`] without a
    /// `cpal` device. See [`Self::advance_sounded_frames_for_test`].
    #[doc(hidden)]
    pub fn enable_playback_progress_for_test(&self) {
        self.shared
            .playback_clock
            .enable_playback_progress_for_test();
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
        self.shared
            .playback_clock
            .advance_sounded_frames_for_test(frames);
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
            self.shared.playback_clock.record_submitted(frames);
            source.fill(out);
        }
    }
}
