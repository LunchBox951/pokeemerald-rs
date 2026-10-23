//! Unit tests for the sample ring buffer.

use super::*;

#[test]
fn a_consumer_is_reclaimed_only_after_the_previous_one_is_dropped() {
    let (producer, consumer) = ring_buffer(4);
    assert!(producer.reclaim_consumer().is_none());
    drop(consumer);
    let second = producer.reclaim_consumer().expect("vacant slot");
    assert!(producer.reclaim_consumer().is_none());
    drop(second);
}

#[test]
fn a_first_read_discard_drops_what_was_queued_before_that_read() {
    let (producer, mut consumer) = ring_buffer(4);
    assert_eq!(producer.push(&[1.0]), 1);
    consumer.discard_queued_at_first_read();
    assert_eq!(producer.push(&[2.0, 3.0]), 2); // queued after the request
    let mut out = [9.0; 2];
    consumer.fill(&mut out);
    assert_eq!(out, [0.0, 0.0]);
    assert_eq!(producer.available_space(), 4);
    assert_eq!(producer.push(&[4.0, 5.0, 6.0]), 3); // wraps the write index
    consumer.fill(&mut out);
    assert_eq!(out, [4.0, 5.0], "the discard applies to one read only");
}

#[test]
fn a_reclaimed_consumer_resumes_where_the_dropped_one_stopped() {
    let (producer, mut consumer) = ring_buffer(4);
    assert_eq!(producer.push(&[1.0, 2.0, 3.0]), 3);
    let mut out = [0.0; 2];
    consumer.fill(&mut out);
    assert_eq!(producer.push(&[4.0, 5.0]), 2); // wraps the write index
    drop(consumer);

    let mut reclaimed = producer.reclaim_consumer().unwrap();
    let mut out = [0.0; 3];
    reclaimed.fill(&mut out);
    assert_eq!(out, [3.0, 4.0, 5.0]);
}

#[test]
fn reclaiming_a_zero_capacity_ring_does_not_panic() {
    let (producer, consumer) = ring_buffer(0);
    drop(consumer);
    assert!(producer.reclaim_consumer().is_some());
}

#[test]
fn a_discard_before_a_mark_keeps_what_was_pushed_after_it() {
    let (producer, mut consumer) = ring_buffer(8);
    assert_eq!(producer.push(&[0.25; 3]), 3);
    let mut out = [0.0; 2];
    consumer.fill(&mut out);
    assert_eq!(producer.push(&[0.25; 5]), 5);
    let mark = producer.pushed();
    assert_eq!(mark, 8);
    assert_eq!(producer.push(&[0.5; 2]), 2);

    consumer.discard_pushed_before(mark);
    assert_eq!(consumer.available(), 2);
    let mut out = [0.0; 3];
    consumer.fill(&mut out);
    assert_eq!(out, [0.5, 0.5, 0.0]);
    assert_eq!(consumer.underruns(), 1, "only the read shortfall counts");

    // A mark at the current count drops everything queued.
    assert_eq!(producer.push(&[0.5; 2]), 2);
    consumer.discard_pushed_before(producer.pushed());
    assert_eq!(consumer.available(), 0);
}

#[test]
fn discard_queued_drops_samples_without_counting_underruns() {
    let (producer, mut consumer) = ring_buffer(4);
    assert_eq!(producer.push(&[1.0, 2.0, 3.0]), 3);
    consumer.discard_queued();
    assert_eq!(consumer.available(), 0);
    assert_eq!(producer.underruns(), 0);
    assert_eq!(producer.available_space(), 4);

    // Samples pushed afterwards, across the wrap, play in order.
    assert_eq!(producer.push(&[7.0, 8.0, 9.0]), 3);
    let mut out = [0.0; 3];
    consumer.fill(&mut out);
    assert_eq!(out, [7.0, 8.0, 9.0]);
}

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
