use super::*;

#[test]
fn no_audio_device_is_the_expected_headless_case() {
    assert_eq!(
        classify_open_error(&PlatformError::NoAudioDevice),
        OpenOutcome::ExpectedHeadless
    );
}

#[test]
fn an_unsupported_audio_config_is_a_playback_setup_failure() {
    assert_eq!(
        classify_open_error(&PlatformError::UnsupportedAudioConfig),
        OpenOutcome::PlaybackSetupFailure
    );
}

#[test]
fn a_device_that_refuses_to_start_is_a_reported_playback_setup_failure() {
    let mut output = AudioOutput::null(4);

    assert_eq!(
        start_playback(&mut output, |_| Err(PlatformError::UnsupportedAudioConfig)),
        StartOutcome::ReportedSetupFailure,
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
        |chunk| producer.push(&chunk[..chunk.len().saturating_sub(1)]),
        |output| {
            starter_ran.set(true);
            AudioOutput::start(output)
        },
    );

    assert_eq!(result, StartOutcome::ReportedSetupFailure);
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
