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
    rig.shared.running.store(false, Ordering::Release);
    rig.transport.step(MS(20));
    assert_eq!(with(&w).pauses, 1);
    with(&w).lose_during_pause = false;

    rig.transport.step(MS(40));
    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::StreamLost)]
    );
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    rig.transport.step(MS(140));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity(),
        "history queued after a loss while playing is discarded"
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
    rig.shared.running.store(false, Ordering::Release);
    rig.transport.step(MS(20));
    with(&w).lose_during_pause = false;
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(rig.producer.available_space(), rig.producer.capacity());
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
