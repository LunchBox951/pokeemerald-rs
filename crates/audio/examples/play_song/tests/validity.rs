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

/// A poll starved through the whole budget sees one aggregate whose first
/// callback was usable and whose 200 ms suffix was stale: the suffix past the
/// usable mark is stale evidence, as the same frames all-stale would be.
#[test]
fn the_stale_suffix_of_a_mixed_aggregate_at_the_deadline_poll_is_evidence() {
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
                // One usable 10 ms callback, then 200 ms of stale ones.
                usable.set(submitted.get() + 480);
                submitted.set(submitted.get() + 480 + 9_600);
            } else {
                *clock.borrow_mut() += duration;
            }
        },
    );

    assert!(result.is_ok(), "the stale suffix finishes the tail");
}

/// The mark is stored before the frames it covers: with a stale callback
/// ending at 200 and the next usable callback's mark already at 300, the
/// submitted span `(100, 200]` holds no usable callback despite `mark > from`.
#[test]
fn an_in_flight_mark_does_not_prove_a_usable_callback_in_the_submitted_span() {
    let snapshot = progress_usable(60, 200, 300);
    assert!(!usable_callback_in_span(
        100,
        snapshot.submitted_frames,
        snapshot.usable_through_frames,
    ));
    // Once the marked callback's frames land the span holds it.
    assert!(usable_callback_in_span(100, 300, 300));
    // A mark at or before `from` leaves a later span entirely stale.
    assert!(!usable_callback_in_span(300, 400, 300));
}

/// Drive a device with `delay_ms` of output latency and a 200 ms derived
/// tail: 50 ms callbacks (2 400 frames at 48 kHz) from 48 000 submitted, the
/// target, usable through 500 ms and stale from 550 ms. Each usable callback
/// estimates its start frame less the delay, so the target sounds at
/// `delay_ms + 50` ms. Returns the result and when the wait ended.
fn late_usable_wait(delay_ms: u64) -> (Result<(), DrainError>, std::time::Duration) {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(0_u64);
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
            if ms.is_multiple_of(50) {
                let callback_start = submitted.get();
                submitted.set(callback_start + 2_400);
                if ms <= 500 {
                    sounded.set(callback_start.saturating_sub(delay_ms * 48));
                    usable.set(submitted.get());
                }
            }
        },
    );
    let held = clock.borrow().duration_since(start);
    (result, held)
}

/// Usable timestamps past the derived tail that still show the target short
/// prove the tail under-estimated: with 650 ms of latency the 500 ms reading
/// still owes 200 ms, so the first stale callback must not finish the wait;
/// it holds until the target sounds at 700 ms.
#[test]
fn a_usable_reading_past_the_tail_holds_the_fallback_for_what_it_owes() {
    let (result, held) = late_usable_wait(650);

    assert!(
        result.is_ok(),
        "stale live callbacks still finish in budget"
    );
    assert!(
        held >= std::time::Duration::from_millis(700),
        "the 500 ms reading owed 200 ms; finished at {held:?}"
    );
}

/// A late usable reading owing less than a full tail holds the wait only for
/// what it owes, not for another derived tail: with 460 ms of latency the
/// 500 ms reading owes 10 ms, so the first stale callback at 550 ms finishes.
#[test]
fn a_late_usable_reading_owing_little_does_not_rerun_the_tail() {
    let (result, held) = late_usable_wait(460);

    assert!(result.is_ok());
    assert!(
        held >= std::time::Duration::from_millis(510),
        "finished at {held:?}, before the target sounded"
    );
    assert!(
        held < std::time::Duration::from_millis(700),
        "held a whole extra tail to {held:?}"
    );
}

/// The valid-to-stale boundary of a 250 ms derived tail: 100 ms callbacks
/// (4 800 frames) at 50, 150, 250 ms and on from 48 000 submitted, the target;
/// the first two carry usable timestamps with `delay_ms` of latency, the rest
/// are stale. The target sounds at `delay_ms + 50` ms.
fn valid_then_stale_wait(
    delay_ms: u64,
    max_wait_ms: u64,
) -> (Result<(), DrainError>, std::time::Duration) {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(max_wait_ms),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(0_u64);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
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
            let ms = clock.borrow().duration_since(start).as_millis();
            if ms % 100 == 50 {
                let callback_start = submitted.get();
                submitted.set(callback_start + 4_800);
                if ms <= 150 {
                    sounded.set(callback_start.saturating_sub(delay_ms * 48));
                    usable.set(submitted.get());
                }
            }
        },
    );
    let held = clock.borrow().duration_since(start);
    (result, held)
}

/// Usable readings consistent with the derived tail leave its finish alone:
/// with 200 ms of latency the 150 ms reading owes 100 ms, and the stale
/// callback at 250 ms finishes there.
#[test]
fn usable_readings_consistent_with_the_tail_keep_its_finish() {
    let (result, held) = valid_then_stale_wait(200, 1_000);

    assert!(result.is_ok());
    assert_eq!(held, std::time::Duration::from_millis(250));
}

/// A usable reading far short of the target bounds the fallback: with 350 ms
/// of latency the 150 ms reading owes 250 ms, so the stale callback at 250 ms
/// must not finish the wait before the target sounds at 400 ms.
#[test]
fn a_far_short_usable_reading_holds_the_fallback_past_the_tail() {
    let (result, held) = valid_then_stale_wait(350, 1_000);

    assert!(
        result.is_ok(),
        "stale live callbacks still finish in budget"
    );
    assert!(
        held >= std::time::Duration::from_millis(400),
        "the 150 ms reading owed 250 ms; finished at {held:?}"
    );
}

/// What a usable reading owes still has to fit `max_wait`: with 350 ms of
/// latency and a 300 ms budget the target sounds past the deadline.
#[test]
fn a_usable_reading_owing_past_max_wait_times_out() {
    let (result, held) = valid_then_stale_wait(350, 300);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { target: 48_000, .. })
    ));
    assert_eq!(held, std::time::Duration::from_millis(300));
}

/// Enter the wait with `submitted` frames submitted, the target, the entry
/// snapshot's usable mark at `mark` and its sounded estimate at `sounded`,
/// then 50 ms stale callbacks (2 400 frames) under a 200 ms derived tail.
/// Returns the result and when the wait ended.
fn entry_reading_wait(
    submitted: u64,
    mark: u64,
    sounded: u64,
    max_wait_ms: u64,
) -> (Result<(), DrainError>, std::time::Duration) {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(max_wait_ms),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let frames = Cell::new(submitted);

    let result = wait_for_measured_tail(
        submitted,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress_usable(sounded, frames.get(), mark)),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            if clock
                .borrow()
                .duration_since(start)
                .as_millis()
                .is_multiple_of(50)
            {
                frames.set(frames.get() + 2_400);
            }
        },
    );
    let held = clock.borrow().duration_since(start);
    (result, held)
}

/// The entry snapshot's latest callback was usable and still owed 500 ms
/// (24 000 of 48 000 frames sounded): the stale callbacks after it must not
/// finish on the 200 ms tail.
#[test]
fn a_usable_entry_reading_bounds_the_fallback() {
    let (result, held) = entry_reading_wait(48_000, 48_000, 24_000, 1_000);

    assert!(
        result.is_ok(),
        "stale live callbacks still finish in budget"
    );
    assert!(
        held >= std::time::Duration::from_millis(500),
        "the entry reading owed 500 ms; finished at {held:?}"
    );
}

/// What the entry reading owes still has to fit `max_wait`.
#[test]
fn a_usable_entry_reading_owing_past_max_wait_times_out() {
    let (result, held) = entry_reading_wait(48_000, 48_000, 24_000, 400);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
    assert_eq!(held, std::time::Duration::from_millis(400));
}

/// An entry mark two seconds of submitted frames behind: the stale callbacks
/// since have played those two seconds, so its 2.2 s-short estimate owes only
/// 200 ms, and a device whose timestamps went stale long ago still finishes on
/// the derived tail rather than being held to the deadline.
#[test]
fn an_old_usable_entry_mark_is_long_since_paid() {
    let (result, held) = entry_reading_wait(144_000, 48_000, 38_400, 1_000);

    assert!(result.is_ok());
    assert_eq!(held, std::time::Duration::from_millis(200));
}

/// A usable reading one stale callback before the drain still owes its
/// playback: the first snapshot's mark trails submitted frames by one 50 ms
/// callback, but the 450 ms latency it measured must not be cut to the
/// derived tail.
#[test]
fn a_usable_mark_one_stale_callback_behind_at_entry_still_owes_its_playback() {
    use std::time::{Duration, Instant};

    let policy = RetryPolicy {
        interval: Duration::from_millis(10),
        max_wait: Duration::from_secs(1),
    };
    let start = Instant::now();
    let clock = Rc::new(RefCell::new(start));
    // The last usable callback (43 200..45 600) measured 450 ms of latency;
    // one stale 50 ms callback (45 600..48 000) followed it.
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(21_600_u64);
    let usable = Cell::new(45_600_u64);

    let result = wait_for_measured_tail(
        48_000,
        Duration::from_millis(200),
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
                .is_multiple_of(50)
            {
                // Stale 50 ms callbacks keep landing.
                submitted.set(submitted.get() + 2_400);
            }
        },
    );
    let elapsed = clock.borrow().duration_since(start);

    // The target sounds ~500 ms after entry (550 ms owed, read 50 ms ago).
    assert!(
        result.is_err() || elapsed >= Duration::from_millis(450),
        "finished at {elapsed:?}, before the measured target sounded"
    );
}

/// A usable reading seen by a delayed poll owes its playback from when its
/// callback ran, not from the previous poll plus its buffer: the poll after
/// the first is held to 120 ms, the one 2 400-frame usable callback lands at
/// 100 ms with 350 ms of latency (the target sounds at 450 ms), and stale
/// 50 ms callbacks follow from 150 ms.
#[test]
fn a_delayed_poll_does_not_backdate_a_usable_reading_past_its_callback() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(24_000_u64);
    let usable = Cell::new(45_600_u64);
    let sleeps = Cell::new(0_u32);

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
            let first = sleeps.get() == 0;
            sleeps.set(sleeps.get() + 1);
            let before = clock.borrow().duration_since(start).as_millis();
            let step = if first {
                std::time::Duration::from_millis(120)
            } else {
                duration
            };
            *clock.borrow_mut() += step;
            let after = clock.borrow().duration_since(start).as_millis();
            for ms in (before + 1)..=after {
                if ms == 100 {
                    let callback_start = submitted.get();
                    submitted.set(callback_start + 2_400);
                    sounded.set(sounded.get().max(callback_start - 350 * 48));
                    usable.set(submitted.get());
                } else if ms >= 150 && ms.is_multiple_of(50) {
                    submitted.set(submitted.get() + 2_400);
                }
            }
        },
    );
    let held = clock.borrow().duration_since(start);

    assert!(result.is_ok(), "finished in budget");
    assert!(
        held >= std::time::Duration::from_millis(450),
        "the target sounds at 450 ms; finished at {held:?}"
    );
}

/// The delayed poll of `a_delayed_poll_does_not_backdate_a_usable_reading_past_its_callback`
/// with an entry mark old enough to owe no more than the derived tail
/// (24 000 frames behind, 14 400 sounded): only the reading at 100 ms, seen
/// at 120 ms, says the target sounds at 450 ms, so its due time alone must
/// hold the wait there.
#[test]
fn a_usable_reading_seen_by_a_delayed_poll_is_stamped_when_seen() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(14_400_u64);
    let usable = Cell::new(24_000_u64);
    let sleeps = Cell::new(0_u32);

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
            let first = sleeps.get() == 0;
            sleeps.set(sleeps.get() + 1);
            let before = clock.borrow().duration_since(start).as_millis();
            let step = if first {
                std::time::Duration::from_millis(120)
            } else {
                duration
            };
            *clock.borrow_mut() += step;
            let after = clock.borrow().duration_since(start).as_millis();
            for ms in (before + 1)..=after {
                if ms == 100 {
                    let callback_start = submitted.get();
                    submitted.set(callback_start + 2_400);
                    sounded.set(sounded.get().max(callback_start - 350 * 48));
                    usable.set(submitted.get());
                } else if ms >= 150 && ms.is_multiple_of(50) {
                    submitted.set(submitted.get() + 2_400);
                }
            }
        },
    );
    let held = clock.borrow().duration_since(start);

    assert!(result.is_ok(), "finished in budget");
    assert!(
        held >= std::time::Duration::from_millis(450),
        "the target sounds at 450 ms; finished at {held:?}"
    );
}

/// Callback sizes vary: a usable 60 ms buffer at 50 ms with 350 ms of latency
/// (the target sounds at 400 ms), then a stale 70 ms buffer at 110 ms, both
/// seen by a poll held to 115 ms, then stale 50 ms buffers from 180 ms. The
/// stale 70 ms buffer ran 60 ms after the reading, not 70 ms, so crediting its
/// whole playback as the reading's age would finish before the target sounds.
#[test]
fn a_larger_stale_buffer_after_a_usable_one_does_not_overstate_its_age() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(6_720_u64);
    let usable = Cell::new(24_000_u64);
    let sleeps = Cell::new(0_u32);

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
            let first = sleeps.get() == 0;
            sleeps.set(sleeps.get() + 1);
            let before = clock.borrow().duration_since(start).as_millis();
            let step = if first {
                std::time::Duration::from_millis(115)
            } else {
                duration
            };
            *clock.borrow_mut() += step;
            let after = clock.borrow().duration_since(start).as_millis();
            for ms in (before + 1)..=after {
                if ms == 50 {
                    let callback_start = submitted.get();
                    submitted.set(callback_start + 2_880);
                    sounded.set(sounded.get().max(callback_start - 350 * 48));
                    usable.set(submitted.get());
                } else if ms == 110 {
                    submitted.set(submitted.get() + 3_360);
                } else if ms >= 180 && (ms - 180).is_multiple_of(50) {
                    submitted.set(submitted.get() + 2_400);
                }
            }
        },
    );
    let held = clock.borrow().duration_since(start);

    assert!(result.is_ok(), "finished in budget");
    assert!(
        held >= std::time::Duration::from_millis(400),
        "the target sounds at 400 ms; finished at {held:?}"
    );
}

/// The same with no earlier usable mark and a stale 200 ms buffer at 110 ms,
/// larger than any size the derived tail implies, then stale 50 ms buffers
/// from 310 ms: the reading is 65 ms old when seen, not 200 ms.
#[test]
fn a_stale_buffer_past_the_tail_size_after_a_usable_one_does_not_overstate_its_age() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(6_720_u64);
    let usable = Cell::new(0_u64);
    let sleeps = Cell::new(0_u32);

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
            let first = sleeps.get() == 0;
            sleeps.set(sleeps.get() + 1);
            let before = clock.borrow().duration_since(start).as_millis();
            let step = if first {
                std::time::Duration::from_millis(115)
            } else {
                duration
            };
            *clock.borrow_mut() += step;
            let after = clock.borrow().duration_since(start).as_millis();
            for ms in (before + 1)..=after {
                if ms == 50 {
                    let callback_start = submitted.get();
                    submitted.set(callback_start + 2_880);
                    sounded.set(sounded.get().max(callback_start - 350 * 48));
                    usable.set(submitted.get());
                } else if ms == 110 {
                    submitted.set(submitted.get() + 9_600);
                } else if ms >= 310 && (ms - 310).is_multiple_of(50) {
                    submitted.set(submitted.get() + 2_400);
                }
            }
        },
    );
    let held = clock.borrow().duration_since(start);

    assert!(result.is_ok(), "finished in budget");
    assert!(
        held >= std::time::Duration::from_millis(400),
        "the target sounds at 400 ms; finished at {held:?}"
    );
}
