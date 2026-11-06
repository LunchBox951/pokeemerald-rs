//! Dropping the output never waits long on a driver blocked in the audio
//! stack.

use std::time::Instant;

use super::*;

#[test]
fn dropping_the_output_does_not_wait_on_a_blocked_discovery() {
    let (release, blocked) = std::sync::mpsc::channel();
    let w = world(|w| w.block_discovery = Some(blocked));
    let output = AudioOutput::open_with_host(FakeHost(Arc::clone(&w)), 64).expect("worker spawns");
    // The driver's first step is a discovery, now blocked in the fake host.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while with(&w).block_discovery.is_some() {
        assert!(std::time::Instant::now() < deadline, "never discovered");
        std::thread::sleep(MS(1));
    }
    // Releases the wedged call well after any acceptable teardown wait.
    let unblock = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        drop(release);
    });

    let dropped = std::time::Instant::now();
    drop(output);
    let waited = dropped.elapsed();
    assert!(
        waited < Duration::from_secs(1),
        "drop waited {waited:?} on the blocked driver"
    );

    // The detached driver finishes and releases the host once the call
    // returns.
    unblock.join().unwrap();
    while Arc::strong_count(&w) > 1 {
        assert!(std::time::Instant::now() < deadline, "driver never exited");
        std::thread::sleep(MS(5));
    }
}

/// Waits until `blocked` has been taken by the driver, i.e. it is inside the
/// blocked call.
fn wait_until_blocked(w: &SharedWorld, blocked: impl Fn(&World) -> bool, deadline: Instant) {
    while blocked(&with(w)) {
        assert!(Instant::now() < deadline, "never reached the blocked call");
        std::thread::sleep(MS(1));
    }
}

fn wait_for_driver_exit(w: &SharedWorld, deadline: Instant) {
    while Arc::strong_count(w) > 1 {
        assert!(Instant::now() < deadline, "driver never exited");
        std::thread::sleep(MS(5));
    }
}

#[test]
fn a_driver_detached_while_discovering_never_plays_a_stream_after_drop() {
    let (release, blocked) = std::sync::mpsc::channel();
    let w = world(|w| {
        w.default = Some("a".into());
        w.block_discovery = Some(blocked);
    });
    let mut output =
        AudioOutput::open_with_host(FakeHost(Arc::clone(&w)), 64).expect("worker spawns");
    output.start().expect("start");
    let deadline = Instant::now() + Duration::from_secs(10);
    wait_until_blocked(&w, |w| w.block_discovery.is_some(), deadline);
    drop(output);
    // The wedged discovery returns only after the output is gone.
    drop(release);
    wait_for_driver_exit(&w, deadline);
    let w = with(&w);
    assert_eq!(
        (w.builds, w.plays),
        (0, 0),
        "a dropped output's detached driver built and played a stream"
    );
}

#[test]
fn a_driver_detached_while_building_never_plays_the_stream_and_releases_it() {
    let (release, blocked) = std::sync::mpsc::channel();
    let w = world(|w| {
        w.default = Some("a".into());
        w.block_build = Some(blocked);
    });
    let mut output =
        AudioOutput::open_with_host(FakeHost(Arc::clone(&w)), 64).expect("worker spawns");
    let producer = output.producer();
    output.start().expect("start");
    let deadline = Instant::now() + Duration::from_secs(10);
    wait_until_blocked(&w, |w| w.block_build.is_some(), deadline);
    drop(output);
    drop(release);
    wait_for_driver_exit(&w, deadline);
    {
        let w = with(&w);
        assert_eq!(w.plays, 0, "a dropped output's driver played its stream");
        assert_eq!(w.log, ["build", "drop"], "the built stream is released");
    }
    assert!(
        producer.reclaim_consumer().is_some(),
        "the abandoned stream released the consumer"
    );
}

#[test]
fn a_cancelled_failed_build_is_neither_published_nor_counted() {
    let (release, blocked) = std::sync::mpsc::channel();
    let w = world(|w| {
        w.default = Some("a".into());
        w.fail_build = true;
        w.block_build = Some(blocked);
    });
    let mut rig = rig(&w, 4);
    let shared = Arc::clone(&rig.shared);
    let stepping = std::thread::spawn(move || {
        rig.transport.step(Duration::ZERO);
        rig
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    wait_until_blocked(&w, |w| w.block_build.is_some(), deadline);
    // The owner drops while the build is wedged; the build then fails.
    shared.shutdown.store(true, Ordering::Release);
    drop(release);
    let rig = stepping.join().unwrap();
    assert_eq!(rig.events(), []);
    assert_eq!(rig.shared.state(), TransportState::Connecting);
    assert_eq!(rig.transport.failures, 0);
    assert!(rig.producer.reclaim_consumer().is_some());
}

/// Sets shutdown from inside a blocking call of a driven transport, as a
/// dropped output would, and returns what the transport did.
fn shutdown_during(hook: &str) -> (Rig, SharedWorld) {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.shared.set_running(true, &rig.producer);
    let shared = Arc::clone(&rig.shared);
    let stop: WorldHook = Box::new(move |_| shared.shutdown.store(true, Ordering::Release));
    match hook {
        "discovery" => with(&w).during_discovery = Some(stop),
        "build" => with(&w).during_build = Some(stop),
        "play" => with(&w).during_play = Some(stop),
        _ => unreachable!(),
    }
    rig.transport.step(Duration::ZERO);
    (rig, w)
}

#[test]
fn shutdown_seen_after_discovery_attempts_nothing_and_publishes_nothing() {
    let (rig, w) = shutdown_during("discovery");
    let (ops, builds) = {
        let w = with(&w);
        (w.ops, w.builds)
    };
    assert_eq!((ops, builds), (1, 0));
    assert_eq!(rig.events(), []);
    assert_eq!(rig.shared.state(), TransportState::Connecting);
    assert_eq!(rig.transport.failures, 0);
}

#[test]
fn shutdown_seen_after_build_drops_the_stream_unplayed_and_unpublished() {
    let (rig, w) = shutdown_during("build");
    let w = with(&w);
    assert_eq!((w.builds, w.plays), (1, 0));
    assert_eq!(w.log, ["build", "drop"]);
    assert_eq!(rig.events(), []);
    assert!(rig.transport.live.is_none());
    assert_eq!(rig.transport.failures, 0);
    assert!(rig.producer.reclaim_consumer().is_some());
}

#[test]
fn shutdown_seen_after_play_drops_the_stream_unpublished() {
    let (rig, w) = shutdown_during("play");
    assert_eq!(with(&w).log, ["build", "drop"]);
    assert_eq!(rig.events(), []);
    assert!(rig.transport.live.is_none());
    assert_eq!(rig.transport.failures, 0);
    assert_eq!(
        rig.shared.device_sample_rate.load(Ordering::Acquire),
        13_379
    );
}

#[test]
fn shutdown_seen_after_a_supervision_discovery_keeps_the_stream_and_publishes_nothing() {
    let w = world(|w| w.default = Some("a".into()));
    let mut rig = rig(&w, 4);
    rig.transport.step(Duration::ZERO);
    rig.events();
    // A changed default would retire and rebuild if shutdown went unseen.
    let shared = Arc::clone(&rig.shared);
    {
        let mut w = with(&w);
        w.default = Some("b".into());
        w.during_discovery = Some(Box::new(move |_| {
            shared.shutdown.store(true, Ordering::Release);
        }));
    }
    rig.transport.step(MS(1_000));
    assert_eq!(rig.events(), []);
    assert_eq!(with(&w).builds, 1);
    assert_eq!(with(&w).log, ["build"]);
}
