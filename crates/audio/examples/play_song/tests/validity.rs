use super::*;

#[test]
fn a_usable_then_stale_transition_keeps_the_capped_budget() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(100_u64);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        100,
        std::time::Duration::from_secs(1),
        48_000,
        &policy,
        || {
            let ms = clock.borrow().duration_since(start).as_millis();
            Some(stamps.progress(
                if ms < 300 {
                    u64::try_from(ms / 10).unwrap_or(0)
                } else {
                    30
                },
                submitted.get(),
                usable.get(),
                *clock.borrow(),
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

#[test]
fn a_fresh_sounded_estimate_below_target_is_not_overridden_by_the_tail() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(48_000_u64 - 19_200);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        std::time::Duration::from_millis(250),
        48_000,
        &policy,
        || {
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
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

#[test]
fn a_preempted_sounded_store_is_not_stale_evidence() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(48_000_u64 - 19_200);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || {
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            let ms = clock.borrow().duration_since(start).as_millis();
            if ms == 50 {
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

fn torn_snapshot_wait(sounded_lags: bool) -> (Result<(), DrainError>, std::time::Duration) {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(48_000_u64 - 19_200);
    let usable = Cell::new(0_u64);
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
                stamps.progress(seen, submitted.get(), usable.get(), *clock.borrow())
            } else {
                stamps.progress(sounded.get(), seen, usable.get(), *clock.borrow())
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
            lagging.set((actual, current));
        },
    );
    let elapsed = clock.borrow().duration_since(start);
    (result, elapsed)
}

#[test]
fn a_submitted_advance_seen_before_its_sounded_update_is_not_stale() {
    let (result, elapsed) = torn_snapshot_wait(true);

    assert!(result.is_ok());
    assert!(
        elapsed >= std::time::Duration::from_millis(400),
        "finished at {elapsed:?}, before the measured target sounded"
    );
}

#[test]
fn a_usable_mark_seen_before_its_submitted_advance_is_not_stale() {
    let (result, elapsed) = torn_snapshot_wait(false);

    assert!(result.is_ok());
    assert!(
        elapsed >= std::time::Duration::from_millis(400),
        "finished at {elapsed:?}, before the measured target sounded"
    );
}

#[test]
fn usable_estimates_that_stay_flat_are_not_stale_timestamps() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(500),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(4_800_u64);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        9_600,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(stamps.progress(100, submitted.get(), usable.get(), *clock.borrow())),
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

#[test]
fn a_lone_aggregate_with_usable_timestamps_times_out_at_the_deadline() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(300),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(4_u64);
    let usable = Cell::new(0_u64);
    let mut first_sleep = true;

    let result = wait_for_measured_tail(
        4,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(stamps.progress(0, submitted.get(), usable.get(), *clock.borrow())),
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

#[test]
fn a_usable_mark_published_before_the_first_snapshot_is_not_stale_evidence() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(28_800_u64);
    let usable = Cell::new(57_600_u64);

    let result = wait_for_measured_tail(
        57_600,
        DEVICE_TAIL_FALLBACK,
        48_000,
        &policy,
        || {
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            let ms = clock.borrow().duration_since(start).as_millis();
            if ms == 10 {
                submitted.set(57_600);
            } else if ms % 200 == 10 {
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

#[test]
fn a_usable_mark_seen_before_its_submitted_advance_clears_stale_evidence() {
    use std::time::{Duration, Instant};

    let policy = RetryPolicy {
        interval: Duration::from_millis(10),
        max_wait: Duration::from_secs(1),
    };
    let start = Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(28_800_u64);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        Duration::from_millis(250),
        48_000,
        &policy,
        || {
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            match clock.borrow().duration_since(start).as_millis() {
                100 | 200 => submitted.set(submitted.get() + 4_800),
                250 => usable.set(62_400),
                260 => submitted.set(62_400),
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
        || {
            Some(progress_reading(
                0,
                submitted.get(),
                usable.get(),
                start + std::time::Duration::from_millis(100),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            if std::mem::take(&mut first_sleep) {
                *clock.borrow_mut() += std::time::Duration::from_millis(310);
                usable.set(submitted.get() + 480);
                submitted.set(submitted.get() + 480 + 9_600);
            } else {
                *clock.borrow_mut() += duration;
            }
        },
    );

    assert!(result.is_ok(), "the stale suffix finishes the tail");
}

#[test]
fn an_in_flight_mark_does_not_prove_a_usable_callback_in_the_submitted_span() {
    let snapshot = FirstSeen::default().progress(60, 200, 300, Instant::now());
    assert!(!usable_callback_in_span(
        100,
        snapshot.submitted_frames,
        snapshot.usable_through_frames,
    ));
    assert!(usable_callback_in_span(100, 300, 300));
    assert!(!usable_callback_in_span(300, 400, 300));
}

fn late_usable_wait(delay_ms: u64) -> (Result<(), DrainError>, std::time::Duration) {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(0_u64);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || {
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
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
    let stamps = FirstSeen::default();
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(0_u64);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        std::time::Duration::from_millis(250),
        48_000,
        &policy,
        || {
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
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

#[test]
fn usable_readings_consistent_with_the_tail_keep_its_finish() {
    let (result, held) = valid_then_stale_wait(200, 1_000);

    assert!(result.is_ok());
    assert_eq!(held, std::time::Duration::from_millis(250));
}

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

#[test]
fn a_usable_reading_owing_past_max_wait_times_out() {
    let (result, held) = valid_then_stale_wait(350, 300);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { target: 48_000, .. })
    ));
    assert_eq!(held, std::time::Duration::from_millis(300));
}

fn entry_reading_wait(
    submitted: u64,
    mark: u64,
    sounded: u64,
    read_ago_ms: u64,
    max_wait_ms: u64,
) -> (Result<(), DrainError>, std::time::Duration) {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(max_wait_ms),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let frames = Cell::new(submitted);
    let read_at = start
        .checked_sub(std::time::Duration::from_millis(read_ago_ms))
        .expect("the clock has run that long");

    let result = wait_for_measured_tail(
        submitted,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(progress_reading(sounded, frames.get(), mark, read_at)),
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

#[test]
fn a_usable_entry_reading_bounds_the_fallback() {
    let (result, held) = entry_reading_wait(48_000, 48_000, 24_000, 0, 1_000);

    assert!(
        result.is_ok(),
        "stale live callbacks still finish in budget"
    );
    assert!(
        held >= std::time::Duration::from_millis(500),
        "the entry reading owed 500 ms; finished at {held:?}"
    );
}

#[test]
fn a_usable_entry_reading_owing_past_max_wait_times_out() {
    let (result, held) = entry_reading_wait(48_000, 48_000, 24_000, 0, 400);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
    assert_eq!(held, std::time::Duration::from_millis(400));
}

#[test]
fn an_old_usable_entry_mark_leaves_only_the_derived_tail() {
    let (result, held) = entry_reading_wait(144_000, 48_000, 38_400, 2_000, 300);

    assert!(result.is_ok());
    assert_eq!(held, std::time::Duration::from_millis(200));
}

#[test]
fn an_entry_reading_older_than_its_debt_finishes_on_the_tail() {
    let (result, held) = entry_reading_wait(48_000, 45_600, 21_600, 1_000, 400);

    assert!(result.is_ok());
    assert_eq!(held, std::time::Duration::from_millis(200));
}

#[test]
fn a_usable_mark_one_stale_callback_behind_at_entry_still_owes_its_playback() {
    use std::time::{Duration, Instant};

    let policy = RetryPolicy {
        interval: Duration::from_millis(10),
        max_wait: Duration::from_secs(1),
    };
    let start = Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(21_600_u64);
    let usable = Cell::new(45_600_u64);

    let result = wait_for_measured_tail(
        48_000,
        Duration::from_millis(200),
        48_000,
        &policy,
        || {
            Some(progress_reading(
                sounded.get(),
                submitted.get(),
                usable.get(),
                start
                    .checked_sub(Duration::from_millis(50))
                    .expect("the clock has run that long"),
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
                submitted.set(submitted.get() + 2_400);
            }
        },
    );
    let elapsed = clock.borrow().duration_since(start);

    assert!(result.is_ok());
    assert_eq!(elapsed, Duration::from_millis(500));
}

#[test]
fn a_delayed_poll_does_not_backdate_a_usable_reading_past_its_callback() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
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
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
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

#[test]
fn a_usable_reading_seen_by_a_delayed_poll_is_stamped_when_seen() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
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
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
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

#[test]
fn a_larger_stale_buffer_after_a_usable_one_does_not_overstate_its_age() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
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
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
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

#[test]
fn a_stale_buffer_past_the_tail_size_after_a_usable_one_does_not_overstate_its_age() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
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
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
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

#[test]
fn a_queued_stale_callback_at_entry_is_not_elapsed_playback() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(50_400_u64);

    let result = wait_for_measured_tail(
        48_000,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || Some(stamps.progress(24_000, submitted.get(), 48_000, *clock.borrow())),
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
                submitted.set(submitted.get() + 2_400);
            }
        },
    );
    let held = clock.borrow().duration_since(start);

    assert!(result.is_ok(), "finished in budget");
    assert!(
        held >= std::time::Duration::from_millis(500),
        "the entry reading owed 500 ms; finished at {held:?}"
    );
}

#[test]
fn a_stale_burst_inside_one_poll_gap_is_not_elapsed_playback() {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_secs(1),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let stamps = FirstSeen::default();
    let submitted = Cell::new(48_000_u64);
    let sounded = Cell::new(0_u64);
    let usable = Cell::new(0_u64);

    let result = wait_for_measured_tail(
        48_000,
        std::time::Duration::from_millis(200),
        48_000,
        &policy,
        || {
            Some(stamps.progress(
                sounded.get(),
                submitted.get(),
                usable.get(),
                *clock.borrow(),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            let before = clock.borrow().duration_since(start).as_millis();
            *clock.borrow_mut() += duration;
            let after = clock.borrow().duration_since(start).as_millis();
            for ms in (before + 1)..=after {
                if ms == 101 {
                    let callback_start = submitted.get();
                    submitted.set(callback_start + 2_400);
                    sounded.set(callback_start - 500 * 48);
                    usable.set(submitted.get());
                } else if ms == 105 {
                    submitted.set(submitted.get() + 14_400);
                } else if ms >= 150 && ms.is_multiple_of(50) {
                    submitted.set(submitted.get() + 2_400);
                }
            }
        },
    );
    let held = clock.borrow().duration_since(start);

    assert!(result.is_ok(), "finished in budget");
    assert!(
        held >= std::time::Duration::from_millis(601),
        "the target sounds at 601 ms; finished at {held:?}"
    );
}

fn startup_saturated_wait(max_wait_ms: u64) -> (Result<(), DrainError>, std::time::Duration) {
    let policy = RetryPolicy {
        interval: std::time::Duration::from_millis(10),
        max_wait: std::time::Duration::from_millis(max_wait_ms),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let frames = Cell::new(12_000_u64);

    let result = wait_for_measured_tail(
        12_000,
        std::time::Duration::from_millis(250),
        48_000,
        &policy,
        || Some(progress_started(0, frames.get(), 12_000, start, 12_000)),
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

#[test]
fn a_startup_saturated_reading_waits_until_its_target_sounds() {
    let (result, held) = startup_saturated_wait(1_000);

    assert!(result.is_ok());
    assert_eq!(held, std::time::Duration::from_millis(500));
}

#[test]
fn a_startup_saturated_reading_owing_past_max_wait_times_out() {
    let (result, held) = startup_saturated_wait(400);

    assert!(matches!(
        result,
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
    assert_eq!(held, std::time::Duration::from_millis(400));
}

#[test]
fn a_reading_seen_late_is_credited_from_its_callback_at_the_deadline() {
    use std::time::Duration;

    let policy = RetryPolicy {
        interval: Duration::from_millis(10),
        max_wait: Duration::from_millis(300),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let usable = Cell::new(0_u64);
    let read_at = start + Duration::from_millis(95);

    let result = wait_for_measured_tail(
        48_000,
        Duration::from_millis(200),
        48_000,
        &policy,
        || {
            Some(progress_reading(
                if usable.get() > 0 { 38_160 } else { 0 },
                submitted.get(),
                usable.get(),
                read_at,
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            *clock.borrow_mut() += duration;
            let ms = clock.borrow().duration_since(start).as_millis();
            submitted.set(submitted.get() + 480);
            if ms == 100 {
                usable.set(submitted.get());
            }
        },
    );

    assert!(result.is_ok(), "the 300 ms due fits the 300 ms budget");
    assert_eq!(
        clock.borrow().duration_since(start),
        Duration::from_millis(300)
    );
}

fn starved_poll_wait(read_at_ms: u64) -> Result<(), DrainError> {
    use std::time::Duration;

    let policy = RetryPolicy {
        interval: Duration::from_millis(10),
        max_wait: Duration::from_millis(300),
    };
    let start = std::time::Instant::now();
    let clock = Rc::new(RefCell::new(start));
    let submitted = Cell::new(48_000_u64);
    let usable = Cell::new(0_u64);
    let mut first_sleep = true;

    wait_for_measured_tail(
        48_000,
        Duration::from_millis(200),
        48_000,
        &policy,
        || {
            Some(progress_reading(
                if usable.get() > 0 { 38_400 } else { 0 },
                submitted.get(),
                usable.get(),
                start + Duration::from_millis(read_at_ms),
            ))
        },
        || 0,
        || *clock.borrow(),
        |duration| {
            if std::mem::take(&mut first_sleep) {
                *clock.borrow_mut() += Duration::from_millis(310);
                usable.set(submitted.get() + 480);
                submitted.set(submitted.get() + 480 + 9_600);
            } else {
                *clock.borrow_mut() += duration;
            }
        },
    )
}

#[test]
fn a_starved_poll_finishes_a_reading_whose_stamp_fits_the_deadline() {
    assert!(starved_poll_wait(95).is_ok());
}

#[test]
fn a_starved_poll_times_out_a_reading_whose_stamp_outruns_the_deadline() {
    assert!(matches!(
        starved_poll_wait(115),
        Err(DrainError::MeasuredTailTimedOut { .. })
    ));
}
