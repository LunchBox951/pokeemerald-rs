//! Playback past a stream error: one the transport recovered from (an XRUN on
//! the attached stream) keeps playing, one that detached the stream ends it.

use super::*;
use platform::audio::{TransportFailure, TransportState};
use std::collections::VecDeque;
use std::time::Duration;

/// The lifetime error count when playback began in every test below.
const BASELINE_ERRORS: u64 = 7;
const RING_SAMPLES: usize = 8;
const FRAME: [f32; RING_SAMPLES] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

/// A transport, its error counter, and a ring the test drains by hand.
struct FakeStream {
    state: Cell<TransportState>,
    published: RefCell<VecDeque<TransportState>>,
    errors: Cell<u64>,
    queued: RefCell<VecDeque<f32>>,
    played: RefCell<Vec<f32>>,
}

impl FakeStream {
    /// A ready stream holding `queued` samples, with the error count at
    /// [`BASELINE_ERRORS`], and the health watching it from there.
    fn new(queued: &[f32]) -> (Self, PlaybackHealth) {
        let stream = Self {
            state: Cell::new(TransportState::Ready),
            published: RefCell::default(),
            errors: Cell::new(BASELINE_ERRORS),
            queued: RefCell::new(queued.iter().copied().collect()),
            played: RefCell::default(),
        };
        let health = PlaybackHealth::begin_with(
            &mut (),
            |()| stream.published.borrow_mut().pop_front(),
            |()| stream.errors.get(),
        );
        (stream, health)
    }

    fn terminal(&self, health: &mut PlaybackHealth) -> Option<u64> {
        health.terminal_errors_with(
            &mut (),
            |()| self.published.borrow_mut().pop_front(),
            |()| self.state.get(),
            |()| self.errors.get(),
        )
    }

    fn push(&self, chunk: &[f32]) -> usize {
        let mut queued = self.queued.borrow_mut();
        let accepted = chunk.len().min(RING_SAMPLES - queued.len());
        queued.extend(&chunk[..accepted]);
        accepted
    }

    fn available_space(&self) -> usize {
        RING_SAMPLES - self.queued.borrow().len()
    }

    fn play(&self, count: usize) {
        let mut queued = self.queued.borrow_mut();
        let count = count.min(queued.len());
        let sounded: Vec<f32> = queued.drain(..count).collect();
        self.played.borrow_mut().extend(sounded);
    }

    /// An XRUN: counted by the callback, recovered on the same stream.
    fn xrun(&self) {
        self.errors.set(self.errors.get() + 1);
    }

    /// The worker retires a stream whose fatal error was already counted.
    fn retire_counted_stream(&self) {
        let lost = TransportState::Unavailable(TransportFailure::StreamLost);
        self.published.borrow_mut().push_back(lost);
        self.state.set(lost);
    }

    /// The stream fails and is detached. With `rebuilt`, the worker has
    /// already attached a new one by the time the application looks.
    fn lose_stream(&self, rebuilt: bool) {
        self.errors.set(self.errors.get() + 1);
        let lost = TransportState::Unavailable(TransportFailure::StreamLost);
        let mut published = self.published.borrow_mut();
        published.push_back(lost);
        if rebuilt {
            published.push_back(TransportState::Ready);
            self.state.set(TransportState::Ready);
        } else {
            self.state.set(lost);
        }
    }
}

fn policy() -> RetryPolicy {
    RetryPolicy {
        interval: Duration::from_millis(1),
        max_wait: Duration::from_mins(1),
    }
}

/// A frame pushed into a ring holding six samples: the first push fits two
/// and `fault` strikes during it.
fn push_through(fault: impl Fn(&FakeStream)) -> (Result<(), PushError>, FakeStream, u32) {
    let (stream, mut health) = FakeStream::new(&[-1.0; 6]);
    let (pushes, sleeps) = (Cell::new(0_u32), Cell::new(0_u32));
    let start = Instant::now();
    let result = push_frame(
        &FRAME,
        &policy(),
        |chunk| {
            pushes.set(pushes.get() + 1);
            let accepted = stream.push(chunk);
            if pushes.get() == 1 {
                fault(&stream);
            }
            accepted
        },
        || stream.terminal(&mut health),
        || start,
        |_| {
            sleeps.set(sleeps.get() + 1);
            stream.play(RING_SAMPLES);
        },
    );
    assert_eq!(pushes.get(), if result.is_ok() { 2 } else { 1 });
    (result, stream, sleeps.get())
}

#[test]
fn a_recovered_error_mid_enqueue_keeps_enqueueing_and_retains_the_audio() {
    let (result, stream, sleeps) = push_through(FakeStream::xrun);

    assert!(result.is_ok(), "an XRUN the stream recovered from plays on");
    assert_eq!(sleeps, 1, "the full ring is waited out, not abandoned");
    stream.play(RING_SAMPLES);
    let expected: Vec<f32> = [-1.0; 6].into_iter().chain(FRAME).collect();
    assert_eq!(*stream.played.borrow(), expected, "no sample is dropped");
    assert_eq!(stream.errors.get(), BASELINE_ERRORS + 1);
}

#[test]
fn a_detached_stream_mid_enqueue_stops_playback_with_the_dropped_samples() {
    for rebuilt in [false, true] {
        let (result, ..) = push_through(|stream| stream.lose_stream(rebuilt));

        assert!(
            matches!(
                result,
                Err(PushError::StreamStopped {
                    errors: 1,
                    dropped: 6
                })
            ),
            "a stream loss (rebuilt: {rebuilt}) must stop the enqueue"
        );
    }
}

/// A ring holding six samples drained over two sleeps, with `fault` striking
/// during the first, which sounds four of them.
fn drain_through(fault: impl Fn(&FakeStream)) -> (Result<(), DrainError>, FakeStream, u32) {
    let (stream, mut health) = FakeStream::new(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let sleeps = Cell::new(0_u32);
    let start = Instant::now();
    let result = wait_for_drain(
        RING_SAMPLES,
        &policy(),
        || stream.available_space(),
        || stream.terminal(&mut health),
        || start,
        |_| {
            sleeps.set(sleeps.get() + 1);
            if sleeps.get() == 1 {
                stream.play(4);
                fault(&stream);
            } else {
                stream.play(RING_SAMPLES);
            }
        },
    );
    (result, stream, sleeps.get())
}

#[test]
fn a_recovered_error_mid_drain_keeps_draining_until_the_ring_empties() {
    let (result, stream, sleeps) = drain_through(FakeStream::xrun);

    assert!(result.is_ok(), "an XRUN the stream recovered from plays on");
    assert_eq!(sleeps, 2, "the drain outlasts the error");
    assert_eq!(*stream.played.borrow(), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
}

#[test]
fn a_detached_stream_mid_drain_stops_with_the_audio_still_queued() {
    for rebuilt in [false, true] {
        let (result, _, sleeps) = drain_through(|stream| stream.lose_stream(rebuilt));

        assert!(
            matches!(
                result,
                Err(DrainError::StreamStopped {
                    errors: 1,
                    remaining: 2
                })
            ),
            "a stream loss (rebuilt: {rebuilt}) must stop the drain"
        );
        assert_eq!(sleeps, 1);
    }
}

#[test]
fn a_recovered_error_during_the_device_tail_still_finishes() {
    let (stream, mut health) = FakeStream::new(&[]);
    let tail = Duration::from_millis(200);
    let mut slept = Vec::new();

    let result = wait_for_device_tail(
        tail,
        || stream.terminal(&mut health),
        |duration| {
            slept.push(duration);
            stream.xrun();
        },
    );

    assert!(result.is_ok(), "the last buffer played despite the XRUN");
    assert_eq!(slept, [tail, SUPERVISION_GRACE]);
}

#[test]
fn a_detached_stream_during_the_device_tail_fails() {
    let (stream, mut health) = FakeStream::new(&[]);

    let result = wait_for_device_tail(
        Duration::from_millis(200),
        || stream.terminal(&mut health),
        |_| stream.lose_stream(true),
    );

    assert!(matches!(
        result,
        Err(DrainError::StreamStoppedDuringTail { errors: 1 })
    ));
}

#[test]
fn a_recovered_error_during_the_measured_tail_still_finishes() {
    let (stream, mut health) = FakeStream::new(&[]);
    let clock = Cell::new(Instant::now());
    let sleeps = Cell::new(0_u32);

    let result = wait_for_measured_tail(
        4,
        Duration::from_millis(200),
        48_000,
        &policy(),
        || {
            // The device sounds the target once the first sleep has passed.
            Some(if sleeps.get() == 0 {
                progress(0, 0)
            } else {
                progress(4, 4)
            })
        },
        || stream.terminal(&mut health),
        || clock.get(),
        |duration| {
            sleeps.set(sleeps.get() + 1);
            clock.set(clock.get() + duration);
            stream.xrun();
        },
    );

    assert!(result.is_ok(), "the target sounded despite the XRUN");
    assert_eq!(sleeps.get(), 1);
}

#[test]
fn a_detached_stream_during_the_measured_tail_fails() {
    let (stream, mut health) = FakeStream::new(&[]);
    let start = Instant::now();

    let result = wait_for_measured_tail(
        4,
        Duration::from_millis(200),
        48_000,
        &policy(),
        || Some(progress(0, 0)),
        || {
            stream.lose_stream(false);
            stream.terminal(&mut health)
        },
        || start,
        |_| panic!("a lost stream must abort before any retry sleep"),
    );

    assert!(matches!(
        result,
        Err(DrainError::StreamStoppedDuringTail { errors: 1 })
    ));
}

#[test]
fn errors_from_before_playback_began_stay_out_of_the_terminal_count() {
    let (stream, mut health) = FakeStream::new(&[]);
    assert_eq!(stream.terminal(&mut health), None);

    stream.xrun();
    assert_eq!(stream.terminal(&mut health), None, "an XRUN is recovered");
    stream.lose_stream(true);
    assert_eq!(
        stream.terminal(&mut health),
        Some(2),
        "counted from the baseline"
    );
    assert_eq!(
        stream.terminal(&mut health),
        Some(2),
        "a rebuilt stream does not clear the interruption"
    );
}

#[test]
fn a_stream_lost_without_a_callback_error_still_ends_playback_after_a_rebuild() {
    // A refused `play` retires the stream with no error counted; the
    // replacement attaches, but the audio queued meanwhile is discarded.
    let (stream, mut health) = FakeStream::new(&[-1.0; 6]);
    {
        let mut published = stream.published.borrow_mut();
        published.push_back(TransportState::Unavailable(TransportFailure::StreamLost));
        published.push_back(TransportState::Ready);
    }
    let start = Instant::now();

    let result = push_frame(
        &FRAME,
        &policy(),
        |chunk| stream.push(chunk),
        || stream.terminal(&mut health),
        || start,
        |_| panic!("a lost stream must abort before any retry sleep"),
    );

    assert!(matches!(
        result,
        Err(PushError::StreamStopped {
            errors: 0,
            dropped: 8
        })
    ));
    assert_eq!(stream.errors.get(), BASELINE_ERRORS);
}

#[test]
fn a_fatal_error_counted_before_the_worker_publishes_still_fails_the_tail() {
    let (stream, mut health) = FakeStream::new(&[]);
    let sleeps = Cell::new(0_u32);

    let result = wait_for_device_tail(
        Duration::from_millis(200),
        || stream.terminal(&mut health),
        |_| {
            sleeps.set(sleeps.get() + 1);
            if sleeps.get() == 1 {
                // The callback counts a fatal error and raises `lost`; the
                // worker has not supervised yet, so nothing is published.
                stream.xrun();
            } else {
                // The worker's next supervise tick retires the stream.
                stream.retire_counted_stream();
            }
        },
    );

    assert!(
        matches!(
            result,
            Err(DrainError::StreamStoppedDuringTail { errors: 1 })
        ),
        "a fatal error the worker had not yet published was reported as success: ok={}",
        result.is_ok()
    );
}

#[test]
fn a_fatal_error_counted_before_the_worker_publishes_still_fails_the_measured_tail() {
    let (stream, mut health) = FakeStream::new(&[]);
    let clock = Cell::new(Instant::now());
    let sleeps = Cell::new(0_u32);

    let result = wait_for_device_tail_or_measured(
        Some(4),
        Duration::from_millis(200),
        48_000,
        &policy(),
        || {
            // The target sounds just as the callback counts a fatal error.
            stream.xrun();
            Some(progress(4, 4))
        },
        || stream.terminal(&mut health),
        || clock.get(),
        |duration| {
            sleeps.set(sleeps.get() + 1);
            clock.set(clock.get() + duration);
            stream.retire_counted_stream();
        },
    );

    assert!(matches!(
        result,
        Err(DrainError::StreamStoppedDuringTail { errors: 1 })
    ));
}
