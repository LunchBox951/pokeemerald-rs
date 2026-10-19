//! Build-stage classification and the bounded unpolled event queue.

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
