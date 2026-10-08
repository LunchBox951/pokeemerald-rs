//! Exercises audio output, negotiation, and stream behavior without a device.

use super::config::{sample_format_rank, select_config};
use super::stream::{
    f32_to_i16, fill_i16_output, i16_scratch_capacity, OutputDevice, DEFAULT_SCRATCH_SAMPLES,
    MAX_SCRATCH_CALLBACK_FRAMES,
};
use super::*;

/// A device whose every `cpal` call fails with one chosen kind.
///
/// Only the failure arms are needed: both boundary tests below drive a
/// stage that fails, and `cpal::Stream` has no public constructor to
/// return from a success arm anyway.
struct FailingDevice(cpal::ErrorKind);

impl OutputDevice for FailingDevice {
    fn supported_output_configs(
        &self,
    ) -> Result<Vec<cpal::SupportedStreamConfigRange>, cpal::Error> {
        Err(cpal::Error::new(self.0))
    }

    fn build_output_stream<T, D, E>(
        &self,
        _config: cpal::StreamConfig,
        _data_callback: D,
        _error_callback: E,
        _timeout: Option<std::time::Duration>,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: cpal::SizedSample,
        D: FnMut(&mut [T], &cpal::OutputCallbackInfo) + Send + 'static,
        E: FnMut(cpal::Error) + Send + 'static,
    {
        Err(cpal::Error::new(self.0))
    }
}

/// On the ALSA backend a headless box reaches the logical `default`
/// device and only fails once queried, so an unreachable device arrives
/// as a `cpal` error from [`negotiate`]'s query rather than as an absent
/// `default_output_device`.
#[test]
fn an_unreachable_device_or_host_fails_the_query_as_no_audio_device() {
    for kind in [
        cpal::ErrorKind::DeviceNotAvailable,
        cpal::ErrorKind::HostUnavailable,
    ] {
        let Err(err) = negotiate(&FailingDevice(kind)) else {
            panic!("the fake device always fails the query");
        };
        assert!(
            matches!(err, PlatformError::NoAudioDevice),
            "{kind:?} must read as no audio device, got: {err:?}"
        );
    }
}

/// The other half: a device that answered is real, so every other way
/// the query can fail stays an audio error a caller must report.
#[test]
fn any_other_query_failure_stays_an_audio_error() {
    for kind in [
        cpal::ErrorKind::UnsupportedConfig,
        cpal::ErrorKind::PermissionDenied,
        cpal::ErrorKind::DeviceBusy,
        cpal::ErrorKind::BackendError,
    ] {
        let Err(err) = negotiate(&FailingDevice(kind)) else {
            panic!("the fake device always fails the query");
        };
        assert!(
            matches!(err, PlatformError::Audio(_)),
            "{kind:?} must stay an audio error, got: {err:?}"
        );
    }
}

/// The stream-build stage is deliberately *not* routed through
/// [`classify_query_error`]: a device that answered the query is real,
/// so losing it before the build is a failure, not a headless run.
///
/// Driven through the production [`build_stream`], so it pins that call
/// site's mapping rather than the `From` impl's, and forces the exact
/// `DeviceNotAvailable` the query stage folds into `NoAudioDevice`. Both
/// sample-format arms are covered, since each makes its own build call.
#[test]
fn a_lost_device_after_the_query_stays_an_audio_error() {
    for format in [cpal::SampleFormat::F32, cpal::SampleFormat::I16] {
        let config = cpal::SupportedStreamConfig::new(
            AudioOutput::CHANNELS,
            48_000,
            cpal::SupportedBufferSize::Unknown,
            format,
        );
        let (_producer, consumer) = ring_buffer(64);

        // `cpal::Stream` is not `Debug`, so the failure is matched out
        // by hand rather than via `expect_err`.
        let Err(err) = build_stream(
            &FailingDevice(cpal::ErrorKind::DeviceNotAvailable),
            &config,
            Source::Direct(consumer),
            Arc::new(AtomicU64::new(0)),
            Arc::new(PlaybackClock::new()),
        ) else {
            panic!("the fake device always fails the build");
        };

        assert!(
            matches!(err, PlatformError::Audio(_)),
            "a {format:?} build-stage failure must stay an audio error, got: {err:?}"
        );
    }
}

#[test]
fn source_cadence_hz_is_the_exact_producer_rate_not_the_rounded_mixer_rate() {
    let cadence = AudioOutput::source_cadence_hz();
    let expected = 224.0 / crate::pacing::GBA_FRAME_PERIOD.as_secs_f64();
    assert_eq!(cadence, expected);
    assert!(cadence < f64::from(AudioOutput::M4A_MIXER_RATE));

    let deficit = f64::from(AudioOutput::M4A_MIXER_RATE) - cadence;
    assert!(
        (deficit - 0.039_633_6).abs() < 1e-6,
        "expected the rounded mixer rate to overstate the real production \
         cadence by ~0.0396336 Hz, got {deficit}"
    );
}

#[test]
fn source_for_device_never_takes_the_direct_shortcut_even_at_the_exact_mixer_rate() {
    let (_producer, consumer) = ring_buffer(64);
    let source = source_for_device(
        consumer,
        AudioOutput::CHANNELS,
        AudioOutput::M4A_MIXER_RATE,
        0,
    )
    .expect("an exact-rate device must still construct a resampler");
    assert!(
        matches!(source, Source::Resampled(_)),
        "an exact-M4A_MIXER_RATE device must resample at the real cadence, \
         not take a Direct shortcut"
    );
}

/// Stereo frames per prefill/production block in the long-horizon
/// regression below, matching the real per-game-frame contract.
const LONG_HORIZON_CHANNELS: usize = 2;

/// Simulates a free-running device clock without sleeping or a real
/// device: an integer nanosecond phase accumulator that, each
/// [`Self::tick`], pulls however many device frames the elapsed
/// wall-clock time actually owes the device
/// (`device_rate * GBA_FRAME_PERIOD`), carrying the sub-frame remainder
/// forward exactly like real hardware would. Used only by
/// `long_horizon_playback_keeps_the_ring_level_bounded_at_the_corrected_cadence`.
struct DeviceClock {
    device_rate: u32,
    period_ns: u128,
    total_ns: u128,
    frames_pulled: u128,
    scratch: Vec<f32>,
}

impl DeviceClock {
    fn new(device_rate: u32) -> Self {
        Self {
            device_rate,
            period_ns: crate::pacing::GBA_FRAME_PERIOD.as_nanos(),
            total_ns: 0,
            frames_pulled: 0,
            scratch: vec![0.0; 1024 * LONG_HORIZON_CHANNELS],
        }
    }

    /// One simulated real GBA frame: push the producer's fixed
    /// per-frame contract, then pull whatever elapsed time actually owes
    /// the device (see [`Self`]'s docs).
    fn tick(&mut self, producer: &Producer, source: &mut Source, block: &[f32]) {
        assert_eq!(
            producer.push(block),
            block.len(),
            "the ring must never overrun a producer pushing exactly the \
             real per-frame contract"
        );
        self.total_ns += u128::from(self.device_rate) * self.period_ns;
        let target_frames = self.total_ns / 1_000_000_000;
        let to_pull = target_frames - self.frames_pulled;
        if to_pull > 0 {
            let needed = usize::try_from(to_pull).unwrap() * LONG_HORIZON_CHANNELS;
            if needed > self.scratch.len() {
                self.scratch.resize(needed, 0.0);
            }
            source.fill(&mut self.scratch[..needed]);
            self.frames_pulled = target_frames;
        }
    }
}

/// Queued stereo frames currently sitting in `producer`'s ring.
fn queued_frames(producer: &Producer) -> i64 {
    let queued = producer.capacity() - producer.available_space();
    i64::try_from(queued / LONG_HORIZON_CHANNELS).unwrap()
}

/// One `device_rate`'s worth of
/// `long_horizon_playback_keeps_the_ring_level_bounded_at_the_corrected_cadence`,
/// factored out to keep that test under the line-count lint.
fn assert_ring_level_stays_bounded(device_rate: u32) {
    const CAPACITY_FRAMES: usize = 4096; // matches the real production ring
    const WARMUP_TICKS: u32 = 1_200; // ~20.1 simulated seconds
    const MEASURE_TICKS: u32 = 50_000; // ~837.1 simulated seconds (~14 min)

    // Draining at the rounded 13379 Hz drifts ~0.04 frames/s, ~33 frames
    // over `MEASURE_TICKS`, so a regressed cadence fails by a wide margin.
    const TOLERANCE_FRAMES: i64 = 4;

    let (producer, consumer) = ring_buffer(CAPACITY_FRAMES * LONG_HORIZON_CHANNELS);
    let mut source = source_for_device(consumer, AudioOutput::CHANNELS, device_rate, 0)
        .expect("a real device rate must always construct a Resampler");

    // Prefill to half capacity in whole game-frame blocks, mirroring the
    // real integration crate's startup (`MusicPlayer::prefill`).
    let block = [0.5_f32; 224 * LONG_HORIZON_CHANNELS];
    let target = producer.available_space() / 2;
    let mut queued_samples = 0;
    while queued_samples + block.len() <= target {
        assert_eq!(producer.push(&block), block.len());
        queued_samples += block.len();
    }

    let mut clock = DeviceClock::new(device_rate);
    for _ in 0..WARMUP_TICKS {
        clock.tick(&producer, &mut source, &block);
    }

    let baseline = queued_frames(&producer);
    assert_eq!(
        producer.underruns(),
        0,
        "device_rate {device_rate}: prefill headroom must survive warmup with no underrun"
    );

    for _ in 0..MEASURE_TICKS {
        clock.tick(&producer, &mut source, &block);
    }

    let drift = queued_frames(&producer) - baseline;
    assert_eq!(
        producer.underruns(),
        0,
        "device_rate {device_rate}: the ring must never underrun at the corrected cadence"
    );
    assert!(
        drift.abs() <= TOLERANCE_FRAMES,
        "device_rate {device_rate}: queued level drifted {drift} frames over \
         {MEASURE_TICKS} simulated frames at the corrected cadence (tolerance \
         {TOLERANCE_FRAMES})"
    );

    let measure_seconds = f64::from(MEASURE_TICKS) * crate::pacing::GBA_FRAME_PERIOD.as_secs_f64();
    let implied_drain_hours = if drift < 0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "drift is a small frame count, far below f64's exact-integer range"
        )]
        let drift_rate = (-drift) as f64 / measure_seconds;
        #[expect(
            clippy::cast_precision_loss,
            reason = "baseline is a small frame count, far below f64's exact-integer range"
        )]
        let headroom = baseline as f64;
        (headroom / drift_rate) / 3600.0
    } else {
        f64::INFINITY
    };
    assert!(
        implied_drain_hours > 100.0,
        "device_rate {device_rate}: the measured drift rate implies the prefilled \
         headroom would drain in only {implied_drain_hours:.2}h of continuous play \
         — far short of the 'many hours' this must stay bounded over"
    );
}

/// Simulates ~14 minutes at the exact-mixer-rate and 48 kHz device rates,
/// then extrapolates the measured drift rate to assert the prefilled
/// headroom outlasts 100 hours; at the correct cadence the drift is
/// bounded priming error, not a function of time.
#[test]
fn long_horizon_playback_keeps_the_ring_level_bounded_at_the_corrected_cadence() {
    for device_rate in [AudioOutput::M4A_MIXER_RATE, 48_000] {
        assert_ring_level_stays_bounded(device_rate);
    }
}

#[test]
fn null_backend_reports_the_m4a_mixer_rate() {
    let output = AudioOutput::null(256);
    // The nominal producer contract is upstream's 13379 Hz M4A mixer
    // rate, not the SOUNDBIAS carrier; see `M4A_MIXER_RATE`'s derivation.
    assert_eq!(output.sample_rate(), 13_379);
    assert_eq!(output.sample_rate(), AudioOutput::M4A_MIXER_RATE);
    assert_eq!(output.device_sample_rate(), AudioOutput::M4A_MIXER_RATE);
    assert_eq!(output.max_callback_frames(), None);
    assert_eq!(output.channels(), AudioOutput::CHANNELS);
    assert!(!output.is_running());
    // The null backend owns no cpal stream, so it never records errors.
    assert_eq!(output.stream_errors(), 0);
}

#[test]
fn start_and_stop_toggle_running_state_on_the_null_backend() {
    let mut output = AudioOutput::null(256);
    assert!(!output.is_running());
    output.start().expect("null backend never errors");
    assert!(output.is_running());
    output.stop().expect("null backend never errors");
    assert!(!output.is_running());
}

#[test]
fn producer_writes_are_audible_through_pull_null() {
    let mut output = AudioOutput::null(256);
    let producer = output.producer();
    assert_eq!(producer.push(&[1.0, 2.0, 3.0, 4.0]), 4);

    let mut out = [0.0; 4];
    output.pull_null(&mut out);
    assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(output.underruns(), 0);
}

#[test]
fn null_backend_underrun_fills_silence_and_is_visible_via_underruns() {
    let mut output = AudioOutput::null(256);
    let mut out = [7.0; 3];
    output.pull_null(&mut out);
    assert_eq!(out, [0.0, 0.0, 0.0]);
    assert_eq!(output.underruns(), 3);
}

#[test]
fn producer_from_another_thread_reaches_the_null_backend() {
    let mut output = AudioOutput::null(256);
    let producer = output.producer();
    let data = vec![1.0_f32, -1.0, 0.5, -0.5];
    let expected = data.clone();

    let handle = std::thread::spawn(move || {
        assert_eq!(producer.push(&data), 4);
    });
    handle.join().expect("producer thread panicked");

    let mut out = [0.0; 4];
    output.pull_null(&mut out);
    assert_eq!(out, expected.as_slice());
}

#[test]
fn delay_to_frames_converts_an_exact_delay_at_the_device_rate() {
    assert_eq!(delay_to_frames(Duration::from_millis(10), 48_000), 480);
}

/// Rounding a fractional-frame delay down would let a caller read a
/// frame as sounded one frame before it actually is.
#[test]
fn delay_to_frames_rounds_a_fractional_frame_up() {
    assert_eq!(delay_to_frames(Duration::from_nanos(1), 48_000), 1);
}

#[test]
fn estimate_sounded_frames_treats_an_exact_zero_delay_as_valid() {
    let ts = cpal::OutputStreamTimestamp {
        callback: cpal::StreamInstant::new(1, 0),
        playback: cpal::StreamInstant::new(1, 0),
    };
    assert_eq!(
        estimate_sounded_frames(1_000, ts, 48_000),
        Some(SoundedEstimate {
            sounded_frames: 1_000,
            startup_delay_frames: 0,
        })
    );
}

#[test]
fn estimate_sounded_frames_is_none_for_an_inverted_timestamp_pair() {
    let ts = cpal::OutputStreamTimestamp {
        callback: cpal::StreamInstant::new(2, 0),
        playback: cpal::StreamInstant::new(1, 0),
    };
    assert_eq!(estimate_sounded_frames(1_000, ts, 48_000), None);
}

#[test]
fn estimate_sounded_frames_keeps_the_startup_delay_saturation_hides() {
    // 9,600 frames submitted, 450 ms (21,600 frames) of delay: nothing has
    // sounded, and playback begins 12,000 frames past the callback's start.
    let ts = cpal::OutputStreamTimestamp {
        callback: cpal::StreamInstant::new(1, 0),
        playback: cpal::StreamInstant::new(1, 450_000_000),
    };
    assert_eq!(
        estimate_sounded_frames(9_600, ts, 48_000),
        Some(SoundedEstimate {
            sounded_frames: 0,
            startup_delay_frames: 12_000,
        })
    );
}

#[test]
fn every_usable_callback_restamps_the_reading_and_a_stale_one_keeps_it() {
    let clock = PlaybackClock::new();
    clock.record_reading(100, at(&clock, 10), usable(60));
    clock.record_reading(100, at(&clock, 20), usable(60)); // repeated estimate
    clock.record_reading(100, at(&clock, 30), usable(40)); // regressed estimate
    assert_eq!(
        clock.snapshot().usable_reading,
        Some(reading(&clock, 30, 40))
    );
    assert_eq!(clock.snapshot().sounded_frames, 60);
    clock.record(100, |_| None);
    assert_eq!(
        clock.snapshot().usable_reading,
        Some(reading(&clock, 30, 40))
    );
}

#[test]
fn a_callback_observed_before_the_origin_is_stale() {
    let clock = PlaybackClock::new();
    let Some(before) = clock.origin.checked_sub(Duration::from_millis(1)) else {
        return;
    };
    clock.record_reading(100, before, usable(60));
    assert_eq!(clock.snapshot().usable_reading, None);
    assert_eq!(clock.snapshot().usable_through_frames, 0);
}

#[test]
fn playback_clock_publishes_a_monotonic_estimate_under_host_jitter() {
    let clock = PlaybackClock::new();
    let ts_with_delay = |delay_ms: u64| cpal::OutputStreamTimestamp {
        callback: cpal::StreamInstant::ZERO,
        playback: cpal::StreamInstant::from_millis(delay_ms),
    };

    // Callback 1: nothing submitted yet outsizes a 10ms (480-frame)
    // delay, so the estimate floors at zero.
    clock.record_callback(
        1_000,
        &cpal::OutputCallbackInfo::new(ts_with_delay(10)),
        48_000,
    );
    assert_eq!(clock.sounded_frames.load(Ordering::Acquire), 0);

    // Callback 2: 1000 frames now submitted outsize the same delay.
    clock.record_callback(
        1_000,
        &cpal::OutputCallbackInfo::new(ts_with_delay(10)),
        48_000,
    );
    assert_eq!(clock.sounded_frames.load(Ordering::Acquire), 520);

    // Callback 3: a jitter spike inflates the reported delay past the
    // submitted position, so the naive estimate would fall back to
    // zero -- the published estimate must hold at 520 instead.
    clock.record_callback(
        1_000,
        &cpal::OutputCallbackInfo::new(ts_with_delay(60)),
        48_000,
    );
    assert_eq!(
        clock.sounded_frames.load(Ordering::Acquire),
        520,
        "a jittery spike in the reported delay must not move the published position backward"
    );

    // Callback 4: delay returns to normal, and the estimate resumes
    // advancing past the held value.
    clock.record_callback(
        1_000,
        &cpal::OutputCallbackInfo::new(ts_with_delay(10)),
        48_000,
    );
    assert_eq!(clock.sounded_frames.load(Ordering::Acquire), 2_520);
}

#[test]
fn playback_progress_is_none_until_a_callback_or_test_hook_publishes_one() {
    let output = AudioOutput::null(256);
    assert_eq!(output.playback_progress(), None);
}

#[test]
fn pull_null_advances_submitted_frames_but_playback_progress_stays_none_by_default() {
    let mut output = AudioOutput::null(256);
    let producer = output.producer();
    assert_eq!(producer.push(&[0.0; 8]), 8);
    let mut out = [0.0; 8];
    output.pull_null(&mut out);
    // The null backend never publishes a timestamp on its own; a test
    // must opt in via `enable_playback_progress_for_test`.
    assert_eq!(output.playback_progress(), None);
}

#[test]
fn the_test_hooks_publish_a_measured_playback_position_on_the_null_backend() {
    let mut output = AudioOutput::null(256);
    let producer = output.producer();
    assert_eq!(producer.push(&[0.0; 8]), 8);
    let mut out = [0.0; 8];
    output.pull_null(&mut out); // 4 stereo frames submitted

    output.enable_playback_progress_for_test();
    let progress = output
        .playback_progress()
        .expect("the test hook must publish availability");
    assert_eq!(progress.submitted_frames, 4);
    assert_eq!(progress.sounded_frames, 0);

    output.advance_sounded_frames_for_test(4);
    assert_eq!(output.playback_progress().unwrap().sounded_frames, 4);

    // Monotonic even for the fake clock: a lower value must not regress it.
    output.advance_sounded_frames_for_test(1);
    assert_eq!(output.playback_progress().unwrap().sounded_frames, 4);
    assert_eq!(
        output.playback_progress().unwrap().usable_through_frames,
        4,
        "a hook call marks every frame submitted so far as usable"
    );
}

#[test]
fn a_usable_callback_is_counted_even_when_its_estimate_does_not_grow() {
    let clock = PlaybackClock::new();
    clock.record(100, |_| Some(60));
    clock.record(100, |_| Some(60)); // repeated estimate
    clock.record(100, |_| Some(40)); // regressed estimate
    assert_eq!(clock.sounded_frames.load(Ordering::Acquire), 60);
    assert_eq!(clock.usable_through_frames.load(Ordering::Acquire), 300);
    assert_eq!(clock.submitted_frames.load(Ordering::Acquire), 300);
}

#[test]
fn a_stale_callback_advances_submitted_frames_without_the_usable_mark() {
    let clock = PlaybackClock::new();
    clock.record(100, |_| Some(60));
    clock.record(100, |_| None);
    assert_eq!(clock.usable_through_frames.load(Ordering::Acquire), 100);
    assert_eq!(clock.submitted_frames.load(Ordering::Acquire), 200);
}

#[test]
fn the_submitted_store_lands_after_the_estimate_and_usable_mark_stores() {
    let clock = PlaybackClock::new();
    clock.record(100, |start| {
        // Mid-callback, between the estimate and the stores: the frames are
        // not yet published, so a reader cannot see them ahead of the mark.
        assert_eq!(start, 0);
        assert_eq!(clock.submitted_frames.load(Ordering::Acquire), 0);
        Some(10)
    });
    assert_eq!(clock.submitted_frames.load(Ordering::Acquire), 100);
    assert_eq!(clock.usable_through_frames.load(Ordering::Acquire), 100);
}

#[test]
fn the_null_backend_reports_a_zero_settle_margin() {
    assert_eq!(AudioOutput::null(256).playback_settle_margin_frames(), 0);
}

#[test]
fn null_resampled_reports_the_resamplers_own_settle_margin() {
    // Same rates as the module-level regression this margin exists to
    // fix (see `crate::resample`'s `a_ring_emptied_mid_callback_still_sounds_real_audio_in_the_next_one`):
    // step = 25/100 = 0.25.
    let output = AudioOutput::null_resampled(64, 25.0, 100, 16)
        .expect("a real rate ratio must always construct a Resampler");
    assert_eq!(output.playback_settle_margin_frames(), 8);
}

#[test]
fn null_resampled_refuses_the_same_rate_ratios_resampler_new_refuses() {
    // `AudioOutput` has no `Debug` impl (it owns a non-`Debug` `cpal::Stream`
    // on the real-device path), so the failure is matched out by hand
    // rather than via `expect_err`.
    let Err(err) = AudioOutput::null_resampled(64, f64::NAN, 100, 16) else {
        panic!("a non-finite source rate must be refused");
    };
    assert!(matches!(
        err,
        PlatformError::UnsupportedResampleRatio { .. }
    ));
}

#[test]
fn pull_null_drives_a_resampled_source_and_advances_submitted_frames_by_stereo_frames() {
    let mut output = AudioOutput::null_resampled(64, 25.0, 100, 16)
        .expect("a real rate ratio must always construct a Resampler");
    let producer = output.producer();
    // One interleaved stereo source frame: `null_resampled` always uses
    // `AudioOutput::CHANNELS` (stereo), matching every real device.
    assert_eq!(producer.push(&[1.0, 2.0]), 2);

    output.enable_playback_progress_for_test();
    let mut out = [0.0_f32; 4]; // two interleaved stereo output frames
    output.pull_null(&mut out);
    assert_eq!(
        output.playback_progress().unwrap().submitted_frames,
        2,
        "submitted_frames must count stereo frames (out.len() / channels), not raw \
         interleaved samples, exactly as the real device callback does"
    );
}

#[test]
fn f32_to_i16_clamps_out_of_range_input() {
    assert_eq!(f32_to_i16(0.0), 0);
    assert_eq!(f32_to_i16(1.0), i16::MAX);
    assert_eq!(f32_to_i16(2.0), i16::MAX);
    assert_eq!(f32_to_i16(-2.0), -i16::MAX);
}

#[test]
fn fill_i16_output_processes_data_larger_than_scratch_in_bounded_chunks() {
    // `scratch` here is deliberately smaller than `data`, standing in
    // for a callback larger than the device advertised: every sample
    // must still be converted, processed in `scratch.len()`-sized
    // chunks, without ever resizing `scratch`.
    let (producer, consumer) = ring_buffer(64);
    #[expect(
        clippy::cast_precision_loss,
        reason = "i is 0..20, exactly representable in f32"
    )]
    let pcm: Vec<f32> = (0..20_i32).map(|i| (i as f32 - 10.0) / 10.0).collect();
    assert_eq!(producer.push(&pcm), 20);

    let mut source = Source::Direct(consumer);
    let mut scratch = vec![0.0; 6];
    let scratch_capacity = scratch.len();
    let mut data = vec![0_i16; 20];

    fill_i16_output(&mut source, &mut scratch, &mut data);

    assert_eq!(
        scratch.len(),
        scratch_capacity,
        "fill_i16_output must never grow scratch"
    );
    let expected: Vec<i16> = pcm.iter().map(|&sample| f32_to_i16(sample)).collect();
    assert_eq!(data, expected);
}

#[test]
fn an_extreme_advertised_maximum_does_not_inflate_i16_scratch_capacity() {
    // `i16_scratch_capacity` is what `build_stream`'s `i16` arm sizes
    // scratch against (see `MAX_SCRATCH_CALLBACK_FRAMES`'s docs); pin
    // the cap for a device advertising `u32::MAX` without allocating it.
    let huge_frames = usize::try_from(u32::MAX).unwrap();
    let capacity = i16_scratch_capacity(huge_frames, usize::from(AudioOutput::CHANNELS));
    assert_eq!(capacity, DEFAULT_SCRATCH_SAMPLES);
}

#[test]
fn scratch_capped_from_a_maximum_above_the_cap_still_chunks_an_oversized_fill_correctly() {
    // Route an advertised maximum comfortably above the cap through the
    // actual production capacity function, then drive `fill_i16_output`
    // with scratch sized exactly as `build_stream` would -- proving the
    // cap changes chunk granularity, never correctness.
    let above_cap = MAX_SCRATCH_CALLBACK_FRAMES * 4;
    let channels = usize::from(AudioOutput::CHANNELS);
    let capacity = i16_scratch_capacity(above_cap, channels);
    assert_eq!(capacity, DEFAULT_SCRATCH_SAMPLES);

    let (producer, consumer) = ring_buffer(64);
    #[expect(
        clippy::cast_precision_loss,
        reason = "i is 0..20, exactly representable in f32"
    )]
    let pcm: Vec<f32> = (0..20_i32).map(|i| (i as f32 - 10.0) / 10.0).collect();
    assert_eq!(producer.push(&pcm), 20);

    let mut source = Source::Direct(consumer);
    let mut scratch = vec![0.0; capacity];
    let mut data = vec![0_i16; 20];
    fill_i16_output(&mut source, &mut scratch, &mut data);

    let expected: Vec<i16> = pcm.iter().map(|&sample| f32_to_i16(sample)).collect();
    assert_eq!(data, expected);
}

#[test]
fn capped_scratch_chunks_a_fill_larger_than_the_cap_derived_chunk() {
    let above_cap = MAX_SCRATCH_CALLBACK_FRAMES * 4;
    let channels = usize::from(AudioOutput::CHANNELS);
    let capacity = i16_scratch_capacity(above_cap, channels);
    let len = capacity + 7;

    let (producer, consumer) = ring_buffer(len);
    #[expect(
        clippy::cast_precision_loss,
        reason = "the remainder is below 21, exact in f32"
    )]
    let pcm: Vec<f32> = (0..len).map(|i| ((i % 21) as f32 - 10.0) / 10.0).collect();
    assert_eq!(producer.push(&pcm), len);

    let mut source = Source::Direct(consumer);
    let mut scratch = vec![0.0; capacity];
    let mut data = vec![0_i16; len];
    fill_i16_output(&mut source, &mut scratch, &mut data);

    assert!(
        data.len() > scratch.len(),
        "the fill must cross the cap-derived chunk boundary"
    );
    assert_eq!(scratch.len(), capacity, "scratch must never grow");
    let expected: Vec<i16> = pcm.iter().map(|&sample| f32_to_i16(sample)).collect();
    assert_eq!(data, expected);
}

#[test]
fn sample_format_rank_prefers_f32_then_i16() {
    assert!(
        sample_format_rank(cpal::SampleFormat::F32) < sample_format_rank(cpal::SampleFormat::I16)
    );
    assert!(
        sample_format_rank(cpal::SampleFormat::I16) < sample_format_rank(cpal::SampleFormat::U16)
    );
}

#[test]
fn select_config_prefers_f32_at_the_exact_target_rate() {
    // Both openable formats cover the target; f32 wins on preference.
    let candidates = [
        (cpal::SampleFormat::I16, 8_000, 48_000),
        (cpal::SampleFormat::F32, 8_000, 48_000),
    ];
    assert_eq!(select_config(&candidates, 13_379), Some((1, 13_379)));
}

#[test]
fn select_config_ignores_an_unopenable_format_that_covers_the_target() {
    // Regression: a u16 config whose range covers the target must not be
    // chosen (build_stream can't open it). No openable format covers
    // 13379 here, so selection falls back to the preferred openable
    // format's nearest rate — never the u16 config.
    let candidates = [
        (cpal::SampleFormat::U16, 8_000, 48_000), // covers 13379, unopenable
        (cpal::SampleFormat::F32, 44_100, 48_000), // openable, nearest = 44100
        (cpal::SampleFormat::I16, 44_100, 48_000),
    ];
    assert_eq!(select_config(&candidates, 13_379), Some((1, 44_100)));
}

#[test]
fn select_config_prefers_an_exact_i16_rate_over_a_resampled_f32() {
    // An exact-rate config still builds a Resampler (see Source's docs),
    // but with a near-1.0 step and thus minimal interpolation error, so
    // it must win over an off-rate config even in the ring buffer's
    // preferred format: i16 at 13379 exactly (distance 0) beats f32 at
    // 44100 (distance 30721) despite f32 ranking ahead on format alone.
    let candidates = [
        (cpal::SampleFormat::F32, 44_100, 48_000), // distance 30721
        (cpal::SampleFormat::I16, 8_000, 48_000),  // covers 13379 exactly
    ];
    assert_eq!(select_config(&candidates, 13_379), Some((1, 13_379)));
}

#[test]
fn select_config_picks_nearest_rate_within_a_format() {
    // Two f32 ranges, the LATER one nearer the target: distance decides
    // within a format, independent of device-enumeration order.
    let candidates = [
        (cpal::SampleFormat::F32, 44_100, 48_000), // nearest 44100, distance 30721
        (cpal::SampleFormat::F32, 8_000, 11_025),  // nearest 11025, distance 2354
    ];
    assert_eq!(select_config(&candidates, 13_379), Some((1, 11_025)));
}

#[test]
fn select_config_clamps_to_nearest_when_no_openable_format_covers_target() {
    // Only openable format sits entirely above the target: clamp up.
    let candidates = [(cpal::SampleFormat::F32, 44_100, 48_000)];
    assert_eq!(select_config(&candidates, 13_379), Some((0, 44_100)));
}

#[test]
fn select_config_returns_none_with_no_openable_format() {
    let candidates = [(cpal::SampleFormat::U16, 8_000, 48_000)];
    assert_eq!(select_config(&candidates, 13_379), None);
}

/// `ms` after the clock's origin: an explicit callback observation time.
fn at(clock: &PlaybackClock, ms: u64) -> Instant {
    clock.origin + Duration::from_millis(ms)
}

/// A usable estimate of `sounded` frames with no startup delay.
fn usable(sounded: u64) -> impl FnOnce(u64) -> Option<SoundedEstimate> {
    move |_| {
        Some(SoundedEstimate {
            sounded_frames: sounded,
            startup_delay_frames: 0,
        })
    }
}

/// The reading a usable callback at `ms` with `sounded` frames publishes.
fn reading(clock: &PlaybackClock, ms: u64, sounded: u64) -> UsableReading {
    UsableReading {
        observed_at: at(clock, ms),
        sounded_frames: sounded,
        startup_delay_frames: 0,
    }
}

#[test]
fn a_snapshot_never_pairs_a_new_estimate_with_an_older_mark() {
    // Stale callbacks have left the mark behind the submitted frames; a new
    // usable callback then completes between the reader's mark and estimate
    // loads. Loading the live counters would see the old submitted total and
    // mark with the new, still-short estimate.
    let clock = PlaybackClock::new();
    clock.record_reading(100, at(&clock, 10), usable(10));
    clock.record(100, |_| None);
    let mut interleaved = false;
    let snapshot = clock.snapshot_observing(|loads| {
        if loads == 2 && !std::mem::replace(&mut interleaved, true) {
            clock.record_reading(100, at(&clock, 150), usable(150));
        }
    });

    assert!(interleaved);
    assert_eq!(
        snapshot,
        PlaybackProgress {
            submitted_frames: 300,
            sounded_frames: 150,
            usable_through_frames: 300,
            usable_reading: Some(reading(&clock, 150, 150)),
        },
        "the retried read takes the new callback whole"
    );
}

#[test]
fn a_snapshot_two_callbacks_overtake_is_retried_whole() {
    // Two callbacks complete during one read, rewriting the copy it was
    // reading: it must retry rather than mix the two.
    let clock = PlaybackClock::new();
    clock.record_reading(100, at(&clock, 10), usable(10));
    clock.record(100, |_| None);
    let mut interleaved = false;
    let snapshot = clock.snapshot_observing(|loads| {
        if loads == 2 && !std::mem::replace(&mut interleaved, true) {
            clock.record_reading(100, at(&clock, 150), usable(150));
            clock.record_reading(100, at(&clock, 160), usable(160));
        }
    });

    assert!(interleaved);
    assert_eq!(
        snapshot,
        PlaybackProgress {
            submitted_frames: 400,
            sounded_frames: 160,
            usable_through_frames: 400,
            usable_reading: Some(reading(&clock, 160, 160)),
        }
    );
}

#[test]
fn a_snapshot_during_a_callback_mid_store_reads_the_last_completed_one() {
    // No snapshot taken before: the callback is descheduled mid-store, with
    // its estimate published to the live counters but not its mark.
    let clock = PlaybackClock::new();
    clock.record_reading(100, at(&clock, 10), usable(10));
    clock.record(100, |_| None);
    let mut during = None;
    clock.write(|| {
        clock.sounded_frames.fetch_max(150, Ordering::Release);
        during = Some(clock.snapshot());
        clock.usable_through_frames.store(300, Ordering::Release);
        clock.submitted_frames.store(300, Ordering::Release);
    });

    assert_eq!(
        during,
        Some(PlaybackProgress {
            submitted_frames: 200,
            sounded_frames: 10,
            usable_through_frames: 100,
            usable_reading: Some(reading(&clock, 10, 10)),
        })
    );
    assert_eq!(
        clock.snapshot(),
        PlaybackProgress {
            submitted_frames: 300,
            sounded_frames: 150,
            usable_through_frames: 300,
            usable_reading: Some(reading(&clock, 10, 10)),
        }
    );
}

#[test]
fn a_snapshot_never_takes_a_rewritten_slot_before_it_is_published() {
    // The reader takes the published slot's index; a callback then publishes
    // the other slot, and the next one rewrites the first and is descheduled
    // before naming it published. At either pause point the read must return
    // only published counters, so the following read cannot go backward.
    for pause_at in [0_u32, 3] {
        let clock = PlaybackClock::new();
        clock.record_reading(100, at(&clock, 10), usable(10));
        let mut interleaved = false;
        let first = clock.snapshot_observing(|loads| {
            if loads == pause_at && !std::mem::replace(&mut interleaved, true) {
                clock.record_reading(100, at(&clock, 20), usable(20));
                clock.submitted_frames.store(300, Ordering::Release);
                let unpublished = 1 - clock.published_slot.load(Ordering::Relaxed);
                clock.fill_slot(unpublished);
            }
        });
        let second = clock.snapshot();

        assert!(interleaved);
        assert!(
            first.submitted_frames <= second.submitted_frames,
            "pause at {pause_at}: {first:?} then {second:?}"
        );
        assert_eq!(
            second,
            PlaybackProgress {
                submitted_frames: 200,
                sounded_frames: 20,
                usable_through_frames: 200,
                usable_reading: Some(reading(&clock, 20, 20)),
            },
            "pause at {pause_at}: only the published callback is read"
        );
    }
}

/// Frame capacities whose interleaved sample count overflows `usize` are
/// refused instead of wrapping into a smaller ring, in release builds too.
#[test]
fn frame_ring_rejects_capacity_overflow() {
    let frames = usize::MAX / 2 + 1;
    assert!(matches!(
        frame_ring(frames, AudioOutput::CHANNELS),
        Err(PlatformError::AudioRingCapacityOverflow { frames: f, channels: 2 }) if f == frames
    ));
    assert!(matches!(
        frame_ring(usize::MAX, 2),
        Err(PlatformError::AudioRingCapacityOverflow { .. })
    ));
}

/// An in-range frame capacity yields a ring of `frames * channels` samples.
#[test]
fn frame_ring_accepts_in_range_capacity() {
    let (producer, _consumer) = frame_ring(4, 2).expect("small capacity fits");
    assert_eq!(producer.push(&[0.0; 9]), 8);
}

#[test]
#[should_panic(expected = "overflows the interleaved sample count")]
fn null_panics_on_capacity_overflow() {
    let _ = AudioOutput::null(usize::MAX);
}

#[test]
fn null_resampled_rejects_capacity_overflow() {
    assert!(matches!(
        AudioOutput::null_resampled(usize::MAX, 32_000.0, 48_000, 512),
        Err(PlatformError::AudioRingCapacityOverflow { .. })
    ));
}
