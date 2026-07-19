//! A bounded, underrun-safe sample ring buffer (S-1): the seam between the
//! future `audio` crate (M4A engine, S-3), which renders PCM on its own
//! thread, and [`crate::audio::AudioOutput`], which drains it once per audio
//! device callback.
//!
//! [`ring_buffer`] splits into a cloneable [`Producer`] (write side) and a
//! single [`Consumer`] (read side) sharing one `Mutex`-guarded queue —
//! simple over lock-free, per the project's "prefer a `Mutex`/atomic ring
//! buffer" guidance; a few hundred samples per callback under a short-held
//! lock is not a contention risk here.
//!
//! [`Consumer::fill`] is the hot path every consumer of a ring buffer is
//! built on — the real device callback, [`crate::resample::Resampler`], and
//! the null backend used in tests all drive it (directly, or one frame at a
//! time from the resampler). It takes the queue lock exactly once per call,
//! bulk-drains whatever is queued into the requested slice, and pads any
//! shortfall with silence, adding that shortfall to the underrun counter in a
//! single consolidated update. That keeps the promised "short-held single
//! lock per callback" true: no per-sample lock churn. [`Consumer::pop_or_silence`]
//! is the same "fill silence and count one underrun when the buffer runs dry"
//! rule for a single sample, retained for callers that genuinely want one
//! sample at a time.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Shared state behind a [`Producer`]/[`Consumer`] pair.
struct Shared {
    queue: Mutex<VecDeque<f32>>,
    capacity: usize,
    underruns: AtomicU64,
}

/// Lock `queue`, recovering from poisoning rather than propagating a panic.
///
/// A panic on one side (e.g. a bug in the future `audio` crate's mixer)
/// should not also take down the audio device callback thread; the worst a
/// poisoned lock should cost here is a torn-looking read, not a crash.
fn lock(queue: &Mutex<VecDeque<f32>>) -> std::sync::MutexGuard<'_, VecDeque<f32>> {
    queue.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Create a bounded sample ring buffer, split into a producer half and a
/// consumer half.
///
/// `capacity` is in samples, not frames: for interleaved stereo, a
/// `capacity` of `2 * frames` holds `frames` frames of audio.
#[must_use]
pub fn ring_buffer(capacity: usize) -> (Producer, Consumer) {
    let shared = Arc::new(Shared {
        queue: Mutex::new(VecDeque::with_capacity(capacity)),
        capacity,
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
/// `Send + Sync`, which is the point: the `audio` crate's mixer pushes
/// rendered PCM here from its own thread while [`Consumer`] drains it from
/// the audio device callback thread.
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
        let mut queue = lock(&self.shared.queue);
        let space = self.shared.capacity.saturating_sub(queue.len());
        let n = samples.len().min(space);
        queue.extend(&samples[..n]);
        n
    }

    /// Samples free right now.
    #[must_use]
    pub fn available_space(&self) -> usize {
        self.shared
            .capacity
            .saturating_sub(lock(&self.shared.queue).len())
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
        lock(&self.shared.queue).pop_front()
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

    /// Fill `out` completely under a single lock acquisition — bulk-draining
    /// whatever is queued into the front of `out` and padding any shortfall
    /// with silence.
    ///
    /// The queue lock is taken exactly once, regardless of `out.len()`: this
    /// is the callback hot path, so it must not lock per sample. Any
    /// shortfall (`out` longer than the queue) counts as that many underrun
    /// samples, added to the counter in one update — identical accounting to
    /// calling [`Consumer::pop_or_silence`] once per missing sample.
    pub fn fill(&self, out: &mut [f32]) {
        let shortfall = {
            let mut queue = lock(&self.shared.queue);
            let drained = out.len().min(queue.len());
            for (slot, sample) in out.iter_mut().zip(queue.drain(..drained)) {
                *slot = sample;
            }
            for slot in &mut out[drained..] {
                *slot = 0.0;
            }
            out.len() - drained
        };
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
        lock(&self.shared.queue).len()
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
        // is required to match the old per-sample accounting exactly.
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
}
