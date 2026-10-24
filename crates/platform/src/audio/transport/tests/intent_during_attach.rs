//! A `start` or `stop` that lands while a re-attaching stream is built, and
//! a loss or replacement that a discovery on an attached stream reports.

use super::*;

#[test]
fn a_start_that_lands_while_a_stopped_reattach_builds_discards_the_history() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    // Lost while stopped: the retry is a stopped reattach.
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));

    // `start` lands while the stream is built, and the producer keeps
    // pushing with nothing draining.
    let (shared, producer) = (Arc::clone(&rig.shared), rig.producer.clone());
    with(&w).during_build = Some(Box::new(move |_| {
        shared.set_running(true, &producer);
        assert_eq!(producer.push(&[0.25; 6]), 6);
    }));
    let mut now = MS(120);
    for _ in 0..3 {
        now = rig.transport.step(now);
    }
    let last = with(&w).streams.len() - 1;
    let plays = with(&w).plays;
    assert!(plays >= 1, "the start is applied");

    let out = run_data_callback::<f32>(&w, last, 8);
    assert!(
        !out.contains(&0.25),
        "what queued while started with nothing attached is not replayed: {out:?}"
    );
}

#[test]
fn a_stop_that_lands_while_a_started_reattach_builds_keeps_what_queues_after_it() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.25; 2]), 2, "history from the loss");

    // `stop` lands while the started retry is built, then the producer
    // queues a prefill for the next start.
    let (shared, producer) = (Arc::clone(&rig.shared), rig.producer.clone());
    with(&w).during_build = Some(Box::new(move |_| {
        assert_eq!(producer.push(&[0.25; 2]), 2, "history from the build");
        shared.set_running(false, &producer);
        assert_eq!(producer.push(&[0.5; 4]), 4);
    }));
    let mut now = MS(120);
    for _ in 0..3 {
        now = rig.transport.step(now);
    }
    assert_eq!(rig.shared.state(), TransportState::Ready);
    assert_eq!(rig.producer.available_space(), rig.producer.capacity() - 4);

    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(now);
    let last = with(&w).streams.len() - 1;
    let out = run_data_callback::<f32>(&w, last, 8);
    assert!(
        out[..4] == [0.5; 4] && !out.contains(&0.25),
        "the prefill queued after the stop plays at the next start: {out:?}"
    );
}

#[test]
fn a_loss_and_stop_during_discovery_of_a_new_default_still_discard_the_history() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(Duration::ZERO);

    // While the periodic discovery runs, the playing stream dies, the
    // producer queues with nothing draining, `stop` lands, and the default
    // changes.
    let (shared, producer) = (Arc::clone(&rig.shared), rig.producer.clone());
    with(&w).during_discovery = Some(Box::new(move |w| {
        (w.streams[0].error)(cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable));
        assert_eq!(producer.push(&[0.25; 2]), 2);
        shared.set_running(false, &producer);
        assert_eq!(producer.push(&[0.5; 4]), 4);
        w.default = Some("b".into());
    }));
    let mut now = MS(1_000);
    for _ in 0..3 {
        now = rig.transport.step(now);
    }
    let builds = with(&w).builds;
    assert_eq!(builds, 2);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity() - 4,
        "history queued after the loss is discarded; the stopped queue is kept"
    );
}

#[test]
fn a_replacement_found_by_discovery_is_attached_without_querying_again() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    assert_eq!(with(&w).discoveries, 1);

    with(&w).default = Some("b".into());
    rig.transport.step(MS(1_000));
    let builds = with(&w).builds;
    assert_eq!(builds, 2);
    assert_eq!(rig.shared.state(), TransportState::Ready);
    let discoveries = with(&w).discoveries;
    assert_eq!(
        discoveries, 2,
        "the replacement the discovery returned is the one connected"
    );
}

#[test]
fn a_start_and_stop_between_steps_while_detached_still_discard_the_history() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    // Lost while stopped: the retry is a stopped reattach.
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.25; 2]), 2);

    // During the retry delay, before the worker steps again, playback is
    // started, the producer queues with nothing draining, and `stop` lands.
    rig.shared.set_running(true, &rig.producer);
    assert_eq!(rig.producer.push(&[0.25; 2]), 2);
    rig.shared.set_running(false, &rig.producer);
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);

    rig.transport.step(MS(120));
    assert_eq!(rig.shared.state(), TransportState::Ready);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity() - 4,
        "what queued before the stop is history; the stopped queue is kept"
    );
}

#[test]
fn a_start_and_stop_while_a_stopped_reattach_builds_discard_the_history() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.25; 2]), 2);

    let (shared, producer) = (Arc::clone(&rig.shared), rig.producer.clone());
    with(&w).during_build = Some(Box::new(move |_| {
        shared.set_running(true, &producer);
        assert_eq!(producer.push(&[0.25; 2]), 2);
        shared.set_running(false, &producer);
        assert_eq!(producer.push(&[0.5; 4]), 4);
    }));
    let mut now = MS(120);
    for _ in 0..3 {
        now = rig.transport.step(now);
    }
    assert_eq!(rig.shared.state(), TransportState::Ready);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity() - 4,
        "a start held while the stream was built makes the earlier queue history"
    );
}
