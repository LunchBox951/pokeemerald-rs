//! Callback cadence, stalls, and aggregate advances seen by a late poll.

use super::*;
use std::time::{Duration, Instant};

/// Drive `wait_for_measured_tail` with a 10 ms poll, stale timestamps, and
/// submitted frames advancing by `frames` whenever `advance_at` says so.
fn cadence_wait(
    tail_ms: u64,
    max_wait_ms: u64,
    frames: u64,
    advance_at: impl Fn(u128) -> bool,
    oversleep_from_ms: Option<u128>,
) -> Result<(), DrainError> {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(max_wait_ms),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);
    wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(tail_ms),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            let elapsed = clock.borrow().duration_since(start).as_millis();
            let step = if oversleep_from_ms.is_some_and(|from| elapsed >= from) {
                std::time::Duration::from_millis(30)
            } else {
                duration
            };
            *clock.borrow_mut() += step;
            let now_ms = clock.borrow().duration_since(start).as_millis();
            if advance_at(now_ms) {
                submitted.set(submitted.get() + frames);
            }
        },
    )
}

/// The cadence comes from observed advances, so a huge advertised buffer
/// range cannot excuse a stall after one small advance.
#[test]
fn a_stall_after_one_small_advance_fails_however_large_the_advertised_buffer() {
    let result = cadence_wait(1_000, 1_000, 1, |ms| ms == 100, None);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

/// Unknown buffer range: the 200 ms fallback tail with 150 ms callbacks
/// (7 200 frames at 48 kHz) must still take the fallback.
#[test]
fn slow_callbacks_under_the_fallback_tail_take_the_fallback() {
    let result = cadence_wait(200, 1_000, 7_200, |ms| ms % 150 == 0, None);

    assert!(result.is_ok(), "150 ms callbacks are healthy");
}

/// The capped tail equals `max_wait`, so its qualifying poll can land
/// several intervals late; the first such poll finishes, and one without
/// live callbacks times out.
#[test]
fn the_first_qualifying_poll_after_the_capped_deadline_finishes() {
    let live = cadence_wait(200, 200, 1, |ms| ms % 10 == 0, Some(180));
    assert!(live.is_ok(), "a live capped tail must finish");

    let stalled = cadence_wait(200, 200, 1, |ms| ms <= 100 && ms % 10 == 0, Some(180));
    assert!(matches!(
        stalled,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

/// Callbacks that advance 250 ms of playback and then stall must not read
/// as fresh because the poll resumed after the deadline.
#[test]
fn an_aggregate_advance_seen_at_a_late_poll_is_not_fresh() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_secs(1),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            let elapsed = clock.borrow().duration_since(start);
            if elapsed >= std::time::Duration::from_millis(100) {
                // The sleep overruns past the deadline; the only callbacks
                // that ran covered 250 ms (12 000 frames at 48 kHz).
                *clock.borrow_mut() += std::time::Duration::from_secs(1);
                submitted.set(submitted.get() + 12_000);
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

/// One 300 ms polling pause folds 10 ms callbacks into one aggregate;
/// callbacks that later stall 300 ms before the deadline must read as
/// stale rather than inherit that aggregate as their cadence.
#[test]
fn an_old_aggregate_advance_does_not_widen_the_later_cadence() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);
    let mut first_sleep = true;

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_secs(1),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            if std::mem::take(&mut first_sleep) {
                // The poll pauses 300 ms while thirty 10 ms callbacks run.
                *clock.borrow_mut() += std::time::Duration::from_millis(300);
                submitted.set(submitted.get() + 30 * 480);
                return;
            }
            *clock.borrow_mut() += duration;
            if clock.borrow().duration_since(start) <= std::time::Duration::from_millis(700) {
                submitted.set(submitted.get() + 480);
            }
        },
    );

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

/// Stale timestamps with one callback early in the tail and the next only
/// at the deadline: the long stall is neither sustained progress nor the
/// callback cadence, so the resumed callback alone cannot finish the drain.
#[test]
fn a_stall_and_resume_across_the_tail_does_not_prove_callbacks_alive() {
    let result = cadence_wait(1_000, 1_000, 480, |ms| ms == 100 || ms == 1_000, None);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

/// Callbacks that stall past half the tail, resume briefly, and stop again
/// must not read as alive on the strength of the stall gap itself.
#[test]
fn the_stall_gap_does_not_survive_the_cadence_reset() {
    let result = cadence_wait(
        1_000,
        1_000,
        1,
        |ms| matches!(ms, 50 | 600 | 700 | 800 | 850),
        None,
    );

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

/// Stale callbacks at 100 ms and 900 ms that each cover 600 ms of playback:
/// the idle gap between them is a stall, and a large resumed buffer does
/// not excuse it.
#[test]
fn a_large_resumed_buffer_does_not_excuse_the_stall_before_it() {
    let result = cadence_wait(1_000, 1_000, 28_800, |ms| ms == 100 || ms == 900, None);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

/// A poll thread descheduled for the whole `max_wait` sees one aggregate
/// advance from stale callbacks; it is judged at the deadline poll
/// itself, with no following poll to confirm it.
#[test]
fn a_lone_aggregate_at_the_deadline_poll_is_judged_there() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(300),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);
    let mut first_sleep = true;

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            if std::mem::take(&mut first_sleep) {
                // The poll thread sleeps through the whole budget; the
                // callbacks that ran covered 250 ms.
                *clock.borrow_mut() += std::time::Duration::from_millis(310);
                submitted.set(submitted.get() + 12_000);
            } else {
                *clock.borrow_mut() += duration;
            }
        },
    );

    assert!(result.is_ok(), "stale live callbacks finish the tail");
}

/// Drive `wait_for_measured_tail` with stale timestamps, silent callbacks
/// until 400 ms, then a 100 ms buffer every 100 ms. With `late_poll` the
/// poll at 630 ms oversleeps to 700 ms. Returns the result and the time the
/// wait ended.
fn resumed_tail_wait(max_wait_ms: u64, late_poll: bool) -> (Result<(), DrainError>, Duration) {
    let policy = RetryPolicy {
        interval: Duration::from_millis(10),
        max_wait: Duration::from_millis(max_wait_ms),
    };
    let start = Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let result = wait_for_measured_tail(
        48_008,
        Duration::from_millis(250),
        48_000,
        &policy,
        || Some(progress(28_800, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            let before = clock.borrow().duration_since(start).as_millis();
            *clock.borrow_mut() += if late_poll && before == 630 {
                Duration::from_millis(70)
            } else {
                duration
            };
            let ms = clock.borrow().duration_since(start).as_millis();
            if ms >= 400 && ms.is_multiple_of(100) {
                submitted.set(submitted.get() + 4_800);
            }
        },
    );
    let elapsed = clock.borrow().duration_since(start);
    (result, elapsed)
}

/// The audio the resumed callbacks submit sounds a full tail after they
/// resume, not a tail after the wait began.
#[test]
fn the_tail_restarts_from_callbacks_resumed_after_a_stall() {
    let (result, elapsed) = resumed_tail_wait(1_000, false);
    assert!(result.is_ok());
    assert!(
        elapsed >= Duration::from_millis(650),
        "finished at {elapsed:?}"
    );
    assert!(elapsed <= Duration::from_millis(1_010));
}

/// The restarted tail still has to fit the original budget.
#[test]
fn a_restarted_tail_must_fit_inside_the_original_budget() {
    for (budget, late) in [(600, false), (640, true)] {
        let (result, elapsed) = resumed_tail_wait(budget, late);
        assert!(
            matches!(result, Err(DrainError::MeasuredTailTimedOut { .. })),
            "budget={budget} elapsed={elapsed:?}"
        );
    }
}

/// A poll paused 500 ms while 10 ms stale callbacks ran folds them into one
/// aggregate; when the callbacks then stop, that aggregate must not stay the
/// cadence that keeps a 500 ms silence reading as alive at the deadline.
#[test]
fn an_aggregate_from_a_polling_pause_does_not_excuse_the_silence_after_it() {
    for pause_at in [0_u128, 10, 100, 250] {
        let policy = RetryPolicy {
            interval: Duration::from_millis(10),
            max_wait: Duration::from_secs(1),
        };
        let start = Instant::now();
        let clock = Rc::new(RefCell::new(start));
        let elapsed = || clock.borrow().duration_since(start).as_millis();

        let result = wait_for_measured_tail(
            4,
            Duration::from_secs(1),
            48_000,
            &policy,
            // 480 frames (10 ms) per callback until the pause ends, then none.
            || {
                let ran = elapsed().min(pause_at + 500) / 10;
                Some(progress(0, 4 + u64::try_from(ran).unwrap_or(0) * 480))
            },
            || 0,
            || *clock.borrow(),
            |duration| {
                let step = if elapsed() == pause_at {
                    Duration::from_millis(500)
                } else {
                    duration
                };
                *clock.borrow_mut() += step;
            },
        );

        assert!(
            matches!(result, Err(DrainError::MeasuredTailTimedOut { .. })),
            "a pause at {pause_at} ms must not hide the stall after it"
        );
    }
}
