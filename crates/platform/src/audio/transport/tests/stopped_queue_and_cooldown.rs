//! A queue left while stopped survives a reattach, and a replaced default is
//! found during a retry cooldown.

use super::*;

#[test]
fn stopping_a_stream_without_hardware_pause_keeps_the_queue_for_the_next_start() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.no_hw_pause = true;
    });
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    assert_eq!(rig.events(), [TransportState::Ready]);

    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    // `stop`: the pause is refused, so the worker rebuilds a paused stream.
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(20));
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(rig.events().last(), Some(&TransportState::Ready));
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity() - 4,
        "a paused queue is resumed by the next start, not skipped"
    );
}

#[test]
fn a_default_replaced_while_stopped_keeps_the_queue() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(20));
    assert_eq!(with(&w).pauses, 1);

    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    with(&w).default = Some("b".into());
    rig.transport.step(MS(1_000));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(rig.producer.available_space(), rig.producer.capacity() - 4);
}

#[test]
fn a_stream_lost_while_started_is_stale_even_if_stopped_before_it_returns() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);

    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity(),
        "history queued while playback should have advanced is discarded"
    );
}

#[test]
fn a_replacement_default_is_found_during_a_retry_cooldown() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.fail_build = true;
    });
    let mut rig = rig(&w, 4);
    let mut now = Duration::ZERO;
    for _ in 0..4 {
        now = rig.transport.step(now);
    }
    // Four failed attempts at 0, 100, 300 and 700 ms: cooling down to 5.7 s,
    // with the next discovery due first.
    assert_eq!(rig.transport.next_attempt, MS(5_700));
    assert_eq!(now, MS(1_700));

    with(&w).default = Some("b".into());
    with(&w).fail_build = false;
    rig.transport.step(now);
    assert_eq!(with(&w).builds, 1, "the new default attaches at the probe");
    assert_eq!(rig.shared.state(), TransportState::Ready);
}

#[test]
fn an_unchanged_default_waits_out_its_cooldown() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.fail_build = true;
    });
    let mut rig = rig(&w, 4);
    let mut now = Duration::ZERO;
    for _ in 0..4 {
        now = rig.transport.step(now);
    }
    with(&w).fail_build = false;
    while now < MS(5_700) {
        now = rig.transport.step(now);
        assert_eq!(with(&w).builds, 0, "no attempt before {now:?}");
    }
    rig.transport.step(now);
    assert_eq!(with(&w).builds, 1);
}

#[test]
fn a_default_that_appears_after_no_output_starts_a_fresh_burst() {
    let w = world(|_| {});
    let mut rig = rig(&w, 4);
    let mut now = Duration::ZERO;
    for _ in 0..4 {
        now = rig.transport.step(now);
    }
    assert_eq!(rig.transport.next_attempt, MS(5_700));
    assert_eq!(now, MS(1_700));
    with(&w).default = Some("a".into());
    with(&w).fail_build = true;
    rig.transport.step(now);
    assert_eq!(
        rig.transport.next_attempt,
        now + RETRY_BASE,
        "the new identity's first failure retries at the burst's first delay"
    );
}

#[test]
fn a_loss_while_playing_is_stale_even_if_stop_lands_before_the_worker_sees_it() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);

    // The stream dies while playing; the producer keeps queuing, then stops,
    // all before the worker's next health poll.
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(20));
    rig.transport.step(MS(120));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity(),
        "history queued after a loss while playing is discarded"
    );
}

#[test]
fn refused_pauses_do_not_spend_the_retry_budget() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.no_hw_pause = true;
    });
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    assert_eq!(with(&w).builds, 1);

    // Four stop/start cycles, all inside `STABLE_AFTER`. Each refused pause
    // re-attaches a stopped stream; none of them is a failure, so the fourth
    // must not close a burst and push its re-attach out by `COOLDOWN`.
    let mut now = MS(20);
    for cycle in 0..4 {
        rig.shared.set_running(false, &rig.producer);
        rig.transport.step(now);
        rig.transport.step(now + MS(500));
        assert_eq!(
            with(&w).builds,
            cycle + 2,
            "the stopped re-attach after refused pause {} is not delayed",
            cycle + 1
        );
        rig.shared.set_running(true, &rig.producer);
        rig.transport.step(now + MS(520));
        now += MS(540);
    }
    assert!(now < STABLE_AFTER);
    assert_eq!(rig.events().last(), Some(&TransportState::Ready));
}
