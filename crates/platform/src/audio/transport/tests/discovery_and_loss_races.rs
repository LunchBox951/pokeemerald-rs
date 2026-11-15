//! A slow discovery is rescheduled from when it returned, and a loss that
//! races a `stop` still counts as a loss while playing.

use super::*;

#[test]
fn a_slow_discovery_on_an_attached_stream_is_not_repeated_back_to_back() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    assert_eq!(rig.shared.state(), TransportState::Ready);

    with(&w).discovery_latency = MS(1_500);
    with(&w).clock = MS(1_000);
    rig.transport.step(MS(1_000));
    assert_eq!(with(&w).discoveries, 2);
    assert_eq!(with(&w).clock, MS(2_500));
    // The health poll right after the slow query does not query again.
    rig.transport.step(MS(2_520));
    assert_eq!(with(&w).discoveries, 2, "discovery waits a full interval");
    assert_eq!(rig.transport.next_discovery, MS(3_500));
}

#[test]
fn a_slow_discovery_while_unattached_is_not_repeated_back_to_back() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.fail_build = true;
    });
    let mut rig = rig(&w, 4);
    for _ in 0..ATTEMPTS_PER_BURST {
        let now = rig.transport.next_attempt;
        with(&w).clock = now;
        rig.transport.step(now);
    }
    // Cooling down to 5.7 s; the next discovery is due at 1.7 s.
    assert_eq!(rig.transport.next_attempt, MS(5_700));
    let discoveries = with(&w).discoveries;

    with(&w).discovery_latency = MS(1_500);
    with(&w).clock = MS(1_700);
    let next = rig.transport.step(MS(1_700));
    assert_eq!(with(&w).discoveries, discoveries + 1);
    assert_eq!(
        next,
        MS(4_200),
        "the next discovery runs an interval after the slow one returned"
    );
}

#[test]
fn a_loss_raised_while_a_stop_is_applied_is_still_a_loss_while_playing() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    rig.events();

    // The fatal callback lands after the poll's loss check, during the
    // pause that applies a concurrent `stop`.
    with(&w).lose_during_pause = true;
    assert_eq!(
        rig.producer.push(&[0.25; 4]),
        4,
        "undrained before the stop"
    );
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(20));
    assert_eq!(with(&w).pauses, 1);
    with(&w).lose_during_pause = false;

    rig.transport.step(MS(40));
    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::StreamLost)]
    );
    assert_eq!(rig.producer.push(&[0.5; 2]), 2, "queued while stopped");
    rig.transport.step(MS(140));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity() - 2,
        "history queued before the stop is discarded; the stopped queue is kept"
    );
}

#[test]
fn a_loss_raised_during_a_refused_pause_is_still_a_loss_while_playing() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.no_hw_pause = true;
    });
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);

    with(&w).lose_during_pause = true;
    assert_eq!(
        rig.producer.push(&[0.25; 4]),
        4,
        "undrained before the stop"
    );
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(20));
    with(&w).lose_during_pause = false;
    assert_eq!(rig.producer.push(&[0.5; 2]), 2, "queued while stopped");
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(rig.producer.available_space(), rig.producer.capacity() - 2);
}

#[test]
fn a_start_that_lands_during_a_reattach_attempt_discards_the_history() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    // Lost while stopped: the queue is kept for the next start.
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    assert!(!rig.transport.stale_queue);

    // `start` arrives while the retry's discovery is blocking.
    with(&w).start_during_discovery = Some(Arc::clone(&rig.shared));
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(with(&w).plays, 1, "the start is applied at attach");
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity(),
        "the attach that starts playback does not replay the stopped history"
    );
}

#[test]
fn history_queued_during_a_slow_started_rebuild_is_not_replayed() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));

    // The producer keeps pushing while the rebuild blocks in the OS.
    let producer = rig.producer.clone();
    with(&w).during_build = Some(Box::new(move |_| {
        assert_eq!(producer.push(&[0.5; 8]), 8);
    }));
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(rig.events().last(), Some(&TransportState::Ready));

    let out = run_data_callback::<f32>(&w, 1, 8);
    assert!(
        out.iter().all(|&sample| sample == 0.0),
        "the rebuilt stream does not replay what queued during its build: {out:?}"
    );
    assert_eq!(rig.producer.available_space(), rig.producer.capacity());
}

#[test]
fn a_stopped_rebuild_after_a_loss_while_started_keeps_what_queues_during_it() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.25; 8]), 8, "history from the loss");
    rig.shared.set_running(false, &rig.producer);

    // Stopped now, so what queues during the rebuild is a stopped queue.
    let producer = rig.producer.clone();
    with(&w).during_build = Some(Box::new(move |_| {
        assert_eq!(producer.push(&[0.5; 4]), 4);
    }));
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(MS(140));
    assert_eq!(with(&w).plays, 2);

    let out = run_data_callback::<f32>(&w, 1, 8);
    assert!(
        out.iter().any(|&sample| sample != 0.0),
        "the queue left while stopped plays at the next start: {out:?}"
    );
}

#[test]
fn a_stopped_reattach_keeps_the_queue_for_a_later_start() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);

    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(MS(140));
    assert_eq!(with(&w).plays, 1);
    assert_eq!(rig.producer.available_space(), rig.producer.capacity() - 4);
}

#[test]
fn a_failed_running_reattach_keeps_the_queue_stale_after_a_stop() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));

    // The running retry discards the old queue, then its build fails.
    with(&w).fail_build = true;
    rig.transport.step(MS(120));
    with(&w).fail_build = false;
    // Queued while started with nothing draining, then stopped before the
    // worker's next step.
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    rig.shared.set_running(false, &rig.producer);

    rig.transport.step(MS(320));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity(),
        "history queued during the failed running attempt is discarded"
    );
}

#[test]
fn a_queue_left_after_a_stop_survives_the_history_discard() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.25; 2]), 2, "history from the loss");
    rig.shared.set_running(false, &rig.producer);
    // Prefill for the next start, queued before the retry.
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(rig.producer.available_space(), rig.producer.capacity() - 4);

    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(MS(140));
    assert_eq!(with(&w).plays, 2);
    let out = run_data_callback::<f32>(&w, 1, 8);
    assert!(
        out[..4] == [0.5; 4] && !out.contains(&0.25),
        "the prefill plays at the next start, without the history: {out:?}"
    );
}

#[test]
fn history_after_a_later_start_is_still_discarded() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(Duration::ZERO);
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(20));
    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(MS(40));

    // Lost while playing after the second start: nothing drains what the
    // producer queues next, and no stop has landed since it.
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(60));
    assert_eq!(rig.producer.push(&[0.25; 4]), 4);
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(160));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity(),
        "history queued after the second start is discarded"
    );
}

#[test]
fn a_refused_pause_before_the_first_callback_keeps_the_bounded_discard() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.no_hw_pause = true;
    });
    let mut rig = rig(&w, 8);
    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));

    // History queued while the started rebuild blocks.
    let producer = rig.producer.clone();
    with(&w).during_build = Some(Box::new(move |_| {
        assert_eq!(producer.push(&[0.5; 4]), 4);
    }));
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);

    // Stop before stream 1's first callback, then queue a prefill.
    rig.shared.set_running(false, &rig.producer);
    assert_eq!(rig.producer.push(&[0.25; 4]), 4);
    // The pause is refused: stream 1 is replaced by a paused stream 2.
    rig.transport.step(MS(140));
    assert_eq!(with(&w).builds, 3);

    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(MS(160));
    let out = run_data_callback::<f32>(&w, 2, 8);
    assert!(
        out[0] > 0.0 && out.iter().all(|&sample| sample < 0.4),
        "the prefill queued after the stop plays first, without the build-time history: {out:?}"
    );
}

#[test]
fn a_default_change_before_the_first_callback_keeps_the_bounded_discard() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 8);
    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));

    // History queued while the started rebuild blocks.
    let producer = rig.producer.clone();
    with(&w).during_build = Some(Box::new(move |_| {
        assert_eq!(producer.push(&[0.5; 4]), 4);
    }));
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);
    with(&w).during_build = None;

    // Stop before stream 1's first callback; the pause succeeds. Then
    // queue a prefill and let the default change retire the paused stream.
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(140));
    assert_eq!(with(&w).pauses, 1);
    assert_eq!(rig.producer.push(&[0.25; 4]), 4);
    with(&w).default = Some("b".into());
    rig.transport.step(MS(1_200));
    assert_eq!(with(&w).builds, 3);

    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(MS(1_220));
    let out = run_data_callback::<f32>(&w, 2, 8);
    assert!(
        out[0] > 0.0 && out.iter().all(|&sample| sample < 0.4),
        "the prefill queued after the stop plays first, without the build-time history: {out:?}"
    );
}
