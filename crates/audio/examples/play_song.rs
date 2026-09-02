//! Local smoke tool: build a tiny hand-authored song and play it through the
//! real `platform::AudioOutput` device.
//!
//! Not run in CI (examples are compiled, not executed, by `cargo test`); this
//! is the manual "does sound actually come out?" check. On a headless machine
//! with no audio device it prints a note and exits cleanly.
//!
//! Run with: `cargo run -p audio --example play_song`.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use audio::{decode_track, Adsr, Instrument, Sequencer, Song, ToneData, WaveData, MIXER_RATE};
use platform::AudioOutput;

/// Ceiling on time spent retrying a single frame's push before treating the
/// output stream as unrecoverably stalled. Comfortably longer than the
/// ~306 ms the 4096-frame ring can absorb at [`MIXER_RATE`], but short
/// enough that a dead callback fails fast instead of hanging this manual
/// smoke command.
const PUSH_MAX_WAIT: Duration = Duration::from_secs(1);

fn main() -> ExitCode {
    let song = build_song();
    let mut seq = Sequencer::new(song);

    let mut output = match AudioOutput::open(4096) {
        Ok(output) => output,
        Err(err) => {
            println!("no audio device ({err}); nothing to play — this is expected in CI/headless");
            return ExitCode::SUCCESS;
        }
    };
    output.start().expect("start playback");
    println!("playing a short scale at {MIXER_RATE} Hz — Ctrl-C to stop");

    let producer = output.producer();
    let frame_samples = u32::try_from(audio::SAMPLES_PER_FRAME).expect("frame fits u32");
    let frame_period = Duration::from_secs_f64(f64::from(frame_samples) / f64::from(MIXER_RATE));
    let policy = RetryPolicy {
        interval: frame_period / 4,
        max_wait: PUSH_MAX_WAIT,
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
    // Let the tail drain.
    std::thread::sleep(Duration::from_millis(200));
    let errors = output.stream_errors();
    if errors > 0 {
        eprintln!(
            "audio playback stopped: {errors} asynchronous stream error(s) reported during the \
             final drain"
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// How long [`push_frame`] may keep retrying a momentarily full ring before
/// giving up.
struct RetryPolicy {
    /// Sleep between retries while the ring is only momentarily full.
    interval: Duration,
    /// Hard ceiling on total time spent retrying a single frame.
    max_wait: Duration,
}

/// Why [`push_frame`] gave up before queuing every sample.
#[derive(Clone, Copy)]
enum PushError {
    /// The output stream's asynchronous error counter became nonzero: the
    /// device callback has stopped draining the ring, so no amount of
    /// further retrying can help (see `AudioOutput::stream_errors`).
    StreamStopped { errors: u64, dropped: usize },
    /// No progress within `RetryPolicy::max_wait`, even though the stream
    /// reports no errors — e.g. consumption stalled without a visible
    /// stream-error signal.
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
                PUSH_MAX_WAIT.as_secs_f64()
            ),
        }
    }
}

/// Push all of `samples` via `push`, retrying while the ring is only
/// momentarily full, bounded by `policy`.
///
/// Returns `Ok(())` once every sample is queued. Gives up early — instead of
/// spinning forever — as soon as `stream_errors` reports the async device
/// callback has stopped draining the ring, or once `policy.max_wait` has
/// elapsed with no such signal. Either way the unqueued tail is dropped, not
/// queued or blocked on further: the same accounting rule
/// [`platform::Producer::push`] documents, and the one production's
/// `MusicPlayer::advance_frame` applies to a single push instead of a retry
/// loop.
///
/// `push`, `stream_errors`, `now`, and `sleep` are injected so this can be
/// exercised deterministically in tests without a real audio device or wall
/// clock.
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
        // Re-check right away: an async error can land between the check
        // above and this push completing, and reporting it now is more
        // specific than letting the loop fall through to a deadline.
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

    use super::{push_frame, PushError, RetryPolicy};

    #[test]
    fn a_stream_error_aborts_the_retry_without_waiting_out_the_deadline() {
        let push_calls = Cell::new(0_u32);
        let policy = RetryPolicy {
            interval: std::time::Duration::from_millis(1),
            max_wait: std::time::Duration::from_secs(60),
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
}
