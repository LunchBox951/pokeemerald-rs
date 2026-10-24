//! A stream the backend reroutes to a new default stays attached.

use super::*;

#[test]
fn a_rerouted_stream_survives_the_changed_default_identity() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    rig.events();
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);

    // The backend follows the new default and says so.
    with(&w).default = Some("b".into());
    raise_error(&w, 0, cpal::ErrorKind::DeviceChanged);
    rig.transport.step(MS(1_000));
    // And again: the stream keeps following.
    with(&w).default = Some("c".into());
    raise_error(&w, 0, cpal::ErrorKind::DeviceChanged);
    rig.transport.step(MS(2_000));

    assert_eq!(rig.events(), [], "no loss is reported for a reroute");
    assert_eq!(with(&w).log, ["build"], "the healthy stream is kept");
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity() - 4,
        "the queue the rerouted stream is playing is kept"
    );

    // A genuine loss afterwards still rebuilds on the current default.
    with(&w).default = None;
    rig.transport.step(MS(3_000));
    assert_eq!(
        rig.events(),
        [TransportState::Unavailable(TransportFailure::NoOutput)]
    );
}

#[test]
fn a_reroute_reported_while_discovery_runs_keeps_the_stream() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.running.store(true, Ordering::Release);
    rig.transport.step(Duration::ZERO);
    rig.events();
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);

    // The backend reroutes and reports it while the query that finds the
    // new default is still running.
    with(&w).default = Some("b".into());
    with(&w).during_discovery = Some(Box::new(|w: &mut World| {
        (w.streams[0].error)(cpal::Error::new(cpal::ErrorKind::DeviceChanged));
    }));
    rig.transport.step(MS(1_000));

    assert_eq!(rig.events(), [], "no loss is reported for a reroute");
    assert_eq!(with(&w).log, ["build"], "the rerouted stream is kept");
    assert_eq!(rig.producer.available_space(), rig.producer.capacity() - 4);
}
