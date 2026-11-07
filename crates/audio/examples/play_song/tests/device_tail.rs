//! The derived device tail, its orchestrator, and the measured drain target.

use super::*;

/// A fake clock: `now` reads it, `sleep` advances it and logs the sleep.
struct FakeClock {
    origin: Instant,
    now: Rc<RefCell<Instant>>,
    slept: Rc<RefCell<Vec<std::time::Duration>>>,
}

impl FakeClock {
    fn new() -> Self {
        let origin = Instant::now();
        Self {
            origin,
            now: Rc::new(RefCell::new(origin)),
            slept: Rc::default(),
        }
    }

    fn now(&self) -> impl FnMut() -> Instant {
        let now = Rc::clone(&self.now);
        move || *now.borrow()
    }

    fn sleep(&self) -> impl FnMut(std::time::Duration) {
        self.sleep_then(|| {})
    }

    /// [`Self::sleep`] that also runs `after` at the end of every sleep.
    fn sleep_then(&self, after: impl Fn()) -> impl FnMut(std::time::Duration) {
        let now = Rc::clone(&self.now);
        let slept = Rc::clone(&self.slept);
        move |d| {
            *now.borrow_mut() += d;
            slept.borrow_mut().push(d);
            after();
        }
    }

    fn elapsed(&self) -> std::time::Duration {
        self.now.borrow().duration_since(self.origin)
    }
}

fn fallback_policy() -> RetryPolicy {
    RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(300),
    }
}

#[test]
fn a_stream_error_during_the_device_tail_is_not_a_successful_finish() {
    let clock = FakeClock::new();
    let mut errors_seen = 0_u64;

    let result = wait_for_device_tail(
        std::time::Duration::from_millis(200),
        48_000,
        0,
        &fallback_policy(),
        || None,
        || {
            errors_seen += 1;
            errors_seen.saturating_sub(1)
        },
        clock.now(),
        clock.sleep(),
    );

    assert!(matches!(
        result,
        Err(DrainError::StreamStoppedDuringTail { errors: 1 })
    ));
    assert_eq!(
        clock.slept.borrow().len(),
        1,
        "the error is reported at the poll after the first sleep, not after the whole tail"
    );
}

#[test]
fn a_clean_device_tail_finishes_successfully() {
    let clock = FakeClock::new();

    let result = wait_for_device_tail(
        std::time::Duration::from_millis(200),
        48_000,
        0,
        &fallback_policy(),
        || None,
        || 0,
        clock.now(),
        clock.sleep(),
    );

    assert!(result.is_ok());
    assert_eq!(clock.elapsed(), std::time::Duration::from_millis(200));
}

#[test]
fn the_device_tail_is_derived_from_the_advertised_callback_bound() {
    let tail = device_tail_wait(Some(12_000), 48_000);
    assert_eq!(
        tail,
        std::time::Duration::from_millis(500) + DEVICE_TAIL_MARGIN
    );
    assert!(tail > DEVICE_TAIL_FALLBACK);
}

#[test]
fn the_device_tail_covers_every_queued_host_period() {
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
    assert!(device_tail_wait(Some(512), 48_000) >= DEVICE_TAIL_FALLBACK);
}

#[test]
fn an_oversized_advertised_buffer_is_capped_rather_than_slept_out() {
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
        0,
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
    // Progress is now consulted during the fallback (a timestamp may become
    // usable mid-tail); with it staying `None` the wait is still bounded at
    // the single derived tail, polled at the policy interval.
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(100),
        max_wait: std::time::Duration::from_secs(1),
    };
    let derived_tail = device_tail_wait(Some(12_000), 48_000);
    let clock = FakeClock::new();
    let polls = Cell::new(0_u32);

    let result = wait_for_device_tail_or_measured(
        None,
        derived_tail,
        48_000,
        0,
        &policy,
        || {
            polls.set(polls.get() + 1);
            None
        },
        || 0,
        clock.now(),
        clock.sleep(),
    );

    assert!(result.is_ok());
    assert_eq!(
        clock.elapsed(),
        std::time::Duration::from_millis(500) + DEVICE_TAIL_MARGIN,
        "the fallback must hold exactly the derived tail, not the policy's max_wait"
    );
    assert!(
        clock.slept.borrow().iter().all(|d| *d <= policy.interval),
        "the fallback must poll at the policy interval"
    );
    assert_eq!(polls.get(), 7, "progress is consulted at every poll");
}

#[test]
fn a_timestamp_usable_during_the_fallback_tail_that_shows_stalled_playback_is_not_success() {
    // Ring drained with no usable timestamp yet: `main` latches `None`.
    // During the derived tail a usable timestamp appears showing nothing
    // sounded and callbacks no longer advancing -- the tail never played.
    let clock = FakeClock::new();
    let became_usable = Rc::new(Cell::new(false));
    let usable = Rc::clone(&became_usable);

    let result = wait_for_device_tail_or_measured(
        None,
        std::time::Duration::from_millis(200),
        48_000,
        0,
        &fallback_policy(),
        || became_usable.get().then(|| progress(0, 4)),
        || 0,
        clock.now(),
        clock.sleep_then(move || usable.set(true)),
    );

    assert!(
        result.is_err(),
        "a stalled callback whose usable timestamp shows the tail unsounded must not finish successfully"
    );
    assert!(
        clock.elapsed() <= std::time::Duration::from_millis(10 + 300),
        "the measured wait's own deadline bounds the failure, held {:?}",
        clock.elapsed()
    );
}

#[test]
fn a_timestamp_usable_during_the_fallback_tail_that_shows_the_tail_sounded_is_success() {
    // The timestamp appears after one 10 ms poll and shows every one of the
    // 100 submitted frames already sounded.
    let clock = FakeClock::new();
    let became_usable = Rc::new(Cell::new(false));
    let usable = Rc::clone(&became_usable);

    let result = wait_for_device_tail_or_measured(
        None,
        std::time::Duration::from_millis(200),
        48_000,
        0,
        &fallback_policy(),
        || became_usable.get().then(|| progress(100, 100)),
        || 0,
        clock.now(),
        clock.sleep_then(move || usable.set(true)),
    );

    assert!(result.is_ok());
    assert_eq!(
        clock.elapsed(),
        std::time::Duration::from_millis(10),
        "a tail already sounded ends the wait without holding the rest of the derived tail"
    );
}

#[test]
fn a_late_timestamp_that_is_catching_up_finishes_once_the_target_sounds() {
    // The timestamp appears short by a frame and the device sounds it by the
    // next poll: the measured wait takes over and ends there, inside the tail.
    let clock = FakeClock::new();
    let became_usable = Rc::new(Cell::new(false));
    let usable = Rc::clone(&became_usable);
    let sounded = Rc::new(Cell::new(99_u64));
    let sounded_read = Rc::clone(&sounded);
    let sounded_sleep = Rc::clone(&sounded);

    let result = wait_for_device_tail_or_measured(
        None,
        std::time::Duration::from_millis(200),
        48_000,
        0,
        &fallback_policy(),
        || {
            became_usable
                .get()
                .then(|| progress(sounded_read.get(), 100))
        },
        || 0,
        clock.now(),
        clock.sleep_then(move || {
            if usable.replace(true) {
                sounded_sleep.set(100);
            }
        }),
    );

    assert!(result.is_ok());
    assert_eq!(clock.elapsed(), std::time::Duration::from_millis(20));
}

#[test]
fn a_timestamp_first_usable_late_in_the_tail_does_not_fail_a_healthy_device() {
    // 48 kHz, callbacks every 10 ms submitting 480 frames (silence after the
    // drain), the device sounding 30 ms behind what is submitted. The
    // timestamp first appears at 190 ms; the silence submitted by then sounds
    // at 220 ms, past the 200 ms derived tail but not a stall.
    let clock = FakeClock::new();
    let origin = clock.origin;
    let now = Rc::clone(&clock.now);
    let usable_from = std::time::Duration::from_millis(190);

    let result = wait_for_device_tail_or_measured(
        None,
        std::time::Duration::from_millis(200),
        48_000,
        0,
        &fallback_policy(),
        || {
            let at = *now.borrow();
            let elapsed = at.duration_since(origin);
            let callbacks = u64::try_from(elapsed.as_millis() / 10).unwrap();
            let submitted = callbacks * 480;
            (elapsed >= usable_from).then(|| {
                progress_reading(submitted.saturating_sub(1_440), submitted, submitted, at)
            })
        },
        || 0,
        clock.now(),
        clock.sleep(),
    );

    assert!(result.is_ok());
    assert_eq!(clock.elapsed(), std::time::Duration::from_millis(220));
}

#[test]
fn a_late_timestamp_target_carries_the_resampler_settle_margin() {
    // Every submitted frame has sounded, but a resampled device still holds
    // 8 interpolated frames of real audio: short of the margin is not done.
    let drained = |sounded: u64| {
        let clock = FakeClock::new();
        let became_usable = Rc::new(Cell::new(false));
        let usable = Rc::clone(&became_usable);
        wait_for_device_tail_or_measured(
            None,
            std::time::Duration::from_millis(200),
            48_000,
            8,
            &fallback_policy(),
            || became_usable.get().then(|| progress(sounded, 100)),
            || 0,
            clock.now(),
            clock.sleep_then(move || usable.set(true)),
        )
    };

    assert!(drained(100).is_err());
    assert!(drained(108).is_ok());
}

#[test]
fn a_stream_error_after_a_timestamp_turns_usable_during_the_fallback_still_fails() {
    let clock = FakeClock::new();
    let became_usable = Rc::new(Cell::new(false));
    let usable = Rc::clone(&became_usable);

    let result = wait_for_device_tail_or_measured(
        None,
        std::time::Duration::from_millis(200),
        48_000,
        0,
        &fallback_policy(),
        || became_usable.get().then(|| progress(100, 100)),
        || u64::from(became_usable.get()),
        clock.now(),
        clock.sleep_then(move || usable.set(true)),
    );

    assert!(matches!(
        result,
        Err(DrainError::StreamStoppedDuringTail { errors: 1 })
    ));
}
