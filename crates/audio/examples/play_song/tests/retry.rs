//! The push retry loop, frame-deadline waits, and ring drain.

use super::*;

/// Pacing must subtract render/push work already spent from each wait
/// rather than sleeping a full period on top of it, and a late iteration
/// must not let that deficit compound into the next one.
#[test]
fn frame_deadline_waits_subtract_work_without_accumulating_drift() {
    let period = std::time::Duration::from_millis(10);
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let sleeps = Rc::new(RefCell::new(Vec::new()));
    let mut next_deadline = start + period;

    // Five iterations whose simulated work comfortably fits under the
    // period, the steady-state case.
    for _ in 0..5 {
        *clock.borrow_mut() += std::time::Duration::from_millis(3);
        let clock_now = Rc::clone(&clock);
        let clock_sleep = Rc::clone(&clock);
        let sleeps_sleep = Rc::clone(&sleeps);
        wait_for_frame_deadline(
            &mut next_deadline,
            period,
            move || *clock_now.borrow(),
            move |duration| {
                sleeps_sleep.borrow_mut().push(duration);
                *clock_sleep.borrow_mut() += duration;
            },
        );
    }
    assert_eq!(
        *sleeps.borrow(),
        vec![std::time::Duration::from_millis(7); 5],
        "work already spent must be subtracted from each wait, not added on top of it"
    );
    assert_eq!(
        *clock.borrow(),
        start + 5 * period,
        "five rounds of work-plus-wait must land on exactly five periods elapsed, proving no \
         drift accumulated"
    );
    assert_eq!(next_deadline, start + 6 * period);

    // A sixth iteration whose work overruns the period entirely.
    sleeps.borrow_mut().clear();
    *clock.borrow_mut() += std::time::Duration::from_millis(15);
    let clock_now = Rc::clone(&clock);
    let sleeps_sleep = Rc::clone(&sleeps);
    wait_for_frame_deadline(
        &mut next_deadline,
        period,
        move || *clock_now.borrow(),
        move |duration| sleeps_sleep.borrow_mut().push(duration),
    );

    assert_eq!(
        *sleeps.borrow(),
        vec![std::time::Duration::ZERO],
        "a late iteration must not sleep a negative duration"
    );
    assert_eq!(
        next_deadline,
        start + 7 * period,
        "the cadence must still advance by exactly one period from the missed deadline, not \
         reset from the late clock reading"
    );
}

#[test]
fn a_stream_error_aborts_the_retry_without_waiting_out_the_deadline() {
    let push_calls = Cell::new(0_u32);
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_mins(1),
    };
    let samples = [0.0_f32; 4];
    let start = std::time::Instant::now();

    let result = push_frame(
        &samples,
        &policy,
        |_chunk| {
            push_calls.set(push_calls.get() + 1);
            0
        },
        // Healthy on the pre-push check, unhealthy immediately after —
        // the "error lands mid-push" case the double check exists for.
        || u64::from(push_calls.get() > 0),
        || start,
        |_| panic!("a stream error must abort before any retry sleep"),
    );

    assert!(
        matches!(
            result,
            Err(PushError::StreamStopped {
                errors: 1,
                dropped: 4
            })
        ),
        "expected a StreamStopped error dropping the whole frame"
    );
    assert_eq!(push_calls.get(), 1, "must not retry once the stream errors");
}

#[test]
fn a_stalled_ring_with_no_stream_error_gives_up_at_the_deadline() {
    // A real (but headless) ring buffer, filled completely and never
    // drained — `Producer::push` genuinely returns 0 forever, the same
    // as a stopped device callback that never reports a stream error.
    let output = AudioOutput::null(1);
    let producer = output.producer();
    assert_eq!(producer.push(&[0.0; 2]), 2, "fill the null ring solid");

    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(30),
    };
    let clock = Rc::new(RefCell::new(std::time::Instant::now()));
    let sleeps = Cell::new(0_u32);

    let result = push_frame(
        &[0.0_f32; 2],
        &policy,
        |chunk| producer.push(chunk),
        || output.stream_errors(),
        || *clock.borrow(),
        |duration| {
            sleeps.set(sleeps.get() + 1);
            *clock.borrow_mut() += duration;
        },
    );

    assert!(
        matches!(result, Err(PushError::DeadlineExceeded { dropped: 2 })),
        "a permanently full ring with no stream error must time out, not hang"
    );
    assert_eq!(output.stream_errors(), 0, "the null backend never errors");
    assert!(
        sleeps.get() > 0,
        "must have retried at least once before giving up"
    );
}

#[test]
fn a_push_that_completes_before_the_deadline_succeeds() {
    let output = AudioOutput::null(64);
    let producer = output.producer();
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_secs(1),
    };

    let result = push_frame(
        &[0.0_f32; 4],
        &policy,
        |chunk| producer.push(chunk),
        || output.stream_errors(),
        std::time::Instant::now,
        |_| panic!("plenty of room; must not need to retry"),
    );

    assert!(result.is_ok());
}

#[test]
fn wait_for_drain_succeeds_immediately_once_the_ring_is_already_empty() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_secs(1),
    };

    let result = wait_for_drain(
        4,
        &policy,
        || 4, // fully free: nothing queued
        || 0,
        std::time::Instant::now,
        |_| panic!("an already-empty ring must not need to retry"),
    );

    assert!(result.is_ok());
}

#[test]
fn an_empty_ring_with_a_stream_error_is_not_a_successful_finish() {
    // A callback that dequeues the last samples and then reports an
    // asynchronous device failure must not read as a successful drain
    // just because the ring happens to be empty.
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_mins(1),
    };
    let start = std::time::Instant::now();

    let result = wait_for_drain(
        4,
        &policy,
        || 4, // fully free: the ring drained
        || 1, // but the stream is already unhealthy
        || start,
        |_| panic!("a stream error must abort before any retry sleep"),
    );

    assert!(
        matches!(
            result,
            Err(DrainError::StreamStopped {
                errors: 1,
                remaining: 0
            })
        ),
        "an empty ring must not mask a reported stream error"
    );
}

#[test]
fn an_error_that_lands_exactly_as_the_ring_reports_empty_is_still_caught() {
    // The real race this guards: the callback drains the last sample and
    // raises a stream error in the same instant. Here `available_space`
    // itself is what makes the error visible, so a `stream_errors` read
    // taken *before* `available_space` would still observe the old,
    // healthy count and wrongly report success once it sees the ring
    // empty.
    let errors = Rc::new(Cell::new(0_u64));
    let errors_probe = Rc::clone(&errors);
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_mins(1),
    };
    let start = std::time::Instant::now();

    let result = wait_for_drain(
        4,
        &policy,
        move || {
            errors_probe.set(1);
            4 // fully free: the ring drained in the same instant
        },
        move || errors.get(),
        || start,
        |_| panic!("a stream error must abort before any retry sleep"),
    );

    assert!(
        matches!(
            result,
            Err(DrainError::StreamStopped {
                errors: 1,
                remaining: 0
            })
        ),
        "an error surfacing exactly as the ring empties must not be missed"
    );
}

#[test]
fn wait_for_drain_reports_a_stream_error_instead_of_waiting_out_the_deadline() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_mins(1),
    };
    let start = std::time::Instant::now();

    let result = wait_for_drain(
        4,
        &policy,
        || 0, // the ring never drains
        || 1, // already unhealthy
        || start,
        |_| panic!("a stream error must abort before any retry sleep"),
    );

    assert!(
        matches!(
            result,
            Err(DrainError::StreamStopped {
                errors: 1,
                remaining: 4
            })
        ),
        "expected a StreamStopped error naming every sample still queued"
    );
}

#[test]
fn wait_for_drain_times_out_when_the_ring_never_empties_and_reports_no_stream_error() {
    // A real (but headless) ring buffer, filled completely and never
    // drained: `available_space` genuinely stays at 0 forever, the same
    // as a device callback that stopped consuming without ever
    // reporting a stream error. Checking only `stream_errors() == 0`
    // here would declare success regardless — this is exactly the case
    // that let a stopped callback pass as a successful finish.
    let output = AudioOutput::null(1);
    let producer = output.producer();
    assert_eq!(producer.push(&[0.0; 2]), 2, "fill the null ring solid");

    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(30),
    };
    let clock = Rc::new(RefCell::new(std::time::Instant::now()));

    let result = wait_for_drain(
        2,
        &policy,
        || producer.available_space(),
        || output.stream_errors(),
        || *clock.borrow(),
        |duration| *clock.borrow_mut() += duration,
    );

    assert!(
        matches!(result, Err(DrainError::DeadlineExceeded { remaining: 2 })),
        "a permanently full ring with no stream error must time out, not report success"
    );
    assert_eq!(output.stream_errors(), 0, "the null backend never errors");
}
