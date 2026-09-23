//! The audio device callback must not touch the heap, whatever size buffer
//! the host hands it.
//!
//! `cpal` calls `Source::fill` (and, on the common resampled path,
//! `Resampler::fill`) on the OS audio thread under a hard deadline. A buffer
//! larger than the size the device advertised is exactly the moment the
//! callback must stay allocation-free: pre-sizing off the thread covers
//! every realistic advertised size, and anything past it is chunked, so an
//! in-callback grow would only ever fire when the deadline is least forgiving.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use platform::{ring_buffer, Resampler};

thread_local! {
    /// Heap operations on *this* thread since the counter was last read.
    /// `const`-initialised and non-`Drop`, so reading it never itself
    /// allocates or registers a TLS destructor.
    static HEAP_OPS: Cell<usize> = const { Cell::new(0) };
}

fn heap_ops() -> usize {
    HEAP_OPS.with(Cell::get)
}

fn record_heap_op() {
    let _ = HEAP_OPS.try_with(|ops| ops.set(ops.get() + 1));
}

/// The system allocator, counting every allocating operation per thread.
struct Counting;

// SAFETY: every method forwards directly to `System`, which upholds the
// `GlobalAlloc` contract; the counter is a thread-local `Cell` update that
// never re-enters the allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_heap_op();
        // SAFETY: `layout` comes straight from the caller, who already owes
        // `GlobalAlloc::alloc` its contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr`/`layout` come from a matching `System` allocation,
        // since every allocating method here forwards to `System`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_heap_op();
        // SAFETY: as `dealloc` — the block is `System`'s own allocation, and
        // the caller owes `GlobalAlloc::realloc` its contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_heap_op();
        // SAFETY: as `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const CHANNELS: u16 = 2;
const SOURCE_RATE: f64 = 13_379.0;
const DEVICE_RATE: u32 = 48_000;
/// The largest callback size the device advertised, which is all
/// `Resampler::new` gets to pre-size against.
const ADVERTISED_FRAMES: usize = 512;

#[test]
fn an_oversized_callback_does_not_allocate_on_the_audio_thread() {
    let (producer, consumer) = ring_buffer(1 << 16);
    let mut resampler = Resampler::new(
        consumer,
        CHANNELS,
        SOURCE_RATE,
        DEVICE_RATE,
        ADVERTISED_FRAMES,
    )
    .unwrap();

    let pcm = vec![0.25_f32; 1 << 14];
    let mut advertised = vec![0.0_f32; ADVERTISED_FRAMES * usize::from(CHANNELS)];
    // A host may hand the callback more than it advertised; four times the
    // advertised size is well inside what a stressed backend can produce.
    let mut oversized = vec![0.0_f32; 4 * ADVERTISED_FRAMES * usize::from(CHANNELS)];

    // Steady state first: prime the interpolator and settle every buffer,
    // outside the measured window.
    let _ = producer.push(&pcm);
    resampler.fill(&mut advertised);

    // Control: a callback at the advertised size is what the constructor
    // pre-sized for, so it must be heap-free — and proves this counter reads
    // zero when the callback behaves.
    let _ = producer.push(&pcm);
    let before = heap_ops();
    resampler.fill(&mut advertised);
    assert_eq!(
        heap_ops() - before,
        0,
        "an advertised-size callback should never touch the heap"
    );

    let _ = producer.push(&pcm);
    let before = heap_ops();
    resampler.fill(&mut oversized);
    let after = heap_ops();

    assert_eq!(
        after - before,
        0,
        "a {}-frame callback against a {ADVERTISED_FRAMES}-frame advertised maximum \
         performed {} heap operation(s) on the audio callback thread",
        oversized.len() / usize::from(CHANNELS),
        after - before,
    );
}
