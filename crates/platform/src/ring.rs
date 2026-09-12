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
//! the null backend used in tests all drive it, each in one bulk call per
//! callback. It bulk-drains whatever is published into the requested slice
//! without ever blocking, and pads any shortfall with silence, adding that
//! shortfall to the underrun counter in a single consolidated update.
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

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
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
    });
    (
        Producer {
            shared: Arc::clone(&shared),
        },
        Consumer { shared, tail: 0 },
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
        }
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
}

impl Consumer {
    /// Pop the next queued sample, or `None` if the buffer is empty.
    ///
    /// A pure query: does not touch the underrun counter. Most callers want
    /// [`Consumer::pop_or_silence`] or [`Consumer::fill`] instead.
    pub fn try_pop(&mut self) -> Option<f32> {
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

#[cfg(test)]
// Tests compare PCM sample arrays for exact equality on purpose: every
// value here is the deliberate, exactly-representable output of a ring
// buffer push/fill, not the result of accumulated floating-point math.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn fill_drains_queued_samples_in_order() {
        let (producer, mut consumer) = ring_buffer(8);
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
        let (producer, mut consumer) = ring_buffer(8);
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
        let (producer, mut consumer) = ring_buffer(64);
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
        let (producer, mut consumer) = ring_buffer(4);
        assert_eq!(producer.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]), 4);
        assert_eq!(producer.available_space(), 0);
        assert_eq!(consumer.available(), 4);

        let mut out = [0.0; 4];
        consumer.fill(&mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn sustained_push_and_drain_cycles_never_grow_past_capacity() {
        // The backing array is fixed-size and each side's index wraps by
        // `% capacity` (see the module docs) — this exercises that wrap
        // under sustained use: capacity never grows, and values survive many
        // drain/refill cycles that each cross the initial capacity boundary.
        let (producer, mut consumer) = ring_buffer(4);
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
        let (producer, mut consumer) = ring_buffer(1024);
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
    /// game thread. A producer descheduled mid-push — holding `Shared::head`
    /// — must therefore not be able to stall the callback: the callback has
    /// to keep making progress on what is already published, since it never
    /// touches that lock.
    #[test]
    fn fill_does_not_block_on_a_producer_stalled_mid_push() {
        let (producer, mut consumer) = ring_buffer(8);
        assert_eq!(producer.push(&[1.0, 2.0, 3.0, 4.0]), 4);

        // Stand in for the producer being preempted while it holds
        // `Shared::head`'s lock: a real scheduler can stall it for an
        // arbitrary slice, far past one callback period.
        // The stall lasts until `fill` reports back, so a `fill` that waits
        // on the lock can only report after the hold gives up on it: the
        // verdict is which side let go first, not how long the host
        // scheduler happened to take. The hold's timeout is a ceiling for a
        // regressed `fill`, never a bound this test measures against.
        let (acquired_tx, acquired_rx) = std::sync::mpsc::channel();
        let (filled_tx, filled_rx) = std::sync::mpsc::channel();
        let shared = Arc::clone(&producer.shared);
        let stalled_producer = std::thread::spawn(move || {
            let _guard = lock_head(&shared.head);
            acquired_tx.send(()).expect("receiver dropped");
            filled_rx
                .recv_timeout(std::time::Duration::from_secs(10))
                .is_ok()
        });
        acquired_rx.recv().expect("stalled producer never started");

        let mut out = [9.0; 4];
        consumer.fill(&mut out);
        let filled_while_stalled = filled_tx.send(()).is_ok();
        let released_after_fill = stalled_producer.join().expect("producer thread panicked");

        assert!(
            filled_while_stalled && released_after_fill,
            "callback fill waited on a stalled producer"
        );
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(consumer.underruns(), 0);
    }

    /// Each side's index wraps by `% capacity` on every advance (see the
    /// module docs), never via an unbounded counter reduced `% capacity`
    /// only at the point of indexing — that alternative aliases two
    /// different sequence positions onto the same slot for any capacity
    /// that does not evenly divide the width such a counter wraps at, i.e.
    /// any non-power-of-two capacity, the instant it wrapped. Seed both
    /// indices to the last valid slot and push across the capacity boundary
    /// to prove the wrap itself lands correctly, for a non-power-of-two
    /// capacity (3) and a power-of-two one (4) alike.
    #[test]
    fn cursor_wraparound_is_correct_for_a_non_power_of_two_capacity() {
        for capacity in [3, 4] {
            let (producer, mut consumer) = ring_buffer(capacity);
            *producer.shared.head.lock().unwrap() = capacity - 1;
            consumer.tail = capacity - 1;

            assert_eq!(producer.push(&[1.0, 2.0]), 2);
            assert_eq!(consumer.available(), 2);

            let mut out = [0.0; 2];
            consumer.fill(&mut out);
            assert_eq!(out, [1.0, 2.0], "capacity {capacity}");
            assert_eq!(consumer.underruns(), 0);
        }
    }

    /// The rollover this ring has no exposure to is the counter's own, at
    /// `usize::MAX + 1`; it is unreachable because each cursor is reduced
    /// `% capacity` on every advance. An unbounded-counter implementation
    /// fails this on the second iteration for either capacity.
    #[test]
    fn cursors_stay_bounded_by_capacity_so_no_usize_rollover_exists() {
        for capacity in [3usize, 4] {
            let (producer, mut consumer) = ring_buffer(capacity);
            for _ in 0..=(2 * capacity) {
                assert_eq!(producer.push(&[1.0, 2.0]), 2);
                let mut out = [0.0; 2];
                consumer.fill(&mut out);
                assert_eq!(out, [1.0, 2.0], "capacity {capacity}");
                assert!(
                    *lock_head(&producer.shared.head) < capacity,
                    "head left unbounded at capacity {capacity}"
                );
                assert!(
                    consumer.tail < capacity,
                    "tail left unbounded at capacity {capacity}"
                );
            }
            assert_eq!(consumer.underruns(), 0);
        }
    }
}
