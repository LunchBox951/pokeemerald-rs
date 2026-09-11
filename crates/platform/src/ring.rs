//! A bounded, underrun-safe sample ring buffer: the seam between a producer
//! of rendered PCM — in practice the integration crate's frame-driven music
//! player, pushing the `audio` crate's M4A output — and
//! [`crate::audio::AudioOutput`], which drains it once per audio device
//! callback.
//!
//! [`ring_buffer`] splits into a cloneable [`Producer`] (write side) and a
//! single [`Consumer`] (read side) sharing one fixed-capacity ring of atomic
//! slots. The consumer side — the real-time device callback — never takes a
//! lock at all: it only loads and stores plain atomics, so a producer
//! stalled or descheduled mid-push can never block it. See "Design" below.
//!
//! [`Consumer::fill`] is the hot path every consumer of a ring buffer is
//! built on — the real device callback, [`crate::resample::Resampler`], and
//! the null backend used in tests all drive it, each in one bulk call per
//! callback. It bulk-drains whatever is published into the requested slice
//! without ever blocking, and pads any shortfall with silence, adding that
//! shortfall to the underrun counter in a single consolidated update.
//! [`Consumer::pop_or_silence`] is the same "fill silence and count one
//! underrun when the buffer runs dry" rule for a single sample, retained for
//! callers that genuinely want one sample at a time.
//!
//! ## Design
//!
//! The queue is a fixed-size `Box<[AtomicU32]>` (each slot an `f32`'s bit
//! pattern, via [`f32::to_bits`]/[`f32::from_bits`] — plain `AtomicU32`
//! rather than `UnsafeCell<f32>` plus a hand-proved `unsafe impl Sync`,
//! because there is no need to take on that soundness burden when the
//! platform's native atomics already give the same result safely), plus two
//! monotonically increasing cursors: `head` (samples ever published, moved
//! only by [`Producer::push`]) and `tail` (samples ever consumed, moved only
//! by the single [`Consumer`]). Occupancy is always `head - tail`.
//!
//! `Producer` is `Clone`, so more than one handle could in principle push
//! concurrently; a producer-only `Mutex<()>` (`Shared::push_lock`) serializes
//! them. Crucially, the consumer never touches that lock — it is pure
//! producer-vs-producer mutual exclusion, invisible to the callback thread —
//! so a producer stalled while holding it still cannot block
//! [`Consumer::fill`]/[`Consumer::try_pop`]/[`Consumer::available`].
//!
//! Ordering: the producer publishes samples by storing them with `Relaxed`
//! ordering and then advancing `head` with `Release`; the consumer's
//! `Acquire` load of `head` synchronizes-with that store, so every sample up
//! to the observed `head` is visible before it is read. Symmetrically, the
//! consumer frees slots by advancing `tail` with `Release` after reading
//! them, and the producer's `Acquire` load of `tail` (while holding
//! `push_lock`) synchronizes-with that, so it never overwrites a slot the
//! consumer has not finished reading. Each side's *own* cursor — `head` for
//! the producer (read only while holding `push_lock`, which serializes every
//! writer to it) and `tail` for the consumer (the sole writer) — only ever
//! needs `Relaxed`, since a thread's own prior write to a location is always
//! visible to its own later read of it, lock or no lock.
//!
//! `head.wrapping_sub(tail)` (occupancy) never produces a nonsensical result:
//! `tail <= head` is a standing invariant (the consumer never advances `tail`
//! past a `head` it has observed), and whichever side reads its own cursor
//! exactly (under the reasoning above) always reads it no smaller than the
//! other side's fresh snapshot of the same real-time instant.
//! `wrapping_sub`, not plain `-`, is deliberate: both cursors run for the
//! process lifetime and are never reset, so they do eventually wrap (after
//! roughly 13.5 hours on a 32-bit target at a nominal 88.2k samples/s stereo
//! rate); plain subtraction would panic in a debug build at that instant,
//! while wrapping arithmetic keeps the same modular difference across the
//! wraparound. A zero-capacity ring never
//! indexes the backing slice at all: occupancy is always `0` there (`head`
//! never advances because `push`'s computed free space is always `0`), so
//! every drain/write loop below runs zero iterations and the `% capacity`
//! used to wrap an index is never evaluated.

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Shared state behind a [`Producer`]/[`Consumer`] pair.
struct Shared {
    /// Fixed-capacity backing storage; slot `i % capacity` holds the bit
    /// pattern of the sample at sequence number `i`.
    buffer: Box<[AtomicU32]>,
    capacity: usize,
    /// Serializes [`Producer::push`]/[`Producer::available_space`] across
    /// however many `Producer` clones exist. Never taken by [`Consumer`] —
    /// see the module docs' "Design" section.
    push_lock: Mutex<()>,
    /// Samples ever published, moved only by the producer side.
    head: AtomicUsize,
    /// Samples ever consumed, moved only by the single [`Consumer`].
    tail: AtomicUsize,
    underruns: AtomicU64,
}

/// Lock `push_lock`, recovering from poisoning rather than propagating a
/// panic.
///
/// A panic on the producer side (e.g. a bug in the mixer feeding it) should
/// not also take down the audio device callback thread. The callback never
/// takes this lock at all, so poisoning it can only ever affect other
/// producer clones — the worst it should cost is a torn-looking push, not a
/// crash.
fn lock_producers(push_lock: &Mutex<()>) -> std::sync::MutexGuard<'_, ()> {
    push_lock.lock().unwrap_or_else(PoisonError::into_inner)
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
        push_lock: Mutex::new(()),
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
        underruns: AtomicU64::new(0),
    });
    (
        Producer {
            shared: Arc::clone(&shared),
        },
        Consumer { shared },
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
        let _guard = lock_producers(&self.shared.push_lock);
        // Exact: only this critical section ever advances `head`, and the
        // lock serializes every producer clone through it (see module docs).
        let head = self.shared.head.load(Ordering::Relaxed);
        // Acquire: synchronizes-with the consumer's `Release` store of
        // `tail`, so the samples it already read are visible as free before
        // this call overwrites their slots.
        let tail = self.shared.tail.load(Ordering::Acquire);
        // `wrapping_sub`: both cursors are monotonically increasing `usize`
        // counts that run for the process lifetime and are never reset, so
        // they do eventually wrap (after roughly 13.5 hours on a 32-bit
        // target at a nominal 88.2k samples/s stereo rate). Plain `-` would
        // panic in a debug build at that instant; wrapping arithmetic keeps
        // the same modular difference across the wraparound, which is all
        // `space` needs (see the module docs).
        let space = self.shared.capacity.saturating_sub(head.wrapping_sub(tail));
        let n = samples.len().min(space);
        for (i, &sample) in samples[..n].iter().enumerate() {
            let idx = head.wrapping_add(i) % self.shared.capacity;
            self.shared.buffer[idx].store(sample.to_bits(), Ordering::Relaxed);
        }
        // Release: publishes both the counter and every slot store above to
        // the consumer's `Acquire` load of `head`.
        self.shared
            .head
            .store(head.wrapping_add(n), Ordering::Release);
        n
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
        // Same critical section as `push`, so the same exactness/ordering
        // argument applies: `head` is exact, `tail` is a fresh `Acquire`
        // snapshot, and `tail <= head`'s invariant rules out underflow —
        // `wrapping_sub` additionally keeps this correct across a cursor
        // wraparound (see `push`).
        let _guard = lock_producers(&self.shared.push_lock);
        let head = self.shared.head.load(Ordering::Relaxed);
        let tail = self.shared.tail.load(Ordering::Acquire);
        self.shared.capacity.saturating_sub(head.wrapping_sub(tail))
    }

    /// Total samples emitted as silence so far because a consumer wanted
    /// more than was queued (see [`Consumer::pop_or_silence`]).
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }
}

/// The read half of a sample ring buffer, driven once per audio device
/// callback (or, for the null backend, once per manual test/harness call).
pub struct Consumer {
    shared: Arc<Shared>,
}

impl Consumer {
    /// Pop the next queued sample, or `None` if the buffer is empty.
    ///
    /// A pure query: does not touch the underrun counter. Most callers want
    /// [`Consumer::pop_or_silence`] or [`Consumer::fill`] instead.
    #[must_use]
    pub fn try_pop(&self) -> Option<f32> {
        // Exact: only this method (and `fill`, never called concurrently —
        // there is exactly one `Consumer`) advances `tail`.
        let tail = self.shared.tail.load(Ordering::Relaxed);
        // Acquire: synchronizes-with the producer's `Release` store of
        // `head`, so a published slot's value is visible before this reads
        // it.
        let head = self.shared.head.load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        let idx = tail % self.shared.capacity;
        let sample = f32::from_bits(self.shared.buffer[idx].load(Ordering::Relaxed));
        // Release: publishes the freed slot to the producer's `Acquire`
        // load of `tail`.
        self.shared
            .tail
            .store(tail.wrapping_add(1), Ordering::Release);
        Some(sample)
    }

    /// Pop the next queued sample; if none is queued, count one underrun
    /// sample and return silence (`0.0`).
    ///
    /// The single-sample form of the underrun-safe rule; the callback hot
    /// path uses the bulk [`Consumer::fill`] instead, but the accounting is
    /// identical.
    #[must_use]
    pub fn pop_or_silence(&self) -> f32 {
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
    pub fn fill(&self, out: &mut [f32]) {
        let tail = self.shared.tail.load(Ordering::Relaxed);
        let head = self.shared.head.load(Ordering::Acquire);
        let drained = out.len().min(head.wrapping_sub(tail));
        for (i, slot) in out.iter_mut().take(drained).enumerate() {
            let idx = tail.wrapping_add(i) % self.shared.capacity;
            *slot = f32::from_bits(self.shared.buffer[idx].load(Ordering::Relaxed));
        }
        for slot in &mut out[drained..] {
            *slot = 0.0;
        }
        self.shared
            .tail
            .store(tail.wrapping_add(drained), Ordering::Release);
        let shortfall = out.len() - drained;
        if shortfall > 0 {
            self.shared.underruns.fetch_add(
                u64::try_from(shortfall).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        }
    }

    /// Samples currently queued and ready to read.
    #[must_use]
    pub fn available(&self) -> usize {
        let tail = self.shared.tail.load(Ordering::Relaxed);
        let head = self.shared.head.load(Ordering::Acquire);
        head.wrapping_sub(tail)
    }

    /// Total samples filled with silence due to underrun so far (see
    /// [`Consumer::pop_or_silence`]).
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
// Tests compare PCM sample arrays for exact equality on purpose: every
// value here is the deliberate, exactly-representable output of a ring
// buffer push/fill, not the result of accumulated floating-point math.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn fill_drains_queued_samples_in_order() {
        let (producer, consumer) = ring_buffer(8);
        assert_eq!(producer.push(&[1.0, 2.0, 3.0, 4.0]), 4);
        assert_eq!(consumer.available(), 4);

        let mut out = [0.0; 4];
        consumer.fill(&mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(consumer.available(), 0);
        assert_eq!(consumer.underruns(), 0);
    }

    #[test]
    fn underrun_pads_shortfall_with_silence_and_counts_it() {
        let (producer, consumer) = ring_buffer(8);
        assert_eq!(producer.push(&[1.0, 2.0]), 2);

        let mut out = [9.0; 5];
        consumer.fill(&mut out);
        assert_eq!(out, [1.0, 2.0, 0.0, 0.0, 0.0]);
        // 3 slots had nothing queued: 3 individual underrun samples.
        assert_eq!(consumer.underruns(), 3);
        // Producer's view of the counter is the same shared counter.
        assert_eq!(producer.underruns(), 3);
    }

    #[test]
    fn bulk_fill_consolidates_shortfall_into_the_underrun_count() {
        // A single `fill` that outruns the queue must add exactly the missing
        // sample count to the counter — the bulk drain's consolidated update
        // must match per-sample accounting exactly.
        let (producer, consumer) = ring_buffer(64);
        assert_eq!(producer.push(&[1.0, 2.0, 3.0]), 3);

        let mut out = [9.0; 10];
        consumer.fill(&mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        // 3 drained, 7 padded: exactly 7 underrun samples from one call.
        assert_eq!(consumer.underruns(), 7);

        // A wholly-empty fill counts its entire length, accumulating onto the
        // previous consolidated total.
        let mut out2 = [0.0; 5];
        consumer.fill(&mut out2);
        assert_eq!(consumer.underruns(), 12);

        // A fill fully satisfied by the queue adds nothing.
        assert_eq!(producer.push(&[4.0, 5.0]), 2);
        let mut out3 = [0.0; 2];
        consumer.fill(&mut out3);
        assert_eq!(out3, [4.0, 5.0]);
        assert_eq!(consumer.underruns(), 12);
    }

    /// Capacity is the ring's fixed size, not a reading of how much of it is
    /// free: queueing samples must not appear to shrink the ring.
    #[test]
    fn capacity_is_unchanged_by_what_is_queued() {
        let (producer, _consumer) = ring_buffer(8);
        assert_eq!(producer.capacity(), 8);
        assert_eq!(producer.available_space(), 8);

        assert_eq!(producer.push(&[1.0, 2.0, 3.0]), 3);
        assert_eq!(producer.capacity(), 8);
        assert_eq!(producer.available_space(), 5);
    }

    #[test]
    fn push_beyond_capacity_drops_the_overflow() {
        let (producer, consumer) = ring_buffer(4);
        assert_eq!(producer.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]), 4);
        assert_eq!(producer.available_space(), 0);
        assert_eq!(consumer.available(), 4);

        let mut out = [0.0; 4];
        consumer.fill(&mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn sustained_push_and_drain_cycles_never_grow_past_capacity() {
        // The queue is not a fixed-size array with manual index wraparound,
        // but it must still *behave* like a bounded ring under sustained
        // use: capacity never grows, and values survive many drain/refill
        // cycles that each cross the initial capacity boundary.
        let (producer, consumer) = ring_buffer(4);
        for cycle in 0..1000_u32 {
            // `cycle` never exceeds 1000, well within `f32`'s 24-bit exact
            // integer range, so this loses no precision.
            #[allow(clippy::cast_precision_loss)]
            let a = cycle as f32;
            let b = a + 0.5;
            assert_eq!(producer.push(&[a, b]), 2);
            assert!(producer.available_space() <= 4);

            let mut out = [0.0; 2];
            consumer.fill(&mut out);
            assert_eq!(out, [a, b]);
        }
        assert_eq!(consumer.underruns(), 0);
        assert_eq!(consumer.available(), 0);
    }

    #[test]
    fn producer_pushes_from_another_thread() {
        let (producer, consumer) = ring_buffer(1024);
        // `i` never exceeds 512, well within `f32`'s 24-bit exact integer
        // range, so this loses no precision.
        #[allow(clippy::cast_precision_loss)]
        let data: Vec<f32> = (0..512).map(|i| i as f32).collect();
        let expected = data.clone();

        let producer_thread = producer.clone();
        let handle = std::thread::spawn(move || {
            let mut written = 0;
            while written < data.len() {
                written += producer_thread.push(&data[written..]);
            }
        });
        handle.join().expect("producer thread panicked");

        let mut out = vec![0.0; expected.len()];
        consumer.fill(&mut out);
        assert_eq!(out, expected);
        assert_eq!(consumer.underruns(), 0);
    }

    /// The device callback drains through [`Consumer::fill`] under a hard
    /// deadline, while the producer pushes from an ordinary (preemptible)
    /// game thread. A producer descheduled mid-push — holding
    /// `Shared::push_lock` — must therefore not be able to stall the
    /// callback: the callback has to keep making progress on what is
    /// already published, since it never touches that lock.
    #[test]
    fn fill_does_not_block_on_a_producer_stalled_mid_push() {
        let (producer, consumer) = ring_buffer(8);
        assert_eq!(producer.push(&[1.0, 2.0, 3.0, 4.0]), 4);

        // Stand in for the producer being preempted while it holds
        // `push_lock`: a real scheduler can stall it for an arbitrary slice,
        // far past one callback period.
        let (acquired_tx, acquired_rx) = std::sync::mpsc::channel();
        let shared = Arc::clone(&producer.shared);
        let stalled_producer = std::thread::spawn(move || {
            let _guard = lock_producers(&shared.push_lock);
            acquired_tx.send(()).expect("receiver dropped");
            std::thread::sleep(std::time::Duration::from_millis(500));
        });
        acquired_rx.recv().expect("stalled producer never started");

        let start = std::time::Instant::now();
        let mut out = [9.0; 4];
        consumer.fill(&mut out);
        let elapsed = start.elapsed();
        stalled_producer.join().expect("producer thread panicked");

        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "callback fill waited {elapsed:?} on a stalled producer"
        );
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(consumer.underruns(), 0);
    }
}
