//! Settling the output transport after `open`, which returns before any
//! device is discovered: follow its transitions until one decides whether
//! playback goes on, ends as the expected headless case, or fails setup.

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
