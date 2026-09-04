//! Local smoke tool: build a tiny hand-authored song and play it through the
//! real `platform::AudioOutput` device.
//!
//! `main` is a manual, not-run-in-CI "does sound actually come out?" check —
//! on a headless machine with no audio device it prints a note and exits
//! cleanly. The helper tests below do run under `cargo test`; see this
//! crate's `Cargo.toml`.
//!
//! Run with: `cargo run -p audio --example play_song`.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use audio::{decode_track, Adsr, Instrument, Sequencer, Song, ToneData, WaveData, MIXER_RATE};
use platform::AudioOutput;

const RING_CAPACITY_FRAMES: usize = 4096;

/// Comfortably longer than the ~306 ms the ring can absorb at [`MIXER_RATE`],
/// but short enough that a dead callback fails fast instead of hanging this
/// manual smoke command.
const RETRY_MAX_WAIT: Duration = Duration::from_secs(1);

/// Added to the device's own callback bound in [`device_tail_wait`] to cover
/// the resampler's one-frame lookahead and scheduler jitter.
const DEVICE_TAIL_MARGIN: Duration = Duration::from_millis(50);

/// The tail wait when the device advertises no callback size at all: longer
/// than any common callback buffer, short enough not to drag the smoke run.
const DEVICE_TAIL_FALLBACK: Duration = Duration::from_millis(200);

fn main() -> ExitCode {
    let song = build_song();
    let mut seq = Sequencer::new(song);

    let mut output = match AudioOutput::open(RING_CAPACITY_FRAMES) {
        Ok(output) => output,
        Err(err) => {
            println!("no audio device ({err}); nothing to play — this is expected in CI/headless");
            return ExitCode::SUCCESS;
        }
    };
    output.start().expect("start playback");
    println!("playing a short scale at {MIXER_RATE} Hz — Ctrl-C to stop");

    let producer = output.producer();
    let ring_capacity_samples = RING_CAPACITY_FRAMES * usize::from(output.channels());
    let frame_samples = u32::try_from(audio::SAMPLES_PER_FRAME).expect("frame fits u32");
    let frame_period = Duration::from_secs_f64(f64::from(frame_samples) / f64::from(MIXER_RATE));
    let policy = RetryPolicy {
        interval: frame_period / 4,
        max_wait: RETRY_MAX_WAIT,
    };
    let mut buffer = vec![0.0_f32; Sequencer::FRAME_SAMPLES];

    // Render frame by frame, pacing to real time, until the song finishes.
    while !seq.is_finished() {
        seq.render_frame(&mut buffer);
        let pushed = push_frame(
            &buffer,
            &policy,
            |chunk| producer.push(chunk),
            || output.stream_errors(),
            Instant::now,
            std::thread::sleep,
        );
        if let Err(err) = pushed {
            eprintln!("audio playback stopped: {}", err.describe());
            return ExitCode::FAILURE;
        }
        std::thread::sleep(frame_period);
    }
    if let Err(err) = wait_for_drain(
        ring_capacity_samples,
        &policy,
        || producer.available_space(),
        || output.stream_errors(),
        Instant::now,
        std::thread::sleep,
    ) {
        eprintln!("audio playback stopped: {}", err.describe());
        return ExitCode::FAILURE;
    }
    let tail = device_tail_wait(output.max_callback_frames(), output.device_sample_rate());
    if let Err(err) = wait_for_device_tail(tail, || output.stream_errors(), std::thread::sleep) {
        eprintln!("audio playback stopped: {}", err.describe());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Bounds on how long [`push_frame`] and [`wait_for_drain`] keep retrying.
struct RetryPolicy {
    interval: Duration,
    max_wait: Duration,
}

/// Why [`push_frame`] gave up before queuing every sample.
#[derive(Clone, Copy)]
enum PushError {
    /// `AudioOutput::stream_errors` went nonzero: the device callback has
    /// stopped draining the ring, so retrying cannot help.
    StreamStopped { errors: u64, dropped: usize },
    /// `RetryPolicy::max_wait` elapsed with no stream error reported.
    DeadlineExceeded { dropped: usize },
}

impl PushError {
    fn describe(&self) -> String {
        match *self {
            PushError::StreamStopped { errors, dropped } => format!(
                "{errors} asynchronous stream error(s) reported; {dropped} sample(s) from the \
                 current frame were not queued"
            ),
            PushError::DeadlineExceeded { dropped } => format!(
                "no progress queuing the ring buffer before the {:.1}s retry deadline; \
                 {dropped} sample(s) from the current frame were not queued",
                RETRY_MAX_WAIT.as_secs_f64()
            ),
        }
    }
}

/// Push all of `samples` via `push`, retrying a momentarily full ring within
/// `policy`.
///
/// On either error the unqueued tail is dropped rather than blocked on — the
/// accounting rule [`platform::Producer::push`] documents. `push`,
/// `stream_errors`, `now`, and `sleep` are injected so the tests below need
/// no audio device or wall clock.
fn push_frame(
    samples: &[f32],
    policy: &RetryPolicy,
    mut push: impl FnMut(&[f32]) -> usize,
    mut stream_errors: impl FnMut() -> u64,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<(), PushError> {
    let deadline = now() + policy.max_wait;
    let mut queued = 0;
    loop {
        let errors = stream_errors();
        if errors > 0 {
            return Err(PushError::StreamStopped {
                errors,
                dropped: samples.len() - queued,
            });
        }
        queued += push(&samples[queued..]);
        if queued >= samples.len() {
            return Ok(());
        }
        // Re-check: an async error can land between the check above and this
        // push completing, and naming it beats falling through to the
        // deadline.
        let errors = stream_errors();
        if errors > 0 {
            return Err(PushError::StreamStopped {
                errors,
                dropped: samples.len() - queued,
            });
        }
        if now() >= deadline {
            return Err(PushError::DeadlineExceeded {
                dropped: samples.len() - queued,
            });
        }
        sleep(policy.interval);
    }
}

/// Why [`wait_for_drain`] gave up before confirming the ring emptied.
#[derive(Clone, Copy)]
enum DrainError {
    /// `AudioOutput::stream_errors` went nonzero while samples were still
    /// queued and unplayed.
    StreamStopped { errors: u64, remaining: usize },
    /// `RetryPolicy::max_wait` elapsed with no stream error reported.
    DeadlineExceeded { remaining: usize },
    /// `AudioOutput::stream_errors` went nonzero after the ring emptied,
    /// while the device was still playing its final buffer.
    StreamStoppedDuringTail { errors: u64 },
}

impl DrainError {
    fn describe(&self) -> String {
        match *self {
            DrainError::StreamStopped { errors, remaining } => format!(
                "{errors} asynchronous stream error(s) reported while {remaining} sample(s) were \
                 still queued and unplayed"
            ),
            DrainError::StreamStoppedDuringTail { errors } => format!(
                "{errors} asynchronous stream error(s) reported while the device played its \
                 final buffer"
            ),
            DrainError::DeadlineExceeded { remaining } => format!(
                "no drain progress before the {:.1}s retry deadline; {remaining} sample(s) were \
                 still queued and unplayed",
                RETRY_MAX_WAIT.as_secs_f64()
            ),
        }
    }
}

/// Wait, within `policy`, for `available_space` to report the ring fully
/// drained.
///
/// A stopped callback must never read as a successful finish, so a nonzero
/// `stream_errors` outranks an empty ring — hence the read order below.
/// Callbacks are injected as in [`push_frame`].
fn wait_for_drain(
    capacity: usize,
    policy: &RetryPolicy,
    mut available_space: impl FnMut() -> usize,
    mut stream_errors: impl FnMut() -> u64,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<(), DrainError> {
    let deadline = now() + policy.max_wait;
    loop {
        // Read `available_space` before `stream_errors`: reading errors
        // first could pair a stale, pre-drain count with the freshly-emptied
        // ring and report success for a stream that had gone unhealthy.
        let remaining = capacity.saturating_sub(available_space());
        let errors = stream_errors();
        if errors > 0 {
            return Err(DrainError::StreamStopped { errors, remaining });
        }
        if remaining == 0 {
            return Ok(());
        }
        if now() >= deadline {
            return Err(DrainError::DeadlineExceeded { remaining });
        }
        sleep(policy.interval);
    }
}

/// How long the device may still be playing after the ring reads empty:
/// its largest advertised callback buffer at its own rate, plus
/// [`DEVICE_TAIL_MARGIN`]; [`DEVICE_TAIL_FALLBACK`] when it advertises none.
/// An empty ring only means the callback took the samples, and dropping
/// `AudioOutput` closes the stream rather than draining it.
fn device_tail_wait(max_callback_frames: Option<usize>, device_sample_rate: u32) -> Duration {
    match max_callback_frames {
        Some(frames) if device_sample_rate > 0 => {
            let frames = u32::try_from(frames).unwrap_or(u32::MAX);
            Duration::from_secs_f64(f64::from(frames) / f64::from(device_sample_rate))
                + DEVICE_TAIL_MARGIN
        }
        _ => DEVICE_TAIL_FALLBACK,
    }
}

/// Hold the stream open for `tail`, then re-read `stream_errors`: a device
/// error during the tail still means the last buffer never played.
fn wait_for_device_tail(
    tail: Duration,
    mut stream_errors: impl FnMut() -> u64,
    mut sleep: impl FnMut(Duration),
) -> Result<(), DrainError> {
    sleep(tail);
    match stream_errors() {
        0 => Ok(()),
        errors => Err(DrainError::StreamStoppedDuringTail { errors }),
    }
}

/// A short ascending scale played on a looping square-wave instrument.
fn build_song() -> Song {
    // A 64-sample square wave; `freq` chosen so key 60 renders near unity.
    let mut data = vec![90_i8; 64];
    for sample in data.iter_mut().skip(32) {
        *sample = -90;
    }
    let wave = Arc::new(WaveData::looping(13_697_024, 0, data));
    let instrument = ToneData::new(
        wave,
        Adsr {
            attack: 0xFF,
            decay: 0xF0,
            sustain: 0xA0,
            release: 0xE0,
        },
    );

    // VOICE 0; VOL 110; then C-D-E-F-G-A-B-C quarter notes (key 60..72) each
    // followed by a quarter-note wait; FINE.
    let mut bytes = vec![0xBD, 0x00, 0xBE, 110];
    for key in [60_u8, 62, 64, 65, 67, 69, 71, 72] {
        bytes.push(0xE7); // N24 (quarter note)
        bytes.push(key);
        bytes.push(0x7F); // velocity
        bytes.push(0x98); // W24
    }
    bytes.push(0xB1); // FINE

    let events = decode_track(&bytes).expect("valid demo track");
    Song::new(vec![Instrument::DirectSound(instrument)], vec![events], 120)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use platform::AudioOutput;

    use super::{
        device_tail_wait, push_frame, wait_for_device_tail, wait_for_drain, DrainError, PushError,
        RetryPolicy, DEVICE_TAIL_FALLBACK, DEVICE_TAIL_MARGIN,
    };

    #[test]
    fn a_stream_error_aborts_the_retry_without_waiting_out_the_deadline() {
        let push_calls = Cell::new(0_u32);
        let policy = RetryPolicy {
            interval: std::time::Duration::from_millis(1),
            max_wait: std::time::Duration::from_mins(1),
        };
        let samples = [0.0_f32; 4];
        let start = std::time::Instant::now();

        let result = push_frame(
            &samples,
            &policy,
            |_chunk| {
                push_calls.set(push_calls.get() + 1);
                0
            },
            // Healthy on the pre-push check, unhealthy immediately after —
            // the "error lands mid-push" case the double check exists for.
            || u64::from(push_calls.get() > 0),
            || start,
            |_| panic!("a stream error must abort before any retry sleep"),
        );

        assert!(
            matches!(
                result,
                Err(PushError::StreamStopped {
                    errors: 1,
                    dropped: 4
                })
            ),
            "expected a StreamStopped error dropping the whole frame"
        );
        assert_eq!(push_calls.get(), 1, "must not retry once the stream errors");
    }

    #[test]
    fn a_stalled_ring_with_no_stream_error_gives_up_at_the_deadline() {
        // A real (but headless) ring buffer, filled completely and never
        // drained — `Producer::push` genuinely returns 0 forever, the same
        // as a stopped device callback that never reports a stream error.
        let output = AudioOutput::null(1);
        let producer = output.producer();
        assert_eq!(producer.push(&[0.0; 2]), 2, "fill the null ring solid");

        let policy = RetryPolicy {
            interval: std::time::Duration::from_millis(10),
            max_wait: std::time::Duration::from_millis(30),
        };
        let clock = Rc::new(RefCell::new(std::time::Instant::now()));
        let sleeps = Cell::new(0_u32);

        let result = push_frame(
            &[0.0_f32; 2],
            &policy,
            |chunk| producer.push(chunk),
            || output.stream_errors(),
            || *clock.borrow(),
            |duration| {
                sleeps.set(sleeps.get() + 1);
                *clock.borrow_mut() += duration;
            },
        );

        assert!(
            matches!(result, Err(PushError::DeadlineExceeded { dropped: 2 })),
            "a permanently full ring with no stream error must time out, not hang"
        );
        assert_eq!(output.stream_errors(), 0, "the null backend never errors");
        assert!(
            sleeps.get() > 0,
            "must have retried at least once before giving up"
        );
    }

    #[test]
    fn a_push_that_completes_before_the_deadline_succeeds() {
        let output = AudioOutput::null(64);
        let producer = output.producer();
        let policy = RetryPolicy {
            interval: std::time::Duration::from_millis(1),
            max_wait: std::time::Duration::from_secs(1),
        };

        let result = push_frame(
            &[0.0_f32; 4],
            &policy,
            |chunk| producer.push(chunk),
            || output.stream_errors(),
            std::time::Instant::now,
            |_| panic!("plenty of room; must not need to retry"),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn wait_for_drain_succeeds_immediately_once_the_ring_is_already_empty() {
        let policy = RetryPolicy {
            interval: std::time::Duration::from_millis(1),
            max_wait: std::time::Duration::from_secs(1),
        };

        let result = wait_for_drain(
            4,
            &policy,
            || 4, // fully free: nothing queued
            || 0,
            std::time::Instant::now,
            |_| panic!("an already-empty ring must not need to retry"),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn an_empty_ring_with_a_stream_error_is_not_a_successful_finish() {
        // A callback that dequeues the last samples and then reports an
        // asynchronous device failure must not read as a successful drain
        // just because the ring happens to be empty.
        let policy = RetryPolicy {
            interval: std::time::Duration::from_millis(1),
            max_wait: std::time::Duration::from_mins(1),
        };
        let start = std::time::Instant::now();

        let result = wait_for_drain(
            4,
            &policy,
            || 4, // fully free: the ring drained
            || 1, // but the stream is already unhealthy
            || start,
            |_| panic!("a stream error must abort before any retry sleep"),
        );

        assert!(
            matches!(
                result,
                Err(DrainError::StreamStopped {
                    errors: 1,
                    remaining: 0
                })
            ),
            "an empty ring must not mask a reported stream error"
        );
    }

    #[test]
    fn an_error_that_lands_exactly_as_the_ring_reports_empty_is_still_caught() {
        // The real race this guards: the callback drains the last sample and
        // raises a stream error in the same instant. Here `available_space`
        // itself is what makes the error visible, so a `stream_errors` read
        // taken *before* `available_space` would still observe the old,
        // healthy count and wrongly report success once it sees the ring
        // empty.
        let errors = Rc::new(Cell::new(0_u64));
        let errors_probe = Rc::clone(&errors);
        let policy = RetryPolicy {
            interval: std::time::Duration::from_millis(1),
            max_wait: std::time::Duration::from_mins(1),
        };
        let start = std::time::Instant::now();

        let result = wait_for_drain(
            4,
            &policy,
            move || {
                errors_probe.set(1);
                4 // fully free: the ring drained in the same instant
            },
            move || errors.get(),
            || start,
            |_| panic!("a stream error must abort before any retry sleep"),
        );

        assert!(
            matches!(
                result,
                Err(DrainError::StreamStopped {
                    errors: 1,
                    remaining: 0
                })
            ),
            "an error surfacing exactly as the ring empties must not be missed"
        );
    }

    #[test]
    fn wait_for_drain_reports_a_stream_error_instead_of_waiting_out_the_deadline() {
        let policy = RetryPolicy {
            interval: std::time::Duration::from_millis(1),
            max_wait: std::time::Duration::from_mins(1),
        };
        let start = std::time::Instant::now();

        let result = wait_for_drain(
            4,
            &policy,
            || 0, // the ring never drains
            || 1, // already unhealthy
            || start,
            |_| panic!("a stream error must abort before any retry sleep"),
        );

        assert!(
            matches!(
                result,
                Err(DrainError::StreamStopped {
                    errors: 1,
                    remaining: 4
                })
            ),
            "expected a StreamStopped error naming every sample still queued"
        );
    }

    #[test]
    fn wait_for_drain_times_out_when_the_ring_never_empties_and_reports_no_stream_error() {
        // A real (but headless) ring buffer, filled completely and never
        // drained: `available_space` genuinely stays at 0 forever, the same
        // as a device callback that stopped consuming without ever
        // reporting a stream error. Checking only `stream_errors() == 0`
        // here would declare success regardless — this is exactly the case
        // that let a stopped callback pass as a successful finish.
        let output = AudioOutput::null(1);
        let producer = output.producer();
        assert_eq!(producer.push(&[0.0; 2]), 2, "fill the null ring solid");

        let policy = RetryPolicy {
            interval: std::time::Duration::from_millis(10),
            max_wait: std::time::Duration::from_millis(30),
        };
        let clock = Rc::new(RefCell::new(std::time::Instant::now()));

        let result = wait_for_drain(
            2,
            &policy,
            || producer.available_space(),
            || output.stream_errors(),
            || *clock.borrow(),
            |duration| *clock.borrow_mut() += duration,
        );

        assert!(
            matches!(result, Err(DrainError::DeadlineExceeded { remaining: 2 })),
            "a permanently full ring with no stream error must time out, not report success"
        );
        assert_eq!(output.stream_errors(), 0, "the null backend never errors");
    }

    #[test]
    fn a_stream_error_during_the_device_tail_is_not_a_successful_finish() {
        let mut slept = Vec::new();
        let mut errors_seen = 0_u64;

        let result = wait_for_device_tail(
            std::time::Duration::from_millis(200),
            || {
                // The disconnect lands while the device plays its last buffer:
                // the counter is clean when the drain ended and nonzero after
                // the tail wait.
                errors_seen += 1;
                errors_seen
            },
            |d| slept.push(d),
        );

        assert_eq!(slept, [std::time::Duration::from_millis(200)]);
        assert!(matches!(
            result,
            Err(DrainError::StreamStoppedDuringTail { errors: 1 })
        ));
    }

    #[test]
    fn a_clean_device_tail_finishes_successfully() {
        let result = wait_for_device_tail(std::time::Duration::from_millis(200), || 0, |_| {});

        assert!(result.is_ok());
    }

    #[test]
    fn the_device_tail_is_derived_from_the_advertised_callback_bound() {
        // 24 000 frames at 48 kHz is half a second of queued audio: longer
        // than the fixed fallback, which would have clipped it.
        let tail = device_tail_wait(Some(24_000), 48_000);
        assert_eq!(
            tail,
            std::time::Duration::from_millis(500) + DEVICE_TAIL_MARGIN
        );
        assert!(tail > DEVICE_TAIL_FALLBACK);
    }

    #[test]
    fn an_unknown_callback_bound_falls_back_to_the_fixed_tail() {
        assert_eq!(device_tail_wait(None, 48_000), DEVICE_TAIL_FALLBACK);
        assert_eq!(device_tail_wait(Some(4_096), 0), DEVICE_TAIL_FALLBACK);
    }
}
