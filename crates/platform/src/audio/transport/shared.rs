//! The state shared between the game-loop facade, the worker, and the device
//! callbacks, and the transport states it publishes.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::ring::Producer;

use super::PlaybackClock;

/// Most transitions kept for a caller that does not poll; older ones drop.
pub(super) const MAX_PENDING_EVENTS: usize = 8;

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
pub(in crate::audio) struct Shared {
    /// Playback intent set by `start`/`stop`.
    pub(in crate::audio) running: AtomicBool,
    /// [`Producer::pushed`] at the latest `stop` of a started transport:
    /// what was queued after it is a stopped queue.
    pub(super) stopped_at: AtomicU64,
    /// How many times playback has been started, so the worker sees a
    /// `start` even when a `stop` overwrote it before the next step.
    pub(super) starts: AtomicU64,
    pub(in crate::audio) shutdown: AtomicBool,
    pub(in crate::audio) stream_errors: Arc<AtomicU64>,
    pub(in crate::audio) playback_clock: Arc<PlaybackClock>,
    pub(in crate::audio) device_sample_rate: AtomicU32,
    /// `0` when no concrete range is advertised.
    pub(in crate::audio) max_callback_frames: AtomicUsize,
    pub(in crate::audio) settle_margin_frames: AtomicU64,
    state: Mutex<TransportState>,
    /// Transitions not yet polled; bounded by [`MAX_PENDING_EVENTS`].
    events: Mutex<VecDeque<TransportState>>,
}

impl Shared {
    pub(in crate::audio) fn new(
        state: TransportState,
        device_sample_rate: u32,
        settle_margin_frames: u64,
    ) -> Self {
        Self {
            running: AtomicBool::new(false),
            stopped_at: AtomicU64::new(0),
            starts: AtomicU64::new(0),
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

    /// Record playback intent. A stop of a started transport first marks
    /// where it landed in `producer`'s queue, so a re-attach that discards
    /// history keeps what is queued after it. A start of a stopped transport
    /// is counted, so one the worker never saw running is still history.
    pub(in crate::audio) fn set_running(&self, running: bool, producer: &Producer) {
        let was_running = self.running.load(Ordering::Acquire);
        if running && !was_running {
            self.starts.fetch_add(1, Ordering::Relaxed);
        }
        if !running && was_running {
            let mark = producer.pushed();
            self.stopped_at.store(mark, Ordering::Relaxed);
            // A started stream that has not read yet still holds its
            // first-read discard; what is queued after this stop is a
            // stopped queue that discard must keep.
            producer.keep_pushed_from_at_first_read(mark);
        }
        // Release: a worker that reads this stop also reads its mark.
        self.running.store(running, Ordering::Release);
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

    /// Record `state` as the latest and queue the transition for the
    /// application.
    pub(super) fn publish(&self, state: TransportState) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = state;
        self.push_event(state);
    }

    pub(in crate::audio) fn pop_event(&self) -> Option<TransportState> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }

    pub(in crate::audio) fn state(&self) -> TransportState {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
