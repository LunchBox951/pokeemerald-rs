//! The recoverable output transport: discovers the OS default output,
//! connects a stream to it, and reconnects when the stream fails or the
//! default output changes.
//!
//! [`Transport`] is a plain state machine over an [`OutputHost`] and a
//! caller-supplied clock, so every decision is testable with a fake host and
//! no sleeping. In production a worker thread (see `worker`) is its only
//! driver: discovery, negotiation, stream building, play/pause and the retry
//! schedule all run there. The device callbacks only touch atomics and the
//! ring buffer, and the game loop only flips atomics and reads published
//! state, so neither ever waits on the OS audio stack.
//!
//! ## Recovery
//!
//! - A stream is replaced when its error callback reports a failure that
//!   needs a rebuild (see `stream::error_requires_rebuild`), when the default
//!   output's identity differs from the one the stream was built for, or when
//!   the default output disappears. A stream whose backend has reported
//!   `DeviceChanged` follows the default by itself, so a changed identity
//!   is recorded for it instead of rebuilt; a change discovered before that
//!   report arrives still rebuilds. The default is re-discovered
//!   [`DISCOVERY_INTERVAL`] after the previous query returned, because `cpal`
//!   offers no default-change subscription, attached or not: a retry
//!   cooldown delays the next attempt on the identity that failed, never the
//!   discovery of a different one. A failed identity query is not an absent
//!   default, so it never retires an attached stream.
//! - Failed attempts retry [`ATTEMPTS_PER_BURST`] times with doubling delays
//!   from [`RETRY_BASE`], then wait [`COOLDOWN`] before the next burst,
//!   each measured from when the failed attempt finished, so a
//!   missing or flapping output is polled at a bounded rate and never given
//!   up on. A newly observed default identity starts a fresh burst; a stream
//!   that fails again before surviving [`STABLE_AFTER`] continues the old one.
//! - When a stream re-attaches after an earlier one and playback was meant to
//!   advance while no stream drained the ring (a stream was lost while
//!   started, or `start` held while none was attached), the queued samples
//!   are discarded, so playback resumes at the producer's current position
//!   instead of replaying history. A stream that starts as it attaches also
//!   drops, at its first callback, what the producer queued while it was
//!   being built. A queue left while stopped is kept for the
//!   next `start`. The first attach keeps the queue: that is the producer's
//!   startup prefill, whether or not it has already called `start`.
//! - Each distinct [`TransportState`] is published once, in order, for the
//!   application to report. At most [`MAX_PENDING_EVENTS`] unpolled
//!   transitions are kept; a caller that never polls loses the oldest, and
//!   [`Shared::state`] always holds the latest.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::error::PlatformError;
use crate::ring::{Consumer, Producer};

use super::config::{max_buffer_frames, negotiate};
use super::playback::PlaybackClock;
use super::source_for_device;
use super::stream::{build_stream, OutputDevice, OutputStream, StreamHealth};

mod worker;
pub(super) use worker::{CpalHost, Worker};

#[cfg(test)]
mod tests;

/// Most transitions kept for a caller that does not poll; older ones drop.
pub(super) const MAX_PENDING_EVENTS: usize = 8;
/// How often a healthy stream's failure flag and play intent are checked.
pub(super) const HEALTH_POLL: Duration = Duration::from_millis(20);
/// How often the OS default output is re-discovered, attached or not.
pub(super) const DISCOVERY_INTERVAL: Duration = Duration::from_secs(1);
/// Attempts made per burst before the cooldown.
pub(super) const ATTEMPTS_PER_BURST: u32 = 4;
/// Delay after the first failed attempt of a burst; doubles per failure.
pub(super) const RETRY_BASE: Duration = Duration::from_millis(100);
/// Wait between bursts.
pub(super) const COOLDOWN: Duration = Duration::from_secs(5);
/// How long a stream must stay healthy before a failure counts as new.
pub(super) const STABLE_AFTER: Duration = Duration::from_secs(5);

/// Where the output transport currently stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportState {
    /// No stream has been attempted yet.
    Connecting,
    /// A stream is attached to the default output.
    Ready,
    /// No stream is attached; the transport keeps retrying. The producer
    /// contract is unchanged meanwhile: pushes are accepted until the ring is
    /// full and then dropped.
    Unavailable(TransportFailure),
    /// The headless backend, which has no transport to fail.
    Null,
}

/// Why no stream is attached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportFailure {
    /// There is no default output device, or the audio host is unreachable.
    NoOutput,
    /// The default output changed; the stream is being rebuilt for the new
    /// one.
    DefaultChanged,
    /// The attached stream reported a failure that needs a rebuild.
    StreamLost,
    /// The default output has no configuration this crate can drive.
    Unsupported,
    /// The device was reachable but a stream could not be built or started.
    ConnectFailed,
}

/// State shared by the game-loop facade, the worker, and the callbacks.
/// Everything here is an atomic except `state`, which only the worker and the
/// facade lock, never a callback.
pub(super) struct Shared {
    /// Playback intent set by `start`/`stop`.
    pub(super) running: AtomicBool,
    pub(super) shutdown: AtomicBool,
    pub(super) stream_errors: Arc<AtomicU64>,
    pub(super) playback_clock: Arc<PlaybackClock>,
    pub(super) device_sample_rate: AtomicU32,
    /// `0` when no concrete range is advertised.
    pub(super) max_callback_frames: AtomicUsize,
    pub(super) settle_margin_frames: AtomicU64,
    state: Mutex<TransportState>,
    /// Transitions not yet polled; bounded by [`MAX_PENDING_EVENTS`].
    events: Mutex<VecDeque<TransportState>>,
}

impl Shared {
    pub(super) fn new(
        state: TransportState,
        device_sample_rate: u32,
        settle_margin_frames: u64,
    ) -> Self {
        Self {
            running: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            stream_errors: Arc::new(AtomicU64::new(0)),
            playback_clock: Arc::new(PlaybackClock::new()),
            device_sample_rate: AtomicU32::new(device_sample_rate),
            max_callback_frames: AtomicUsize::new(0),
            settle_margin_frames: AtomicU64::new(settle_margin_frames),
            state: Mutex::new(state),
            events: Mutex::new(VecDeque::new()),
        }
    }

    /// Queue a transition for the application, discarding the oldest pending
    /// one when an unpolled queue is full. The latest state always survives.
    fn push_event(&self, state: TransportState) {
        let mut events = self.events.lock().unwrap_or_else(PoisonError::into_inner);
        if events.len() >= MAX_PENDING_EVENTS {
            events.pop_front();
        }
        events.push_back(state);
    }

    pub(super) fn pop_event(&self) -> Option<TransportState> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }

    pub(super) fn state(&self) -> TransportState {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Discovery of the current OS default output.
pub(super) trait OutputHost: Send + 'static {
    type Device: OutputDevice;

    /// The current default output as a stable identity plus the device, or
    /// `None` when there is none. Called only from the transport's driver.
    ///
    /// # Errors
    ///
    /// A `cpal` failure to reach the host or the device.
    fn default_output(&mut self) -> Result<Option<(String, Self::Device)>, cpal::Error>;
}

struct Live<S> {
    stream: S,
    id: String,
    /// Raised by this stream's error callback; a replacement gets its own.
    health: Arc<StreamHealth>,
    playing: bool,
    since: Duration,
}

/// The transport state machine. See the module docs.
pub(super) struct Transport<H: OutputHost> {
    host: H,
    producer: Producer,
    shared: Arc<Shared>,
    live: Option<Live<<H::Device as OutputDevice>::Stream>>,
    state: TransportState,
    failures: u32,
    /// Whether any stream has attached yet; the first attach keeps the
    /// producer's startup prefill.
    attached_before: bool,
    /// Whether playback was meant to advance while no stream drained the
    /// ring, so the queue holds history the next attach must discard.
    stale_queue: bool,
    /// The default identity the last discovery saw.
    observed: Observed,
    next_attempt: Duration,
    next_discovery: Duration,
    /// Reads the driver's monotonic time after blocking OS calls, so retry
    /// delays and stability run from when an attempt finished, not from the
    /// `now` it started at. Never earlier than that `now`.
    clock: Box<dyn Fn() -> Duration + Send>,
}

impl<H: OutputHost> Transport<H> {
    /// A transport whose first attempt is due at time zero. `producer`'s
    /// consumer must already be dropped so the transport can claim it.
    pub(super) fn new(host: H, producer: Producer, shared: Arc<Shared>) -> Self {
        Self {
            host,
            producer,
            shared,
            live: None,
            state: TransportState::Connecting,
            failures: 0,
            attached_before: false,
            stale_queue: false,
            observed: Observed::Nothing,
            next_attempt: Duration::ZERO,
            next_discovery: Duration::ZERO,
            clock: Box::new(|| Duration::ZERO),
        }
    }

    /// Read `clock` after blocking work; it must share `step`'s origin.
    pub(super) fn with_clock(mut self, clock: impl Fn() -> Duration + Send + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }

    /// The time a blocking call that began at `now` finished.
    fn finished(&self, now: Duration) -> Duration {
        now.max((self.clock)())
    }

    /// Advance to time `now` (a monotonic offset from any fixed origin) and
    /// return when the driver should call again.
    pub(super) fn step(&mut self, now: Duration) -> Duration {
        if self.live.is_some() {
            self.supervise(now);
        }
        if self.live.is_none() {
            if self.shared.running.load(Ordering::Acquire) {
                self.stale_queue = true;
            }
            if now >= self.next_attempt {
                self.attempt(now);
            } else if now >= self.next_discovery {
                self.probe(now);
            }
        }
        if self.live.is_some() {
            now + HEALTH_POLL
        } else {
            self.next_attempt.min(self.next_discovery)
        }
    }

    /// Re-discover the default while waiting out a retry delay, and attempt
    /// at once on a fresh burst when its identity changed.
    fn probe(&mut self, now: Duration) {
        let found = self.host.default_output();
        // Scheduled from when the query returned, so a slow one cannot run
        // back to back.
        self.next_discovery = self.finished(now) + DISCOVERY_INTERVAL;
        // A failed query says nothing new; the scheduled retry stands.
        let Ok(found) = found else {
            return;
        };
        let id = found.as_ref().map(|(id, _)| id.as_str());
        if !self.observed.is(id) {
            self.attempt_with(now, Ok(found));
        }
    }

    fn supervise(&mut self, now: Duration) {
        let Some(live) = self.live.as_ref() else {
            return;
        };
        if live.health.lost.load(Ordering::Acquire) {
            // The stream died while it was playing, so playback was meant to
            // advance even if `stop` landed before this poll. A refused
            // pause below is a deliberate stop and stays exempt.
            if live.playing {
                self.stale_queue = true;
            }
            self.retire(TransportFailure::StreamLost);
            self.fail(now);
            return;
        }
        if now.saturating_sub(live.since) >= STABLE_AFTER {
            self.failures = 0;
        }
        if now >= self.next_discovery {
            let found = self.host.default_output();
            self.next_discovery = self.finished(now) + DISCOVERY_INTERVAL;
            let Some(live) = self.live.as_ref() else {
                return;
            };
            // Read after the query, so a reroute reported while it ran counts.
            let follows_default = live.health.follows_default.load(Ordering::Acquire);
            let current = live.id.as_str();
            let verdict = match found {
                // The backend already rerouted this stream to the new
                // default; record its identity instead of rebuilding.
                Ok(Some((id, _))) if id != current && follows_default => {
                    self.observed = Observed::Default(Some(id.clone()));
                    if let Some(live) = self.live.as_mut() {
                        live.id = id;
                    }
                    None
                }
                Ok(Some((id, _))) if id != current => Some(TransportFailure::DefaultChanged),
                Ok(None) => Some(TransportFailure::NoOutput),
                // A failed query says nothing about the healthy stream.
                Ok(Some(_)) | Err(_) => None,
            };
            if let Some(reason) = verdict {
                // A different (or missing) output deserves a fresh burst.
                self.retire(reason);
                self.failures = 0;
                self.next_attempt = now;
                return;
            }
        }
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let want = self.shared.running.load(Ordering::Acquire);
        if want != live.playing {
            let result = if want {
                live.stream.play()
            } else {
                live.stream.pause()
            };
            // A loss raised after the check above, while this stream was
            // still playing, happened while started; latch it before a
            // concurrent `stop` clears `playing` or retires the stream.
            if live.playing && live.health.lost.load(Ordering::Acquire) {
                self.stale_queue = true;
            }
            if result.is_ok() {
                live.playing = want;
            } else {
                self.retire(TransportFailure::StreamLost);
                self.fail(self.finished(now));
            }
        }
    }

    fn attempt(&mut self, now: Duration) {
        let found = self.host.default_output();
        self.attempt_with(now, found);
    }

    /// Connect to `found`, the default a discovery just returned. A default
    /// whose identity differs from the last one observed starts a fresh
    /// burst.
    fn attempt_with(&mut self, now: Duration, found: Discovered<H>) {
        self.next_discovery = now + DISCOVERY_INTERVAL;
        if let Ok(found) = &found {
            let id = found.as_ref().map(|(id, _)| id.as_str());
            if matches!(self.observed, Observed::Default(_)) && !self.observed.is(id) {
                self.failures = 0;
            }
            self.observed = Observed::Default(id.map(str::to_owned));
        }
        match self.connect(now, found) {
            Ok(()) => {}
            Err(reason) => {
                let done = self.finished(now);
                self.next_discovery = done + DISCOVERY_INTERVAL;
                self.publish(TransportState::Unavailable(reason));
                self.fail(done);
            }
        }
    }

    fn connect(&mut self, now: Duration, found: Discovered<H>) -> Result<(), TransportFailure> {
        let (id, device) = match found {
            Ok(Some(found)) => found,
            Ok(None) => return Err(TransportFailure::NoOutput),
            Err(err) => return Err(failure_of(&PlatformError::Audio(err))),
        };
        let config = negotiate(&device).map_err(|err| failure_of(&err))?;
        let mut consumer = self
            .producer
            .reclaim_consumer()
            .ok_or(TransportFailure::ConnectFailed)?;
        // One read of intent decides both the discard and whether the new
        // stream starts: a `start` that landed while this attempt blocked
        // was held with nothing attached, and a later one finds an attached
        // paused stream. The latch is taken, so a stopped retry after a
        // failed build keeps what the producer queues next.
        let playing = self.shared.running.load(Ordering::Acquire);
        if (std::mem::take(&mut self.stale_queue) || playing) && self.attached_before {
            consumer.discard_queued();
        }
        // A started producer keeps pushing while the build below blocks,
        // and the consumer is the new stream's from then on, so what queues
        // meanwhile is dropped at its first callback. A stopped attach keeps
        // it, as it keeps any queue left while stopped.
        if playing && self.attached_before {
            consumer.discard_queued_at_first_read();
        }
        // A started attempt that fails leaves what was queued meanwhile as
        // history, even if `stop` lands before the next step.
        self.stale_queue = playing;
        self.attach(now, id, &device, &config, consumer, playing)?;
        self.stale_queue = false;
        Ok(())
    }

    fn attach(
        &mut self,
        now: Duration,
        id: String,
        device: &H::Device,
        config: &cpal::SupportedStreamConfig,
        consumer: Consumer,
        playing: bool,
    ) -> Result<(), TransportFailure> {
        let max_frames = max_buffer_frames(config);
        let source = source_for_device(
            consumer,
            config.channels(),
            config.sample_rate(),
            max_frames,
        )
        .map_err(|err| failure_of(&err))?;
        let settle_margin = source.settle_margin_frames();
        let health = Arc::new(StreamHealth::default());
        let stream = build_stream(
            device,
            config,
            source,
            Arc::clone(&self.shared.stream_errors),
            Arc::clone(&health),
            Arc::clone(&self.shared.playback_clock),
        )
        .map_err(|err| match err {
            // The device was found and negotiated, so a build that reports it
            // missing is a failed connection, not an absent output.
            PlatformError::Audio(_) => TransportFailure::ConnectFailed,
            other => failure_of(&other),
        })?;

        // A built stream is paused, so only a started transport plays it.
        if playing {
            stream.play().map_err(|_| TransportFailure::ConnectFailed)?;
        }
        let done = self.finished(now);
        self.shared
            .device_sample_rate
            .store(config.sample_rate(), Ordering::Release);
        self.shared
            .max_callback_frames
            .store(max_frames, Ordering::Release);
        self.shared
            .settle_margin_frames
            .store(settle_margin, Ordering::Release);
        self.live = Some(Live {
            stream,
            id,
            health,
            playing,
            since: done,
        });
        self.attached_before = true;
        self.next_discovery = done + DISCOVERY_INTERVAL;
        self.publish(TransportState::Ready);
        Ok(())
    }

    /// Drop the attached stream on this (the driver's) thread and publish why.
    fn retire(&mut self, reason: TransportFailure) {
        self.live = None;
        if self.shared.running.load(Ordering::Acquire) {
            self.stale_queue = true;
        }
        self.publish(TransportState::Unavailable(reason));
    }

    /// Schedule the next attempt after a failure: doubling delays, then a
    /// cooldown that closes the burst.
    fn fail(&mut self, now: Duration) {
        self.failures += 1;
        let delay = if self.failures < ATTEMPTS_PER_BURST {
            RETRY_BASE * (1 << (self.failures - 1))
        } else {
            self.failures = 0;
            COOLDOWN
        };
        self.next_attempt = now + delay;
    }

    fn publish(&mut self, state: TransportState) {
        if state == self.state {
            return;
        }
        self.state = state;
        *self
            .shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = state;
        self.shared.push_event(state);
    }
}

/// The default identity a transport last discovered.
enum Observed {
    /// No discovery has returned yet.
    Nothing,
    /// The last discovery's identity, `None` when there was no output.
    Default(Option<String>),
}

impl Observed {
    fn is(&self, id: Option<&str>) -> bool {
        matches!(self, Self::Default(seen) if seen.as_deref() == id)
    }
}

/// What one [`OutputHost::default_output`] call returned.
type Discovered<H> = Result<Option<(String, <H as OutputHost>::Device)>, cpal::Error>;

fn failure_of(err: &PlatformError) -> TransportFailure {
    match err {
        PlatformError::NoAudioDevice => TransportFailure::NoOutput,
        PlatformError::Audio(err)
            if matches!(
                err.kind(),
                cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::HostUnavailable
            ) =>
        {
            TransportFailure::NoOutput
        }
        PlatformError::UnsupportedAudioConfig
        | PlatformError::UnsupportedResampleRatio { .. }
        | PlatformError::AudioRingCapacityOverflow { .. } => TransportFailure::Unsupported,
        _ => TransportFailure::ConnectFailed,
    }
}
