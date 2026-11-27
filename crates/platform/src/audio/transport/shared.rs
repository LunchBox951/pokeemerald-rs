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

/// A start count and the intent that went with it; see [`Shared::intent`].
#[derive(Clone, Copy)]
pub(super) struct Intent {
    /// [`Shared::starts`].
    pub(super) starts: u64,
    /// [`Shared::running`].
    pub(super) running: bool,
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
    /// Brackets the publication of a start: odd while [`Self::starts`] and
    /// [`Self::running`] are being updated for it, so [`Self::intent`] never
    /// pairs one with the other's stale value.
    start_seq: AtomicU64,
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
            start_seq: AtomicU64::new(0),
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
    ///
    /// Only the facade calls this, and it takes `&mut self` to start or stop,
    /// so publications are serial. See [`Self::intent`] for what a start's
    /// publication guarantees a reader.
    pub(in crate::audio) fn set_running(&self, running: bool, producer: &Producer) {
        let was_running = self.running.load(Ordering::Acquire);
        if running && !was_running {
            self.begin_start();
            self.finish_start();
            return;
        }
        if !running && was_running {
            let mark = producer.pushed();
            self.stopped_at.store(mark, Ordering::Relaxed);
            // A started stream that has not read yet still holds its
            // first-read discard; what is queued after this stop is a
            // stopped queue that discard must keep.
            producer.keep_pushed_from_at_first_read(mark);
        }
        // Stored after the mark, and read with acquire by the worker, so a
        // worker that reads this stop also reads its mark.
        self.running.store(running, Ordering::SeqCst);
    }

    /// The first half of publishing a start of a stopped transport: open the
    /// bracket and count the start. [`Self::finish_start`] must follow;
    /// split only so tests can pause between the two.
    pub(super) fn begin_start(&self) {
        self.start_seq.fetch_add(1, Ordering::SeqCst);
        self.starts.fetch_add(1, Ordering::SeqCst);
    }

    /// Complete the start [`Self::begin_start`] opened, making it visible to
    /// [`Self::intent`].
    pub(super) fn finish_start(&self) {
        self.running.store(true, Ordering::SeqCst);
        self.start_seq.fetch_add(1, Ordering::SeqCst);
    }

    /// A start count and the playback intent read together, or `None` while a
    /// start is being published.
    ///
    /// A start is published as `starts += 1` then `running = true`, so a
    /// reader that loaded the two separately could see the new count with the
    /// stopped intent (acknowledging a start that has not finished) or the
    /// started intent with the old count (applying a start it then counts as
    /// missed). This is a seqlock on [`Self::start_seq`], which the writer
    /// holds odd across both stores. Every operation on `start_seq`,
    /// `starts` and `running` on this path is `SeqCst`, so they have one
    /// total order consistent with the writer's program order. The reader
    /// accepts only when `start_seq` is the same even value before and after
    /// its two loads: no bracket opened or closed between them, so in that
    /// order both stores of any start lie wholly before the loads or wholly
    /// after them, and the pair is a state that existed. A stop after the
    /// start stores `stopped_at` before `running = false`, so a reader that
    /// sees the stop sees its mark. Readers never wait for an unfinished
    /// start: they get `None`, acknowledge nothing, and ask again.
    pub(super) fn intent(&self) -> Option<Intent> {
        let before = self.start_seq.load(Ordering::SeqCst);
        if before % 2 == 1 {
            return None;
        }
        let starts = self.starts.load(Ordering::SeqCst);
        let running = self.running.load(Ordering::SeqCst);
        (self.start_seq.load(Ordering::SeqCst) == before).then_some(Intent { starts, running })
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
