//! Audio output device: opens the default output device — or a
//! headless-friendly null backend for tests/CI, since CI runners have no
//! audio device — and streams PCM pulled from a [`crate::ring`] ring buffer
//! that its caller fills with the `audio` crate's rendered M4A output (in
//! practice the integration crate's frame-driven music player).
//!
//! `cpal` is owner-approved for exactly this crate and exactly this use:
//! open the default output device, one stream, a ring-buffer callback. No
//! decoding, no effects — see [`Resampler`] below for the one deliberate
//! exception (bridging a sample-rate mismatch is format adaptation, not an
//! effect).
//!
//! ## Design
//!
//! - **Sample format**: the ring buffer always carries interleaved `f32`
//!   samples (cpal's most portable format, and natural headroom for
//!   downstream mixing). If the device's negotiated stream format is `i16`
//!   instead (common on Linux/ALSA), the device callback converts on the
//!   way out; the ring buffer and its producers never need to know.
//! - **Sample rate**: [`AudioOutput::M4A_MIXER_RATE`] (13379 Hz, the rate
//!   upstream's M4A engine actually renders PCM at — see the const's docs) is
//!   the ring buffer's nominal rate for pitch/synthesis purposes; the `audio`
//!   crate renders at this rate unconditionally. The ring buffer's *actual*
//!   production cadence is [`AudioOutput::source_cadence_hz`] instead.
//!   Devices virtually never advertise 13379 Hz, so a
//!   [`crate::resample::Resampler`] bridging nominal to actual rate inside
//!   the callback (`Source::Resampled`) is the common path; direct 1:1
//!   streaming (`Source::Direct`) is reserved for the null backend.
//! - **Channels**: fixed at [`AudioOutput::CHANNELS`] (stereo), matching the
//!   GBA's Direct Sound A/B stereo output. A device with no stereo output
//!   config at all is out of scope and reported as
//!   [`PlatformError::UnsupportedAudioConfig`].
//! - **Underruns**: `Source::fill` always fills its output buffer
//!   completely; any shortfall is silence, counted via
//!   [`crate::ring::Consumer::fill`]'s non-blocking bulk drain (see
//!   `crate::ring` and `crate::resample`) so audio-health checks can
//!   observe shortfalls.
//! - **Stream health**: underruns cover the producer-outran-consumer case,
//!   but a `cpal` stream can also fail asynchronously (device disconnect,
//!   driver error) on its own callback thread. Those are counted separately
//!   via [`AudioOutput::stream_errors`]; a nonzero count means the stream is
//!   unhealthy even when [`AudioOutput::underruns`] stays flat.
//!
//! CI is headless, so nothing here opens a real cpal stream in a test: only
//! [`AudioOutput::open`] and the private `negotiate`/stream-building helpers
//! touch `cpal` directly. Both take their cpal calls behind
//! [`OutputDevice`], so each stage's error mapping is testable against a
//! fake that opens no device. The ring buffer and resampler — the logic that
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
/// `Direct` is reserved for [`AudioOutput::null`], which its caller clocks
/// by hand. A real device, built through [`source_for_device`], is always
/// `Resampled`, even at exactly [`AudioOutput::M4A_MIXER_RATE`] Hz: its
/// clock free-runs at that integer rate, not at
/// [`AudioOutput::source_cadence_hz`].
///
/// Both variants bottom out in [`crate::ring::Consumer::fill`]'s non-blocking
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

/// Build the [`Source`] a real device's stream is driven through — see
/// [`Source`]'s docs for why this is always [`Source::Resampled`].
///
/// Extracted from [`AudioOutput::open`] so this selection is unit-testable
/// without a `cpal` device (only `open` itself touches `cpal` to get here —
/// see the module docs).
///
/// # Errors
///
/// See [`crate::resample::Resampler::new`]'s `UnsupportedResampleRatio` doc.
fn source_for_device(
    consumer: Consumer,
    channels: u16,
    device_sample_rate: u32,
    max_output_frames: usize,
) -> Result<Source, PlatformError> {
    Ok(Source::Resampled(Resampler::new(
        consumer,
        channels,
        AudioOutput::source_cadence_hz(),
        device_sample_rate,
        max_output_frames,
    )?))
}

/// The open output stream/device, or the null stand-in used by tests and
/// headless environments.
enum Backend {
    Null(Source),
    Device(cpal::Stream),
}

/// An owned audio-output subsystem: opens (at most) one output stream and
/// exposes a [`Producer`] handle its caller fills with rendered PCM.
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
    /// `max_buffer_frames` of the negotiated config; `0` for the null backend
    /// and for a device advertising no concrete range.
    max_callback_frames: usize,
}

impl AudioOutput {
    /// The rate upstream's M4A engine actually renders PCM at — the nominal
    /// producer contract for the ring buffer and the `audio` crate, which
    /// renders at exactly this rate.
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

    /// Stereo frames the ring buffer's producer (in practice the integration
    /// crate's frame-driven music player) pushes per game frame — one call to
    /// `MusicPlayer::advance_frame` per `App::step`, each rendering
    /// `Sequencer::FRAME_SAMPLES / CHANNELS` frames. A duplicate of the
    /// `audio` crate's `SAMPLES_PER_FRAME`, not an import of it: `platform`
    /// has no dependency on `audio`, only the reverse, as a dev dependency
    /// (see `crates/audio/Cargo.toml`). Used only by
    /// [`Self::source_cadence_hz`].
    const M4A_SAMPLES_PER_GAME_FRAME: u32 = 224;

    /// Interleaved channel count the ring buffer and device stream use
    /// (stereo, matching the GBA's Direct Sound A/B output).
    pub const CHANNELS: u16 = 2;

    /// The ring buffer's real production cadence, about 13378.96 Hz:
    /// [`Self::M4A_SAMPLES_PER_GAME_FRAME`] frames per
    /// [`crate::pacing::GBA_FRAME_PERIOD`]. The [`Resampler`] drains at this
    /// rate, not the rounded [`Self::M4A_MIXER_RATE`] upstream uses for
    /// pitch; the difference is a rate bias that drains the ring over hours.
    fn source_cadence_hz() -> f64 {
        f64::from(Self::M4A_SAMPLES_PER_GAME_FRAME) / crate::pacing::GBA_FRAME_PERIOD.as_secs_f64()
    }

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
    ///   device, or the one the host named cannot be reached at all
    ///   (headless CI, no audio hardware, no driver running) — see
    ///   [`classify_query_error`].
    /// - [`PlatformError::UnsupportedAudioConfig`] if the device has no
    ///   usable stereo output configuration.
    /// - [`PlatformError::Audio`] if `cpal` fails to query a reachable
    ///   device, or fails to build the stream. A device lost *after* the
    ///   query stays here rather than collapsing into `NoAudioDevice`: the
    ///   device was real, so losing it is a failure, not a headless run.
    /// - [`PlatformError::UnsupportedResampleRatio`] if the negotiated
    ///   device rate pairs with [`Self::source_cadence_hz`] into a ratio the
    ///   resampler's bounded scratch cannot carry (see
    ///   [`crate::resample::Resampler::new`]).
    pub fn open(ring_capacity_frames: usize) -> Result<Self, PlatformError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(PlatformError::NoAudioDevice)?;
        let config = negotiate(&device)?;

        let device_sample_rate = config.sample_rate();
        let channels = config.channels();
        let (producer, consumer) = ring_buffer(ring_capacity_frames * channels as usize);
        let source = source_for_device(
            consumer,
            channels,
            device_sample_rate,
            max_buffer_frames(&config),
        )?;

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
            max_callback_frames: max_buffer_frames(&config),
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
            max_callback_frames: 0,
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

    /// The largest callback buffer the device advertises, in frames at
    /// [`Self::device_sample_rate`], or `None` when it advertises no concrete
    /// range. It bounds one callback period only, not the periods the host
    /// keeps queued behind it, so a caller waiting for the tail to sound
    /// derives its wait from this bound rather than sleeping it verbatim.
    /// Always `None` for the null backend.
    #[must_use]
    pub fn max_callback_frames(&self) -> Option<usize> {
        match self.max_callback_frames {
            0 => None,
            frames => Some(frames),
        }
    }

    /// A cloneable producer handle for filling the ring buffer with
    /// rendered PCM. See [`crate::ring::Producer`].
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

    /// Records one asynchronous stream error as the `cpal` error closure in
    /// `build_stream` does, so a null-backend test can stand in for a device.
    #[doc(hidden)]
    pub fn record_stream_error_for_test(&self) {
        self.stream_errors.fetch_add(1, Ordering::Relaxed);
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
/// resampling on an available openable format. Each openable candidate is then
/// scored by `(distance, format rank)`, where `distance` is how far `target`
/// must be clamped to land inside the candidate's `[min, max]` range, and the
/// minimum is chosen:
///
/// - **Distance is primary**: the nearest achievable rate wins, regardless of
///   format or the order the device enumerated its ranges. A range that
///   covers `target` has distance `0`, so an exact-rate candidate always
///   beats one that needs resampling further — an exact-Hz device still
///   builds a [`Resampler`] (see [`Source`]'s docs), but with a step nearest
///   `1.0`, minimizing interpolation error — so it must not lose to a mere
///   format preference.
/// - **Format rank is the tie-break** ([`sample_format_rank`]: `f32` before
///   `i16`) between candidates equally far from `target`. Ties (equal
///   distance and rank) keep device-enumeration order.
///
/// Returns the chosen candidate's index into `candidates` and the rate to
/// open it at, or `None` if no openable candidate exists.
fn select_config(
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
fn negotiate<D: OutputDevice>(device: &D) -> Result<cpal::SupportedStreamConfig, PlatformError> {
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
/// real, so [`build_stream`] losing it afterwards stays on the plain
/// [`From<cpal::Error>`] mapping to [`PlatformError::Audio`] — a caller that
/// tolerates a headless run must still hear about a device that vanished
/// mid-setup. `a_lost_device_after_the_query_stays_an_audio_error` pins that
/// split against a real device.
fn classify_query_error(err: cpal::Error) -> PlatformError {
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
/// it first — see [`MAX_SCRATCH_CALLBACK_FRAMES`] and [`Resampler::new`].
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

/// Ceiling (in frames) applied to a device-advertised buffer maximum before
/// it sizes the `i16` scratch buffer. A device may advertise an
/// unconstrained callback range as an enormous concrete number instead of
/// `Unknown` (cpal's ALSA backend hands back `u32::MAX` for exactly this),
/// and [`fill_i16_output`] already chunks a callback past this scale, so
/// capping the preallocation here loses nothing but reserved-but-unused
/// memory.
const MAX_SCRATCH_CALLBACK_FRAMES: usize = 8192;

/// Capacity (interleaved `f32` samples) for the `i16` callback's fixed
/// scratch buffer: `max_frames` capped at [`MAX_SCRATCH_CALLBACK_FRAMES`]
/// and floored at [`DEFAULT_SCRATCH_SAMPLES`], times `channels`. Pure so the
/// cap is unit-testable against an extreme advertised maximum without
/// allocating the buffer it sizes.
fn i16_scratch_capacity(max_frames: usize, channels: usize) -> usize {
    max_frames
        .min(MAX_SCRATCH_CALLBACK_FRAMES)
        .saturating_mul(channels)
        .max(DEFAULT_SCRATCH_SAMPLES)
}

/// Fill `data` (interleaved `i16`) from `source`, converting through the
/// fixed `scratch` buffer in `scratch.len()`-bounded chunks rather than
/// resizing it — so a `data` larger than `scratch` still never allocates.
/// `scratch` is never resized here, regardless of how `data` compares to it.
fn fill_i16_output(source: &mut Source, scratch: &mut [f32], data: &mut [i16]) {
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
trait OutputDevice {
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
/// the counter [`AudioOutput::stream_errors`] reads.
fn build_stream<D: OutputDevice>(
    device: &D,
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
            // Fix the `f32` scratch buffer's length here, off the real-time
            // callback thread, so the callback never resizes it — see
            // `i16_scratch_capacity`. The callback below processes `data`
            // in `scratch`-sized chunks instead, so a `data` cpal hands us
            // larger than anything advertised still never grows `scratch`.
            let max_frames = max_buffer_frames(config);
            let channels = usize::from(config.channels());
            let scratch_capacity = i16_scratch_capacity(max_frames, channels);
            let mut scratch: Vec<f32> = vec![0.0; scratch_capacity];
            device.build_output_stream(
                stream_config,
                move |data: &mut [i16], _| fill_i16_output(&mut source, &mut scratch, data),
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

        let measure_seconds =
            f64::from(MEASURE_TICKS) * crate::pacing::GBA_FRAME_PERIOD.as_secs_f64();
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
}
