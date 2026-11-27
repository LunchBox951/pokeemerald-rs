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

#[test]
fn a_stop_after_a_started_reattach_but_before_its_first_callback_keeps_the_stopped_queue() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.set_running(true, &rig.producer);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    // The started retry attaches and plays, holding its first-read discard
    // for what queued while it was built.
    let now = rig.transport.step(MS(120));
    assert_eq!(rig.shared.state(), TransportState::Ready);
    assert_eq!(rig.producer.push(&[0.25; 2]), 2, "history from the build");

    // `stop` lands before the stream's first callback, the pause succeeds,
    // and the producer queues a prefill for the next start.
    rig.shared.set_running(false, &rig.producer);
    let now = rig.transport.step(now);
    let pauses = with(&w).pauses;
    assert_eq!(pauses, 1);
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);

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
fn a_start_counted_but_not_yet_published_is_not_acknowledged_by_a_stopped_reattach() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    // Lost while stopped: the retry is a stopped reattach.
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.25; 2]), 2, "stopped prefill");

    // `set_running(true)` paused after it counted the start and before it
    // published `running`.
    rig.shared.begin_start();
    let streams = with(&w).streams.len();
    let mut now = MS(120);
    for _ in 0..3 {
        now = rig.transport.step(now);
    }
    assert_eq!(
        with(&w).streams.len(),
        streams,
        "no stream is built on an unfinished start"
    );
    assert_eq!(rig.producer.available_space(), 6, "nothing is discarded");

    // The start finishes, history queues, and a stop leaves a new prefill.
    rig.shared.finish_start();
    assert_eq!(rig.producer.push(&[0.25; 2]), 2, "history while started");
    rig.shared.set_running(false, &rig.producer);
    assert_eq!(rig.producer.push(&[0.5; 4]), 4, "stopped queue");
    for _ in 0..3 {
        now = rig.transport.step(now);
    }
    assert_eq!(rig.shared.state(), TransportState::Ready);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity() - 4,
        "what queued before the stop is history; only the stopped queue is kept"
    );
}

#[test]
fn a_start_left_unfinished_while_a_stopped_reattach_builds_rebuilds_it_once_published() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    raise_error(&w, 0, cpal::ErrorKind::DeviceNotAvailable);
    rig.transport.step(MS(20));
    assert_eq!(rig.producer.push(&[0.25; 2]), 2, "stopped prefill");

    // The build blocks while `set_running(true)` counts a start and pauses
    // before publishing `running`.
    let shared = Arc::clone(&rig.shared);
    with(&w).during_build = Some(Box::new(move |_| shared.begin_start()));
    let mut now = MS(120);
    for _ in 0..3 {
        now = rig.transport.step(now);
    }
    assert_ne!(
        rig.shared.state(),
        TransportState::Ready,
        "the stream built across an unfinished start is not accepted"
    );

    rig.shared.finish_start();
    assert_eq!(rig.producer.push(&[0.25; 2]), 2, "history while started");
    rig.shared.set_running(false, &rig.producer);
    assert_eq!(rig.producer.push(&[0.5; 4]), 4, "stopped queue");
    for _ in 0..3 {
        now = rig.transport.step(now);
    }
    assert_eq!(rig.shared.state(), TransportState::Ready);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity() - 4,
        "what queued before the stop is history; only the stopped queue is kept"
    );
}

#[test]
fn a_start_unfinished_when_an_attached_stream_polls_is_applied_once_and_keeps_the_queue() {
    let w = world(|w| {
        w.default = Some("a".into());
        w.no_hw_pause = true;
    });
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);

    // The poll finds a start counted but not published: it applies nothing.
    rig.shared.begin_start();
    rig.transport.step(MS(20));
    assert_eq!(with(&w).plays, 0, "an unfinished start is not applied");
    rig.shared.finish_start();
    rig.transport.step(MS(40));
    assert_eq!(with(&w).plays, 1, "the finished start is applied");

    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    // `stop`: the pause is refused, so the worker rebuilds a paused stream.
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(60));
    rig.transport.step(MS(160));
    assert_eq!(with(&w).builds, 2);
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity() - 4,
        "a start the attached stream applied is no missed playback"
    );
}

#[test]
fn a_start_that_begins_during_a_play_call_and_finishes_later_keeps_the_queue_across_a_refused_pause(
) {
    let w = world(|w| {
        w.default = Some("a".into());
        w.no_hw_pause = true;
    });
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    rig.shared.set_running(true, &rig.producer);

    // While the stream's `play` runs, the producer stops and begins a second
    // start, which the poll after `play` cannot read yet.
    let (shared, producer) = (Arc::clone(&rig.shared), rig.producer.clone());
    with(&w).during_play = Some(Box::new(move |_| {
        shared.set_running(false, &producer);
        shared.begin_start();
    }));
    rig.transport.step(MS(20));
    assert_eq!(with(&w).plays, 1);

    // The second start finishes and queues, and `stop` lands before the next
    // poll; the stream was playing throughout, so nothing was missed.
    rig.shared.finish_start();
    assert_eq!(rig.producer.push(&[0.5; 4]), 4);
    rig.shared.set_running(false, &rig.producer);
    rig.transport.step(MS(40));
    rig.transport.step(MS(140));
    assert_eq!(with(&w).builds, 2, "the refused pause rebuilds the stream");
    assert_eq!(
        rig.producer.available_space(),
        rig.producer.capacity() - 4,
        "starts a playing stream saw through are no missed playback"
    );
}
