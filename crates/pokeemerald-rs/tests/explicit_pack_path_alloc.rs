//! A failed `App::new_headless_real_at` boot must not retain the pack path it
//! was given: the explicit pack source is owned and freed with the boot.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use pokeemerald_rs::App;

thread_local! {
    /// Net live heap bytes allocated on *this* thread (frees from another
    /// thread are not seen, which only matters for cross-thread handoff this
    /// test never does). `const`-initialised and non-`Drop`, so it never
    /// allocates itself.
    static LIVE_BYTES: Cell<usize> = const { Cell::new(0) };
}

fn live_bytes() -> usize {
    LIVE_BYTES.with(Cell::get)
}

/// Wrapping, so a block freed on a thread other than the one that allocated
/// it cannot panic; the test only compares two snapshots for equality.
fn adjust(grow: usize, shrink: usize) {
    let _ =
        LIVE_BYTES.try_with(|live| live.set(live.get().wrapping_add(grow).wrapping_sub(shrink)));
}

/// The system allocator, tracking net live bytes per thread.
struct Counting;

// SAFETY: every method forwards directly to `System`, which upholds the
// `GlobalAlloc` contract; the counter is a thread-local `Cell` update that
// never re-enters the allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the caller, who owes the contract.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            adjust(layout.size(), 0);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        adjust(0, layout.size());
        // SAFETY: `ptr`/`layout` match a prior `System` allocation.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the block is `System`'s own; the caller owes the contract.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            adjust(new_size, layout.size());
        }
        new
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as `alloc`.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            adjust(layout.size(), 0);
        }
        ptr
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn failed_explicit_boots_do_not_retain_the_pack_path() {
    let dir = std::env::temp_dir().join(format!("pokeemerald-1953-{}", std::process::id()));
    let pack = dir.join("a-deliberately-long-missing-pack-name-to-make-a-leak-visible.pack");
    let save = dir.join("save.sav");

    // Warm up once so one-time lazy allocations are outside the interval.
    drop(App::new_headless_real_at(&pack, &save));

    let before = live_bytes();
    for _ in 0..100 {
        let result = App::new_headless_real_at(&pack, &save);
        assert!(result.is_err());
        drop(result);
    }
    let after = live_bytes();

    assert_eq!(
        after,
        before,
        "failed boots retained {} bytes",
        after - before
    );
}
