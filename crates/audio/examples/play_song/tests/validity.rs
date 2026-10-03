//! The usable mark: usable timestamps keep the measured wait in force.

use super::*;

/// Timestamps usable for 300 ms and then stale must still finish inside
/// the capped one-second budget while submitted frames keep advancing.
#[test]
fn a_valid_then_stale_transition_keeps_the_capped_budget() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(100_u64);
    // Callbacks are usable until 300 ms, then stale.
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        100,
        std::time::Duration::from_secs(1),
        48_000,
        &policy,
        || {
            let ms = clock.borrow().duration_since(start).as_millis();
            Some(progress_usable(
                if ms < 300 {
                    u64::try_from(ms / 10).unwrap_or(0)
                } else {
                    30
                },
                submitted.get(),
                usable.get(),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            submitted.set(submitted.get() + 1);
            if clock.borrow().duration_since(start).as_millis() < 300 {
                usable.set(submitted.get());
            }
        },
    );

    assert!(result.is_ok(), "the transition must not forfeit the budget");
}

/// A high-latency device whose usable timestamps keep advancing the
/// sounded estimate must wait for the measured target, even when one
/// callback covers more than a quarter of the derived tail.
#[test]
fn a_fresh_sounded_estimate_below_target_is_not_overridden_by_the_tail() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    // 1 s submitted before the drain; 400 ms of output latency.
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(48_000_u64 - 19_200);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        // `device_tail_wait(Some(4_800), 48_000)`: two 100 ms periods
        // plus the margin.
        std::time::Duration::from_millis(250),
        48_000,
        &policy,
        || {
            Some(progress_usable(
                sounded.get(),
                submitted.get(),
                usable.get(),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            if clock
                .borrow()
                .duration_since(start)
                .as_millis()
                .is_multiple_of(100)
            {
                // Every 100 ms callback carries a usable timestamp.
                submitted.set(submitted.get() + 4_800);
                usable.set(submitted.get());
                sounded.set(sounded.get() + 4_800);
            }
        },
    );
    let elapsed = clock.borrow().duration_since(start);

    assert!(result.is_ok());
    assert!(
        elapsed >= std::time::Duration::from_millis(400),
        "finished at {elapsed:?}, before the measured target sounded"
    );
}

/// A callback preempted between its submitted and sounded stores for far
/// longer than one poll is not stale-timestamp evidence: its timestamp was
/// usable, so the wait must hold for the measured target.
#[test]
fn a_preempted_sounded_store_is_not_stale_evidence() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(48_000_u64 - 19_200);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || {
            Some(progress_usable(
                sounded.get(),
                submitted.get(),
                usable.get(),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            let ms = clock.borrow().duration_since(start).as_millis();
            if ms == 50 {
                // 200 ms of playback; the callback is preempted before its
                // sounded store, but its usable mark is already out.
                submitted.set(submitted.get() + 9_600);
                usable.set(submitted.get());
            }
            if ms == 300 {
                sounded.set(48_000);
            }
        },
    );
    let elapsed = clock.borrow().duration_since(start);

    assert!(result.is_ok());
    assert!(
        elapsed >= std::time::Duration::from_millis(300),
        "finished at {elapsed:?}, before the measured target sounded"
    );
}

/// Drive the high-latency, all-usable-timestamps device of
/// `a_fresh_sounded_estimate_below_target_is_not_overridden_by_the_tail`,
/// but let one snapshot field lag the other by a poll, as separately
/// loaded atomics can: the sounded estimate when `sounded_lags`, else the
/// submitted frames (so the usable mark is seen a poll early). Returns
/// the wait's result and its elapsed time.
fn torn_snapshot_wait(sounded_lags: bool) -> (Result<(), DrainError>, std::time::Duration) {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(48_000_u64 - 19_200);
    let usable = Cell::new(0_u64);
    // The field the poll sees one interval late, as (seen, actual).
    let initial = if sounded_lags {
        sounded.get()
    } else {
        submitted.get()
    };
    let lagging = Cell::new((initial, initial));
    let result = wait_for_measured_tail(
        48_000,
        std::time::Duration::from_millis(250),
        48_000,
        &policy,
        || {
            let (seen, _) = lagging.get();
            Some(if sounded_lags {
                progress_usable(seen, submitted.get(), usable.get())
            } else {
                progress_usable(sounded.get(), seen, usable.get())
            })
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            let (_, actual) = lagging.get();
            if clock
                .borrow()
                .duration_since(start)
                .as_millis()
                .is_multiple_of(100)
            {
                submitted.set(submitted.get() + 4_800);
                sounded.set(sounded.get() + 4_800);
                usable.set(submitted.get());
            }
            let current = if sounded_lags {
                sounded.get()
            } else {
                submitted.get()
            };
            // Publish last poll's value now; hold the fresh one back.
            lagging.set((actual, current));
        },
    );
    let elapsed = clock.borrow().duration_since(start);
    (result, elapsed)
}

/// A poll that sees a usable callback's submitted frames and usable
/// mark before its sounded estimate must not read it as timestamp-stale.
#[test]
fn a_submitted_advance_seen_before_its_sounded_update_is_not_stale() {
    let (result, elapsed) = torn_snapshot_wait(true);

    assert!(result.is_ok());
    assert!(
        elapsed >= std::time::Duration::from_millis(400),
        "finished at {elapsed:?}, before the measured target sounded"
    );
}

/// A poll that sees a usable callback's usable mark before its
/// submitted frames must not read the next poll's advance as stale.
#[test]
fn a_usable_mark_seen_before_its_submitted_advance_is_not_stale() {
    let (result, elapsed) = torn_snapshot_wait(false);

    assert!(result.is_ok());
    assert!(
        elapsed >= std::time::Duration::from_millis(400),
        "finished at {elapsed:?}, before the measured target sounded"
    );
}

/// Usable estimates that stay flat (equal, or backward under growing
/// device delay) are not stale timestamps: the wait holds for the
/// measured target and times out at the deadline, never taking the tail.
#[test]
fn usable_estimates_that_stay_flat_are_not_stale_timestamps() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(500),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_800_u64);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        9_600,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress_usable(100, submitted.get(), usable.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            submitted.set(submitted.get() + 480);
            usable.set(submitted.get());
        },
    );
    let elapsed = clock.borrow().duration_since(start);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
    assert!(
        elapsed >= policy.max_wait,
        "gave up at {elapsed:?}, before the deadline"
    );
}

/// The same descheduled poll, but the usable mark moved with the
/// aggregate: usable timestamps keep the measured wait in force.
#[test]
fn a_lone_aggregate_with_usable_timestamps_times_out_at_the_deadline() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(300),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);
    let usable = Cell::new(0_u64);
    let mut first_sleep = true;

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress_usable(0, submitted.get(), usable.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            if std::mem::take(&mut first_sleep) {
                *clock.borrow_mut() += std::time::Duration::from_millis(310);
                submitted.set(submitted.get() + 12_000);
                usable.set(submitted.get());
            } else {
                *clock.borrow_mut() += duration;
            }
        },
    );

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

/// The first callback's usable mark can be published one poll before its
/// submitted frames (`PlaybackClock::record` stores the mark first). When the
/// wait's initial snapshot already holds that mark, the callback's submitted
/// advance is still a usable callback, not stale evidence: the wait must hold
/// for the measured target rather than finishing on the derived tail.
#[test]
fn a_usable_mark_published_before_the_first_snapshot_is_not_stale_evidence() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    // The in-flight 200 ms callback has stored its usable mark (57 600) but
    // not yet its submitted frames.
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(28_800_u64);
    let usable = Cell::new(57_600_u64);

    let result = wait_for_measured_tail(
        57_600,
        // `device_tail_wait(None, 48_000)`.
        DEVICE_TAIL_FALLBACK,
        48_000,
        &policy,
        || {
            Some(progress_usable(
                sounded.get(),
                submitted.get(),
                usable.get(),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            let ms = clock.borrow().duration_since(start).as_millis();
            if ms == 10 {
                // The pending submitted store of the already-marked callback.
                submitted.set(57_600);
            } else if ms % 200 == 10 {
                // Every later 200 ms callback carries a usable timestamp.
                submitted.set(submitted.get() + 9_600);
                usable.set(submitted.get());
                sounded.set(sounded.get() + 9_600);
            }
        },
    );
    let elapsed = clock.borrow().duration_since(start);

    assert!(result.is_ok());
    assert!(
        elapsed >= std::time::Duration::from_millis(600),
        "finished at {elapsed:?}, before the measured target sounded"
    );
}

/// A usable mark published ahead of its submitted frames, on a poll where
/// submitted frames did not move, discards the pending stale evidence: the
/// tail exit must not fire on the stale callbacks it already outdates.
#[test]
fn a_usable_mark_seen_before_its_submitted_advance_clears_stale_evidence() {
    use std::time::{Duration, Instant};

    let policy = RetryPolicy {
        interval: Duration::from_millis(10),
        max_wait: Duration::from_secs(1),
    };
    let start = Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(28_800_u64);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        Duration::from_millis(250),
        48_000,
        &policy,
        || {
            Some(progress_usable(
                sounded.get(),
                submitted.get(),
                usable.get(),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            match clock.borrow().duration_since(start).as_millis() {
                // Two stale 100 ms callbacks accumulate tail evidence.
                100 | 200 => submitted.set(submitted.get() + 4_800),
                // Only the usable mark appears at the tail boundary.
                250 => usable.set(62_400),
                // The marked callback's submitted store arrives next poll.
                260 => submitted.set(62_400),
                // A later usable callback confirms the measured target.
                400 => {
                    sounded.set(48_000);
                    usable.set(67_200);
                    submitted.set(67_200);
                }
                _ => {}
            }
        },
    );

    let elapsed = clock.borrow().duration_since(start);
    assert!(result.is_ok());
    assert_eq!(elapsed, Duration::from_millis(400));
    assert_eq!(sounded.get(), 48_000);
}
