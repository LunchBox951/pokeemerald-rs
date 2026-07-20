//! Audio output device (S-1): opens the default output device — or a
//! headless-friendly null backend for tests/CI, since CI runners have no
//! audio device — and streams PCM pulled from a [`crate::ring`] ring buffer
//! that the future `audio` crate (M4A sequence engine, S-3) fills from its
//! own thread.
//!
//! `cpal` is owner-approved for exactly this crate and exactly this use
//! (Discussion #78): open the default output device, one stream, a
//! ring-buffer callback. No decoding, no effects — see [`Resampler`] below
//! for the one deliberate exception (bridging a sample-rate mismatch is
//! format adaptation, not an effect).
//!
//! ## Design
//!
//! - **Sample format**: the ring buffer always carries interleaved `f32`
//!   samples (cpal's most portable format, and natural headroom for
//!   downstream mixing). If the device's negotiated stream format is `i16`
//!   instead (common on Linux/ALSA), the device callback converts on the
//!   way out; the ring buffer and the `audio` crate's producer API never
//!   need to know.
//! - **Sample rate**: [`AudioOutput::M4A_MIXER_RATE`] (13379 Hz, the rate
//!   upstream's M4A engine actually renders PCM at — see the const's docs) is
//!   always the ring buffer's nominal rate; the `audio` crate renders at this
//!   rate unconditionally. Real output devices are 44.1/48 kHz and virtually
//!   never advertise 13379 Hz, so a [`crate::resample::Resampler`] linearly
//!   interpolating from nominal to the device's actual rate inside the
//!   callback (see [`Source::Resampled`]) is the common path. Direct 1:1
//!   streaming (see [`Source::Direct`]) only happens when a device supports
//!   13379 Hz exactly, or for the null backend.
//! - **Channels**: fixed at [`AudioOutput::CHANNELS`] (stereo), matching the
//!   GBA's Direct Sound A/B stereo output. A device with no stereo output
//!   config at all is out of scope and reported as
//!   [`PlatformError::UnsupportedAudioConfig`].
//! - **Underruns**: [`Source::fill`] always fills its output buffer
//!   completely; any shortfall is silence, counted via
//!   [`crate::ring::Consumer::fill`]'s single-lock bulk drain (see
//!   `crate::ring` and `crate::resample`) for later use by V-5 audio checks.
//! - **Stream health**: underruns cover the producer-outran-consumer case,
//!   but a `cpal` stream can also fail asynchronously (device disconnect,
//!   driver error) on its own callback thread. Those are counted separately
//!   via [`AudioOutput::stream_errors`]; a nonzero count means the stream is
//!   unhealthy even when [`AudioOutput::underruns`] stays flat.
//!
//! CI is headless, so nothing here opens a real cpal stream in a test: only
//! [`AudioOutput::open`] and the private `negotiate`/stream-building helpers
//! touch `cpal` directly. The ring buffer and resampler — the logic that
//! actually matters for correctness — are pure and fully unit tested
//! against [`AudioOutput::null`] and the `ring`/`resample` modules directly.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::error::PlatformError;
use crate::resample::Resampler;
use crate::ring::{ring_buffer, Consumer, Producer};

/// Either play ring-buffer samples straight through, or bridge a sample-rate
/// mismatch via [`Resampler`] — see the module docs.
///
/// Both variants bottom out in [`crate::ring::Consumer::fill`]'s single-lock
/// bulk drain, so the underrun-safe behaviour tested against the null backend
/// below is exactly what the real device callback runs.
enum Source {
    Direct(Consumer),
    Resampled(Resampler),
}

impl Source {
    fn fill(&mut self, out: &mut [f32]) {
        match self {
            Self::Direct(consumer) => consumer.fill(out),
            Self::Resampled(resampler) => resampler.fill(out),
        }
    }
}

/// The open output stream/device, or the null stand-in used by tests and
/// headless environments.
enum Backend {
    Null(Source),
    Device(cpal::Stream),
}

/// An owned audio-output subsystem: opens (at most) one output stream and
/// exposes a [`Producer`] handle the future `audio` crate fills from another
/// thread.
///
/// No global state: every [`AudioOutput`] owns its own device/stream (or
/// null stand-in) and ring buffer. Dropping it tears the backend down
/// cleanly — `cpal::Stream`'s own `Drop` stops the stream and releases the
/// device; the null backend holds no OS resources to release.
pub struct AudioOutput {
    backend: Backend,
    producer: Producer,
    /// The rate the `audio` crate should always render at; see the module
    /// docs. Not necessarily the device's physical rate — see
    /// `device_sample_rate`.
    sample_rate: u32,
    device_sample_rate: u32,
    channels: u16,
    running: bool,
    /// Count of asynchronous `cpal` stream errors reported to the callback's
    /// error function (device disconnect, driver failure, …). Shared with the
    /// stream's error closure; a nonzero value means the stream is unhealthy.
    /// Always zero for the null backend, which owns no `cpal` stream.
    stream_errors: Arc<AtomicU64>,
}

impl AudioOutput {
    /// The rate upstream's M4A engine actually renders PCM at — the nominal
    /// producer contract for the ring buffer and the future `audio` crate
    /// (S-3), which render against exactly this rate.
    ///
    /// Derived from `pokeemerald/src/m4a.c`: `m4aSoundInit` selects
    /// `SOUND_MODE_FREQ_13379` (m4a.c:79), and `SoundInit` calls
    /// `SampleFreqSet(SOUND_MODE_FREQ_13379)` (m4a.c:395). `SampleFreqSet`
    /// (m4a.c:400) looks up `gPcmSamplesPerVBlank = gPcmSamplesPerVBlankTable[
    /// freq - 1]` — with the `13379` frequency index that is table entry `224`
    /// (`m4a_tables.c:107`) — then computes
    /// `pcmFreq = (597275 * pcmSamplesPerVBlank + 5000) / 10000`
    /// (m4a.c:410) = `(597275 * 224 + 5000) / 10000` = **13379 Hz**.
    ///
    /// Note: 32768 Hz (a value seen elsewhere in GBA audio docs) is only the
    /// `SOUNDBIAS` DAC/PWM carrier frequency, *not* the mixer's PCM render
    /// rate. Using it here would bake in a ~2.45x pitch/timing error, so this
    /// contract is the mixer rate. Devices virtually never advertise 13379 Hz,
    /// so resampling to the device rate is the norm — see the module docs.
    pub const M4A_MIXER_RATE: u32 = 13_379;

    /// Interleaved channel count the ring buffer and device stream use
    /// (stereo, matching the GBA's Direct Sound A/B output).
    pub const CHANNELS: u16 = 2;

    /// Open the default output device and negotiate [`Self::M4A_MIXER_RATE`]
    /// or the nearest supported rate (falling back to on-the-fly resampling
    /// if the exact rate is unavailable — the common case, see the module
    /// docs). The stream is created but not started; call
    /// [`AudioOutput::start`].
    ///
    /// `ring_capacity_frames` sizes the ring buffer in stereo frames (e.g.
    /// `4096` is ~306ms of headroom at the nominal 13379 Hz rate).
    ///
    /// # Errors
    ///
    /// - [`PlatformError::NoAudioDevice`] if there is no default output
    ///   device (headless CI, no audio hardware, no driver running).
    /// - [`PlatformError::UnsupportedAudioConfig`] if the device has no
    ///   usable stereo output configuration.
    /// - [`PlatformError::Audio`] if `cpal` fails to query the device or
    ///   build the stream.
    pub fn open(ring_capacity_frames: usize) -> Result<Self, PlatformError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(PlatformError::NoAudioDevice)?;
        let config = negotiate(&device)?;

        let device_sample_rate = config.sample_rate();
        let channels = config.channels();
        let (producer, consumer) = ring_buffer(ring_capacity_frames * channels as usize);
        let source = if device_sample_rate == Self::M4A_MIXER_RATE {
            Source::Direct(consumer)
        } else {
            // Pre-size the resampler's source-frame scratch off the real-time
            // thread, bounded by the device's largest advertised callback (in
            // frames); see `Resampler::new` and `max_buffer_frames`.
            Source::Resampled(Resampler::new(
                consumer,
                channels,
                Self::M4A_MIXER_RATE,
                device_sample_rate,
                max_buffer_frames(&config),
            ))
        };

        let stream_errors = Arc::new(AtomicU64::new(0));
        let stream = build_stream(&device, &config, source, Arc::clone(&stream_errors))?;

        Ok(Self {
            backend: Backend::Device(stream),
            producer,
            sample_rate: Self::M4A_MIXER_RATE,
            device_sample_rate,
            channels,
            running: false,
            stream_errors,
        })
    }

    /// An explicit headless/null backend: opens no OS audio device.
    ///
    /// Always available (no hardware required), and the only backend unit
    /// tests may construct — CI runners have no audio device, so `cargo
    /// test` must never open a real `cpal` stream. Drive it by hand with
    /// [`AudioOutput::pull_null`].
    #[must_use]
    pub fn null(ring_capacity_frames: usize) -> Self {
        let (producer, consumer) = ring_buffer(ring_capacity_frames * usize::from(Self::CHANNELS));
        Self {
            backend: Backend::Null(Source::Direct(consumer)),
            producer,
            sample_rate: Self::M4A_MIXER_RATE,
            device_sample_rate: Self::M4A_MIXER_RATE,
            channels: Self::CHANNELS,
            running: false,
            stream_errors: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Start (or resume) playback.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Audio`] if `cpal` refuses to play the
    /// stream (e.g. the device was disconnected). Always succeeds for the
    /// null backend.
    pub fn start(&mut self) -> Result<(), PlatformError> {
        if let Backend::Device(stream) = &self.backend {
            stream.play()?;
        }
        self.running = true;
        Ok(())
    }

    /// Pause playback; the device/stream stays open and can be
    /// [`AudioOutput::start`]ed again.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Audio`] if `cpal` refuses to pause the
    /// stream. Always succeeds for the null backend.
    pub fn stop(&mut self) -> Result<(), PlatformError> {
        if let Backend::Device(stream) = &self.backend {
            stream.pause()?;
        }
        self.running = false;
        Ok(())
    }

    /// Whether [`AudioOutput::start`] has been called more recently than
    /// [`AudioOutput::stop`].
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// The nominal PCM sample rate the `audio` crate should always render
    /// at ([`Self::M4A_MIXER_RATE`]), regardless of the device's physical
    /// rate.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The audio device's actual negotiated sample rate. Equal to
    /// [`AudioOutput::sample_rate`] unless a [`Resampler`] is bridging a
    /// mismatch (see the module docs); always equal for the null backend.
    #[must_use]
    pub fn device_sample_rate(&self) -> u32 {
        self.device_sample_rate
    }

    /// Interleaved channel count of the ring buffer / device stream.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// A cloneable producer handle for the future `audio` crate to fill
    /// from another thread. See [`crate::ring::Producer`].
    #[must_use]
    pub fn producer(&self) -> Producer {
        self.producer.clone()
    }

    /// Total samples played as silence so far due to ring-buffer underrun
    /// (producer outran consumer). Distinct from [`Self::stream_errors`],
    /// which covers asynchronous device/driver failures.
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.producer.underruns()
    }

    /// Count of asynchronous `cpal` stream errors reported since the stream
    /// was built (device disconnect, driver failure, …).
    ///
    /// A nonzero value means the stream is unhealthy: playback may have
    /// stopped at the OS level even though [`Self::is_running`] still reports
    /// `true` and [`Self::underruns`] is flat, because the callback that
    /// drains the ring buffer is no longer being invoked. Always zero for the
    /// null backend, which owns no `cpal` stream.
    #[must_use]
    pub fn stream_errors(&self) -> u64 {
        self.stream_errors.load(Ordering::Relaxed)
    }

    /// Drive the null backend by hand, filling `out` through the exact same
    /// underrun-safe path the real device callback runs (see the module
    /// docs and [`crate::ring::Consumer::fill`]).
    ///
    /// A no-op (leaves `out` untouched) if this instance was opened against
    /// a real device via [`AudioOutput::open`] — the OS drives consumption
    /// there instead, on its own callback thread.
    pub fn pull_null(&mut self, out: &mut [f32]) {
        if let Backend::Null(source) = &mut self.backend {
            source.fill(out);
        }
    }
}

/// Rank a device's sample format by how directly the ring buffer's `f32`
/// samples map onto it — lower is preferred.
fn sample_format_rank(format: cpal::SampleFormat) -> u8 {
    match format {
        cpal::SampleFormat::F32 => 0,
        cpal::SampleFormat::I16 => 1,
        _ => 2,
    }
}

/// Whether [`build_stream`] can actually open a stream in this sample format.
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
/// Candidates are first restricted to formats [`build_stream`] can open
/// (`f32`/`i16`) — a `u16`-only config whose rate range happens to cover the
/// target must never win, or `AudioOutput::open` would hard-fail instead of
/// resampling on an available openable format. Among the openable survivors
/// (ranked by [`sample_format_rank`], `f32` before `i16`) it prefers any
/// candidate whose `[min, max]` range covers `target` exactly; failing that,
/// it clamps `target` into the most-preferred openable candidate's range for
/// [`AudioOutput::open`] to resample against.
///
/// Returns the chosen candidate's index into `candidates` and the rate to
/// open it at, or `None` if no openable candidate exists.
fn select_config(
    candidates: &[(cpal::SampleFormat, u32, u32)],
    target: u32,
) -> Option<(usize, u32)> {
    let mut openable: Vec<usize> = (0..candidates.len())
        .filter(|&i| is_openable_format(candidates[i].0))
        .collect();
    // Stable sort by format preference keeps the original device order within
    // a format.
    openable.sort_by_key(|&i| sample_format_rank(candidates[i].0));

    if let Some(&i) = openable
        .iter()
        .find(|&&i| (candidates[i].1..=candidates[i].2).contains(&target))
    {
        return Some((i, target));
    }

    let &i = openable.first()?;
    let (_, min, max) = candidates[i];
    Some((i, target.clamp(min, max)))
}

/// Pick a stereo output configuration for [`AudioOutput::M4A_MIXER_RATE`],
/// delegating the selection policy to [`select_config`] and mapping the
/// chosen candidate back to a concrete [`cpal::SupportedStreamConfig`].
fn negotiate(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, PlatformError> {
    let candidates: Vec<cpal::SupportedStreamConfigRange> = device
        .supported_output_configs()
        .map_err(PlatformError::from)?
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

/// The device's largest advertised callback size in frames, or `0` if the
/// device advertises no concrete range (`Unknown`). Used to pre-size
/// real-time scratch buffers off the callback thread (see [`build_stream`]'s
/// `i16` path and [`Resampler::new`]).
fn max_buffer_frames(config: &cpal::SupportedStreamConfig) -> usize {
    match config.buffer_size() {
        cpal::SupportedBufferSize::Range { max, .. } => usize::try_from(*max).unwrap_or(0),
        cpal::SupportedBufferSize::Unknown => 0,
    }
}

/// Convert one `f32` sample in `[-1.0, 1.0]` to `i16`, clamping out-of-range
/// input rather than wrapping.
fn f32_to_i16(sample: f32) -> i16 {
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
const DEFAULT_SCRATCH_SAMPLES: usize = 8192 * 2;

/// Build (but do not start) the output stream for `config`, driven by
/// `source`. Asynchronous stream errors are recorded into `stream_errors`,
/// the counter [`AudioOutput::stream_errors`] reads.
fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    mut source: Source,
    stream_errors: Arc<AtomicU64>,
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

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            stream_config,
            move |data: &mut [f32], _| source.fill(data),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => {
            // Pre-size the `f32` scratch buffer here, off the real-time
            // callback thread, so steady-state callbacks never allocate. The
            // device's largest supported buffer (in frames) bounds any
            // `data.len()` cpal will hand us; multiply by the channel count
            // for interleaved samples. A device that reports no buffer-size
            // range gets a generous fallback. The in-callback `resize` below
            // then only reallocates in the last-resort case where cpal hands
            // us a buffer larger than anything advertised.
            let max_frames = max_buffer_frames(config);
            let channels = usize::from(config.channels());
            let scratch_capacity = max_frames
                .saturating_mul(channels)
                .max(DEFAULT_SCRATCH_SAMPLES);
            let mut scratch: Vec<f32> = Vec::with_capacity(scratch_capacity);
            device.build_output_stream(
                stream_config,
                move |data: &mut [i16], _| {
                    if scratch.len() != data.len() {
                        scratch.resize(data.len(), 0.0);
                    }
                    source.fill(&mut scratch);
                    for (dst, &sample) in data.iter_mut().zip(scratch.iter()) {
                        *dst = f32_to_i16(sample);
                    }
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

#[cfg(test)]
// Tests compare PCM sample arrays for exact equality on purpose: every
// value here is the deliberate, exactly-representable output of a ring
// buffer push/fill, not the result of accumulated floating-point math.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn null_backend_reports_the_m4a_mixer_rate() {
        let output = AudioOutput::null(256);
        // The nominal producer contract is upstream's 13379 Hz M4A mixer
        // rate, not the SOUNDBIAS carrier; see `M4A_MIXER_RATE`'s derivation.
        assert_eq!(output.sample_rate(), 13_379);
        assert_eq!(output.sample_rate(), AudioOutput::M4A_MIXER_RATE);
        assert_eq!(output.device_sample_rate(), AudioOutput::M4A_MIXER_RATE);
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
    fn f32_to_i16_clamps_out_of_range_input() {
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(1.0), i16::MAX);
        assert_eq!(f32_to_i16(2.0), i16::MAX);
        assert_eq!(f32_to_i16(-2.0), -i16::MAX);
    }

    #[test]
    fn sample_format_rank_prefers_f32_then_i16() {
        assert!(
            sample_format_rank(cpal::SampleFormat::F32)
                < sample_format_rank(cpal::SampleFormat::I16)
        );
        assert!(
            sample_format_rank(cpal::SampleFormat::I16)
                < sample_format_rank(cpal::SampleFormat::U16)
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
    fn select_config_takes_i16_exact_over_f32_noncovering() {
        // f32 is preferred but does not cover the target; the exact-rate i16
        // config beats resampling on f32.
        let candidates = [
            (cpal::SampleFormat::F32, 44_100, 48_000), // does not cover 13379
            (cpal::SampleFormat::I16, 8_000, 48_000),  // covers 13379 exactly
        ];
        assert_eq!(select_config(&candidates, 13_379), Some((1, 13_379)));
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
}
