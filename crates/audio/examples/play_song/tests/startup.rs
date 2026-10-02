//! Open-outcome classification, prefill, and the start handshake.

use super::*;

#[test]
fn no_audio_device_is_the_expected_headless_case() {
    assert_eq!(
        classify_open_error(&PlatformError::NoAudioDevice),
        OpenOutcome::ExpectedHeadless
    );
}

/// A device that answered and then refused must not read as headless.
/// A failed stream build's `PlatformError::Audio(cpal::Error)` takes
/// this same wildcard arm, but `cpal` is not a dependency of this crate,
/// so `platform`'s own `a_lost_device_after_the_query_stays_an_audio_error`
/// pins that the build stage keeps the `Audio` variant this arm catches.
#[test]
fn an_unsupported_audio_config_is_a_playback_setup_failure() {
    assert_eq!(
        classify_open_error(&PlatformError::UnsupportedAudioConfig),
        OpenOutcome::PlaybackSetupFailure
    );
}

/// A device that answered `open` can still refuse `play`
/// (`platform/src/audio.rs`); the refusal must come back as the reported
/// failure outcome, never as a panic.
#[test]
fn a_device_that_refuses_to_start_is_a_reported_playback_setup_failure() {
    let mut output = AudioOutput::null(4);

    assert_eq!(
        start_playback(&mut output, |_| Err(PlatformError::UnsupportedAudioConfig)),
        StartOutcome::PlaybackSetupFailure,
        "a refused start must be reported and handed back, not panicked on"
    );
    assert!(
        !output.is_running(),
        "a refused start must not read as playing"
    );

    assert_eq!(
        start_playback(&mut output, AudioOutput::start),
        StartOutcome::Playing,
        "the null backend always starts"
    );
    assert!(output.is_running(), "an accepted start must play");
}

/// The device's first callback can fire the instant `start` returns, so
/// samples must already be sitting in the ring before that call — not
/// queued afterward, by which point a real callback could already have
/// drained an empty ring into an underrun.
///
/// This drives `prefill_then_start`, the single startup step `main`
/// performs, rather than sequencing the two halves here: reversing them
/// at that call site leaves the starter looking at an untouched ring and
/// fails this test, which reproducing the order locally would not catch.
#[test]
fn samples_are_queued_before_the_starter_runs() {
    let mut output = AudioOutput::null(512);
    let producer = output.producer();
    let capacity = producer.capacity();
    let mut seq = Sequencer::new(build_song());
    let mut buffer = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let starter_saw_queued_samples = Cell::new(false);

    let result = prefill_then_start(
        &mut seq,
        &producer,
        &mut buffer,
        &mut output,
        |chunk| producer.push(chunk),
        |output| {
            starter_saw_queued_samples.set(producer.available_space() < capacity);
            AudioOutput::start(output)
        },
    );

    assert_eq!(
        result,
        StartOutcome::Playing,
        "a fresh ring must accept the whole prefill with room to spare"
    );
    assert!(
        starter_saw_queued_samples.get(),
        "samples must already be queued when the starter is invoked, not after"
    );
    assert!(output.is_running(), "an accepted start must play");
}

/// A ring that refuses part of the prefill means the startup assumption
/// this file rests on is broken, so the stream must never be started on
/// that short fill — the empty-ring underrun the prefill exists to
/// prevent would simply happen a frame later.
#[test]
fn a_refused_prefill_is_a_setup_failure_and_never_starts_the_device() {
    let mut output = AudioOutput::null(512);
    let producer = output.producer();
    let capacity = producer.capacity();
    let mut seq = Sequencer::new(build_song());
    let mut buffer = vec![0.0_f32; Sequencer::FRAME_SAMPLES];
    let starter_ran = Cell::new(false);

    let result = prefill_then_start(
        &mut seq,
        &producer,
        &mut buffer,
        &mut output,
        // One sample short of the frame, every frame: the accounting
        // `platform::Producer::push` documents for a refused tail.
        |chunk| producer.push(&chunk[..chunk.len().saturating_sub(1)]),
        |output| {
            starter_ran.set(true);
            AudioOutput::start(output)
        },
    );

    assert_eq!(result, StartOutcome::PlaybackSetupFailure);
    assert!(
        !starter_ran.get(),
        "a prefill the ring refused must not reach the starter"
    );
    assert!(
        !output.is_running(),
        "a refused prefill must not leave the device playing"
    );
    assert!(
        producer.available_space() < capacity,
        "the accepted head of the prefill is still queued; only the tail was refused"
    );
}
