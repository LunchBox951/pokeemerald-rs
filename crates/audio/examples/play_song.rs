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
use platform::audio::{PlaybackProgress, UsableReading};
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

/// How many recent submitted-frame advances size the callback cadence in
/// [`wait_for_measured_tail`]: enough to ride out a jittery period, few enough
/// to adapt when callbacks change their real pace.
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

    match prefill_then_start(
        &mut seq,
        &producer,
        &mut buffer,
        &mut output,
        |chunk| producer.push(chunk),
        AudioOutput::start,
    ) {
        StartOutcome::Playing => {}
        StartOutcome::ReportedSetupFailure => return ExitCode::FAILURE,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenOutcome {
    ExpectedHeadless,
    PlaybackSetupFailure,
}

/// Let [`AudioOutput::open`] own headless detection; do not reclassify
/// backend errors here.
fn classify_open_error(error: &PlatformError) -> OpenOutcome {
    match error {
        PlatformError::NoAudioDevice => OpenOutcome::ExpectedHeadless,
        _ => OpenOutcome::PlaybackSetupFailure,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartOutcome {
    Playing,
    ReportedSetupFailure,
}

fn start_playback(
    output: &mut AudioOutput,
    start: impl FnOnce(&mut AudioOutput) -> Result<(), PlatformError>,
) -> StartOutcome {
    match start(output) {
        Ok(()) => StartOutcome::Playing,
        Err(err) => {
            eprintln!("audio playback setup failed: {err}");
            StartOutcome::ReportedSetupFailure
        }
    }
}

/// Leave half the ring free for producer/consumer drift after startup.
const PREFILL_DIVISOR: usize = 2;

/// Returns the number of rendered samples refused by `push`.
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

/// The first device callback may run immediately; starting with an empty
/// ring risks an underrun.
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
        return StartOutcome::ReportedSetupFailure;
    }
    start_playback(output, start)
}

/// Retry budgets for [`push_frame`] and [`wait_for_drain`]: deadlines on elapsed
/// time, checked between attempts, not attempt counts.
struct RetryPolicy {
    interval: Duration,
    max_wait: Duration,
}

/// Why [`push_frame`] gave up before queuing every sample. `dropped` counts only
/// the unqueued suffix, never samples accepted by earlier partial pushes.
#[derive(Clone, Copy)]
enum PushError {
    StreamStopped { errors: u64, dropped: usize },
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

/// Push all of `samples`, retrying a full ring within `policy`.
///
/// The retry budget covers the whole frame; partial progress does not renew it.
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
        // An asynchronous stream failure is more informative than retry expiry.
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
/// Absolute game-frame deadlines keep render/push work and late frames from
/// shifting the cadence.
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
/// `remaining` may be zero even on a stream failure.
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
/// A reported stream failure takes precedence over ring emptiness.
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
        // An empty-ring observation must not be paired with an older
        // healthy-stream observation.
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

/// `submitted_frames` plus the device's resampler settle margin, so the wait
/// outlasts interpolation state still to sound (`platform::Resampler`'s
/// deferred lookahead pull). Saturates instead of overflowing.
fn measured_drain_target(submitted_frames: u64, settle_margin_frames: u64) -> u64 {
    submitted_frames.saturating_add(settle_margin_frames)
}

/// Derived tail for a device with no measured playback position:
/// [`HOST_QUEUED_PERIODS`] of its largest advertised callback buffer plus
/// [`DEVICE_TAIL_MARGIN`], clamped to [`DEVICE_TAIL_FALLBACK`]..=
/// [`DEVICE_TAIL_MAX`]. With no advertised buffer, or a zero device rate,
/// the floor alone.
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

/// Sleeps `tail`, then fails if `stream_errors` is nonzero: a device error
/// during the tail means the last buffer never played.
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

/// The longest of `samples`, or zero when there are none.
fn longest(samples: impl IntoIterator<Item = Duration>) -> Duration {
    samples.into_iter().max().unwrap_or_default()
}

/// When `target` sounds according to `reading`: its callback's observation
/// time plus the frames it still owed, startup delay included, at
/// `device_sample_rate`. The stamp is the callback's own, so however many
/// stale callbacks or late polls followed, the reading's debt is exact.
/// `None` only when the sum overflows the clock.
fn reading_due(reading: UsableReading, target: u64, device_sample_rate: u32) -> Option<Instant> {
    let owed = reading
        .startup_delay_frames
        .saturating_add(target.saturating_sub(reading.sounded_frames));
    reading
        .observed_at
        .checked_add(frames_duration(owed, device_sample_rate))
}

/// Whether `reading` proves `target` sounded by `deadline`. A reading past
/// the target backdates the crossing by the overshoot at
/// `device_sample_rate`: the position cannot outrun the device rate, so the
/// target sounded at least that long before the callback. Otherwise the
/// target sounds at [`reading_due`].
fn reading_completes_by(
    reading: UsableReading,
    target: u64,
    device_sample_rate: u32,
    deadline: Instant,
) -> bool {
    if reading.sounded_frames > target {
        let overshoot = frames_duration(reading.sounded_frames - target, device_sample_rate);
        deadline
            .checked_add(overshoot)
            .is_none_or(|latest| reading.observed_at <= latest)
    } else {
        reading_due(reading, target, device_sample_rate).is_some_and(|due| due <= deadline)
    }
}

/// How this wait has actually been polling: the shortest of the recent poll
/// gaps, never under the requested `interval`. A scheduler that holds every
/// sleep late raises it; one late poll among prompt ones does not.
fn usual_poll_gap(poll_gaps: &[Duration], interval: Duration) -> Duration {
    poll_gaps
        .iter()
        .copied()
        .min()
        .unwrap_or_default()
        .max(interval)
}

/// The cadence samples still meaningful at polling resolution `usual_poll`:
/// a sample seen while polling ran more than twice as coarse measured that
/// resolution rather than the callbacks, so it lapses once polling is prompt
/// again.
fn retained_cadence(
    samples: &[(Duration, Duration)],
    usual_poll: Duration,
) -> impl Iterator<Item = Duration> + '_ {
    samples
        .iter()
        .filter(move |&&(_, seen_at)| seen_at <= usual_poll * 2)
        .map(|&(sample, _)| sample)
}

/// Whether a usable callback ended inside the submitted advance `(from, to]`
/// that a `usable_through` mark describes: only a mark within the span proves
/// it. A mark past `to` belongs to a callback still in flight, whose frames
/// have not landed, so it says nothing about the callbacks already submitted.
fn usable_callback_in_span(from: u64, to: u64, usable_through: u64) -> bool {
    from < usable_through && usable_through <= to
}

/// Wait, within `policy`, for the device's measured playback position (see
/// `platform::AudioOutput::playback_progress`) to reach `target` submitted
/// device frames, polling `progress` at `policy.interval`.
///
/// - Measured finish: the target sounding ends the wait. A position still
///   short at the deadline is [`DrainError::MeasuredTailTimedOut`], and so is
///   a first sight of the target past the deadline unless the latest usable
///   callback's own stamp, backdated by any overshoot, completes within it.
/// - Stream errors: any error is [`DrainError::StreamStoppedDuringTail`], and
///   outranks every other outcome. The progress signal disappearing is not a
///   failure.
/// - Stale evidence: frames submitted past both the previous poll and
///   `PlaybackProgress::usable_through_frames`. A usable callback inside an
///   advance discards the evidence before it; the frames after its mark count.
/// - Live callbacks: stale evidence spanning `derived_tail / 4` (or, before any
///   stall, one advance covering that much), and the latest advance within the
///   longest recent cadence sample (see [`CADENCE_WINDOW`]) plus two polls. A sample is `gap.max(played)`, except a stall, which
///   samples only `played`, restarts the evidence, and restarts the tail.
/// - Tail finish: only while callbacks are live, once the tail has elapsed, the
///   latest due across usable readings (see [`reading_due`]) has passed, and no
///   callback is in flight. Past `policy.max_wait` it finishes only if it was
///   due by the deadline, else times out.
/// - Overshoot: the deadline is checked after each `sleep` returns, so the hold
///   can exceed `max_wait` by one sleep's actual duration, which
///   `std::thread::sleep` only bounds from below.
///
/// `progress`, `stream_errors`, `now`, and `sleep` are injected as in
/// [`push_frame`]; [`MeasuredTail`] owns the per-snapshot decisions.
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
    let mut wait = MeasuredTail::new(
        now(),
        target,
        derived_tail,
        device_sample_rate,
        policy.interval,
        policy.max_wait,
    );
    loop {
        let snapshot = progress();
        let errors = stream_errors();
        if errors > 0 {
            return Err(DrainError::StreamStoppedDuringTail { errors });
        }
        let Some(snapshot) = snapshot else {
            return Ok(());
        };
        if let Some(outcome) = wait.poll(snapshot, now()) {
            return outcome;
        }
        sleep(policy.interval);
    }
}

/// The stale-callback evidence [`MeasuredTail`] gathers: when frames from
/// callbacks past the last usable one advanced, and by how much at most.
#[derive(Default)]
struct StaleEvidence {
    first: Option<Instant>,
    last: Option<Instant>,
    max_advance: Duration,
    /// A stall restarted the evidence: one big advance is no longer proof.
    after_stall: bool,
}

impl StaleEvidence {
    /// Discards the pending evidence.
    fn clear(&mut self) {
        *self = Self::default();
    }

    /// Records an advance of `stale` playback credited at `at`.
    fn record(&mut self, at: Instant, stale: Duration) {
        self.first.get_or_insert(at);
        self.last = Some(at);
        self.max_advance = self.max_advance.max(stale);
    }

    /// Whether the evidence spans at least `derived_tail / 4`: a span of
    /// advances, or one advance covering that much playback.
    fn is_sufficient(&self, derived_tail: Duration) -> bool {
        self.first
            .zip(self.last)
            .is_some_and(|(first, last)| last.duration_since(first) >= derived_tail / 4)
            || (!self.after_stall && self.max_advance >= derived_tail / 4)
    }
}

/// How this wait has been polling: the recent gaps between polls.
struct PollClock {
    last_poll: Instant,
    gaps: [Duration; CADENCE_WINDOW],
    polls: usize,
}

impl PollClock {
    fn new(started: Instant, interval: Duration) -> Self {
        Self {
            last_poll: started,
            gaps: [interval; CADENCE_WINDOW],
            polls: 0,
        }
    }

    /// See [`usual_poll_gap`].
    fn usual_gap(&self, interval: Duration) -> Duration {
        usual_poll_gap(&self.gaps, interval)
    }

    /// Notes a poll at `current`.
    fn record(&mut self, current: Instant) {
        self.gaps[self.polls % CADENCE_WINDOW] = current.duration_since(self.last_poll);
        self.polls = self.polls.wrapping_add(1);
        self.last_poll = current;
    }
}

/// The callback cadence seen across submitted-frame advances.
struct Cadence {
    /// Each cadence sample beside the polling resolution it was seen at.
    recent: [(Duration, Duration); CADENCE_WINDOW],
    advances: usize,
    last_played: Duration,
    last_advance: Option<Instant>,
}

impl Cadence {
    fn new() -> Self {
        Self {
            recent: [(Duration::ZERO, Duration::ZERO); CADENCE_WINDOW],
            advances: 0,
            last_played: Duration::ZERO,
            last_advance: None,
        }
    }

    /// Forgets the cadence seen so far.
    fn restart(&mut self) {
        self.recent = [(Duration::ZERO, Duration::ZERO); CADENCE_WINDOW];
    }

    /// The longest retained cadence at polling resolution `usual_poll`,
    /// counting `pause_sample` when a polling pause produced one.
    fn longest(&self, usual_poll: Duration, pause_sample: Option<Duration>) -> Duration {
        longest(retained_cadence(&self.recent, usual_poll).chain(pause_sample))
    }
}

/// One measured-tail wait across polls: every decision made from a snapshot.
/// [`wait_for_measured_tail`] owns the injected I/O.
struct MeasuredTail {
    target: u64,
    derived_tail: Duration,
    device_sample_rate: u32,
    interval: Duration,
    started: Instant,
    deadline: Instant,
    tail_started: Instant,
    measured_due: Instant,
    last_submitted: Option<u64>,
    last_reading: Option<UsableReading>,
    polls: PollClock,
    cadence: Cadence,
    stale: StaleEvidence,
}

impl MeasuredTail {
    fn new(
        started: Instant,
        target: u64,
        derived_tail: Duration,
        device_sample_rate: u32,
        interval: Duration,
        max_wait: Duration,
    ) -> Self {
        Self {
            target,
            derived_tail,
            device_sample_rate,
            interval,
            started,
            deadline: started + max_wait,
            tail_started: started,
            measured_due: started,
            last_submitted: None,
            last_reading: None,
            polls: PollClock::new(started, interval),
            cadence: Cadence::new(),
            stale: StaleEvidence::default(),
        }
    }

    /// Takes the `snapshot` read at `current`; `Some` ends the wait.
    fn poll(
        &mut self,
        snapshot: PlaybackProgress,
        current: Instant,
    ) -> Option<Result<(), DrainError>> {
        let PlaybackProgress {
            sounded_frames: sounded,
            submitted_frames: submitted,
            usable_through_frames: usable_through,
            usable_reading,
        } = snapshot;
        if sounded >= self.target {
            return Some(self.target_reached(sounded, usable_reading, current));
        }
        self.note_reading(usable_reading);
        // A mark past the submitted total is a callback in flight, proving
        // nothing about the frames submitted; once they land it discards the
        // pending stale evidence.
        let in_flight = usable_through > submitted;
        if self
            .last_submitted
            .is_some_and(|prev| usable_callback_in_span(prev, submitted, usable_through))
        {
            self.stale.clear();
        }
        let pause_sample = self.note_advance(submitted, usable_through, in_flight, current);
        self.polls.record(current);
        self.last_submitted = Some(submitted);
        self.decide(sounded, in_flight, pause_sample, current)
    }

    /// The outcome of a poll that sees the target sounded. A poll past the
    /// deadline accepts it only when the callback's own stamp, backdated by
    /// any overshoot, puts completion inside the budget.
    fn target_reached(
        &self,
        sounded: u64,
        usable_reading: Option<UsableReading>,
        current: Instant,
    ) -> Result<(), DrainError> {
        if current <= self.deadline
            || usable_reading.is_some_and(|reading| {
                reading_completes_by(reading, self.target, self.device_sample_rate, self.deadline)
            })
        {
            Ok(())
        } else {
            Err(DrainError::MeasuredTailTimedOut {
                sounded,
                target: self.target,
            })
        }
    }

    /// A usable reading short of the target says how much playback it still
    /// owed: stale callbacks after it must not finish the wait before that
    /// has run at the device rate from the callback that took it, however
    /// many stale callbacks or late polls came since. Never lowered by a
    /// later reading.
    fn note_reading(&mut self, usable_reading: Option<UsableReading>) {
        if let Some(reading) = usable_reading.filter(|&reading| Some(reading) != self.last_reading)
        {
            self.last_reading = Some(reading);
            self.measured_due = self.measured_due.max(
                reading_due(reading, self.target, self.device_sample_rate)
                    .unwrap_or(self.measured_due),
            );
        }
    }

    /// Folds a submitted-frame advance since the previous poll into the
    /// cadence and the stale evidence. Returns the cadence sample of a
    /// polling pause, which counts only on this poll.
    fn note_advance(
        &mut self,
        submitted: u64,
        usable_through: u64,
        in_flight: bool,
        current: Instant,
    ) -> Option<Duration> {
        let previous = self.last_submitted.filter(|&last| submitted > last)?;
        let rate = self.device_sample_rate;
        let played = frames_duration(submitted - previous, rate);
        // The advance happened somewhere since the previous poll. A late
        // poll can fold many callbacks into it, so credit it no later
        // than the playback it covers past that poll rather than
        // stamping it with the (possibly much later) observation time.
        let at = current.min(self.polls.last_poll + played);
        // The first advance's gap runs from the start of the wait, so
        // callbacks idle from the start are a stall like any other.
        let gap = at.saturating_duration_since(self.cadence.last_advance.unwrap_or(self.started));
        // A healthy gap is about one period: what the window has seen, or
        // the playback the last advance covered. One that outruns both by
        // a tenth of the tail is a stall: what came before says nothing
        // about the resumed callbacks, and the gap itself is not their
        // cadence.
        let usual_poll = self.polls.usual_gap(self.interval);
        let seen = longest(retained_cadence(&self.cadence.recent, usual_poll));
        // With one advance seen there is no cadence yet, only the gap.
        let floor = self.derived_tail / if self.cadence.advances >= 2 { 4 } else { 2 };
        // Before any advance, the only period to hold the gap to is this buffer.
        if self.cadence.advances == 0 {
            self.cadence.last_played = played;
        }
        let stalled =
            gap > floor && gap > seen.max(self.cadence.last_played) + self.derived_tail / 10;
        let cadence_sample = if stalled {
            self.stale.clear();
            self.cadence.restart();
            self.stale.after_stall = true;
            // Audio the resumed callbacks submit sounds a tail later.
            self.tail_started = current;
            played
        } else {
            gap.max(played)
        };
        // A polling pause can fold many short callbacks into one advance,
        // and its backdated `at` can hide the idle span before them. It
        // widens freshness only on this poll and finishes only at the
        // deadline, where no later poll can confirm it. A pause is late
        // against how this thread has actually been polling (see
        // [`usual_poll_gap`]), so a poller the scheduler holds
        // systematically late still settles.
        let settled = current.duration_since(self.polls.last_poll) <= usual_poll * 2;
        let slot = self.cadence.advances % CADENCE_WINDOW;
        let pause_sample = if settled {
            self.cadence.recent[slot] = (cadence_sample, usual_poll);
            None
        } else {
            self.cadence.recent[slot] = (Duration::ZERO, Duration::ZERO);
            Some(cadence_sample)
        };
        self.cadence.advances = self.cadence.advances.wrapping_add(1);
        self.cadence.last_advance = Some(at);
        self.cadence.last_played = played;
        // Frames past both the previous poll and the usable mark came
        // from callbacks after the last usable one: stale evidence, even
        // when one late poll folds a usable callback in ahead of them.
        let stale_frames = submitted.saturating_sub(previous.max(usable_through));
        if stale_frames > 0 && !in_flight {
            self.stale.record(at, frames_duration(stale_frames, rate));
        }
        pause_sample
    }

    /// Whether the wait ends at this poll, once the snapshot is folded in.
    fn decide(
        &self,
        sounded: u64,
        in_flight: bool,
        pause_sample: Option<Duration>,
        current: Instant,
    ) -> Option<Result<(), DrainError>> {
        let usual_poll = self.polls.usual_gap(self.interval);
        let cadence = self.cadence.longest(usual_poll, pause_sample);
        let callbacks_alive = self.stale.is_sufficient(self.derived_tail)
            && self
                .cadence
                .last_advance
                .is_some_and(|last| current.duration_since(last) <= cadence + self.interval * 2);
        // The tail runs from `started` or from the resumption after a stall;
        // a usable reading does not restart it, so a valid-then-stale device
        // keeps its budget, but the finish waits for the latest playback any
        // usable reading still owed. A callback in flight vetoes the finish.
        let tail_due = (self.tail_started + self.derived_tail).max(self.measured_due);
        let stale_tail_elapsed = !in_flight && callbacks_alive && current >= tail_due;
        if current >= self.deadline {
            // The first poll at or past the deadline is the last: the tail may
            // finish it only if it could have elapsed within the budget.
            return Some(if stale_tail_elapsed && tail_due <= self.deadline {
                Ok(())
            } else {
                Err(DrainError::MeasuredTailTimedOut {
                    sounded,
                    target: self.target,
                })
            });
        }
        (stale_tail_elapsed && pause_sample.is_none()).then_some(Ok(()))
    }
}

/// `main`'s "hold the stream open until the last samples sound" step.
/// With a `submitted_target`, it runs [`wait_for_measured_tail`], whose
/// contract covers stale timestamps, live callbacks, and the deadline. Without
/// one it sleeps `derived_tail` via [`wait_for_device_tail`], ignoring
/// `policy`; stream errors fail both. Hooks are injected as in [`push_frame`].
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
    const VOICE: u8 = 0xBD;
    const VOL: u8 = 0xBE;
    const NOTE_24: u8 = 0xE7;
    const WAIT_24: u8 = 0x98;
    const FINE: u8 = 0xB1;
    const VELOCITY: u8 = 0x7F;

    // The wave frequency makes key 60 render near unity.
    let mut square_wave_samples = vec![90_i8; 64];
    for sample in square_wave_samples.iter_mut().skip(32) {
        *sample = -90;
    }
    let wave = Arc::new(WaveData::looping(13_697_024, 0, square_wave_samples));
    let instrument = ToneData::new(
        wave,
        Adsr {
            attack: 0xFF,
            decay: 0xF0,
            sustain: 0xA0,
            release: 0xE0,
        },
    );

    let ascending_scale_keys = [60_u8, 62, 64, 65, 67, 69, 71, 72];
    let mut scale_program = vec![VOICE, 0x00, VOL, 110];
    for key in ascending_scale_keys {
        scale_program.extend([NOTE_24, key, VELOCITY, WAIT_24]);
    }
    scale_program.push(FINE);

    let events = decode_track(&scale_program).expect("valid demo track");
    Song::new(vec![Instrument::DirectSound(instrument)], vec![events], 120)
}

#[cfg(test)]
#[path = "play_song/tests/mod.rs"]
mod tests;
