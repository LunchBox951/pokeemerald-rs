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
    /// No default audio output device is available (headless CI, no audio
    /// hardware, no driver running, etc).
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
            | Self::ScriptedInputRequiresHeadless => None,
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

impl PlatformError {
    /// Whether no audio output device could be reached at all: the tolerated
    /// headless case.
    ///
    /// [`Self::NoAudioDevice`] is only half of that case. `cpal`'s ALSA
    /// backend hands out the logical `default` device whether or not any
    /// sound hardware or sound server exists, so a headless Linux box with
    /// `libasound` installed gets past
    /// [`AudioOutput::open`](crate::AudioOutput::open)'s device lookup and
    /// fails at the first query against that phantom device instead —
    /// arriving here as [`Self::Audio`] carrying
    /// [`cpal::ErrorKind::DeviceNotAvailable`], or
    /// [`cpal::ErrorKind::HostUnavailable`] when the host itself is absent.
    /// Both say what `NoAudioDevice` says: there is nothing to play through.
    ///
    /// A device that *was* reached and then refused is deliberately not
    /// covered — [`Self::UnsupportedAudioConfig`], a denied permission, an
    /// exhausted resource, and a refused stream build are real failures a
    /// caller should report rather than shrug off.
    #[must_use]
    pub fn is_audio_device_unavailable(&self) -> bool {
        match self {
            Self::NoAudioDevice => true,
            Self::Audio(err) => matches!(
                err.kind(),
                cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::HostUnavailable
            ),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(kind: cpal::ErrorKind) -> PlatformError {
        PlatformError::from(cpal::Error::new(kind))
    }

    #[test]
    fn no_audio_device_is_an_unavailable_device() {
        assert!(PlatformError::NoAudioDevice.is_audio_device_unavailable());
    }

    /// The regression this predicate exists for: on the ALSA backend a
    /// headless box reaches the logical `default` device and only fails when
    /// queried, so the unreachable-device case arrives as `Audio`, not
    /// `NoAudioDevice`.
    #[test]
    fn an_unreachable_cpal_device_or_host_is_an_unavailable_device() {
        assert!(audio(cpal::ErrorKind::DeviceNotAvailable).is_audio_device_unavailable());
        assert!(audio(cpal::ErrorKind::HostUnavailable).is_audio_device_unavailable());
    }

    #[test]
    fn a_reached_but_refused_device_is_not_an_unavailable_device() {
        assert!(!PlatformError::UnsupportedAudioConfig.is_audio_device_unavailable());
        assert!(!audio(cpal::ErrorKind::UnsupportedConfig).is_audio_device_unavailable());
        assert!(!audio(cpal::ErrorKind::PermissionDenied).is_audio_device_unavailable());
        assert!(!audio(cpal::ErrorKind::DeviceBusy).is_audio_device_unavailable());
        assert!(!audio(cpal::ErrorKind::BackendError).is_audio_device_unavailable());
    }

    #[test]
    fn a_non_audio_error_is_not_an_unavailable_device() {
        assert!(!PlatformError::ScriptedInputRequiresHeadless.is_audio_device_unavailable());
    }
}
