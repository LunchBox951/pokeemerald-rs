//! Error type for the `platform` crate.
//!
//! A concrete per-crate enum `(oop-boundaries)` — no `anyhow` in library
//! crates.

use std::fmt;

/// An error produced while creating or driving the platform window or audio
/// output.
#[derive(Debug)]
pub enum PlatformError {
    /// The windowing event loop could not be created, or failed while
    /// running.
    EventLoop(winit::error::EventLoopError),
    /// The OS refused a window-creation (or other OS-level) request.
    Os(winit::error::OsError),
    /// `softbuffer` failed to create, resize, or present a surface.
    SoftBuffer(softbuffer::SoftBufferError),
    /// No usable audio output device could be reached (headless CI, no audio
    /// hardware, no driver running, etc).
    ///
    /// Covers an unreachable device as well as an absent one: `cpal`'s ALSA
    /// host hands out the logical `default` device whether or not sound
    /// hardware exists, so `crate::audio` reports the phantom that fails its
    /// first query here rather than as [`Self::Audio`]. See
    /// `crate::audio::classify_query_error`.
    NoAudioDevice,
    /// The default audio output device has no stream configuration this
    /// crate can use (see `crate::audio` for the requirements).
    UnsupportedAudioConfig,
    /// `cpal` failed to query, configure, build, or drive an audio stream.
    Audio(cpal::Error),
    /// Scripted buttons were supplied to a windowed backend. Only the
    /// explicit null backend accepts injected input; windowed input remains
    /// owned by the OS event loop.
    ScriptedInputRequiresHeadless,
    /// The negotiated source-to-device sample-rate ratio is too extreme for
    /// [`crate::resample::Resampler`]'s bounded source-frame scratch to carry
    /// in one call: even the smallest possible chunk would still need more
    /// source frames than the scratch cap allows, which would otherwise leave
    /// an unresolved interpolation cursor behind for `fill` to extrapolate
    /// with. Refused up front rather than degrading into incorrect audio.
    UnsupportedResampleRatio {
        /// The nominal source sample rate the resampler was asked to bridge
        /// from.
        source_rate: u32,
        /// The negotiated device sample rate the resampler was asked to
        /// bridge to.
        device_rate: u32,
    },
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLoop(err) => write!(f, "windowing event loop error: {err}"),
            Self::Os(err) => write!(f, "OS window error: {err}"),
            Self::SoftBuffer(err) => write!(f, "softbuffer presentation error: {err}"),
            Self::NoAudioDevice => write!(f, "no default audio output device is available"),
            Self::UnsupportedAudioConfig => {
                write!(
                    f,
                    "the default audio output device has no usable stream configuration"
                )
            }
            Self::Audio(err) => write!(f, "audio device error: {err}"),
            Self::ScriptedInputRequiresHeadless => {
                write!(f, "scripted input requires the headless platform backend")
            }
            Self::UnsupportedResampleRatio {
                source_rate,
                device_rate,
            } => write!(
                f,
                "the {source_rate} Hz -> {device_rate} Hz resample ratio is too extreme for \
                 the resampler's bounded scratch to carry"
            ),
        }
    }
}

impl std::error::Error for PlatformError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::EventLoop(err) => Some(err),
            Self::Os(err) => Some(err),
            Self::SoftBuffer(err) => Some(err),
            Self::NoAudioDevice
            | Self::UnsupportedAudioConfig
            | Self::ScriptedInputRequiresHeadless
            | Self::UnsupportedResampleRatio { .. } => None,
            Self::Audio(err) => Some(err),
        }
    }
}

impl From<winit::error::EventLoopError> for PlatformError {
    fn from(err: winit::error::EventLoopError) -> Self {
        Self::EventLoop(err)
    }
}

impl From<winit::error::OsError> for PlatformError {
    fn from(err: winit::error::OsError) -> Self {
        Self::Os(err)
    }
}

impl From<softbuffer::SoftBufferError> for PlatformError {
    fn from(err: softbuffer::SoftBufferError) -> Self {
        Self::SoftBuffer(err)
    }
}

impl From<cpal::Error> for PlatformError {
    fn from(err: cpal::Error) -> Self {
        Self::Audio(err)
    }
}
