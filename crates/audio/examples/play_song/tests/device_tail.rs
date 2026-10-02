//! The derived device tail, its orchestrator, and the measured drain target.

use super::*;

#[test]
fn a_stream_error_during_the_device_tail_is_not_a_successful_finish() {
    let mut slept = Vec::new();
    let mut errors_seen = 0_u64;

    let result = wait_for_device_tail(
        std::time::Duration::from_millis(200),
        || {
            // The disconnect lands while the device plays its last buffer:
            // the counter is clean when the drain ended and nonzero after
            // the tail wait.
            errors_seen += 1;
            errors_seen
        },
        |d| slept.push(d),
    );

    assert_eq!(slept, [std::time::Duration::from_millis(200)]);
    assert!(matches!(
        result,
        Err(DrainError::StreamStoppedDuringTail { errors: 1 })
    ));
}

#[test]
fn a_clean_device_tail_finishes_successfully() {
    let result = wait_for_device_tail(std::time::Duration::from_millis(200), || 0, |_| {});

    assert!(result.is_ok());
}

#[test]
fn the_device_tail_is_derived_from_the_advertised_callback_bound() {
    // 12 000 frames at 48 kHz is a quarter second per period, so the
    // host's two queued periods hold half a second: longer than the
    // fixed fallback, which would have clipped it.
    let tail = device_tail_wait(Some(12_000), 48_000);
    assert_eq!(
        tail,
        std::time::Duration::from_millis(500) + DEVICE_TAIL_MARGIN
    );
    assert!(tail > DEVICE_TAIL_FALLBACK);
}

#[test]
fn the_device_tail_covers_every_queued_host_period() {
    // A 160 ms period exceeds the fallback on its own only once the
    // second queued period is counted.
    let tail = device_tail_wait(Some(7_680), 48_000);
    assert_eq!(
        tail,
        std::time::Duration::from_millis(320) + DEVICE_TAIL_MARGIN
    );
}

#[test]
fn an_unknown_callback_bound_falls_back_to_the_fixed_tail() {
    assert_eq!(device_tail_wait(None, 48_000), DEVICE_TAIL_FALLBACK);
    assert_eq!(device_tail_wait(Some(4_096), 0), DEVICE_TAIL_FALLBACK);
}

#[test]
fn the_derived_tail_never_undercuts_the_conservative_floor() {
    // The advertised callback size bounds one callback slice, not the
    // host pipeline's presentation latency: cpal opens the stream with
    // `BufferSize::Default` and its ALSA path then keeps DEFAULT_PERIODS
    // (2) periods queued behind the callback, while JACK advertises
    // min == max == its period. A device advertising 512 frames at 48 kHz
    // must therefore still wait out at least the fixed fallback.
    assert!(device_tail_wait(Some(512), 48_000) >= DEVICE_TAIL_FALLBACK);
}

#[test]
fn an_oversized_advertised_buffer_is_capped_rather_than_slept_out() {
    // ALSA-style backends advertise maxima of seconds; the *selected*
    // callback buffer is far smaller, so waiting the maximum would look
    // like a hang.
    assert_eq!(device_tail_wait(Some(480_000), 48_000), DEVICE_TAIL_MAX);
}

#[test]
fn measured_drain_target_adds_the_settle_margin() {
    assert_eq!(measured_drain_target(100, 8), 108);
    assert_eq!(measured_drain_target(100, 0), 100);
}

#[test]
fn measured_drain_target_saturates_rather_than_overflowing() {
    assert_eq!(measured_drain_target(u64::MAX, 5), u64::MAX);
}

#[test]
fn the_orchestrator_prefers_the_measured_target_when_available() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_secs(1),
    };

    let result = wait_for_device_tail_or_measured(
        Some(4),
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress(4, 4)),
        || 0,
        std::time::Instant::now,
        |_| panic!("the measured target is already reached; must not sleep"),
    );

    assert!(result.is_ok());
}

#[test]
fn the_orchestrator_falls_back_to_the_derived_tail_with_no_measured_target() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(1),
        max_wait: std::time::Duration::from_secs(1),
    };
    let derived_tail = device_tail_wait(Some(12_000), 48_000);
    let slept = Rc::new(RefCell::new(Vec::new()));
    let slept_sleep = Rc::clone(&slept);

    let result = wait_for_device_tail_or_measured(
        None,
        derived_tail,
        48_000,
        &policy,
        || panic!("the measured path must not be consulted once no target is available"),
        || 0,
        std::time::Instant::now,
        move |d| slept_sleep.borrow_mut().push(d),
    );

    assert!(result.is_ok());
    assert_eq!(
        *slept.borrow(),
        vec![std::time::Duration::from_millis(500) + DEVICE_TAIL_MARGIN],
        "the fallback must sleep the single derived tail duration, not poll"
    );
}
