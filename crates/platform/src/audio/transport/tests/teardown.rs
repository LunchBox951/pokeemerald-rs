//! Dropping the output never waits long on a driver blocked in the audio
//! stack.

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
