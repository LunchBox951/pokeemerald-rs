//! Build-stage classification, slow builds, and the bounded unpolled event
//! queue.

use super::*;

#[test]
fn a_device_lost_during_build_is_a_connect_failure_not_no_output() {
    for kind in [
        cpal::ErrorKind::DeviceNotAvailable,
        cpal::ErrorKind::HostUnavailable,
    ] {
        let w = world(|w| {
            w.default = Some("a".into());
            w.fail_build = true;
            w.build_kind = Some(kind);
        });
        let mut rig = rig(&w, 4);
        rig.transport.step(Duration::ZERO);
        assert_eq!(
            rig.events(),
            [TransportState::Unavailable(TransportFailure::ConnectFailed)],
            "{kind:?}"
        );
    }
}

#[test]
fn unpolled_status_queue_stays_bounded_under_repeated_default_flapping() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    let mut now = Duration::ZERO;
    rig.transport.step(now);
    for flip in 0..1_000 {
        with(&w).default = Some(if flip % 2 == 0 { "b" } else { "a" }.into());
        now += DISCOVERY_INTERVAL;
        rig.transport.step(now);
    }
    let pending = rig.events();
    assert!(pending.len() <= 8, "{} pending", pending.len());
    assert_eq!(pending.last(), Some(&rig.shared.state()));
}

#[test]
fn retry_delays_and_stability_run_from_when_a_slow_attempt_finished() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.fail_build = true;
        w.build_latency = MS(300);
    });
    let mut rig = rig(&w, 4);
    let mut scheduled = Vec::new();
    for _ in 0..ATTEMPTS_PER_BURST {
        let now = rig.transport.next_attempt;
        with(&w).clock = now;
        rig.transport.step(now);
        scheduled.push(rig.transport.next_attempt);
    }
    // Each build takes 300 ms; every delay, the cooldown included, follows it.
    assert_eq!(scheduled, [MS(400), MS(900), MS(1_600), MS(6_900)]);

    with(&w).fail_build = false;
    with(&w).clock = MS(6_900);
    rig.transport.step(MS(6_900));
    assert_eq!(rig.shared.state(), TransportState::Ready);
    assert_eq!(
        rig.transport.live.as_ref().map(|live| live.since),
        Some(MS(7_200)),
        "a stream is stable from when it attached, not when the build began"
    );
}
