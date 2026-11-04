//! The output transport's lifecycle around playback. After `open`, which
//! returns before any device is discovered, follow its transitions until one
//! decides whether playback goes on, ends as the expected headless case, or
//! fails setup. Once playback runs, [`PlaybackHealth`] tells a stream error
//! the transport recovered from apart from one that ended the playback.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use platform::audio::{TransportFailure, TransportState};
use platform::AudioOutput;

/// How long to wait for the output transport to settle after `open`, which
/// returns before any device is discovered.
const TRANSPORT_WAIT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TransportOutcome {
    Ready,
    ExpectedHeadless,
    /// A transport failure, or the state still reached when the wait ran out.
    SetupFailure(TransportState),
}

/// The outcome a transport state settles, or `None` while it is still
/// connecting. No output at all is the expected headless case; any other
/// unavailability is a real setup failure.
pub(super) fn classify_transport(state: TransportState) -> Option<TransportOutcome> {
    match state {
        TransportState::Ready | TransportState::Null => Some(TransportOutcome::Ready),
        TransportState::Unavailable(TransportFailure::NoOutput) => {
            Some(TransportOutcome::ExpectedHeadless)
        }
        TransportState::Unavailable(_) => Some(TransportOutcome::SetupFailure(state)),
        TransportState::Connecting => None,
    }
}

/// Wait for the transport and report an exit code when playback cannot go on:
/// success for the expected headless case, failure otherwise.
pub(super) fn settle_transport(output: &mut AudioOutput) -> Option<ExitCode> {
    match await_transport(output, TRANSPORT_WAIT) {
        TransportOutcome::Ready => None,
        TransportOutcome::ExpectedHeadless => {
            println!("no audio device; nothing to play — this is expected in CI/headless");
            Some(ExitCode::SUCCESS)
        }
        TransportOutcome::SetupFailure(state) => {
            eprintln!("audio playback setup failed: transport {state:?}");
            Some(ExitCode::FAILURE)
        }
    }
}

/// Follow the output's transport transitions until one settles the outcome or
/// `timeout` passes.
pub(super) fn await_transport(output: &mut AudioOutput, timeout: Duration) -> TransportOutcome {
    await_transport_with(
        output,
        AudioOutput::poll_transport_state,
        AudioOutput::transport_state,
        timeout,
    )
}

/// [`await_transport`] over any `poll`/`current` pair. The state read at the
/// deadline is classified like any other, so one the worker published after
/// the last look settles the outcome; only a transport still connecting
/// fails for the timeout.
pub(super) fn await_transport_with<O>(
    output: &mut O,
    mut poll: impl FnMut(&mut O) -> Option<TransportState>,
    mut current: impl FnMut(&O) -> TransportState,
    timeout: Duration,
) -> TransportOutcome {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(outcome) = settle_transport_once(output, &mut poll, &mut current) {
            return outcome;
        }
        if Instant::now() >= deadline {
            let state = current(output);
            return classify_transport(state).unwrap_or(TransportOutcome::SetupFailure(state));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// One look at the transport: the `queued` transitions are drained unread,
/// then `current` is read and decides. An older `Unavailable` still queued
/// behind a later `Ready` cannot end the run as headless, and reading after
/// the drain means no discarded transition is newer than the deciding state.
pub(super) fn settle_transport_once<O>(
    output: &mut O,
    mut poll: impl FnMut(&mut O) -> Option<TransportState>,
    current: impl FnOnce(&O) -> TransportState,
) -> Option<TransportOutcome> {
    while poll(output).is_some() {}
    classify_transport(current(output))
}

fn is_unavailable(state: TransportState) -> bool {
    matches!(state, TransportState::Unavailable(_))
}

/// Tells a recovered stream error from a terminal one while a song plays.
///
/// [`AudioOutput::stream_errors`] counts every error the callback reports,
/// including an XRUN the backend re-prepares and carries on from, so a rise
/// alone does not mean playback stopped. An error is terminal once it left
/// its stream unusable ([`AudioOutput::fatal_stream_errors`], which the
/// callback counts itself, so no worker poll has to confirm it first) or the
/// transport has been unavailable since playback began: it detached the
/// stream, and a later rebuild does not return the audio queued meanwhile.
#[derive(Clone, Copy, Debug)]
pub(super) struct PlaybackHealth {
    /// The lifetime error count when playback began, so an error a
    /// since-replaced stream reported earlier does not fail it.
    baseline: u64,
    /// The lifetime count of errors that left their stream unusable when
    /// playback began.
    fatal_baseline: u64,
    /// The transport reported itself unavailable since playback began.
    interrupted: bool,
}

impl PlaybackHealth {
    /// Starts watching `output`, discarding the transitions it published
    /// before playback began.
    pub(super) fn begin(output: &mut AudioOutput) -> Self {
        Self::begin_with(
            output,
            AudioOutput::poll_transport_state,
            AudioOutput::stream_errors,
            AudioOutput::fatal_stream_errors,
        )
    }

    /// [`Self::begin`] over any `poll`/`errors`/`fatal` triple.
    pub(super) fn begin_with<O>(
        output: &mut O,
        mut poll: impl FnMut(&mut O) -> Option<TransportState>,
        errors: impl FnOnce(&O) -> u64,
        fatal: impl FnOnce(&O) -> u64,
    ) -> Self {
        while poll(output).is_some() {}
        Self {
            // The total is read after the fatal count: the callback counts
            // the total first, so a fatal error is never missing from it.
            fatal_baseline: fatal(output),
            baseline: errors(output),
            interrupted: false,
        }
    }

    /// The stream errors reported since playback began (possibly none, for a
    /// stream lost without a callback error) once the transport has ended the
    /// playback, else `None`: an error it recovered from is not one.
    pub(super) fn terminal_errors(&mut self, output: &mut AudioOutput) -> Option<u64> {
        self.terminal_errors_with(
            output,
            AudioOutput::poll_transport_state,
            AudioOutput::transport_state,
            AudioOutput::stream_errors,
            AudioOutput::fatal_stream_errors,
        )
    }

    /// [`Self::terminal_errors`] over any `poll`/`state`/`errors`/`fatal`
    /// quadruple.
    ///
    /// The transitions and current state are read before the counts, so an
    /// error counted after them is judged by the next look. A fatal error is
    /// terminal as soon as the callback counts it, whether or not the worker
    /// has retired the stream and published the loss yet. The reading stays
    /// terminal once it has been.
    pub(super) fn terminal_errors_with<O>(
        &mut self,
        output: &mut O,
        mut poll: impl FnMut(&mut O) -> Option<TransportState>,
        state: impl FnOnce(&O) -> TransportState,
        errors: impl FnOnce(&O) -> u64,
        fatal: impl FnOnce(&O) -> u64,
    ) -> Option<u64> {
        while let Some(published) = poll(output) {
            self.interrupted |= is_unavailable(published);
        }
        self.interrupted |= is_unavailable(state(output));
        // Fatal before the total, which the callback counts first.
        self.interrupted |= fatal(output) > self.fatal_baseline;
        let since = errors(output).saturating_sub(self.baseline);
        self.interrupted.then_some(since)
    }
}
