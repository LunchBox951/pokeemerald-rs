//! Local smoke tool: build a tiny hand-authored song and play it through the
//! real `platform::AudioOutput` device.
//!
//! `main` is a manual, not-run-in-CI "does sound actually come out?" check —
//! on a headless machine with no audio device it prints a note and exits
//! cleanly. The helper tests (`play_song/tests/`) do run under `cargo test`; see this
//! crate's `Cargo.toml`.
//!
//! Run with: `cargo run -p audio --example play_song`.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use audio::{decode_track, Adsr, Instrument, Sequencer, Song, ToneData, WaveData, MIXER_RATE};
use platform::audio::PlaybackProgress;
use platform::{AudioOutput, PlatformError, Producer, GBA_FRAME_PERIOD};

const RING_CAPACITY_FRAMES: usize = 4096;

/// Comfortably longer than the ~306 ms the ring can absorb at [`MIXER_RATE`],
/// but short enough that a dead callback fails fast instead of hanging this
/// manual smoke command.
const RETRY_MAX_WAIT: Duration = Duration::from_secs(1);

/// Added to the device's own callback bound in [`device_tail_wait`] to cover
/// the resampler's one-frame lookahead and scheduler jitter.
const DEVICE_TAIL_MARGIN: Duration = Duration::from_millis(50);

/// Callback periods the host keeps queued behind the one being filled:
/// cpal's ALSA path holds two, and no supported host holds more.
const HOST_QUEUED_PERIODS: u32 = 2;

/// Floor on the tail, and the whole tail when the device advertises no
/// callback size. The advertised size bounds one callback slice, not the
/// host pipeline's presentation latency (cpal's ALSA path keeps two periods
/// queued behind the callback), so a small callback never shortens this.
const DEVICE_TAIL_FALLBACK: Duration = Duration::from_millis(200);

/// Ceiling on the derived tail. A backend's advertised maximum is the largest
/// buffer it *supports*, not the default it selects, and can run to seconds;
/// this keeps a manual smoke run from looking hung after the last note.
const DEVICE_TAIL_MAX: Duration = Duration::from_secs(1);

/// How many of the most recent submitted-frame advances size the callback
/// cadence in [`wait_for_measured_tail`]: enough to ride out a jittery
/// period, few enough that one aggregate a late poll folded together ages out
/// once callbacks resume at their real pace.
const CADENCE_WINDOW: usize = 4;

fn main() -> ExitCode {
    let song = build_song();
    let mut seq = Sequencer::new(song);

    let mut output = match AudioOutput::open(RING_CAPACITY_FRAMES) {
        Ok(output) => output,
        Err(err) => {
            return match classify_open_error(&err) {
                OpenOutcome::ExpectedHeadless => {
                    println!(
                        "no audio device ({err}); nothing to play — this is expected in CI/headless"
                    );
                    ExitCode::SUCCESS
                }
                OpenOutcome::PlaybackSetupFailure => {
                    eprintln!("audio playback setup failed: {err}");
                    ExitCode::FAILURE
                }
            };
        }
    };
    let producer = output.producer();
    let ring_capacity_samples = RING_CAPACITY_FRAMES * usize::from(output.channels());
    // Pace at the real game-frame period, the same cadence the output's
    // resampler drains the ring at (`AudioOutput::source_cadence_hz`), not at
    // the rounded `SAMPLES_PER_FRAME / MIXER_RATE`: that rounding produces
    // about 0.04 frames/s more than the resampler consumes, which is a rate
    // bias no ring depth absorbs over a long enough run.
    let frame_period = GBA_FRAME_PERIOD;
    let policy = RetryPolicy {
        interval: frame_period / 4,
        max_wait: RETRY_MAX_WAIT,
    };
    let mut buffer = vec![0.0_f32; Sequencer::FRAME_SAMPLES];

    // Queue samples the device can consume the instant it starts, before the
    // stream exists to consume anything. The ordering lives inside
    // `prefill_then_start`, which is the only startup step `main` performs,
    // so the test that drives it pins the sequence this call site uses.
    match prefill_then_start(
        &mut seq,
        &producer,
        &mut buffer,
        &mut output,
        |chunk| producer.push(chunk),
        AudioOutput::start,
    ) {
        StartOutcome::Playing => {}
        StartOutcome::PlaybackSetupFailure => return ExitCode::FAILURE,
    }
    println!("playing a short scale at {MIXER_RATE} Hz — Ctrl-C to stop");

    // Render frame by frame, pacing to real time, until the song finishes.
    // Each deadline is derived from the last one rather than from `now()`
    // after the work below, so render/push time is subtracted from the wait
    // instead of stacking on top of it.
    let mut next_deadline = Instant::now() + frame_period;
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
        wait_for_frame_deadline(
            &mut next_deadline,
            frame_period,
            Instant::now,
            std::thread::sleep,
        );
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
    // Prefer the device's own measured playback position when it has one
    // (see `platform::AudioOutput::playback_progress`); the derived
    // `device_tail_wait` bound below is only the fallback for a host that
    // never reports a usable timestamp. The target is widened by the
    // resampler's own settle margin (`playback_settle_margin_frames`), or a
    // resampled device's buffered interpolation tail could still be sounding
    // real audio after the wait already ended -- see `measured_drain_target`.
    let submitted_target = output.playback_progress().map(|progress| {
        measured_drain_target(
            progress.submitted_frames,
            output.playback_settle_margin_frames(),
        )
    });
    let derived_tail = device_tail_wait(output.max_callback_frames(), output.device_sample_rate());
    let tail_result = wait_for_device_tail_or_measured(
        submitted_target,
        derived_tail,
        output.device_sample_rate(),
        &policy,
        || output.playback_progress(),
        || output.stream_errors(),
        Instant::now,
        std::thread::sleep,
    );
    if let Err(err) = tail_result {
        eprintln!("audio playback stopped: {}", err.describe());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// How to react to an [`AudioOutput::open`] failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenOutcome {
    /// No output device could be reached: the expected CI/headless case.
    ExpectedHeadless,
    /// A device was reached, but querying, configuring, or building its
    /// stream failed.
    PlaybackSetupFailure,
}

/// Classify an [`AudioOutput::open`] error so only the headless case exits
/// cleanly.
///
/// [`PlatformError::NoAudioDevice`] alone is the headless case, because
/// `platform` decides at the stage that knows: an unreachable device fails
/// `open`'s query and is reported as `NoAudioDevice` there, cpal's phantom
/// ALSA `default` included. Everything left, `Audio(cpal::Error)` from a
/// stream build included, means a device answered and then refused.
fn classify_open_error(error: &PlatformError) -> OpenOutcome {
    match error {
        PlatformError::NoAudioDevice => OpenOutcome::ExpectedHeadless,
        _ => OpenOutcome::PlaybackSetupFailure,
    }
}

/// What `main` does after the stream-start step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartOutcome {
    /// The device accepted `play`: the render loop can run.
    Playing,
    /// The device answered [`AudioOutput::open`] and then refused `play`.
    /// The failure has already been reported; `main` only returns
    /// `ExitCode::FAILURE`.
    PlaybackSetupFailure,
}

/// Start playback, reporting a refusal in the wording [`classify_open_error`]
/// uses for a playback-setup failure. `start` is injected as in
/// [`push_frame`].
fn start_playback(
    output: &mut AudioOutput,
    start: impl FnOnce(&mut AudioOutput) -> Result<(), PlatformError>,
) -> StartOutcome {
    match start(output) {
        Ok(()) => StartOutcome::Playing,
        Err(err) => {
            eprintln!("audio playback setup failed: {err}");
            StartOutcome::PlaybackSetupFailure
        }
    }
}

/// Fraction of the ring's available space [`prefill`] queues before playback
/// starts, matching the product player's own startup rule
/// (`pokeemerald_rs::music::player`): half absorbs a full frame's worth of
/// startup jitter while leaving the other half free for producer/consumer
/// drift once the device is running.
const PREFILL_DIVISOR: usize = 2;

/// Render and queue whole frames up to half the ring's currently available
/// space, so the device has samples queued the instant it starts instead of
/// consuming an empty ring on its first callback.
///
/// Returns the number of rendered samples `push` refused; a real, unstarted
/// device should never refuse a fill this far under capacity. `push` is
/// injected as in [`push_frame`] so tests need no audio device.
fn prefill(
    seq: &mut Sequencer,
    producer: &Producer,
    buffer: &mut [f32],
    mut push: impl FnMut(&[f32]) -> usize,
) -> usize {
    let target = producer.available_space() / PREFILL_DIVISOR;
    let mut queued = 0;
    let mut dropped = 0;
    while queued + buffer.len() <= target {
        seq.render_frame(buffer);
        let pushed = push(buffer);
        dropped += buffer.len() - pushed;
        queued += buffer.len();
    }
    dropped
}

/// Fill the ring, then start the stream — `main`'s whole startup step, in
/// one place.
///
/// The device's first callback can fire the instant `start` returns, so every
/// sample [`prefill`] queues has to be in the ring before that call; a stream
/// started on an empty ring zero-fills and counts an underrun before the
/// first frame exists. Keeping both steps here, rather than as two statements
/// in `main`, gives that ordering a single call site the tests below drive
/// through the injected `start`, the same seam
/// `pokeemerald_rs::music::player`'s `start_with_context_and_starter` uses.
///
/// A prefill the ring refuses is a setup failure reported in the wording
/// [`classify_open_error`] uses, and the stream is never started. `push` and
/// `start` are injected as in [`push_frame`].
fn prefill_then_start(
    seq: &mut Sequencer,
    producer: &Producer,
    buffer: &mut [f32],
    output: &mut AudioOutput,
    push: impl FnMut(&[f32]) -> usize,
    start: impl FnOnce(&mut AudioOutput) -> Result<(), PlatformError>,
) -> StartOutcome {
    let dropped = prefill(seq, producer, buffer, push);
    if dropped > 0 {
        eprintln!("audio playback setup failed: prefill dropped {dropped} sample(s)");
        return StartOutcome::PlaybackSetupFailure;
    }
    start_playback(output, start)
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

/// Sleep until `*next_deadline`, then advance it by one `frame_period`.
///
/// The deadline is tracked absolutely rather than derived from `now()` after
/// each call, so render and push time already spent is subtracted from the
/// wait instead of stacking on top of it: a call that lands late sleeps zero
/// and the following deadline still advances from the missed one, rather
/// than resetting from the late `now()` and letting drift compound. `now`
/// and `sleep` are injected as in [`push_frame`].
fn wait_for_frame_deadline(
    next_deadline: &mut Instant,
    frame_period: Duration,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) {
    sleep(next_deadline.saturating_duration_since(now()));
    *next_deadline += frame_period;
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
    /// `RetryPolicy::max_wait` elapsed with the device's measured playback
    /// position still short of the submitted target and no stream error
    /// reported.
    MeasuredTailTimedOut { sounded: u64, target: u64 },
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
            DrainError::MeasuredTailTimedOut { sounded, target } => format!(
                "the device had sounded {sounded} of {target} submitted frame(s) at the {:.1}s \
                 retry deadline; the final audio is unconfirmed",
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

/// The measured wait's target: `submitted_frames` plus `settle_margin_frames`
/// (`platform::AudioOutput::playback_settle_margin_frames`, read at the same
/// poll), so a target latched from `AudioOutput::playback_progress` does not
/// stop waiting before a resampled device's buffered interpolation state
/// finishes sounding real audio -- see `platform::Resampler`'s deferred
/// lookahead pull. Zero margin (no resampling, or an exact-rate device)
/// leaves the raw submitted-frame count unchanged. Saturates rather than
/// overflowing -- `submitted_frames` is a real device's frame counter,
/// nowhere near `u64::MAX`.
fn measured_drain_target(submitted_frames: u64, settle_margin_frames: u64) -> u64 {
    submitted_frames.saturating_add(settle_margin_frames)
}

/// Fallback for a device with no measured playback position (see
/// `platform::AudioOutput::playback_progress` and
/// [`wait_for_device_tail_or_measured`]): how long the device may still be
/// playing after the ring reads empty, derived rather than measured --
/// [`HOST_QUEUED_PERIODS`] of its largest advertised callback buffer at its
/// own rate, plus [`DEVICE_TAIL_MARGIN`], clamped between [`DEVICE_TAIL_FALLBACK`] and
/// [`DEVICE_TAIL_MAX`]; the floor alone when it advertises none. An empty
/// ring only means the callback took the samples, and dropping
/// `AudioOutput` closes the stream rather than draining it.
fn device_tail_wait(max_callback_frames: Option<usize>, device_sample_rate: u32) -> Duration {
    match max_callback_frames {
        Some(frames) if device_sample_rate > 0 => {
            let frames = u32::try_from(frames).unwrap_or(u32::MAX);
            let queued = f64::from(frames) * f64::from(HOST_QUEUED_PERIODS);
            let buffered = Duration::from_secs_f64(queued / f64::from(device_sample_rate));
            (buffered + DEVICE_TAIL_MARGIN).clamp(DEVICE_TAIL_FALLBACK, DEVICE_TAIL_MAX)
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

/// The playback `frames` device frames cover at `device_sample_rate`; zero
/// when the rate is unknown.
fn frames_duration(frames: u64, device_sample_rate: u32) -> Duration {
    let frames = u32::try_from(frames).unwrap_or(u32::MAX);
    if device_sample_rate > 0 {
        Duration::from_secs_f64(f64::from(frames) / f64::from(device_sample_rate))
    } else {
        Duration::ZERO
    }
}

/// Wait, within `policy`, for the device's measured playback position (see
/// `platform::AudioOutput::playback_progress`) to reach `target` submitted
/// device frames, polling `progress` at `policy.interval`.
///
/// `policy.max_wait` bounds how long a stream is held open, but unlike the
/// derived [`wait_for_device_tail`]'s fixed sleep the measured wait knows
/// whether the target was reached: a position still short of it at the
/// deadline is [`DrainError::MeasuredTailTimedOut`], not a finish. A stream
/// error before the target is reached is not a finish either. The signal
/// disappearing mid-wait is not itself a failure.
///
/// Stale timestamps are read from `usable_through_frames`, never from the
/// sounded estimate's shape: a mark past `from` in a submitted advance
/// `(from, to]` discards the pending evidence (a usable callback keeps the
/// measured wait in force), and the frames past both `from` and the mark are
/// stale evidence at once. Evidence of at least
/// `derived_tail / 4` (a span of advances, or one covering that much
/// playback) with the latest advance within one recent inter-advance gap
/// (over the last [`CADENCE_WINDOW`]) plus two polls means live callbacks,
/// and once `derived_tail` has run from the start of the wait the wait
/// finishes as [`wait_for_device_tail`] would. A gap that outruns both the
/// recent cadence and the playback the previous advance covered by a tenth of
/// `derived_tail` (and over a quarter of it, or half before a cadence is seen)
/// is a stall: evidence and cadence restart from the playback the resumed
/// callback covers, and that callback alone is no evidence.
///
/// The tail finish needs `derived_tail <= policy.max_wait`; a poll past the
/// deadline takes it only with live callbacks and otherwise times out.
/// `progress`, `stream_errors`, `now`, and `sleep` are injected as in
/// [`push_frame`].
#[expect(
    clippy::too_many_arguments,
    reason = "the injected clock, sleep, and stream hooks are the seam the tests drive"
)]
fn wait_for_measured_tail(
    target: u64,
    derived_tail: Duration,
    device_sample_rate: u32,
    policy: &RetryPolicy,
    mut progress: impl FnMut() -> Option<PlaybackProgress>,
    mut stream_errors: impl FnMut() -> u64,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<(), DrainError> {
    let started = now();
    let deadline = started + policy.max_wait;
    let mut last_submitted = None;
    let mut last_poll = started;
    let mut last_advance = None;
    let mut recent_cadence = [Duration::ZERO; CADENCE_WINDOW];
    let mut advances = 0_usize;
    let mut last_played = Duration::ZERO;
    let mut first_stale = None;
    let mut last_stale = None;
    let mut max_stale_advance = Duration::ZERO;
    let mut after_stall = false;
    loop {
        let snapshot = progress();
        let errors = stream_errors();
        if errors > 0 {
            return Err(DrainError::StreamStoppedDuringTail { errors });
        }
        let Some(PlaybackProgress {
            sounded_frames: sounded,
            submitted_frames: submitted,
            usable_through_frames: usable_through,
        }) = snapshot
        else {
            return Ok(());
        };
        if sounded >= target {
            return Ok(());
        }
        let current = now();
        // A mark past the previous poll's submitted total belongs to a
        // usable callback. It is stored before the frames it covers, so it
        // can be seen a poll ahead of them; it discards pending stale
        // evidence on the poll it is seen, whether or not frames moved.
        let usable = last_submitted.is_some_and(|previous| usable_through > previous);
        if usable {
            first_stale = None;
            last_stale = None;
            max_stale_advance = Duration::ZERO;
            after_stall = false;
        }
        if let Some(previous) = last_submitted.filter(|&last| submitted > last) {
            let played = frames_duration(submitted - previous, device_sample_rate);
            // The advance happened somewhere since the previous poll. A late
            // poll can fold many callbacks into it, so credit it no later
            // than the playback it covers past that poll rather than
            // stamping it with the (possibly much later) observation time.
            let at = current.min(last_poll + played);
            // The first advance's gap runs from the start of the wait, so
            // callbacks idle from the start are a stall like any other.
            let gap = at.saturating_duration_since(last_advance.unwrap_or(started));
            // A healthy gap is about one period: what the window has seen, or
            // the playback the last advance covered. One that outruns both by
            // a tenth of the tail is a stall: what came before says nothing
            // about the resumed callbacks, and the gap itself is not their
            // cadence.
            let seen = recent_cadence.iter().copied().max().unwrap_or_default();
            // With one advance seen there is no cadence yet, only the gap.
            let floor = if advances >= 2 {
                derived_tail / 4
            } else {
                derived_tail / 2
            };
            // Before any advance, the only period to hold the gap to is this buffer.
            if advances == 0 {
                last_played = played;
            }
            let stalled = gap > floor && gap > seen.max(last_played) + derived_tail / 10;
            let cadence_sample = if stalled {
                first_stale = None;
                last_stale = None;
                max_stale_advance = Duration::ZERO;
                recent_cadence = [Duration::ZERO; CADENCE_WINDOW];
                after_stall = true;
                played
            } else {
                gap.max(played)
            };
            recent_cadence[advances % CADENCE_WINDOW] = cadence_sample;
            advances = advances.wrapping_add(1);
            last_advance = Some(at);
            last_played = played;
            // Frames past both the previous poll and the usable mark came
            // from callbacks after the last usable one: stale evidence, even
            // when one late poll folds a usable callback in ahead of them.
            let stale_frames = submitted.saturating_sub(previous.max(usable_through));
            if stale_frames > 0 {
                first_stale.get_or_insert(at);
                last_stale = Some(at);
                let stale = frames_duration(stale_frames, device_sample_rate);
                max_stale_advance = max_stale_advance.max(stale);
            }
        }
        last_poll = current;
        last_submitted = Some(submitted);
        let cadence = recent_cadence.iter().copied().max().unwrap_or_default();
        let quarter_tail = derived_tail / 4;
        let evidence = first_stale
            .zip(last_stale)
            .is_some_and(|(first, last)| last.duration_since(first) >= quarter_tail)
            || (!after_stall && max_stale_advance >= quarter_tail);
        let callbacks_alive = evidence
            && last_advance
                .is_some_and(|last| current.duration_since(last) <= cadence + policy.interval * 2);
        // Everything submitted before the drain began has sounded once the
        // tail has run from `started`; a usable timestamp mid-wait does not
        // restart it, so a valid-then-stale device keeps its budget.
        let stale_tail_elapsed = callbacks_alive && current.duration_since(started) >= derived_tail;
        if current > deadline {
            // The first poll past the deadline is the last: the tail may finish
            // it only if it could have elapsed within the budget.
            return if stale_tail_elapsed && derived_tail <= policy.max_wait {
                Ok(())
            } else {
                Err(DrainError::MeasuredTailTimedOut { sounded, target })
            };
        }
        if stale_tail_elapsed {
            return Ok(());
        }
        sleep(policy.interval);
    }
}

/// `main`'s whole "hold the stream open until the last samples sound" step:
/// prefer the measured wait when `submitted_target` is `Some` (i.e.
/// `AudioOutput::playback_progress` returned a value when `main` checked),
/// otherwise fall back to sleeping `derived_tail` (`main`'s
/// [`device_tail_wait`] result) via [`wait_for_device_tail`] -- see the
/// module docs. `progress`, `stream_errors`, `now`, and `sleep` are
/// injected as in [`push_frame`].
#[expect(
    clippy::too_many_arguments,
    reason = "the injected clock, sleep, and stream hooks are the seam the tests drive"
)]
fn wait_for_device_tail_or_measured(
    submitted_target: Option<u64>,
    derived_tail: Duration,
    device_sample_rate: u32,
    policy: &RetryPolicy,
    progress: impl FnMut() -> Option<PlaybackProgress>,
    stream_errors: impl FnMut() -> u64,
    now: impl FnMut() -> Instant,
    sleep: impl FnMut(Duration),
) -> Result<(), DrainError> {
    if let Some(target) = submitted_target {
        wait_for_measured_tail(
            target,
            derived_tail,
            device_sample_rate,
            policy,
            progress,
            stream_errors,
            now,
            sleep,
        )
    } else {
        wait_for_device_tail(derived_tail, stream_errors, sleep)
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
#[path = "play_song/tests/mod.rs"]
mod tests;
