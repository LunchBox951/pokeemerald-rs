//! Error type for the `platform` crate.
//!
//! A concrete per-crate enum `(oop-boundaries)` — no `anyhow` in library
//! crates.

use std::fmt;

/// An error produced while creating or driving the platform window.
#[derive(Debug)]
pub enum PlatformError {
    /// The windowing event loop could not be created, or failed while
    /// running.
    EventLoop(winit::error::EventLoopError),
    /// The OS refused a window-creation (or other OS-level) request.
    Os(winit::error::OsError),
    /// `softbuffer` failed to create, resize, or present a surface.
    SoftBuffer(softbuffer::SoftBufferError),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLoop(err) => write!(f, "windowing event loop error: {err}"),
            Self::Os(err) => write!(f, "OS window error: {err}"),
            Self::SoftBuffer(err) => write!(f, "softbuffer presentation error: {err}"),
        }
    }
}

impl std::error::Error for PlatformError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::EventLoop(err) => Some(err),
            Self::Os(err) => Some(err),
            Self::SoftBuffer(err) => Some(err),
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
