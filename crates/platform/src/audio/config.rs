//! Selects a supported stereo output format and rate for the audio stream.

use crate::error::PlatformError;

use super::stream::OutputDevice;
use super::AudioOutput;

/// Rank a device's sample format by how directly the ring buffer's `f32`
/// samples map onto it — lower is preferred.
pub(super) fn sample_format_rank(format: cpal::SampleFormat) -> u8 {
    match format {
        cpal::SampleFormat::F32 => 0,
        cpal::SampleFormat::I16 => 1,
        _ => 2,
    }
}

/// Whether [`build_stream`](super::stream::build_stream) can actually open a stream in this sample format.
/// Only `f32` and `i16` are convertible to/from the ring buffer's `f32`
/// samples; any other format (e.g. `u16`) hits `build_stream`'s `_ =>` error
/// arm, so it must never be selected — see [`select_config`].
fn is_openable_format(format: cpal::SampleFormat) -> bool {
    matches!(format, cpal::SampleFormat::F32 | cpal::SampleFormat::I16)
}

/// Pure config selection over device candidates reduced to
/// `(sample_format, min_rate, max_rate)` tuples, so it is unit-testable
/// without a `cpal` device (see the tests below).
///
/// Candidates are first restricted to formats [`build_stream`](super::stream::build_stream) can open
/// (`f32`/`i16`) — a `u16`-only config whose rate range happens to cover the
/// target must never win, or `AudioOutput::open` would hard-fail instead of
/// resampling on an available openable format. Each openable candidate is then
/// scored by `(distance, format rank)`, where `distance` is how far `target`
/// must be clamped to land inside the candidate's `[min, max]` range, and the
/// minimum is chosen:
///
/// - **Distance is primary**: the nearest achievable rate wins, regardless of
///   format or the order the device enumerated its ranges. A range that
///   covers `target` has distance `0`, so an exact-rate candidate always
///   beats one that needs resampling further — an exact-Hz device still
///   builds a [`Resampler`](crate::resample::Resampler) (see [`Source`](super::Source)'s docs), but with a step nearest
///   `1.0`, minimizing interpolation error — so it must not lose to a mere
///   format preference.
/// - **Format rank is the tie-break** ([`sample_format_rank`]: `f32` before
///   `i16`) between candidates equally far from `target`. Ties (equal
///   distance and rank) keep device-enumeration order.
///
/// Returns the chosen candidate's index into `candidates` and the rate to
/// open it at, or `None` if no openable candidate exists.
pub(super) fn select_config(
    candidates: &[(cpal::SampleFormat, u32, u32)],
    target: u32,
) -> Option<(usize, u32)> {
    (0..candidates.len())
        .filter(|&i| is_openable_format(candidates[i].0))
        .map(|i| {
            let (format, min, max) = candidates[i];
            let rate = target.clamp(min, max);
            (i, rate, rate.abs_diff(target), sample_format_rank(format))
        })
        .min_by_key(|&(_, _, distance, rank)| (distance, rank))
        .map(|(i, rate, _, _)| (i, rate))
}

/// Pick a stereo output configuration for [`AudioOutput::M4A_MIXER_RATE`],
/// delegating the selection policy to [`select_config`] and mapping the
/// chosen candidate back to a concrete [`cpal::SupportedStreamConfig`].
pub(super) fn negotiate<D: OutputDevice>(
    device: &D,
) -> Result<cpal::SupportedStreamConfig, PlatformError> {
    let candidates: Vec<cpal::SupportedStreamConfigRange> = device
        .supported_output_configs()
        .map_err(classify_query_error)?
        .into_iter()
        .filter(|c| c.channels() == AudioOutput::CHANNELS)
        .collect();

    let tuples: Vec<(cpal::SampleFormat, u32, u32)> = candidates
        .iter()
        .map(|c| (c.sample_format(), c.min_sample_rate(), c.max_sample_rate()))
        .collect();

    let (index, rate) = select_config(&tuples, AudioOutput::M4A_MIXER_RATE)
        .ok_or(PlatformError::UnsupportedAudioConfig)?;
    Ok(candidates[index].with_sample_rate(rate))
}

/// Map a failure of the *device query* stage, the first thing
/// [`AudioOutput::open`] asks of the device the host named.
///
/// A host names a device it cannot actually reach: `cpal`'s ALSA backend
/// hands out the logical `default` device whether or not any sound hardware
/// or sound server exists, so a headless Linux box with `libasound`
/// installed gets past the device lookup and only fails here. For every
/// caller that is the same fact [`PlatformError::NoAudioDevice`] states, so
/// it is reported as such.
///
/// Only this stage collapses that way. A device that answered the query is
/// real, so [`build_stream`](super::stream::build_stream) losing it afterwards stays on the plain
/// [`From<cpal::Error>`] mapping to [`PlatformError::Audio`] — a caller that
/// tolerates a headless run must still hear about a device that vanished
/// mid-setup. `a_lost_device_after_the_query_stays_an_audio_error` pins that
/// split against a real device.
pub(super) fn classify_query_error(err: cpal::Error) -> PlatformError {
    match err.kind() {
        cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::HostUnavailable => {
            PlatformError::NoAudioDevice
        }
        _ => PlatformError::Audio(err),
    }
}

/// The device's largest advertised callback size in frames, or `0` if the
/// device advertises no concrete range (`Unknown`). This is the *raw*
/// advertised value — [`AudioOutput::max_callback_frames`] exposes it
/// unmodified for callers that need the device's actual claim (e.g.
/// tail-wait timing). Consumers that pre-size real-time scratch off it cap
/// it first — see [`MAX_SCRATCH_CALLBACK_FRAMES`](super::stream::MAX_SCRATCH_CALLBACK_FRAMES) and [`Resampler::new`](crate::resample::Resampler::new).
pub(super) fn max_buffer_frames(config: &cpal::SupportedStreamConfig) -> usize {
    match config.buffer_size() {
        cpal::SupportedBufferSize::Range { max, .. } => usize::try_from(*max).unwrap_or(0),
        cpal::SupportedBufferSize::Unknown => 0,
    }
}
