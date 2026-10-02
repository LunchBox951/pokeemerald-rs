//! The validity count: usable timestamps keep the measured wait in force.

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

    let result = wait_for_measured_tail(
        100,
        std::time::Duration::from_secs(1),
        48_000,
        &policy,
        || {
            let ms = clock.borrow().duration_since(start).as_millis();
            Some(progress_valid(
                if ms < 300 {
                    u64::try_from(ms / 10).unwrap_or(0)
                } else {
                    30
                },
                submitted.get(),
                u64::try_from((ms / 10).min(30)).unwrap_or(0),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            submitted.set(submitted.get() + 1);
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
    let valid = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        // `device_tail_wait(Some(4_800), 48_000)`: two 100 ms periods
        // plus the margin.
        std::time::Duration::from_millis(250),
        48_000,
        &policy,
        || Some(progress_valid(sounded.get(), submitted.get(), valid.get())),
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
                valid.set(valid.get() + 1);
                submitted.set(submitted.get() + 4_800);
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
    let valid = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress_valid(sounded.get(), submitted.get(), valid.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            let ms = clock.borrow().duration_since(start).as_millis();
            if ms == 50 {
                // 200 ms of playback; the callback is preempted before its
                // sounded store, but its validity count is already out.
                valid.set(valid.get() + 1);
                submitted.set(submitted.get() + 9_600);
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
/// submitted frames (so the validity count is seen a poll early). Returns
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
    let valid = Cell::new(0_u64);
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
                progress_valid(seen, submitted.get(), valid.get())
            } else {
                progress_valid(sounded.get(), seen, valid.get())
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
                valid.set(valid.get() + 1);
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

/// A poll that sees a usable callback's submitted frames and validity
/// count before its sounded estimate must not read it as timestamp-stale.
#[test]
fn a_submitted_advance_seen_before_its_sounded_update_is_not_stale() {
    let (result, elapsed) = torn_snapshot_wait(true);

    assert!(result.is_ok());
    assert!(
        elapsed >= std::time::Duration::from_millis(400),
        "finished at {elapsed:?}, before the measured target sounded"
    );
}

/// A poll that sees a usable callback's validity count before its
/// submitted frames must not read the next poll's advance as stale.
#[test]
fn a_validity_count_seen_before_its_submitted_advance_is_not_stale() {
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
    let valid = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        9_600,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress_valid(100, submitted.get(), valid.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            valid.set(valid.get() + 1);
            submitted.set(submitted.get() + 480);
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

/// The same descheduled poll, but the validity count moved with the
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
    let valid = Cell::new(0_u64);
    let mut first_sleep = true;

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress_valid(0, submitted.get(), valid.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            if std::mem::take(&mut first_sleep) {
                *clock.borrow_mut() += std::time::Duration::from_millis(310);
                submitted.set(submitted.get() + 12_000);
                valid.set(valid.get() + 1);
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
