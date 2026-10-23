//! A bounded, underrun-safe sample ring buffer: the seam between a producer
//! of rendered PCM — in practice the integration crate's frame-driven music
//! player, pushing the `audio` crate's M4A output — and
//! [`crate::audio::AudioOutput`], which drains it once per audio device
//! callback.
//!
//! [`ring_buffer`] splits into a cloneable [`Producer`] (write side) and a
//! single [`Consumer`] (read side) sharing one fixed-capacity array of
//! atomic slots. The consumer side — the real-time device callback — never
//! takes a lock at all: it only loads and stores plain atomics, so a
//! producer stalled or descheduled mid-push can never block it.
//!
//! [`Consumer::fill`] is the hot path every consumer of a ring buffer is
//! built on — the real device callback, [`crate::resample::Resampler`], and
//! the null backend used in tests all drive it, in one bulk call per
//! callback (or, for an oversized callback the resampler bounds into several
//! fixed-size chunks, one call per chunk — see `crate::resample`). It
//! bulk-drains whatever is published into the requested slice without ever
//! blocking, and pads any shortfall with silence, adding that shortfall to
//! the underrun counter in a single consolidated update.
//! [`Consumer::pop_or_silence`] is the same "fill silence and count one
//! underrun when the buffer runs dry" rule for a single sample, retained for
//! callers that genuinely want one sample at a time.
//!
//! ## Invariants
//!
//! The only state both sides share is `Shared::occupied`, an `AtomicUsize`
//! in `[0, capacity]`: the producer publishes with `fetch_add(Release)`
//! after its slot stores, the consumer frees with `fetch_sub(Release)` after
//! its slot loads, and each side reads it with `Acquire`, which orders every
//! slot access against the other side's. The consumer never takes
//! `Shared::head`'s mutex, which only serialises cloned producers, so a
//! producer stalled mid-push cannot block the callback. Each index is
//! reduced `% capacity` on every advance rather than left as an unbounded
//! counter: a capacity that does not divide `usize::MAX + 1` would alias
//! two sequence positions onto one slot the instant such a counter wrapped.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Shared state behind a [`Producer`]/[`Consumer`] pair.
struct Shared {
    /// Fixed-capacity backing storage; slot `i % capacity` holds the bit
    /// pattern of the sample at sequence position `i`.
    buffer: Box<[AtomicU32]>,
    capacity: usize,
    /// The producer-side write index (`< capacity`), guarded by the mutex
    /// that also serializes concurrent [`Producer`] clones. Never locked by
    /// [`Consumer`] — see the module docs' "Invariants" section.
    head: Mutex<usize>,
    /// Samples published and not yet consumed; the only state the producer
    /// and consumer sides share. Always in `[0, capacity]`.
    occupied: AtomicUsize,
    underruns: AtomicU64,
    /// Samples ever accepted by [`Producer::push`], wrapping. Written only
    /// while `head` is held, so it and the write index advance together.
    pushed: AtomicU64,
    /// Whether a [`Consumer`] currently exists. Lets a replacement consumer be
    /// claimed after the previous one is dropped with its device stream.
    consumer_alive: AtomicBool,
}

/// Lock `head`, recovering from poisoning rather than propagating a panic.
///
/// A panic on the producer side (e.g. a bug in the mixer feeding it) should
/// not also take down the audio device callback thread. The callback never
/// takes this lock at all, so poisoning it can only ever affect other
/// producer clones — the worst it should cost is a torn-looking push, not a
/// crash.
fn lock_head(head: &Mutex<usize>) -> std::sync::MutexGuard<'_, usize> {
    head.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Create a bounded sample ring buffer, split into a producer half and a
/// consumer half.
///
/// `capacity` is in samples, not frames: for interleaved stereo, a
/// `capacity` of `2 * frames` holds `frames` frames of audio.
#[must_use]
pub fn ring_buffer(capacity: usize) -> (Producer, Consumer) {
    let buffer: Box<[AtomicU32]> = (0..capacity).map(|_| AtomicU32::new(0)).collect();
    let shared = Arc::new(Shared {
        buffer,
        capacity,
        head: Mutex::new(0),
        occupied: AtomicUsize::new(0),
        underruns: AtomicU64::new(0),
        pushed: AtomicU64::new(0),
        consumer_alive: AtomicBool::new(true),
    });
    (
        Producer {
            shared: Arc::clone(&shared),
        },
        Consumer {
            shared,
            tail: 0,
            discard_pending: false,
        },
    )
}

/// The write half of a sample ring buffer.
///
/// Cheap to clone (all clones share the same underlying buffer) and
/// `Send + Sync`, which is the point: a producer may push rendered PCM here
/// from any thread while [`Consumer`] drains it from the audio device
/// callback thread.
#[derive(Clone)]
pub struct Producer {
    shared: Arc<Shared>,
}

impl Producer {
    /// Push as many of `samples` as fit; returns the number actually
    /// written.
    ///
    /// Excess samples (buffer full) are dropped, not queued or blocked on —
    /// a slow producer must never stall the audio device thread, and a full
    /// buffer already means playback has fallen behind generation.
    #[must_use = "a return value less than `samples.len()` means some samples were dropped"]
    pub fn push(&self, samples: &[f32]) -> usize {
        let mut head = lock_head(&self.shared.head);
        // Acquire: synchronizes-with the consumer's `Release` `fetch_sub`,
        // so the slots it already read are visible as free before this call
        // overwrites them. May already be stale-high (the consumer can free
        // more concurrently) — that only makes `space` an underestimate,
        // never wrong in a way that corrupts the buffer.
        let occupied = self.shared.occupied.load(Ordering::Acquire);
        let space = self.shared.capacity - occupied;
        let n = samples.len().min(space);
        for (i, &sample) in samples[..n].iter().enumerate() {
            let idx = (*head + i) % self.shared.capacity;
            self.shared.buffer[idx].store(sample.to_bits(), Ordering::Relaxed);
        }
        if n > 0 {
            *head = (*head + n) % self.shared.capacity;
            // Release: publishes the slot stores above to the consumer's
            // `Acquire` load of `occupied`.
            self.shared.occupied.fetch_add(n, Ordering::Release);
            // Relaxed: only ever read or written while `head` is held.
            let pushed = self.shared.pushed.load(Ordering::Relaxed);
            self.shared.pushed.store(
                pushed.wrapping_add(u64::try_from(n).unwrap_or(u64::MAX)),
                Ordering::Relaxed,
            );
        }
        n
    }

    /// Samples ever accepted by [`Self::push`], wrapping on overflow.
    ///
    /// A mark taken here splits the queue: [`Consumer::discard_pushed_before`]
    /// keeps only what was pushed after it. Used by the audio transport to
    /// record where a `stop` landed.
    pub(crate) fn pushed(&self) -> u64 {
        let _head = lock_head(&self.shared.head);
        self.shared.pushed.load(Ordering::Relaxed)
    }

    /// Total samples this ring holds, fixed when it was created.
    ///
    /// Distinct from [`Self::available_space`], which is this minus whatever
    /// is queued at the moment of the call. A caller asking "is the ring
    /// empty?" wants to compare against this; a snapshot of free space taken
    /// while anything was queued understates it and would read a still-queued
    /// ring as empty.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.shared.capacity
    }

    /// Samples free right now.
    #[must_use]
    pub fn available_space(&self) -> usize {
        self.shared
            .capacity
            .saturating_sub(self.shared.occupied.load(Ordering::Acquire))
    }

    /// Total samples emitted as silence so far because a consumer wanted
    /// more than was queued (see [`Consumer::pop_or_silence`]).
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }

    /// Claim a replacement [`Consumer`] once the previous one has been
    /// dropped, keeping the queued samples and the underrun counter.
    ///
    /// Returns `None` while a consumer still exists: the one-consumer
    /// invariant is never relaxed. The replacement's read index is rebuilt
    /// from the write index and the queued count, so it resumes exactly
    /// where the dropped consumer stopped. Used by the audio transport to
    /// re-attach a rebuilt device stream to the same queue.
    pub(crate) fn reclaim_consumer(&self) -> Option<Consumer> {
        // Holding `head` excludes pushes, so the write index and queued
        // count describe one consistent state while the tail is derived.
        let head = lock_head(&self.shared.head);
        self.shared
            .consumer_alive
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        let capacity = self.shared.capacity;
        let occupied = self.shared.occupied.load(Ordering::Acquire);
        let tail = if capacity == 0 {
            0
        } else {
            (*head + capacity - occupied) % capacity
        };
        Some(Consumer {
            shared: Arc::clone(&self.shared),
            tail,
            discard_pending: false,
        })
    }
}

/// The read half of a sample ring buffer, driven once per audio device
/// callback (or, for the null backend, once per manual test/harness call).
///
/// Not `Clone`, and its draining methods take `&mut self`: there is exactly
/// one `Consumer`, and the borrow checker — not a runtime lock, which is the
/// whole point of this type — is what rules out two threads racing on the
/// same drain.
pub struct Consumer {
    shared: Arc<Shared>,
    /// This consumer's own read index (`< capacity`). Touched by no one
    /// else, ever — see the module docs' "Invariants" section.
    tail: usize,
    /// Set by [`Consumer::discard_queued_at_first_read`]: the next read
    /// drops whatever is queued at that moment before reading.
    discard_pending: bool,
}

impl Consumer {
    /// Pop the next queued sample, or `None` if the buffer is empty.
    ///
    /// A pure query: does not touch the underrun counter. Most callers want
    /// [`Consumer::pop_or_silence`] or [`Consumer::fill`] instead.
    pub fn try_pop(&mut self) -> Option<f32> {
        self.settle_pending_discard();
        // Acquire: synchronizes-with the producer's `Release` `fetch_add`,
        // so a published slot's value is visible before this reads it.
        if self.shared.occupied.load(Ordering::Acquire) == 0 {
            return None;
        }
        let idx = self.tail % self.shared.capacity;
        let sample = f32::from_bits(self.shared.buffer[idx].load(Ordering::Relaxed));
        self.tail = (self.tail + 1) % self.shared.capacity;
        // Release: publishes the freed slot to the producer's `Acquire`
        // load of `occupied`.
        self.shared.occupied.fetch_sub(1, Ordering::Release);
        Some(sample)
    }

    /// Pop the next queued sample; if none is queued, count one underrun
    /// sample and return silence (`0.0`).
    ///
    /// The single-sample form of the underrun-safe rule; the callback hot
    /// path uses the bulk [`Consumer::fill`] instead, but the accounting is
    /// identical.
    #[must_use]
    pub fn pop_or_silence(&mut self) -> f32 {
        self.try_pop().unwrap_or_else(|| {
            self.shared.underruns.fetch_add(1, Ordering::Relaxed);
            0.0
        })
    }

    /// Fill `out` completely without ever blocking — bulk-draining whatever
    /// is published into the front of `out` and padding any shortfall with
    /// silence.
    ///
    /// This never takes a lock, regardless of `out.len()`: it is the
    /// callback hot path, and a producer stalled mid-push must never be able
    /// to delay it (see the module docs). Any shortfall (`out` longer than
    /// what is published) counts as that many underrun samples, added to the
    /// counter in one update — identical accounting to calling
    /// [`Consumer::pop_or_silence`] once per missing sample.
    pub fn fill(&mut self, out: &mut [f32]) {
        self.settle_pending_discard();
        // Acquire: synchronizes-with the producer's `Release` `fetch_add`,
        // so every published slot this observes is visible before the reads
        // below.
        let occupied = self.shared.occupied.load(Ordering::Acquire);
        let drained = out.len().min(occupied);
        for (i, slot) in out.iter_mut().take(drained).enumerate() {
            let idx = (self.tail + i) % self.shared.capacity;
            *slot = f32::from_bits(self.shared.buffer[idx].load(Ordering::Relaxed));
        }
        for slot in &mut out[drained..] {
            *slot = 0.0;
        }
        if drained > 0 {
            self.tail = (self.tail + drained) % self.shared.capacity;
            // Release: publishes the freed slots to the producer's `Acquire`
            // load of `occupied`.
            self.shared.occupied.fetch_sub(drained, Ordering::Release);
        }
        let shortfall = out.len() - drained;
        if shortfall > 0 {
            self.shared.underruns.fetch_add(
                u64::try_from(shortfall).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        }
    }

    /// Drop every queued sample without reading it or counting an underrun,
    /// so playback resumes from whatever is pushed next.
    ///
    /// Takes the producer lock, so it belongs on the thread that attaches a
    /// consumer to a device stream, never in the device callback itself.
    pub(crate) fn discard_queued(&mut self) {
        let head = lock_head(&self.shared.head);
        self.tail = *head;
        self.shared.occupied.store(0, Ordering::Release);
    }

    /// Drop the queued samples pushed before [`Producer::pushed`] returned
    /// `mark`, keeping those pushed since, without counting an underrun.
    ///
    /// Queued samples are unread and in push order, so the newest
    /// `pushed - mark` of them are the ones pushed after the mark. Takes the
    /// producer lock, like [`Self::discard_queued`].
    pub(crate) fn discard_pushed_before(&mut self, mark: u64) {
        let _head = lock_head(&self.shared.head);
        let since = self
            .shared
            .pushed
            .load(Ordering::Relaxed)
            .wrapping_sub(mark);
        let since = usize::try_from(since).unwrap_or(usize::MAX);
        // Acquire: synchronizes-with the producer's `Release` `fetch_add`.
        let queued = self.shared.occupied.load(Ordering::Acquire);
        let stale = queued - queued.min(since);
        if stale == 0 {
            return;
        }
        self.tail = (self.tail + stale) % self.shared.capacity;
        // Release: publishes the freed slots to the producer's `Acquire`
        // load of `occupied`.
        self.shared.occupied.fetch_sub(stale, Ordering::Release);
    }

    /// Drop whatever is queued when this consumer is next read, before that
    /// read, without counting an underrun.
    ///
    /// Lock-free, so it holds once the consumer has moved into a device
    /// callback: everything the producer pushed before the stream's first
    /// callback is dropped there, however long the stream took to build.
    pub(crate) fn discard_queued_at_first_read(&mut self) {
        self.discard_pending = true;
    }

    fn settle_pending_discard(&mut self) {
        if !std::mem::take(&mut self.discard_pending) {
            return;
        }
        // Acquire: synchronizes-with the producer's `Release` `fetch_add`.
        // Only this consumer ever lowers `occupied`, so these slots stay
        // queued until the `fetch_sub` below frees them.
        let queued = self.shared.occupied.load(Ordering::Acquire);
        if queued == 0 {
            return;
        }
        self.tail = (self.tail + queued) % self.shared.capacity;
        // Release: publishes the freed slots to the producer's `Acquire`
        // load of `occupied`.
        self.shared.occupied.fetch_sub(queued, Ordering::Release);
    }

    /// Samples currently queued and ready to read.
    #[must_use]
    pub fn available(&self) -> usize {
        self.shared.occupied.load(Ordering::Acquire)
    }

    /// Total samples filled with silence due to underrun so far (see
    /// [`Consumer::pop_or_silence`]).
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }
}

impl Drop for Consumer {
    fn drop(&mut self) {
        self.shared.consumer_alive.store(false, Ordering::Release);
    }
}

#[cfg(test)]
// Tests compare PCM sample arrays for exact equality on purpose: every
// value here is the deliberate, exactly-representable output of a ring
// buffer push/fill, not the result of accumulated floating-point math.
#[allow(clippy::float_cmp)]
mod tests;
