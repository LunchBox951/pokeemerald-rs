//! Queued samples the output transport discards are not a played drain.

use super::device_tail::FakeClock;
use super::*;
use crate::discards::{finish_playback, queue_intact, DiscardBaseline};
use std::time::Duration;

const CAPACITY: usize = 8192;

fn policy() -> RetryPolicy {
    RetryPolicy {
        interval: Duration::from_millis(10),
        max_wait: Duration::from_millis(300),
    }
}

/// The three signals `play_song` reads, as the transport moves them: samples
/// still queued, samples discarded unplayed, and asynchronous stream errors.
#[derive(Default)]
struct FakeOutput {
    queued: Cell<usize>,
    discarded: Cell<u64>,
    errors: Cell<u64>,
}

impl FakeOutput {
    fn with_queued(queued: usize) -> Self {
        let output = Self::default();
        output.queued.set(queued);
        output
    }

    fn available_space(&self) -> usize {
        CAPACITY - self.queued.get()
    }

    /// A default-output change: the replacement stream discards whatever is
    /// queued, unplayed, and raises no stream error.
    fn default_changes(&self) {
        let queued = self.queued.replace(0);
        self.discarded.set(self.discarded.get() + queued as u64);
    }
}

/// `finish_playback` over `output` on a host with no usable timestamp,
/// running `on_sleep` at the end of every sleep.
fn finish(output: &FakeOutput, on_sleep: impl Fn()) -> Result<(), DrainError> {
    let clock = FakeClock::new();
    finish_playback(
        CAPACITY,
        Duration::from_millis(200),
        48_000,
        0,
        &policy(),
        || output.available_space(),
        || None,
        || output.errors.get(),
        || output.discarded.get(),
        clock.now(),
        clock.sleep_then(on_sleep),
    )
}

#[test]
fn a_default_change_discarding_the_queued_tail_fails_the_run() {
    // The song's tail is queued and the first sleep is the default change.
    let output = FakeOutput::with_queued(500);

    // Without the discard count, the emptied ring and flat error counter read
    // as a clean drain: the reported bug.
    let blind = FakeOutput::with_queued(500);
    assert!(wait_for_drain(
        CAPACITY,
        &policy(),
        || blind.available_space(),
        || blind.errors.get(),
        FakeClock::new().now(),
        FakeClock::new().sleep_then(|| blind.default_changes()),
    )
    .is_ok());

    let result = finish(&output, || output.default_changes());

    let Err(err @ DrainError::QueueDiscarded { samples: 500 }) = result else {
        panic!(
            "a discarded tail must fail the run, got {:?}",
            result.err().map(|e| e.describe())
        );
    };
    assert!(err.describe().contains("500 queued sample(s)"));
    assert_eq!(output.errors.get(), 0, "no stream error was raised");
}

#[test]
fn a_discard_already_counted_when_the_ring_is_found_empty_fails_the_run() {
    let output = FakeOutput::with_queued(500);
    output.default_changes();

    let result = finish(&output, || panic!("an empty ring must not need to sleep"));

    assert!(matches!(
        result,
        Err(DrainError::QueueDiscarded { samples: 500 })
    ));
}

#[test]
fn a_discard_during_the_device_tail_fails_the_run() {
    let output = FakeOutput::default();
    let slept = Cell::new(0_u32);

    let result = finish(&output, || {
        slept.set(slept.get() + 1);
        if slept.get() == 3 {
            output.discarded.set(64);
        }
    });

    assert!(matches!(
        result,
        Err(DrainError::QueueDiscarded { samples: 64 })
    ));
}

#[test]
fn a_default_change_with_an_empty_ring_is_a_clean_finish() {
    let output = FakeOutput::default();

    let result = finish(&output, || output.default_changes());

    assert!(result.is_ok());
    assert_eq!(output.discarded.get(), 0);
}

#[test]
fn a_reported_stream_error_outranks_a_discard() {
    let output = FakeOutput::with_queued(500);
    let result = finish(&output, || {
        output.default_changes();
        output.errors.set(2);
    });

    assert!(matches!(
        result,
        Err(DrainError::StreamStopped { errors: 2, .. })
    ));
}

#[test]
fn a_discard_outranks_the_timeout_of_a_replaced_stream() {
    // A measured clock that never reaches its target after the reconnect.
    let output = FakeOutput::with_queued(500);
    let clock = FakeClock::new();
    let sleeps = Cell::new(0_u32);
    let result = finish_playback(
        CAPACITY,
        Duration::from_millis(200),
        48_000,
        0,
        &policy(),
        || output.available_space(),
        || Some(progress(0, 1_000)),
        || output.errors.get(),
        || output.discarded.get(),
        clock.now(),
        clock.sleep_then(|| {
            sleeps.set(sleeps.get() + 1);
            if sleeps.get() == 1 {
                output.default_changes();
            }
        }),
    );

    assert!(matches!(
        result,
        Err(DrainError::QueueDiscarded { samples: 500 })
    ));
}

#[test]
fn only_samples_discarded_since_the_baseline_count() {
    let output = AudioOutput::null(256);
    let baseline = DiscardBaseline::of(&output);
    assert_eq!(baseline.since(&output), 0);
    assert!(queue_intact(baseline.since(&output)).is_ok());
    assert!(matches!(
        queue_intact(3),
        Err(DrainError::QueueDiscarded { samples: 3 })
    ));
}
