//! Callback cadence, stalls, and aggregate advances seen by a late poll.

use super::*;
use std::time::{Duration, Instant};

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

#[test]
fn a_stall_after_one_small_advance_fails_however_large_the_advertised_buffer() {
    let result = cadence_wait(1_000, 1_000, 1, |ms| ms == 100, None);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

#[test]
fn slow_callbacks_under_the_fallback_tail_take_the_fallback() {
    let result = cadence_wait(200, 1_000, 7_200, |ms| ms % 150 == 0, None);

    assert!(result.is_ok(), "150 ms callbacks are healthy");
}

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

#[test]
fn a_stall_and_resume_across_the_tail_does_not_prove_callbacks_alive() {
    let result = cadence_wait(1_000, 1_000, 480, |ms| ms == 100 || ms == 1_000, None);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

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

#[test]
fn a_large_resumed_buffer_does_not_excuse_the_stall_before_it() {
    let result = cadence_wait(1_000, 1_000, 28_800, |ms| ms == 100 || ms == 900, None);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

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
                *clock.borrow_mut() += std::time::Duration::from_millis(310);
                submitted.set(submitted.get() + 12_000);
            } else {
                *clock.borrow_mut() += duration;
            }
        },
    );

    assert!(result.is_ok(), "stale live callbacks finish the tail");
}

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

/// Models scheduler load or coarse sleep-timer resolution.
fn late_poll_wait(
    interval_us: u64,
    step_us: u64,
    period_ms: u64,
    phase_ms: u64,
    tail_ms: u64,
) -> (Result<(), DrainError>, Duration) {
    let policy = RetryPolicy {
        interval: Duration::from_micros(interval_us),
        max_wait: Duration::from_secs(1),
    };
    let start = Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);
    let landed =
        |us: u64| (us + period_ms * 1_000).saturating_sub(phase_ms * 1_000) / (period_ms * 1_000);
    let result = wait_for_measured_tail(
        4,
        Duration::from_millis(tail_ms),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |_| {
            let before =
                u64::try_from(clock.borrow().duration_since(start).as_micros()).unwrap_or(0);
            *clock.borrow_mut() += Duration::from_micros(step_us);
            let callbacks = landed(before + step_us) - landed(before);
            submitted.set(submitted.get() + callbacks * period_ms * 48);
        },
    );
    let held = clock.borrow().duration_since(start);
    (result, held)
}

#[test]
fn systematically_late_polls_with_healthy_callbacks_take_the_fallback() {
    let (result, held) = late_poll_wait(4_167, 15_600, 50, 23, 250);
    assert!(result.is_ok(), "15.6 ms polls; finished at {held:?}");
    assert!(
        held < Duration::from_millis(300),
        "held to {held:?}, past the 250 ms tail"
    );

    let (result, held) = late_poll_wait(4_167, 12_000, 50, 31, 1_000);
    assert!(
        result.is_ok(),
        "12 ms polls at the capped tail; ended at {held:?}"
    );
}

#[test]
fn a_cadence_seen_at_slow_polling_lapses_once_polling_is_prompt() {
    for stop_ms in [500_u64, u64::MAX] {
        let policy = RetryPolicy {
            interval: Duration::from_millis(10),
            max_wait: Duration::from_secs(1),
        };
        let start = Instant::now();
        let clock = Rc::new(RefCell::new(start));
        let ms = || u64::try_from(clock.borrow().duration_since(start).as_millis()).unwrap_or(0);

        let result = wait_for_measured_tail(
            4,
            Duration::from_millis(600),
            48_000,
            &policy,
            || Some(progress(0, 4 + ms().min(stop_ms) / 10 * 480)),
            || 0,
            || *clock.borrow(),
            |_| {
                let step = if ms() < 500 { 100 } else { 10 };
                *clock.borrow_mut() += Duration::from_millis(step);
            },
        );

        if stop_ms == u64::MAX {
            assert!(result.is_ok(), "live callbacks take the fallback");
        } else {
            assert!(
                matches!(result, Err(DrainError::MeasuredTailTimedOut { .. })),
                "callbacks stopped at {stop_ms} ms must not read as alive"
            );
        }
    }
}
