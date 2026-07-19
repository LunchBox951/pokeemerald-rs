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
//! - **Sample rate**: [`AudioOutput::GBA_SAMPLE_RATE`] (32768 Hz, upstream's
//!   M4A mixing rate) is always the ring buffer's nominal rate — the
//!   `audio` crate renders at this rate unconditionally. If the device's
//!   default output config offers this rate directly, samples stream
//!   through 1:1 (see [`Source::Direct`]). If not, a
//!   [`crate::resample::Resampler`] linearly interpolates from nominal to
//!   the device's actual rate inside the callback (see [`Source::Resampled`]).
//! - **Channels**: fixed at [`AudioOutput::CHANNELS`] (stereo), matching the
//!   GBA's Direct Sound A/B stereo output. A device with no stereo output
//!   config at all is out of scope and reported as
//!   [`PlatformError::UnsupportedAudioConfig`].
//! - **Underruns**: [`Source::fill`] always fills its output buffer
//!   completely; any shortfall is silence, counted via
//!   [`crate::ring::Consumer::pop_or_silence`] (see `crate::ring` and
//!   `crate::resample`) for later use by V-5 audio checks.
//!
//! CI is headless, so nothing here opens a real cpal stream in a test: only
//! [`AudioOutput::open`] and the private `negotiate`/stream-building helpers
//! touch `cpal` directly. The ring buffer and resampler — the logic that
//! actually matters for correctness — are pure and fully unit tested
//! against [`AudioOutput::null`] and the `ring`/`resample` modules directly.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::error::PlatformError;
use crate::resample::Resampler;
use crate::ring::{ring_buffer, Consumer, Producer};

/// Either play ring-buffer samples straight through, or bridge a sample-rate
/// mismatch via [`Resampler`] — see the module docs.
///
/// Both variants bottom out in [`crate::ring::Consumer::pop_or_silence`], so
/// the underrun-safe behaviour tested against the null backend below is
/// exactly what the real device callback runs.
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
}

impl AudioOutput {
    /// The GBA's M4A mixing rate (`pokeemerald/src/m4a.c`), and the only
    /// sample rate the ring buffer / `audio` crate ever need to reason
    /// about — see the module docs for how a device that doesn't support it
    /// directly is handled.
    pub const GBA_SAMPLE_RATE: u32 = 32_768;

    /// Interleaved channel count the ring buffer and device stream use
    /// (stereo, matching the GBA's Direct Sound A/B output).
    pub const CHANNELS: u16 = 2;

    /// Open the default output device and negotiate [`Self::GBA_SAMPLE_RATE`]
    /// or the nearest supported rate (falling back to on-the-fly resampling
    /// if the exact rate is unavailable — see the module docs). The stream
    /// is created but not started; call [`AudioOutput::start`].
    ///
    /// `ring_capacity_frames` sizes the ring buffer in stereo frames (e.g.
    /// `4096` is ~125ms of headroom at the nominal rate).
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
        let source = if device_sample_rate == Self::GBA_SAMPLE_RATE {
            Source::Direct(consumer)
        } else {
            Source::Resampled(Resampler::new(
                consumer,
                channels,
                Self::GBA_SAMPLE_RATE,
                device_sample_rate,
            ))
        };

        let stream = build_stream(&device, &config, source)?;

        Ok(Self {
            backend: Backend::Device(stream),
            producer,
            sample_rate: Self::GBA_SAMPLE_RATE,
            device_sample_rate,
            channels,
            running: false,
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
            sample_rate: Self::GBA_SAMPLE_RATE,
            device_sample_rate: Self::GBA_SAMPLE_RATE,
            channels: Self::CHANNELS,
            running: false,
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
    /// at ([`Self::GBA_SAMPLE_RATE`]), regardless of the device's physical
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

    /// Total samples played as silence so far due to ring-buffer underrun.
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.producer.underruns()
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

/// Pick a stereo output configuration: [`AudioOutput::GBA_SAMPLE_RATE`] if
/// any candidate supports it directly, otherwise the nearest supported rate
/// on the most-preferred sample format (see [`sample_format_rank`]) —
/// [`AudioOutput::open`] falls back to resampling in that case.
fn negotiate(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, PlatformError> {
    let mut candidates: Vec<cpal::SupportedStreamConfigRange> = device
        .supported_output_configs()
        .map_err(PlatformError::from)?
        .filter(|c| c.channels() == AudioOutput::CHANNELS)
        .collect();
    candidates.sort_by_key(|c| sample_format_rank(c.sample_format()));

    let target = AudioOutput::GBA_SAMPLE_RATE;
    if let Some(exact) = candidates
        .iter()
        .find(|c| (c.min_sample_rate()..=c.max_sample_rate()).contains(&target))
    {
        return Ok((*exact).with_sample_rate(target));
    }

    let best = candidates
        .into_iter()
        .next()
        .ok_or(PlatformError::UnsupportedAudioConfig)?;
    let nearest = target.clamp(best.min_sample_rate(), best.max_sample_rate());
    Ok(best.with_sample_rate(nearest))
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

/// Build (but do not start) the output stream for `config`, driven by
/// `source`.
fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    mut source: Source,
) -> Result<cpal::Stream, PlatformError> {
    let stream_config = config.config();
    let err_fn = |_err: cpal::Error| {
        // Nothing actionable to do from inside the audio callback thread;
        // `AudioOutput::underruns` and a future health check are the
        // observable signal for playback problems.
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            stream_config,
            move |data: &mut [f32], _| source.fill(data),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => {
            let mut scratch: Vec<f32> = Vec::new();
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
    fn null_backend_reports_the_nominal_gba_rate() {
        let output = AudioOutput::null(256);
        assert_eq!(output.sample_rate(), AudioOutput::GBA_SAMPLE_RATE);
        assert_eq!(output.device_sample_rate(), AudioOutput::GBA_SAMPLE_RATE);
        assert_eq!(output.channels(), AudioOutput::CHANNELS);
        assert!(!output.is_running());
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
}
