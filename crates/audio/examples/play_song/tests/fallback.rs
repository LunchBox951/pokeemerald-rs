//! The derived-tail fallback for stale timestamps: basic liveness and the deadline.

use super::*;

fn stale_timestamp_wait(
    submitted_advances: bool,
    errors: u64,
) -> (Result<(), DrainError>, std::time::Duration) {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(300),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);
    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || {
            if clock.borrow().duration_since(start) > std::time::Duration::from_millis(50) {
                errors
            } else {
                0
            }
        },
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            if submitted_advances {
                submitted.set(submitted.get() + 1);
            }
        },
    );
    let elapsed = clock.borrow().duration_since(start);
    (result, elapsed)
}

#[test]
fn stale_timestamps_with_live_callbacks_fall_back_to_the_derived_tail() {
    let (result, elapsed) = stale_timestamp_wait(true, 0);

    assert!(result.is_ok(), "a healthy drain must not fail");
    assert!(
        elapsed >= std::time::Duration::from_millis(200),
        "must hold the stream for the derived tail, held {elapsed:?}"
    );
}

#[test]
fn stale_timestamps_with_stalled_callbacks_still_time_out() {
    let (result, _) = stale_timestamp_wait(false, 0);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut {
            sounded: 0,
            target: 4
        })
    ));
}

#[test]
fn callbacks_that_stall_partway_through_the_derived_tail_still_time_out() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(300),
    };
    let clock = Rc::new(RefCell::new(std::time::Instant::now()));
    let submitted = Cell::new(4_u64);
    let sleeps = Cell::new(0_u32);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            sleeps.set(sleeps.get() + 1);
            // Callbacks advance through the measured wait and the first
            // stretch of the tail, then stop.
            if sleeps.get() < 8 {
                submitted.set(submitted.get() + 1);
            }
        },
    );

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

/// `policy.max_wait` is documented as bounding how long the stream is
/// held open: a callback that advances and then stalls must not keep it
/// open past that bound.
#[test]
fn a_stalling_callback_does_not_hold_the_stream_past_max_wait() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(30),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);
    let sleeps = Cell::new(0_u32);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            sleeps.set(sleeps.get() + 1);
            if sleeps.get() < 5 {
                submitted.set(submitted.get() + 1);
            }
        },
    );
    let held = clock.borrow().duration_since(start);

    assert!(result.is_err(), "a stalled drain must not succeed");
    assert!(
        held <= policy.max_wait + policy.interval,
        "max_wait {:?} must bound the hold, held {held:?}",
        policy.max_wait
    );
}

/// One callback landing late in the tail is not sustained liveness.
#[test]
fn a_single_late_callback_does_not_prove_callbacks_alive() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(300),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            if clock.borrow().duration_since(start) == std::time::Duration::from_millis(190) {
                submitted.set(submitted.get() + 1);
            }
        },
    );

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

/// A poll that oversleeps past `max_wait` reports the timeout even when
/// the callbacks look alive.
#[test]
fn a_delayed_poll_past_max_wait_does_not_take_the_stale_tail_exit() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(190),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            let elapsed = clock.borrow().duration_since(start);
            let step = if elapsed >= std::time::Duration::from_millis(180) {
                std::time::Duration::from_millis(30)
            } else {
                duration
            };
            *clock.borrow_mut() += step;
            submitted.set(submitted.get() + 1);
        },
    );

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}

/// A large advertised callback puts the derived tail well above the
/// 10 ms cadence the other tests use; healthy callbacks that land once per
/// period must still take the fallback.
#[test]
fn slow_healthy_callbacks_still_take_the_derived_tail_fallback() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(370),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            if clock
                .borrow()
                .duration_since(start)
                .as_millis()
                .is_multiple_of(160)
            {
                submitted.set(submitted.get() + 1);
            }
        },
    );

    assert!(result.is_ok(), "a healthy 160 ms cadence must not fail");
}

#[test]
fn a_derived_tail_equal_to_max_wait_still_takes_the_fallback() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(200),
    };
    let clock = Rc::new(RefCell::new(std::time::Instant::now()));
    let submitted = Cell::new(4_u64);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            submitted.set(submitted.get() + 1);
        },
    );

    assert!(result.is_ok(), "the capped production tail must not fail");
}

#[test]
fn sleep_overshoot_at_the_capped_tail_still_takes_the_fallback() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(200),
    };
    let clock = Rc::new(RefCell::new(std::time::Instant::now()));
    let submitted = Cell::new(4_u64);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            // Every sleep overshoots by 3 ms, so no poll lands exactly on
            // the deadline.
            *clock.borrow_mut() += duration + std::time::Duration::from_millis(3);
            submitted.set(submitted.get() + 1);
        },
    );

    assert!(result.is_ok(), "a few ms of overshoot must not fail");
}

#[test]
fn a_capped_tail_with_a_600_ms_callback_period_takes_the_fallback() {
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
            *clock.borrow_mut() += duration;
            if clock.borrow().duration_since(start).as_millis() == 600 {
                // One 600 ms callback's worth of frames at 48 kHz.
                submitted.set(submitted.get() + 28_800);
            }
        },
    );

    assert!(result.is_ok(), "a healthy slow callback must not fail");
}

#[test]
fn a_stream_error_during_the_derived_tail_fallback_fails_the_drain() {
    let (result, _) = stale_timestamp_wait(true, 1);

    assert!(matches!(
        result,
        Err(DrainError::StreamStoppedDuringTail { errors: 1 })
    ));
}

/// A 600 ms gap between healthy 600 ms callbacks is one period, not a stall,
/// even though it is over half a capped one-second tail.
#[test]
fn repeated_phase_shifted_600_ms_callbacks_at_a_capped_tail_take_the_fallback() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(28_800_u64);

    let result = wait_for_measured_tail(
        28_800,
        std::time::Duration::from_secs(1),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            // Healthy callbacks every 600 ms, phase-shifted to 100 and 700 ms.
            let ms = clock.borrow().duration_since(start).as_millis();
            if ms == 100 || ms == 700 {
                submitted.set(submitted.get() + 28_800);
            }
        },
    );

    assert!(
        !matches!(result, Err(DrainError::MeasuredTailTimedOut { .. })),
        "steady 600 ms callbacks must not time out"
    );
}

/// Callbacks silent from the start of the wait, then one large stale buffer
/// late in the tail: the idle span before it is a stall, not cadence, so the
/// lone buffer does not prove the callbacks alive.
#[test]
fn an_initial_idle_span_before_one_large_late_buffer_is_a_stall() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(300),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(4_u64);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, submitted.get())),
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            if clock.borrow().duration_since(start) == std::time::Duration::from_millis(190) {
                // One 100 ms callback's worth of frames at 48 kHz.
                submitted.set(submitted.get() + 4_800);
            }
        },
    );
    let held = clock.borrow().duration_since(start);

    assert!(
        matches!(result, Err(DrainError::MeasuredTailTimedOut { .. })),
        "callbacks idle until 190 ms then one buffer finished at {held:?}"
    );
}
