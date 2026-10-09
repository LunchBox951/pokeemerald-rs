//! The measured wait's direct outcomes: reached, errored, stalled.

use super::*;

#[test]
fn measured_wait_succeeds_immediately_once_the_target_is_already_reached() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_secs(1),
    };

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(4, 4)),
        || 0,
        std::time::Instant::now,
        |_| panic!("the target is already reached; must not sleep"),
    );

    assert!(result.is_ok());
}

#[test]
fn measured_wait_reports_a_stream_error_before_the_target_is_reached() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_mins(1),
    };
    let start = std::time::Instant::now();

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, 0)), // stationary, well short of the target
        || 1,                    // already unhealthy
        || start,
        |_| panic!("a stream error must abort before any retry sleep"),
    );

    assert!(matches!(
        result,
        Err(DrainError::StreamStoppedDuringTail { errors: 1 })
    ));
}

#[test]
fn measured_wait_reports_a_stalled_target_at_the_deadline() {
    // A position still short of the target when the bound elapses is a
    // stalled device, not a finish: the tool must not exit successfully
    // with the final audio unconfirmed.
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(30),
    };
    let clock = Rc::new(RefCell::new(std::time::Instant::now()));
    let sleeps = Cell::new(0_u32);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(0, 0)), // never reaches the target
        || 0,
        || *clock.borrow(),
        |duration| {
            sleeps.set(sleeps.get() + 1);
            *clock.borrow_mut() += duration;
        },
    );

    assert!(
        matches!(
            result,
            Err(DrainError::MeasuredTailTimedOut {
                sounded: 0,
                target: 4
            })
        ),
        "an unmet measured target must not exit successfully"
    );
    assert!(
        sleeps.get() > 0,
        "must have retried at least once before giving up"
    );
}

#[test]
fn a_poll_landing_on_the_deadline_is_the_last() {
    // A poll exactly at `max_wait` with the target still short and no
    // fallback due is the last one: a target that sounds only during a
    // further interval was confirmed past the budget and must not count.
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(30),
    };
    let started = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(started));
    let sleeps = Cell::new(0_u32);

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || {
            // Sounds only after the deadline poll at 30 ms.
            let late = *clock.borrow() > started + policy.max_wait;
            Some(progress(if late { 4 } else { 0 }, 0))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            sleeps.set(sleeps.get() + 1);
            *clock.borrow_mut() += duration;
        },
    );

    assert!(
        matches!(
            result,
            Err(DrainError::MeasuredTailTimedOut {
                sounded: 0,
                target: 4
            })
        ),
        "a target reached only after max_wait must not finish"
    );
    assert_eq!(sleeps.get(), 3, "no sleep past the deadline poll");
}

/// A 30 ms budget polled every 10 ms whose third sleep oversleeps to 40 ms,
/// where a usable callback at `callback_ms` first reports the target sounded.
fn overslept_measured_wait(callback_ms: u64) -> Result<(), DrainError> {
    use std::time::Duration;

    let policy = RetryPolicy {
        interval: Duration::from_millis(10),
        max_wait: Duration::from_millis(30),
    };
    let started = Instant::now();
    let clock = Rc::new(RefCell::new(started));
    let sleeps = Cell::new(0_u32);
    let callback_at = started + Duration::from_millis(callback_ms);
    let result = wait_for_measured_tail(
        4,
        Duration::from_millis(200),
        48_000,
        &policy,
        || {
            Some(if *clock.borrow() >= callback_at {
                progress_reading(4, 8, 8, callback_at)
            } else {
                progress_reading(0, 8, 8, started)
            })
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            sleeps.set(sleeps.get() + 1);
            let extra = if sleeps.get() == 3 {
                Duration::from_millis(10)
            } else {
                Duration::ZERO
            };
            *clock.borrow_mut() += duration + extra;
        },
    );
    assert_eq!(sleeps.get(), 3);
    assert_eq!(
        clock.borrow().duration_since(started),
        Duration::from_millis(40)
    );
    result
}

#[test]
fn an_overslept_poll_rejects_completion_after_the_deadline() {
    // Polls at 0/10/20 ms are short; completion at 35 ms is seen at 40 ms.
    assert!(matches!(
        overslept_measured_wait(35),
        Err(DrainError::MeasuredTailTimedOut {
            sounded: 4,
            target: 4
        })
    ));
}

#[test]
fn an_overslept_poll_accepts_completion_observed_in_budget() {
    // Completion at 25 ms is first polled at 40 ms, but fits the 30 ms budget.
    assert!(overslept_measured_wait(25).is_ok());
}
