//! The underrun guard: a counted silent shortfall during active playback is a
//! failed smoke run, but the silence after the song ends is expected.

use super::{
    check_underruns, push_frame, wait_for_device_tail, wait_for_drain, RetryPolicy, UnderrunError,
};
use platform::AudioOutput;
use std::time::{Duration, Instant};

fn policy() -> RetryPolicy {
    RetryPolicy {
        interval: Duration::from_millis(1),
        max_wait: Duration::from_millis(5),
    }
}

fn push(output: &AudioOutput, samples: &[f32]) {
    let producer = output.producer();
    push_frame(
        samples,
        &policy(),
        |chunk| producer.push(chunk),
        || output.stream_errors(),
        Instant::now,
        |_| panic!("the ring has room; must not retry"),
    )
    .unwrap_or_else(|_| panic!("push must succeed"));
}

#[test]
fn an_unchanged_counter_is_not_a_failure() {
    assert_eq!(check_underruns(0, 0), Ok(()));
}

#[test]
fn a_nonzero_baseline_that_does_not_grow_is_not_a_failure() {
    assert_eq!(check_underruns(64, 64), Ok(()));
}

#[test]
fn a_counter_increase_is_a_failure_sized_by_the_increase() {
    assert_eq!(check_underruns(0, 64), Err(UnderrunError { samples: 64 }));
    let err = check_underruns(7, 71).expect_err("64 new silent samples");
    assert_eq!(err, UnderrunError { samples: 64 });
    assert_eq!(
        err.describe(),
        "64 sample(s) played as silence due to ring-buffer underrun during song rendering"
    );
}

#[test]
fn a_clean_null_playback_passes_the_guard() {
    let mut output = AudioOutput::null(64);
    let baseline = output.midstream_underruns();
    push(&output, &[1.0_f32; 32]);
    let mut device = [9.0_f32; 32];
    output.pull_null(&mut device);

    assert!(device.iter().all(|s| (s - 1.0).abs() < f32::EPSILON));
    assert_eq!(output.underruns(), 0);
    assert_eq!(
        check_underruns(baseline, output.midstream_underruns()),
        Ok(())
    );
}

#[test]
fn a_counted_shortfall_cannot_be_followed_by_a_successful_push_and_a_passing_guard() {
    let mut output = AudioOutput::null(64);
    let baseline = output.midstream_underruns();
    push(&output, &[1.0_f32; 32]);

    // The producer stalls: the device pulls more than is queued.
    let mut device = [9.0_f32; 96];
    output.pull_null(&mut device);
    assert!(device[..32].iter().all(|s| (s - 1.0).abs() < f32::EPSILON));
    assert!(device[32..].iter().all(|s| s.abs() < f32::EPSILON));
    assert_eq!(output.underruns(), 64);
    assert_eq!(
        output.midstream_underruns(),
        0,
        "no sample has been queued after the pad yet"
    );
    assert_eq!(output.stream_errors(), 0);

    // The producer resumes and the enqueue succeeds, yet the run is not clean.
    push(&output, &[1.0_f32; 32]);
    assert_eq!(
        check_underruns(baseline, output.midstream_underruns()),
        Err(UnderrunError { samples: 64 })
    );
}

#[test]
fn a_shortfall_before_the_final_enqueue_is_caught_by_the_check_right_after_it() {
    let mut output = AudioOutput::null(64);
    let baseline = output.midstream_underruns();
    push(&output, &[1.0_f32; 32]);
    assert_eq!(
        check_underruns(baseline, output.midstream_underruns()),
        Ok(())
    );

    // The device outruns the producer during the pacing wait ...
    let mut device = [0.0_f32; 64];
    output.pull_null(&mut device);
    // ... then the final frame is enqueued and checked before any further wait.
    push(&output, &[1.0_f32; 32]);
    assert_eq!(
        check_underruns(baseline, output.midstream_underruns()),
        Err(UnderrunError { samples: 32 })
    );
}

#[test]
fn a_descheduled_producer_after_the_final_enqueue_does_not_fail_a_clean_song() {
    // Mirrors `main`: the final frame is enqueued, then the producer is
    // descheduled before `check_underruns` reads the counter.
    let mut output = AudioOutput::null(64);
    let baseline = output.midstream_underruns();
    push(&output, &[1.0_f32; 32]);

    // Callbacks run in the gap: every song sample is delivered, then the
    // expected end-of-song silence is padded.
    let mut device = [9.0_f32; 96];
    output.pull_null(&mut device);
    assert!(device[..32].iter().all(|s| (s - 1.0).abs() < f32::EPSILON));
    assert!(device[32..].iter().all(|s| s.abs() < f32::EPSILON));
    assert_eq!(output.underruns(), 64, "the padding is counted ...");

    // ... but no song sample was lost, so the guard must pass.
    assert_eq!(
        check_underruns(baseline, output.midstream_underruns()),
        Ok(())
    );
}

#[test]
fn silence_after_the_final_enqueue_during_drain_and_tail_is_expected() {
    let mut output = AudioOutput::null(64);
    let producer = output.producer();
    let baseline = output.midstream_underruns();
    push(&output, &[1.0_f32; 32]);
    assert_eq!(
        check_underruns(baseline, output.midstream_underruns()),
        Ok(())
    );

    let capacity = producer.capacity();
    let mut device = [0.0_f32; 96];
    output.pull_null(&mut device);
    assert!(output.underruns() > 0, "the drained ring pads silence");
    assert_eq!(output.midstream_underruns(), baseline);

    let drained = wait_for_drain(
        capacity,
        &policy(),
        || producer.available_space(),
        || output.stream_errors(),
        Instant::now,
        |_| panic!("the ring is already empty"),
    );
    assert!(drained.is_ok());
    let tail = wait_for_device_tail(Duration::from_millis(1), || output.stream_errors(), |_| {});
    assert!(tail.is_ok());
    assert_eq!(output.stream_errors(), 0);
}

#[test]
fn a_dropout_before_the_final_samples_is_caught_by_the_recheck_after_drain() {
    // Mirrors `main`: the final frame is enqueued behind a pad, the device
    // delivers it, the ring drains, and the guard is read once more.
    let mut output = AudioOutput::null(64);
    let producer = output.producer();
    let baseline = output.midstream_underruns();
    output.pull_null(&mut [0.0_f32; 16]);
    push(&output, &[1.0_f32; 32]);

    let mut device = [0.0_f32; 64];
    output.pull_null(&mut device);
    let drained = wait_for_drain(
        producer.capacity(),
        &policy(),
        || producer.available_space(),
        || output.stream_errors(),
        Instant::now,
        |_| panic!("the ring is already empty"),
    );
    assert!(drained.is_ok());
    assert_eq!(
        check_underruns(baseline, output.midstream_underruns()),
        Err(UnderrunError { samples: 16 })
    );
}
