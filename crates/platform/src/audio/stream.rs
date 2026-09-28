//! Converts and chunks PCM while building the device output stream.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::DeviceTrait;

use crate::error::PlatformError;

use super::config::max_buffer_frames;
use super::{PlaybackClock, Source};

/// Convert one `f32` sample in `[-1.0, 1.0]` to `i16`, clamping out-of-range
/// input rather than wrapping.
pub(super) fn f32_to_i16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0) * f32::from(i16::MAX);
    // The multiply above is bounded to `i16::MIN..=i16::MAX` by the clamp,
    // so this cast never truncates meaningfully.
    #[allow(clippy::cast_possible_truncation)]
    {
        clamped as i16
    }
}

/// Fallback capacity (interleaved `f32` samples) for the `i16` callback's
/// scratch buffer when the device advertises no concrete buffer-size range.
/// Generously larger than any realistic callback buffer (8192 stereo frames)
/// so pre-sizing still spares the real-time thread an allocation.
pub(super) const DEFAULT_SCRATCH_SAMPLES: usize = 8192 * 2;

/// Ceiling (in frames) applied to a device-advertised buffer maximum before
/// it sizes the `i16` scratch buffer. A device may advertise an
/// unconstrained callback range as an enormous concrete number instead of
/// `Unknown` (cpal's ALSA backend hands back `u32::MAX` for exactly this),
/// and [`fill_i16_output`] already chunks a callback past this scale, so
/// capping the preallocation here loses nothing but reserved-but-unused
/// memory.
pub(super) const MAX_SCRATCH_CALLBACK_FRAMES: usize = 8192;

/// Capacity (interleaved `f32` samples) for the `i16` callback's fixed
/// scratch buffer: `max_frames` capped at [`MAX_SCRATCH_CALLBACK_FRAMES`]
/// and floored at [`DEFAULT_SCRATCH_SAMPLES`], times `channels`. Pure so the
/// cap is unit-testable against an extreme advertised maximum without
/// allocating the buffer it sizes.
pub(super) fn i16_scratch_capacity(max_frames: usize, channels: usize) -> usize {
    max_frames
        .min(MAX_SCRATCH_CALLBACK_FRAMES)
        .saturating_mul(channels)
        .max(DEFAULT_SCRATCH_SAMPLES)
}

/// Fill `data` (interleaved `i16`) from `source`, converting through the
/// fixed `scratch` buffer in `scratch.len()`-bounded chunks rather than
/// resizing it — so a `data` larger than `scratch` still never allocates.
/// `scratch` is never resized here, regardless of how `data` compares to it.
pub(super) fn fill_i16_output(source: &mut Source, scratch: &mut [f32], data: &mut [i16]) {
    for dst_chunk in data.chunks_mut(scratch.len()) {
        let buf = &mut scratch[..dst_chunk.len()];
        source.fill(buf);
        for (dst, &sample) in dst_chunk.iter_mut().zip(buf.iter()) {
            *dst = f32_to_i16(sample);
        }
    }
}

/// The two `cpal` calls [`AudioOutput::open`] makes against a device,
/// behind a seam.
///
/// Which of the two failed is the whole distinction [`classify_query_error`]
/// draws: a query failure means the device was never reachable, a build
/// failure means a reachable device was lost. Naming both calls lets tests
/// force either failure with no audio device present, which the module
/// docs' headless rule requires.
///
/// [`negotiate`] collects the query's configurations anyway, so the seam
/// hands back a `Vec` rather than `cpal`'s associated iterator type: it
/// costs nothing and spares every implementor an associated type.
pub(super) trait OutputDevice {
    fn supported_output_configs(
        &self,
    ) -> Result<Vec<cpal::SupportedStreamConfigRange>, cpal::Error>;

    fn build_output_stream<T, D, E>(
        &self,
        config: cpal::StreamConfig,
        data_callback: D,
        error_callback: E,
        timeout: Option<std::time::Duration>,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: cpal::SizedSample,
        D: FnMut(&mut [T], &cpal::OutputCallbackInfo) + Send + 'static,
        E: FnMut(cpal::Error) + Send + 'static;
}

impl OutputDevice for cpal::Device {
    fn supported_output_configs(
        &self,
    ) -> Result<Vec<cpal::SupportedStreamConfigRange>, cpal::Error> {
        DeviceTrait::supported_output_configs(self).map(Iterator::collect)
    }

    fn build_output_stream<T, D, E>(
        &self,
        config: cpal::StreamConfig,
        data_callback: D,
        error_callback: E,
        timeout: Option<std::time::Duration>,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: cpal::SizedSample,
        D: FnMut(&mut [T], &cpal::OutputCallbackInfo) + Send + 'static,
        E: FnMut(cpal::Error) + Send + 'static,
    {
        DeviceTrait::build_output_stream(self, config, data_callback, error_callback, timeout)
    }
}

/// Build (but do not start) the output stream for `config`, driven by
/// `source`. Asynchronous stream errors are recorded into `stream_errors`,
/// the counter [`AudioOutput::stream_errors`] reads. Every callback also
/// records its device-frame count and the host's callback timestamp into
/// `playback_clock` (see [`PlaybackClock::record_callback`]) *before*
/// `source` consumes them, the signal [`AudioOutput::playback_progress`]
/// reads.
pub(super) fn build_stream<D: OutputDevice>(
    device: &D,
    config: &cpal::SupportedStreamConfig,
    mut source: Source,
    stream_errors: Arc<AtomicU64>,
    playback_clock: Arc<PlaybackClock>,
) -> Result<cpal::Stream, PlatformError> {
    let stream_config = config.config();
    // A `cpal` stream reports async failures (device disconnect, driver
    // error) on the callback thread, where nothing here can recover them.
    // Record each into a shared atomic so `AudioOutput::stream_errors` can
    // surface an otherwise-invisible unhealthy stream (`is_running` stays
    // `true`, `underruns` stays flat, because the drain callback simply stops
    // firing).
    let err_fn = move |_err: cpal::Error| {
        stream_errors.fetch_add(1, Ordering::Relaxed);
    };

    // Shared by both format arms below: `PlaybackClock::record_callback`
    // only needs the channel count and device rate, not the sample format.
    let channels = usize::from(config.channels()).max(1);
    let device_sample_rate = config.sample_rate();

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            stream_config,
            move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                let frame_count = u64::try_from(data.len() / channels).unwrap_or(u64::MAX);
                playback_clock.record_callback(frame_count, info, device_sample_rate);
                source.fill(data);
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => {
            // Fix the `f32` scratch buffer's length here, off the real-time
            // callback thread, so the callback never resizes it — see
            // `i16_scratch_capacity`. The callback below processes `data`
            // in `scratch`-sized chunks instead, so a `data` cpal hands us
            // larger than anything advertised still never grows `scratch`.
            let max_frames = max_buffer_frames(config);
            let scratch_capacity = i16_scratch_capacity(max_frames, channels);
            let mut scratch: Vec<f32> = vec![0.0; scratch_capacity];
            device.build_output_stream(
                stream_config,
                move |data: &mut [i16], info: &cpal::OutputCallbackInfo| {
                    let frame_count = u64::try_from(data.len() / channels).unwrap_or(u64::MAX);
                    playback_clock.record_callback(frame_count, info, device_sample_rate);
                    fill_i16_output(&mut source, &mut scratch, data);
                },
                err_fn,
                None,
            )?
        }
        _ => return Err(PlatformError::UnsupportedAudioConfig),
    };
    // `source` (and its underrun counter) is now owned by the callback
    // closure above; `AudioOutput::underruns` reads the same counter via
    // its `Producer` handle instead, since `Producer` and `Consumer` share
    // one `Arc`-backed counter (see `crate::ring`).
    Ok(stream)
}
